use crate::{err, wire, Amount, Result};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
pub const MIN_TICK: i32 = -887272;
pub const MAX_TICK: i32 = 887272;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct V3BitmapWord {
    pub word_pos: i32,
    pub bitmap: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct V3Tick {
    pub tick: i32,
    pub liquidity_gross: Amount,
    pub liquidity_net: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct V3State {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub implementation: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub implementation_code_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub factory_code_sha256: String,
    #[serde(default, skip_serializing_if = "zero_fee_protocol")]
    pub fee_protocol: u32,
    pub sqrt_price_x96: Amount,
    pub tick: i32,
    pub liquidity: Amount,
    pub tick_spacing: i32,
    pub factory: String,
    pub code_sha256: String,
    pub max_steps: u32,
    pub tick_words: Vec<V3BitmapWord>,
    pub ticks: Vec<V3Tick>,
}
const FACTORS: [&str; 20] = [
    "fffcb933bd6fad37aa2d162d1a594001",
    "fff97272373d413259a46990580e213a",
    "fff2e50f5f656932ef12357cf3c7fdcc",
    "ffe5caca7e10e4e61c3624eaa0941cd0",
    "ffcb9843d60f6159c9db58835c926644",
    "ff973b41fa98c081472e6896dfb254c0",
    "ff2ea16466c96a3843ec78b326b52861",
    "fe5dee046a99a2a811c461f1969c3053",
    "fcbe86c7900a88aedcffc83b479aa3a4",
    "f987a7253ac413176f2b074cf7815e54",
    "f3392b0822b70005940c7a398e4b70f3",
    "e7159475a2c29b7443b29c7fa6e889d9",
    "d097f3bdfd2022b8845ad8f792aa5825",
    "a9f746462d870fdf8a65dc1f90e061e5",
    "70d869a156d2a1b890bb3df62baf32f7",
    "31be135f97d08fd981231505542fcfa6",
    "9aa508b5b7a84e1c677de54f3e99bc9",
    "5d6af8dedb81196699c329225ee604",
    "2216e584f5fa1ea926041bedfe98",
    "48a170391f7dc42444e8fa2",
];
pub fn sqrt_ratio_at_tick(tick: i32) -> Result<BigUint> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return Err(err("V3_TICK_RANGE"));
    }
    static NUMBERS: OnceLock<Vec<BigUint>> = OnceLock::new();
    let factors = NUMBERS.get_or_init(|| {
        FACTORS
            .iter()
            .map(|x| BigUint::parse_bytes(x.as_bytes(), 16).expect("constant hex"))
            .collect()
    });
    let abs = tick.unsigned_abs();
    let mut ratio = BigUint::one() << 128usize;
    for (bit, factor) in factors.iter().enumerate() {
        if abs & (1 << bit) != 0 {
            ratio = (ratio * factor) >> 128usize;
        }
    }
    if tick > 0 {
        ratio = ((BigUint::one() << 256usize) - BigUint::one()) / ratio;
    }
    let remainder = &ratio & ((BigUint::one() << 32usize) - BigUint::one());
    Ok((ratio >> 32usize) + BigUint::from(u8::from(!remainder.is_zero())))
}
pub fn tick_at_sqrt_ratio(sqrt: &BigUint) -> Result<i32> {
    if sqrt < &sqrt_ratio_at_tick(MIN_TICK)? || sqrt >= &sqrt_ratio_at_tick(MAX_TICK)? {
        return Err(err("V3_PRICE_RANGE"));
    }
    let (mut low, mut high) = (MIN_TICK, MAX_TICK);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if sqrt_ratio_at_tick(mid)? <= *sqrt {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(low)
}
pub fn parse_liquidity_net(s: &str) -> Result<i128> {
    let n = s
        .parse::<i128>()
        .map_err(|_| err("V3_LIQUIDITY_NET_WIDTH"))?;
    if n.to_string() != s {
        return Err(err("V3_LIQUIDITY_NET_WIDTH"));
    }
    Ok(n)
}
impl V3State {
    pub fn validate(&self) -> Result<()> {
        if !self.factory_code_sha256.is_empty() {
            crate::protocols::validate_identity(&self.factory, &self.factory_code_sha256)?;
        }
        if !self.implementation.is_empty() {
            crate::protocols::validate_identity(
                &self.implementation,
                &self.implementation_code_sha256,
            )?;
        } else if !self.implementation_code_sha256.is_empty() {
            return Err(err("V3_IMPLEMENTATION_IDENTITY"));
        }
        if !(MIN_TICK..MAX_TICK).contains(&self.tick)
            || !(1..16384).contains(&self.tick_spacing)
            || !(1..=8192).contains(&self.max_steps)
            || self.tick_words.is_empty()
            || self.tick_words.len() > 33
            || self.ticks.len() > 4096
        {
            return Err(err("V3_STATE_LIMIT"));
        }
        if self.code_sha256.len() != 64
            || self.code_sha256 != self.code_sha256.to_ascii_lowercase()
            || hex::decode(&self.code_sha256).is_err()
        {
            return Err(err("V3_CODE_IDENTITY"));
        }
        if self.factory.len() != 42
            || !self.factory.starts_with("0x")
            || self.factory != self.factory.to_ascii_lowercase()
            || hex::decode(&self.factory[2..]).is_err()
        {
            return Err(err("V3_FACTORY_IDENTITY"));
        }
        if self.sqrt_price_x96.0.bits() > 160 || self.sqrt_price_x96.0.is_zero() {
            return Err(err("V3_PRICE_WIDTH"));
        }
        if self.liquidity.0.bits() > 128 {
            return Err(err("V3_LIQUIDITY_WIDTH"));
        }
        let price = &self.sqrt_price_x96.0;
        if price < &sqrt_ratio_at_tick(self.tick)?
            || price > &sqrt_ratio_at_tick(self.tick + 1)?
            || price < &sqrt_ratio_at_tick(MIN_TICK)?
            || price >= &sqrt_ratio_at_tick(MAX_TICK)?
        {
            return Err(err("V3_PRICE_TICK_MISMATCH"));
        }
        let mut expected = Vec::new();
        for (i, word) in self.tick_words.iter().enumerate() {
            if !(-32768..=32767).contains(&word.word_pos)
                || i > 0 && word.word_pos != self.tick_words[i - 1].word_pos + 1
            {
                return Err(err("V3_BITMAP_SEQUENCE"));
            }
            if word.bitmap.0.bits() > 256 {
                return Err(err("AMOUNT_OVERFLOW"));
            }
            for bit in 0..256 {
                if word.bitmap.0.bit(bit) {
                    let tick = (i64::from(word.word_pos) * 256 + bit as i64)
                        * i64::from(self.tick_spacing);
                    if !(i64::from(MIN_TICK)..=i64::from(MAX_TICK)).contains(&tick) {
                        return Err(err("V3_BITMAP_TICK_RANGE"));
                    }
                    expected.push(tick as i32);
                }
            }
        }
        let low = i64::from(self.tick_words[0].word_pos) * 256 * i64::from(self.tick_spacing);
        let high = (i64::from(
            self.tick_words
                .last()
                .ok_or_else(|| err("V3_STATE_LIMIT"))?
                .word_pos,
        ) + 1)
            * 256
            * i64::from(self.tick_spacing);
        if i64::from(self.tick) < low - 1 || i64::from(self.tick) >= high {
            return Err(err("V3_CURRENT_TICK_UNPROVEN"));
        }
        if expected.len() != self.ticks.len() {
            return Err(err("V3_INCOMPLETE_TICKS"));
        }
        let num_ticks = MAX_TICK / self.tick_spacing - MIN_TICK / self.tick_spacing + 1;
        let max_gross =
            ((BigUint::one() << 128usize) - BigUint::one()) / BigUint::from(num_ticks as u32);
        for (i, t) in self.ticks.iter().enumerate() {
            if t.tick != expected[i] {
                return Err(err("V3_INCOMPLETE_TICKS"));
            }
            if t.liquidity_gross.0.is_zero() || t.liquidity_gross.0 > max_gross {
                return Err(err("V3_TICK_GROSS"));
            }
            let net = parse_liquidity_net(&t.liquidity_net)?;
            if BigUint::from(net.unsigned_abs()) > t.liquidity_gross.0 {
                return Err(err("V3_TICK_NET"));
            }
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::V3State> {
        self.validate()?;
        Ok(wire::V3State {
            implementation: self.implementation.clone(),
            implementation_code_sha256: hex::decode(&self.implementation_code_sha256)
                .map_err(|_| err("V3_IMPLEMENTATION_IDENTITY"))?,
            factory_code_sha256: hex::decode(&self.factory_code_sha256)
                .map_err(|_| err("V3_FACTORY_CODE"))?,
            fee_protocol: self.fee_protocol,
            sqrt_price_x96: Some(wire::Amount {
                uint256_be: self.sqrt_price_x96.to_be_bytes()?.to_vec(),
            }),
            tick: self.tick,
            liquidity: Some(wire::Amount {
                uint256_be: self.liquidity.to_be_bytes()?.to_vec(),
            }),
            tick_spacing: self.tick_spacing,
            factory: self.factory.clone(),
            code_sha256: hex::decode(&self.code_sha256).map_err(|_| err("V3_CODE_IDENTITY"))?,
            max_steps: self.max_steps,
            tick_words: self
                .tick_words
                .iter()
                .map(|w| {
                    Ok(wire::V3BitmapWord {
                        word_pos: w.word_pos,
                        bitmap: Some(wire::Amount {
                            uint256_be: w.bitmap.to_be_bytes()?.to_vec(),
                        }),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            ticks: self
                .ticks
                .iter()
                .map(|t| {
                    Ok(wire::V3Tick {
                        tick: t.tick,
                        liquidity_gross: Some(wire::Amount {
                            uint256_be: t.liquidity_gross.to_be_bytes()?.to_vec(),
                        }),
                        liquidity_net_i128_be: parse_liquidity_net(&t.liquidity_net)?
                            .to_be_bytes()
                            .to_vec(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
    pub fn from_proto(v: wire::V3State) -> Result<Self> {
        if v.tick_words.len() > 33 || v.ticks.len() > 4096 {
            return Err(err("V3_STATE_LIMIT"));
        }
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(&a.ok_or_else(|| err("V3_AMOUNT_REQUIRED"))?.uint256_be)
        }
        if v.code_sha256.len() != 32 {
            return Err(err("V3_CODE_IDENTITY"));
        }
        if !v.factory_code_sha256.is_empty() && v.factory_code_sha256.len() != 32 {
            return Err(err("V3_FACTORY_CODE"));
        }
        if !v.implementation_code_sha256.is_empty() && v.implementation_code_sha256.len() != 32 {
            return Err(err("V3_IMPLEMENTATION_IDENTITY"));
        }
        let state = Self {
            implementation: v.implementation,
            implementation_code_sha256: hex::encode(v.implementation_code_sha256),
            factory_code_sha256: hex::encode(v.factory_code_sha256),
            fee_protocol: v.fee_protocol,
            sqrt_price_x96: amount(v.sqrt_price_x96)?,
            tick: v.tick,
            liquidity: amount(v.liquidity)?,
            tick_spacing: v.tick_spacing,
            factory: v.factory,
            code_sha256: hex::encode(v.code_sha256),
            max_steps: v.max_steps,
            tick_words: v
                .tick_words
                .into_iter()
                .map(|w| {
                    Ok(V3BitmapWord {
                        word_pos: w.word_pos,
                        bitmap: amount(w.bitmap)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            ticks: v
                .ticks
                .into_iter()
                .map(|t| {
                    let b: [u8; 16] = t
                        .liquidity_net_i128_be
                        .try_into()
                        .map_err(|_| err("V3_LIQUIDITY_NET_WIDTH"))?;
                    Ok(V3Tick {
                        tick: t.tick,
                        liquidity_gross: amount(t.liquidity_gross)?,
                        liquidity_net: i128::from_be_bytes(b).to_string(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        state.validate()?;
        Ok(state)
    }
}

fn zero_fee_protocol(v: &u32) -> bool {
    *v == 0
}
