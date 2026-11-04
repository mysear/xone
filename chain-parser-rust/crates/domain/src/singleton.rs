use crate::v3::{
    parse_liquidity_net, sqrt_ratio_at_tick, V3BitmapWord, V3Tick, MAX_TICK, MIN_TICK,
};
use crate::{err, wire, Amount, Result};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};
pub const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConcentratedState {
    pub sqrt_price_x96: Amount,
    pub tick: i32,
    pub liquidity: Amount,
    pub tick_spacing: i32,
    pub max_steps: u32,
    pub tick_words: Vec<V3BitmapWord>,
    pub ticks: Vec<V3Tick>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SingletonCLState {
    pub manager: String,
    pub manager_code_sha256: String,
    pub pool_id: String,
    pub lp_fee: u32,
    pub protocol_fee0: u32,
    pub protocol_fee1: u32,
    pub hooks: String,
    pub parameters: String,
    pub core: ConcentratedState,
}
impl ConcentratedState {
    pub fn validate(&self, _protocol: &str) -> Result<()> {
        if !(MIN_TICK..MAX_TICK).contains(&self.tick)
            || !(1..32768).contains(&self.tick_spacing)
            || !(1..=8192).contains(&self.max_steps)
            || self.tick_words.is_empty()
            || self.tick_words.len() > 33
            || self.ticks.len() > 4096
        {
            return Err(err("SINGLETON_STATE_LIMIT"));
        }
        if self.sqrt_price_x96.0.bits() > 160 || self.sqrt_price_x96.0.is_zero() {
            return Err(err("SINGLETON_PRICE_WIDTH"));
        }
        if self.liquidity.0.bits() > 128 {
            return Err(err("SINGLETON_LIQUIDITY_WIDTH"));
        }
        let price = &self.sqrt_price_x96.0;
        if price < &sqrt_ratio_at_tick(self.tick)?
            || price > &sqrt_ratio_at_tick(self.tick + 1)?
            || price < &sqrt_ratio_at_tick(MIN_TICK)?
            || price >= &sqrt_ratio_at_tick(MAX_TICK)?
        {
            return Err(err("SINGLETON_PRICE_TICK_MISMATCH"));
        }
        let mut expected = Vec::new();
        for (i, word) in self.tick_words.iter().enumerate() {
            if !(-32768..=32767).contains(&word.word_pos)
                || i > 0 && word.word_pos != self.tick_words[i - 1].word_pos + 1
            {
                return Err(err("SINGLETON_BITMAP_SEQUENCE"));
            }
            if word.bitmap.0.bits() > 256 {
                return Err(err("AMOUNT_OVERFLOW"));
            }
            for bit in 0..256 {
                if word.bitmap.0.bit(bit) {
                    let tick = (i64::from(word.word_pos) * 256 + bit as i64)
                        * i64::from(self.tick_spacing);
                    if !(i64::from(MIN_TICK)..=i64::from(MAX_TICK)).contains(&tick) {
                        return Err(err("SINGLETON_BITMAP_TICK_RANGE"));
                    }
                    expected.push(tick as i32);
                }
            }
        }
        let low = i64::from(self.tick_words[0].word_pos) * 256 * i64::from(self.tick_spacing);
        let high = (i64::from(
            self.tick_words
                .last()
                .ok_or_else(|| err("SINGLETON_STATE_LIMIT"))?
                .word_pos,
        ) + 1)
            * 256
            * i64::from(self.tick_spacing);
        if i64::from(self.tick) < low - 1 || i64::from(self.tick) >= high {
            return Err(err("SINGLETON_CURRENT_TICK_UNPROVEN"));
        }
        if expected.len() != self.ticks.len() {
            return Err(err("SINGLETON_INCOMPLETE_TICKS"));
        }
        let min = MIN_TICK.div_euclid(self.tick_spacing);
        let num_ticks = MAX_TICK / self.tick_spacing - min + 1;
        let max_gross =
            ((BigUint::one() << 128usize) - BigUint::one()) / BigUint::from(num_ticks as u32);
        for (i, t) in self.ticks.iter().enumerate() {
            if t.tick != expected[i] {
                return Err(err("SINGLETON_INCOMPLETE_TICKS"));
            }
            if t.liquidity_gross.0.is_zero() || t.liquidity_gross.0 > max_gross {
                return Err(err("SINGLETON_TICK_GROSS"));
            }
            let net = parse_liquidity_net(&t.liquidity_net)?;
            if BigUint::from(net.unsigned_abs()) > t.liquidity_gross.0 {
                return Err(err("SINGLETON_TICK_NET"));
            }
        }
        Ok(())
    }
}

fn bytes32(value: &str) -> Result<Vec<u8>> {
    if !value.starts_with("0x") || value.len() != 66 || value != value.to_ascii_lowercase() {
        return Err(err("SINGLETON_DIGEST"));
    }
    hex::decode(&value[2..]).map_err(|_| err("SINGLETON_DIGEST"))
}
pub fn pool_id(
    protocol: &str,
    manager: &str,
    token0: &str,
    token1: &str,
    fee: u32,
    spacing: i32,
    parameters: &str,
) -> Result<String> {
    for a in [manager, token0, token1] {
        crate::protocols::validate_address(a)?;
    }
    if manager == ZERO_ADDRESS
        || token0 == ZERO_ADDRESS
        || token0 >= token1
        || fee >= 1000000
        || !(1..32768).contains(&spacing)
    {
        return Err(err("SINGLETON_KEY_UNSUPPORTED"));
    }
    let address = |s: &str| -> Result<Vec<u8>> {
        let mut b = vec![0; 12];
        b.extend(hex::decode(&s[2..]).map_err(|_| err("SINGLETON_ADDRESS"))?);
        Ok(b)
    };
    let number = |n: u32| {
        let mut w = vec![0u8; 28];
        w.extend(n.to_be_bytes());
        w
    };
    let mut key = address(token0)?;
    key.extend(address(token1)?);
    match protocol {
        "uniswap-v4" => {
            if !parameters.is_empty() {
                return Err(err("SINGLETON_PARAMETERS"));
            }
            key.extend(number(fee));
            key.extend(number(spacing as u32));
            key.extend([0u8; 32]);
        }
        "pancake-infinity-cl" => {
            let expected = format!("0x{:064x}", (spacing as u64) << 16);
            if parameters != expected {
                return Err(err("SINGLETON_PARAMETERS"));
            }
            key.extend([0u8; 32]);
            key.extend(address(manager)?);
            key.extend(number(fee));
            key.extend(bytes32(parameters)?);
        }
        _ => return Err(err("SINGLETON_PROTOCOL_UNSUPPORTED")),
    }
    let mut hash = [0u8; 32];
    let mut keccak = Keccak::v256();
    keccak.update(&key);
    keccak.finalize(&mut hash);
    Ok(format!("0x{}", hex::encode(hash)))
}
impl SingletonCLState {
    fn validate_wire(&self) -> Result<()> {
        crate::protocols::validate_identity(&self.manager, &self.manager_code_sha256)?;
        bytes32(&self.pool_id)?;
        if !self.parameters.is_empty() {
            bytes32(&self.parameters)?;
        }
        if self.manager == ZERO_ADDRESS
            || self.hooks != ZERO_ADDRESS
            || self.lp_fee >= 1000000
            || self.protocol_fee0 > 4000
            || self.protocol_fee1 > 4000
        {
            return Err(err("SINGLETON_WIRE_IDENTITY"));
        }
        self.core.validate("pancake-infinity-cl")
    }
    pub fn validate(&self, protocol: &str, token0: &str, token1: &str) -> Result<()> {
        self.validate_wire()?;
        if pool_id(
            protocol,
            &self.manager,
            token0,
            token1,
            self.lp_fee,
            self.core.tick_spacing,
            &self.parameters,
        )? != self.pool_id
        {
            return Err(err("SINGLETON_POOL_ID"));
        }
        let limit = if protocol == "uniswap-v4" { 1000 } else { 4000 };
        if self.protocol_fee0 > limit || self.protocol_fee1 > limit {
            return Err(err("SINGLETON_PROTOCOL_FEE"));
        }
        self.core.validate(protocol)
    }
    pub fn to_proto(&self) -> Result<wire::SingletonClState> {
        self.validate_wire()?;
        Ok(wire::SingletonClState {
            manager: self.manager.clone(),
            manager_code_sha256: hex::decode(&self.manager_code_sha256)
                .map_err(|_| err("SINGLETON_DIGEST"))?,
            pool_id: bytes32(&self.pool_id)?,
            lp_fee: self.lp_fee,
            protocol_fee0: self.protocol_fee0,
            protocol_fee1: self.protocol_fee1,
            hooks: self.hooks.clone(),
            parameters: if self.parameters.is_empty() {
                Vec::new()
            } else {
                bytes32(&self.parameters)?
            },
            core: Some(self.core.to_proto()?),
        })
    }
    pub fn from_proto(s: wire::SingletonClState) -> Result<Self> {
        if s.manager_code_sha256.len() != 32
            || s.pool_id.len() != 32
            || (!s.parameters.is_empty() && s.parameters.len() != 32)
        {
            return Err(err("SINGLETON_WIRE_REQUIRED"));
        }
        let out = Self {
            manager: s.manager,
            manager_code_sha256: hex::encode(s.manager_code_sha256),
            pool_id: format!("0x{}", hex::encode(s.pool_id)),
            lp_fee: s.lp_fee,
            protocol_fee0: s.protocol_fee0,
            protocol_fee1: s.protocol_fee1,
            hooks: s.hooks,
            parameters: if s.parameters.is_empty() {
                String::new()
            } else {
                format!("0x{}", hex::encode(s.parameters))
            },
            core: ConcentratedState::from_proto(
                s.core.ok_or_else(|| err("SINGLETON_CORE_REQUIRED"))?,
            )?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl ConcentratedState {
    fn to_proto(&self) -> Result<wire::ConcentratedState> {
        self.validate("pancake-infinity-cl")?;
        let amount = |a: &Amount| -> Result<wire::Amount> {
            Ok(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            })
        };
        Ok(wire::ConcentratedState {
            sqrt_price_x96: Some(amount(&self.sqrt_price_x96)?),
            tick: self.tick,
            liquidity: Some(amount(&self.liquidity)?),
            tick_spacing: self.tick_spacing,
            max_steps: self.max_steps,
            tick_words: self
                .tick_words
                .iter()
                .map(|w| {
                    Ok(wire::V3BitmapWord {
                        word_pos: w.word_pos,
                        bitmap: Some(amount(&w.bitmap)?),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            ticks: self
                .ticks
                .iter()
                .map(|t| {
                    Ok(wire::V3Tick {
                        tick: t.tick,
                        liquidity_gross: Some(amount(&t.liquidity_gross)?),
                        liquidity_net_i128_be: parse_liquidity_net(&t.liquidity_net)?
                            .to_be_bytes()
                            .to_vec(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
    fn from_proto(c: wire::ConcentratedState) -> Result<Self> {
        let amount = |a: Option<wire::Amount>| {
            Amount::from_be_bytes(
                &a.ok_or_else(|| err("SINGLETON_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        };
        let out = Self {
            sqrt_price_x96: amount(c.sqrt_price_x96)?,
            tick: c.tick,
            liquidity: amount(c.liquidity)?,
            tick_spacing: c.tick_spacing,
            max_steps: c.max_steps,
            tick_words: c
                .tick_words
                .into_iter()
                .map(|w| {
                    Ok(V3BitmapWord {
                        word_pos: w.word_pos,
                        bitmap: amount(w.bitmap)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            ticks: c
                .ticks
                .into_iter()
                .map(|t| {
                    let b: [u8; 16] = t
                        .liquidity_net_i128_be
                        .try_into()
                        .map_err(|_| err("SINGLETON_NET_WIDTH"))?;
                    Ok(V3Tick {
                        tick: t.tick,
                        liquidity_gross: amount(t.liquidity_gross)?,
                        liquidity_net: i128::from_be_bytes(b).to_string(),
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        };
        out.validate("pancake-infinity-cl")?;
        Ok(out)
    }
}

/// Explicit capability boundary; legacy Pool/SingletonCLState admission is unchanged.
pub const DYNAMIC_FEE_FLAG: u32 = 0x800000;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SingletonAdvancedState {
    pub protocol: String,
    pub currency0: String,
    pub currency1: String,
    pub key_fee: u32,
    pub snapshot_block_hash: String,
    pub snapshot: SingletonCLState,
}
impl SingletonAdvancedState {
    pub fn key_id(&self) -> Result<String> {
        let s = &self.snapshot;
        for a in [&s.manager, &self.currency0, &self.currency1, &s.hooks] {
            crate::protocols::validate_address(a)?;
        }
        if s.manager == ZERO_ADDRESS
            || self.currency0 >= self.currency1
            || self.key_fee > 1_000_000 && self.key_fee != DYNAMIC_FEE_FLAG
            || !(1..32768).contains(&s.core.tick_spacing)
        {
            return Err(err("SINGLETON_ADVANCED_KEY"));
        }
        let address = |a: &str| -> Result<Vec<u8>> {
            let mut b = vec![0u8; 12];
            b.extend(hex::decode(&a[2..]).map_err(|_| err("SINGLETON_ADDRESS"))?);
            Ok(b)
        };
        let number = |n: u32| {
            let mut b = vec![0; 28];
            b.extend(n.to_be_bytes());
            b
        };
        let mut key = address(&self.currency0)?;
        key.extend(address(&self.currency1)?);
        match self.protocol.as_str() {
            "uniswap-v4" => {
                if !s.parameters.is_empty() {
                    return Err(err("SINGLETON_PARAMETERS"));
                }
                key.extend(number(self.key_fee));
                key.extend(number(s.core.tick_spacing as u32));
                key.extend(address(&s.hooks)?);
            }
            "pancake-infinity-cl" => {
                let params = bytes32(&s.parameters)?;
                let mut expected = [0u8; 32];
                expected[28..30].copy_from_slice(&(s.core.tick_spacing as u16).to_be_bytes());
                // The bounded positive spacing occupies the low bits of the pinned uint24 field.
                if params[..30] != expected[..30] || params[30] & 0xc0 != 0 {
                    return Err(err("SINGLETON_PARAMETERS"));
                }
                key.extend(address(&s.hooks)?);
                key.extend(address(&s.manager)?);
                key.extend(number(self.key_fee));
                key.extend(params);
            }
            _ => return Err(err("SINGLETON_PROTOCOL_UNSUPPORTED")),
        }
        let mut hash = [0; 32];
        let mut k = Keccak::v256();
        k.update(&key);
        k.finalize(&mut hash);
        Ok(format!("0x{}", hex::encode(hash)))
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_identity()?;
        self.snapshot.core.validate(&self.protocol)
    }
    pub fn validate_identity(&self) -> Result<()> {
        let s = &self.snapshot;
        crate::protocols::validate_identity(&s.manager, &s.manager_code_sha256)?;
        let hash = bytes32(&self.snapshot_block_hash)?;
        if hash.iter().all(|b| *b == 0) {
            return Err(err("SINGLETON_BLOCK_HASH_REQUIRED"));
        }
        if self.key_id()? != s.pool_id {
            return Err(err("SINGLETON_POOL_ID"));
        }
        if s.lp_fee > 1_000_000 || self.key_fee != DYNAMIC_FEE_FLAG && s.lp_fee != self.key_fee {
            return Err(err("SINGLETON_LP_FEE_MISMATCH"));
        }
        let limit = if self.protocol == "uniswap-v4" {
            1000
        } else {
            4000
        };
        if s.protocol_fee0 > limit || s.protocol_fee1 > limit {
            return Err(err("SINGLETON_PROTOCOL_FEE"));
        }
        let flags = if self.protocol == "uniswap-v4" {
            let b = hex::decode(&s.hooks[2..]).map_err(|_| err("SINGLETON_ADDRESS"))?;
            let flags = u16::from_be_bytes([b[18], b[19]]) & 0x3fff;
            if flags & 0xcc != 0 {
                return Err(err("SINGLETON_HOOK_EXECUTION_REQUIRED"));
            }
            if flags & 2 != 0 && flags & (1 << 10) == 0
                || flags & 1 != 0 && flags & (1 << 8) == 0
                || s.hooks != ZERO_ADDRESS && flags == 0 && self.key_fee != DYNAMIC_FEE_FLAG
            {
                return Err(err("SINGLETON_HOOK_PERMISSIONS"));
            }
            flags
        } else {
            let b = bytes32(&s.parameters)?;
            let flags = u16::from_be_bytes([b[30], b[31]]);
            if flags & 0xcc0 != 0 {
                return Err(err("SINGLETON_HOOK_EXECUTION_REQUIRED"));
            }
            if flags & (1 << 12) != 0 && flags & (1 << 3) == 0
                || flags & (1 << 13) != 0 && flags & (1 << 5) == 0
            {
                return Err(err("SINGLETON_HOOK_PERMISSIONS"));
            }
            flags
        };
        if s.hooks == ZERO_ADDRESS && (self.key_fee == DYNAMIC_FEE_FLAG || flags != 0) {
            return Err(err("SINGLETON_HOOK_PERMISSIONS"));
        }
        Ok(())
    }
}

impl SingletonAdvancedState {
    /// The advanced snapshot uses the same protobuf storage shape, with its own
    /// capability validation. Legacy SingletonCLState::to_proto stays strict.
    pub fn to_proto(&self) -> Result<wire::SingletonAdvancedState> {
        self.validate()?;
        let s = &self.snapshot;
        Ok(wire::SingletonAdvancedState {
            protocol: self.protocol.clone(),
            currency0: self.currency0.clone(),
            currency1: self.currency1.clone(),
            key_fee: self.key_fee,
            snapshot_block_hash: bytes32(&self.snapshot_block_hash)?,
            snapshot: Some(wire::SingletonClState {
                manager: s.manager.clone(),
                manager_code_sha256: hex::decode(&s.manager_code_sha256)
                    .map_err(|_| err("SINGLETON_DIGEST"))?,
                pool_id: bytes32(&s.pool_id)?,
                lp_fee: s.lp_fee,
                protocol_fee0: s.protocol_fee0,
                protocol_fee1: s.protocol_fee1,
                hooks: s.hooks.clone(),
                parameters: if s.parameters.is_empty() {
                    Vec::new()
                } else {
                    bytes32(&s.parameters)?
                },
                core: Some(s.core.to_proto()?),
            }),
        })
    }
    pub fn from_proto(value: wire::SingletonAdvancedState) -> Result<Self> {
        if value.snapshot_block_hash.len() != 32 {
            return Err(err("SINGLETON_BLOCK_HASH_REQUIRED"));
        }
        let s = value
            .snapshot
            .ok_or_else(|| err("SINGLETON_STATE_REQUIRED"))?;
        if s.manager_code_sha256.len() != 32
            || s.pool_id.len() != 32
            || (!s.parameters.is_empty() && s.parameters.len() != 32)
        {
            return Err(err("SINGLETON_WIRE_REQUIRED"));
        }
        let out = Self {
            protocol: value.protocol,
            currency0: value.currency0,
            currency1: value.currency1,
            key_fee: value.key_fee,
            snapshot_block_hash: format!("0x{}", hex::encode(value.snapshot_block_hash)),
            snapshot: SingletonCLState {
                manager: s.manager,
                manager_code_sha256: hex::encode(s.manager_code_sha256),
                pool_id: format!("0x{}", hex::encode(s.pool_id)),
                lp_fee: s.lp_fee,
                protocol_fee0: s.protocol_fee0,
                protocol_fee1: s.protocol_fee1,
                hooks: s.hooks,
                parameters: if s.parameters.is_empty() {
                    String::new()
                } else {
                    format!("0x{}", hex::encode(s.parameters))
                },
                core: ConcentratedState::from_proto(
                    s.core.ok_or_else(|| err("SINGLETON_CORE_REQUIRED"))?,
                )?,
            },
        };
        out.validate()?;
        Ok(out)
    }
}
