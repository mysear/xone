//! Meteora's retained 1532e540 SDK/SBF layout. This profile admits only
//! classic SPL, separately versioned input/OnlyY fees, MM bins without rewards or orders.
//! Original bytes are retained so untouched accounting fields remain observable.
use crate::{err, wire, Amount, Pool, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PROFILE: &str = "meteora-dlmm-mm";
pub const ORDERS_PROFILE: &str = "meteora-dlmm-orders";
pub const ORDERS_ADAPTER: &str = "meteora-dlmm/1532e540-orders-1";
pub const REWARDS_PROFILE: &str = "meteora-dlmm-mm-rewards";
pub const REWARDS_ADAPTER: &str = "meteora-dlmm/1532e540-mm-rewards-1";
pub const BITMAP_PROFILE: &str = "meteora-dlmm-mm-bitmap";
pub const BITMAP_ADAPTER: &str = "meteora-dlmm/1532e540-mm-bitmap-1";
pub const BITMAP_SIZE: usize = 1576;
pub const ONLY_Y_PROFILE: &str = "meteora-dlmm-mm-only-y";
pub const ONLY_Y_ADAPTER: &str = "meteora-dlmm/1532e540-mm-only-y-1";
pub fn supports_profile(profile: &str) -> bool {
    matches!(
        profile,
        PROFILE
            | ONLY_Y_PROFILE
            | BITMAP_PROFILE
            | REWARDS_PROFILE
            | ORDERS_PROFILE
            | crate::meteora_token2022::PROFILE
    )
}
pub fn adapter(profile: &str) -> Result<&'static str> {
    match profile {
        PROFILE => Ok(ADAPTER),
        BITMAP_PROFILE => Ok(BITMAP_ADAPTER),
        REWARDS_PROFILE => Ok(REWARDS_ADAPTER),
        ORDERS_PROFILE => Ok(ORDERS_ADAPTER),
        crate::meteora_token2022::PROFILE => Ok(crate::meteora_token2022::ADAPTER),
        ONLY_Y_PROFILE => Ok(ONLY_Y_ADAPTER),
        _ => Err(err("METEORA_PROFILE")),
    }
}
pub const ADAPTER: &str = "meteora-dlmm/1532e540-mm-1";
pub const PROGRAM: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";
pub const CODE: &str = "942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6";
/// Classic SPL executable used by the pinned original-program reference.
pub const TOKEN_CODE: &str = "53e587eee80c786bcfba1450397d92179a0f01def776d19fb4ddf690d64431cc";
pub const PAIR_SIZE: usize = 904;
pub const ARRAY_SIZE: usize = 10136;
pub const BIN_SIZE: usize = 144;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BinArray {
    pub address: String,
    pub index: i64,
    pub data: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BitmapExtension {
    pub address: String,
    pub data: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MeteoraState {
    pub programdata: String,
    pub program_code_sha256: String,
    pub deployed_slot: u64,
    pub token_program_code_sha256: String,
    pub slot: u64,
    pub timestamp: u64,
    pub pair_data: String,
    pub oracle_data: String,
    pub bin_arrays: Vec<BinArray>,
    pub raw_vault0: u64,
    pub raw_vault1: u64,
    pub max_steps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitmap_extension: Option<BitmapExtension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token2022: Option<crate::meteora_token2022::Token2022State>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub derived: bool,
}
pub fn bytes(value: &str, size: usize) -> Result<Vec<u8>> {
    if value.len() != size * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(err("METEORA_ACCOUNT_ENCODING"));
    }
    hex::decode(value).map_err(|_| err("METEORA_ACCOUNT_ENCODING"))
}
pub fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(d[o..o + 2].try_into().unwrap())
}
pub fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
pub fn i32_at(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
pub fn u64_at(d: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(d[o..o + 8].try_into().unwrap())
}
pub fn i64_at(d: &[u8], o: usize) -> i64 {
    i64::from_le_bytes(d[o..o + 8].try_into().unwrap())
}
pub fn u128_at(d: &[u8], o: usize) -> u128 {
    u128::from_le_bytes(d[o..o + 16].try_into().unwrap())
}
pub fn key(d: &[u8], o: usize) -> String {
    bs58::encode(&d[o..o + 32]).into_string()
}
pub fn discriminator(name: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    Sha256::digest(format!("account:{name}").as_bytes())[..8]
        .try_into()
        .unwrap()
}
pub fn pubkey(s: &str) -> Result<[u8; 32]> {
    crate::raydium::validate_pubkey(s)?;
    bs58::decode(s)
        .into_vec()
        .map_err(|_| err("METEORA_IDENTITY"))?
        .try_into()
        .map_err(|_| err("METEORA_IDENTITY"))
}
pub fn pda(seeds: &[&[u8]], program: &str) -> Result<String> {
    use sha2::{Digest, Sha256};
    let program = pubkey(program)?;
    for bump in (0..=255u8).rev() {
        let mut hash = Sha256::new();
        for seed in seeds {
            hash.update(seed);
        }
        hash.update([bump]);
        hash.update(program);
        hash.update(b"ProgramDerivedAddress");
        let raw: [u8; 32] = hash.finalize().into();
        if curve25519_dalek::edwards::CompressedEdwardsY(raw)
            .decompress()
            .is_none()
        {
            return Ok(bs58::encode(raw).into_string());
        }
    }
    Err(err("METEORA_PDA"))
}
impl MeteoraState {
    pub fn pair(&self) -> Result<Vec<u8>> {
        bytes(&self.pair_data, PAIR_SIZE)
    }
    pub fn validate(&self, pool: &Pool) -> Result<()> {
        if !supports_profile(&pool.protocol)
            || pool.adapter_version != adapter(&pool.protocol)?
            || !pool.chain_key.starts_with("solana:")
            || pool.token_behavior != "standard"
            || pool.fee_numerator != 0
            || pool.fee_denominator != 1_000_000_000
        {
            return Err(err("METEORA_PROFILE"));
        }
        let pool_key = pubkey(&pool.pool_id)?;
        for key in [&pool.token0, &pool.token1, &self.programdata] {
            pubkey(key)?;
        }
        if self.program_code_sha256 != CODE
            || self.deployed_slot == 0
            || self.deployed_slot > self.slot
            || self.timestamp > i64::MAX as u64
            || self.token_program_code_sha256 != TOKEN_CODE
            || self.token_program_code_sha256.len() != 64
            || hex::decode(&self.token_program_code_sha256).is_err()
            || !(1..=4096).contains(&self.max_steps)
        {
            return Err(err("METEORA_VERSION"));
        }
        let loader = "BPFLoaderUpgradeab1e11111111111111111111111";
        if pda(&[&pubkey(PROGRAM)?], loader)? != self.programdata {
            return Err(err("METEORA_PROGRAMDATA"));
        }
        if (pool.protocol == BITMAP_PROFILE) != self.bitmap_extension.is_some() {
            return Err(err("METEORA_BITMAP_PROFILE"));
        }
        if let Some(ext) = &self.bitmap_extension {
            let raw = bytes(&ext.data, BITMAP_SIZE)?;
            if ext.address != pda(&[b"bitmap", &pool_key], PROGRAM)?
                || raw[..8] != discriminator("BinArrayBitmapExtension")
                || key(&raw, 8) != pool.pool_id
            {
                return Err(err("METEORA_BITMAP_EXTENSION_IDENTITY"));
            }
        }
        let d = self.pair()?;
        if d[..8] != discriminator("LbPair")
            || d[75] != 0
            || d[82] != 0
            || d[35]
                != if pool.protocol == ORDERS_PROFILE {
                    2
                } else {
                    1
                }
            || d[36] != u8::from(pool.protocol == ONLY_Y_PROFILE)
            || d[880..883]
                != [
                    u8::from(pool.protocol == crate::meteora_token2022::PROFILE),
                    0,
                    0,
                ]
            || (pool.protocol != REWARDS_PROFILE && d[264..552].iter().any(|b| *b != 0))
        {
            return Err(err("METEORA_POLICY_UNSUPPORTED"));
        }
        if (pool.protocol == crate::meteora_token2022::PROFILE) != self.token2022.is_some() {
            return Err(err("METEORA_TOKEN2022_PROFILE"));
        }
        if let Some(token) = &self.token2022 {
            token.validate(self)?;
            let vault = bytes(&token.vault_data, 178)?;
            let mint = bytes(&token.mint_data, 278)?;
            if key(&vault, 0) != pool.token0
                || key(&vault, 32) != pool.pool_id
                || vault[108] != 1
                || u32_at(&vault, 72) != 0
                || u32_at(&vault, 109) != 0
                || u64_at(&vault, 121) != 0
                || u32_at(&vault, 129) != 0
                || u32_at(&mint, 0) > 1
                || u32_at(&mint, 46) > 1
            {
                return Err(err("METEORA_TOKEN2022_ACCOUNT"));
            }
        }
        if pool.protocol == REWARDS_PROFILE {
            for i in 0..2 {
                let o = 264 + i * 144;
                if d[o..o + 32].iter().all(|b| *b == 0) {
                    if d[o..o + 144].iter().any(|b| *b != 0) {
                        return Err(err("METEORA_REWARD_INACTIVE"));
                    }
                } else {
                    let mint = key(&d, o);
                    let vault = key(&d, o + 32);
                    let funder = key(&d, o + 64);
                    pubkey(&mint)?;
                    pubkey(&funder)?;
                    if vault != pda(&[&pool_key, &(i as u64).to_le_bytes()], PROGRAM)?
                        || u64_at(&d, o + 96) == 0
                        || u64_at(&d, o + 104) > i64::MAX as u64
                        || u64_at(&d, o + 128) > self.timestamp
                        || u64_at(&d, o + 128) > u64_at(&d, o + 104)
                    {
                        return Err(err("METEORA_REWARD_POLICY"));
                    }
                }
            }
        }
        if key(&d, 88) != pool.token0 || key(&d, 120) != pool.token1 || pool.token0 == pool.token1 {
            return Err(err("METEORA_TOKEN_IDENTITY"));
        }
        let token0 = pubkey(&pool.token0)?;
        let token1 = pubkey(&pool.token1)?;
        let (low, high) = if token0 < token1 {
            (token0, token1)
        } else {
            (token1, token0)
        };
        if d[83] != 1
            || d[73..75] != d[80..82]
            || pda(&[&low, &high, &d[73..75], &d[84..86]], PROGRAM)? != pool.pool_id
            || pda(&[&pool_key, &token0], PROGRAM)? != key(&d, 152)
            || pda(&[&pool_key, &token1], PROGRAM)? != key(&d, 184)
            || pda(&[b"oracle", &pool_key], PROGRAM)? != key(&d, 552)
        {
            return Err(err("METEORA_POOL_PDA"));
        }
        {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            for s in [&low[..], &high[..], &d[73..75], &d[84..86], &d[72..73]] {
                h.update(s);
            }
            h.update(pubkey(PROGRAM)?);
            h.update(b"ProgramDerivedAddress");
            if h.finalize()[..] != pool_key {
                return Err(err("METEORA_POOL_BUMP"));
            }
        }
        let identities = [
            pool.pool_id.clone(),
            pool.token0.clone(),
            pool.token1.clone(),
            key(&d, 152),
            key(&d, 184),
            key(&d, 552),
            PROGRAM.into(),
            self.programdata.clone(),
        ];
        for id in &identities {
            pubkey(id)?;
        }
        if identities.iter().collect::<BTreeSet<_>>().len() != identities.len() {
            return Err(err("METEORA_IDENTITY_ALIAS"));
        }
        let step = u16_at(&d, 80);
        if step == 0
            || step > 400
            || u16_at(&d, 10) > u16_at(&d, 12)
            || u16_at(&d, 14) > 10000
            || u16_at(&d, 32) > 2500
            || d[34] > 8
            || u32_at(&d, 20) > 10_000_000
            || i32_at(&d, 24) > i32_at(&d, 28)
            || i32_at(&d, 76) < i32_at(&d, 24)
            || i32_at(&d, 76) > i32_at(&d, 28)
            || !(-443636..=443636).contains(&i32_at(&d, 76))
            || i64_at(&d, 56) < 0
            || i64_at(&d, 56) as u64 > self.timestamp
            || u32_at(&d, 40) > u32_at(&d, 20)
            || u32_at(&d, 44) > u32_at(&d, 20)
        {
            return Err(err("METEORA_PARAMETERS"));
        }
        if pool.reserve0 != Amount((self.raw_vault0).into())
            || pool.reserve1 != Amount((self.raw_vault1).into())
            || u64_at(&d, 216) > self.raw_vault0
            || u64_at(&d, 224) > self.raw_vault1
        {
            return Err(err("METEORA_VAULT_ACCOUNTING"));
        }
        if self.bin_arrays.is_empty() || self.bin_arrays.len() > 32 {
            return Err(err("METEORA_ARRAY_SCOPE"));
        }
        let mut seen = BTreeSet::new();
        let mut previous = None;
        let mut sums = [0u128; 2];
        for array in &self.bin_arrays {
            if !(if self.bitmap_extension.is_some() {
                -6656..=6655
            } else {
                -512..=511
            })
            .contains(&array.index)
                || previous.is_some_and(|i| i >= array.index)
                || !seen.insert(array.address.clone())
                || identities.contains(&array.address)
                || pda(
                    &[b"bin_array", &pool_key, &array.index.to_le_bytes()],
                    PROGRAM,
                )? != array.address
            {
                return Err(err("METEORA_ARRAY_IDENTITY"));
            }
            previous = Some(array.index);
            let raw = bytes(&array.data, ARRAY_SIZE)?;
            if raw[..8] != discriminator("BinArray")
                || i64_at(&raw, 8) != array.index
                || raw[16] != 1
                || key(&raw, 24) != pool.pool_id
            {
                return Err(err("METEORA_ARRAY_LAYOUT"));
            }
            let initialized = self.array_initialized(&d, array.index)?;
            if !initialized
                && raw[56..].chunks_exact(BIN_SIZE).any(|b| {
                    u64_at(b, 0) > 0
                        || u64_at(b, 8) > 0
                        || (pool.protocol == ORDERS_PROFILE
                            && (u64_at(b, 112) > 0 || u64_at(b, 128) > 0))
                })
            {
                return Err(err("METEORA_BITMAP_ACCOUNTING"));
            }
            for j in 0..70 {
                let o = 56 + j * BIN_SIZE;
                if (!matches!(pool.protocol.as_str(), REWARDS_PROFILE | ORDERS_PROFILE)
                    && raw[o + 48..o + 80].iter().any(|b| *b != 0))
                    || (pool.protocol != ORDERS_PROFILE
                        && raw[o + 112..o + 144].iter().any(|b| *b != 0))
                {
                    return Err(err("METEORA_LIMIT_ORDER_UNSUPPORTED"));
                }
                if pool.protocol == ORDERS_PROFILE {
                    if raw[o + 140] > 1 || raw[o + 141..o + 144].iter().any(|b| *b != 0) {
                        return Err(err("METEORA_ORDER_LAYOUT"));
                    }
                    let order =
                        u128::from(u64_at(&raw, o + 112)) + u128::from(u64_at(&raw, o + 128));
                    sums[usize::from(raw[o + 140] == 0)] += order;
                }
                let x = u64_at(&raw, o);
                let y = u64_at(&raw, o + 8);
                let supply = u128_at(&raw, o + 32);
                let id = (array.index * 70 + j as i64) as i32;
                let cached = u128_at(&raw, o + 16);
                if pool.protocol == ORDERS_PROFILE
                    && (u64_at(&raw, o + 112) > 0 || u64_at(&raw, o + 128) > 0)
                    && cached == 0
                {
                    return Err(err("METEORA_CACHED_PRICE"));
                }
                if ((x > 0 || y > 0) && cached == 0)
                    || (cached != 0 && cached != bin_price(id, step)?)
                {
                    return Err(err("METEORA_CACHED_PRICE"));
                }
                if (x > 0 || y > 0) && (id < i32_at(&d, 24) || id > i32_at(&d, 28)) {
                    return Err(err("METEORA_BIN_RANGE"));
                }
                if (x > 0 || y > 0) && supply == 0 {
                    return Err(err("METEORA_BIN_SUPPLY"));
                }
                sums[0] += u128::from(x);
                sums[1] += u128::from(y);
            }
        }
        if sums[0] + u128::from(u64_at(&d, 216)) > u128::from(self.raw_vault0)
            || sums[1] + u128::from(u64_at(&d, 224)) > u128::from(self.raw_vault1)
        {
            return Err(err("METEORA_VAULT_ACCOUNTING"));
        }
        // This first profile requires the active array and admits only in-bitmap arrays.
        if !self
            .bin_arrays
            .iter()
            .any(|a| a.index == i64::from(i32_at(&d, 76).div_euclid(70)))
        {
            return Err(err("METEORA_ACTIVE_ARRAY_MISSING"));
        }
        let size = self.oracle_data.len() / 2;
        if !(64..=32800).contains(&size) || !(size - 32).is_multiple_of(32) {
            return Err(err("METEORA_ORACLE_LAYOUT"));
        }
        let oracle = bytes(&self.oracle_data, size)?;
        let len = u64_at(&oracle, 24);
        let active = u64_at(&oracle, 16);
        let index = u64_at(&oracle, 8);
        if oracle[..8] != discriminator("Oracle")
            || len == 0
            || len as usize != (size - 32) / 32
            || active == 0
            || active > len
            || index >= active
        {
            return Err(err("METEORA_ORACLE_LAYOUT"));
        }
        if oracle[32 + active as usize * 32..].iter().any(|b| *b != 0) {
            return Err(err("METEORA_ORACLE_ACTIVE_SET"));
        }
        for i in 0..active as usize {
            let o = 32 + i * 32;
            let created = i64_at(&oracle, o + 16);
            let updated = i64_at(&oracle, o + 24);
            if created < 0 || updated < created || updated as u64 > self.timestamp {
                return Err(err("METEORA_ORACLE_CLOCK"));
            }
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::MeteoraState> {
        Ok(wire::MeteoraState {
            programdata: self.programdata.clone(),
            program_code_sha256: hex::decode(&self.program_code_sha256)
                .map_err(|_| err("METEORA_VERSION"))?,
            deployed_slot: self.deployed_slot,
            token_program_code_sha256: hex::decode(&self.token_program_code_sha256)
                .map_err(|_| err("METEORA_VERSION"))?,
            slot: self.slot,
            timestamp: self.timestamp,
            pair_data: bytes(&self.pair_data, PAIR_SIZE)?,
            oracle_data: bytes(&self.oracle_data, self.oracle_data.len() / 2)?,
            bin_arrays: self
                .bin_arrays
                .iter()
                .map(|a| {
                    Ok(wire::MeteoraBinArray {
                        address: a.address.clone(),
                        index: a.index,
                        data: bytes(&a.data, ARRAY_SIZE)?,
                    })
                })
                .collect::<Result<_>>()?,
            raw_vault0: self.raw_vault0,
            raw_vault1: self.raw_vault1,
            max_steps: self.max_steps,
            derived: self.derived,
            token2022: self.token2022.as_ref().map(|s| s.to_proto()).transpose()?,
            bitmap_extension: self
                .bitmap_extension
                .as_ref()
                .map(|e| {
                    Ok(wire::MeteoraBitmapExtension {
                        address: e.address.clone(),
                        data: bytes(&e.data, BITMAP_SIZE)?,
                    })
                })
                .transpose()?,
        })
    }
    pub fn from_proto(s: wire::MeteoraState) -> Result<Self> {
        Ok(Self {
            programdata: s.programdata,
            program_code_sha256: hex::encode(s.program_code_sha256),
            deployed_slot: s.deployed_slot,
            token_program_code_sha256: hex::encode(s.token_program_code_sha256),
            slot: s.slot,
            timestamp: s.timestamp,
            pair_data: hex::encode(s.pair_data),
            oracle_data: hex::encode(s.oracle_data),
            bin_arrays: s
                .bin_arrays
                .into_iter()
                .map(|a| BinArray {
                    address: a.address,
                    index: a.index,
                    data: hex::encode(a.data),
                })
                .collect(),
            raw_vault0: s.raw_vault0,
            raw_vault1: s.raw_vault1,
            max_steps: s.max_steps,
            derived: s.derived,
            token2022: s
                .token2022
                .map(crate::meteora_token2022::Token2022State::from_proto),
            bitmap_extension: s.bitmap_extension.map(|e| BitmapExtension {
                address: e.address,
                data: hex::encode(e.data),
            }),
        })
    }
}

impl MeteoraState {
    /// Case-sensitive Solana identities; this set cannot be used as request funds.
    pub fn array_initialized(&self, pair: &[u8], index: i64) -> Result<bool> {
        if (-512..=511).contains(&index) {
            let bit = (index + 512) as usize;
            return Ok(u64_at(pair, 584 + bit / 64 * 8) & (1u64 << (bit % 64)) != 0);
        }
        if !(-6656..=6655).contains(&index) {
            return Err(err("METEORA_BITMAP_RANGE"));
        }
        let extension = self
            .bitmap_extension
            .as_ref()
            .ok_or_else(|| err("METEORA_BITMAP_EXTENSION_REQUIRED"))?;
        let raw = bytes(&extension.data, BITMAP_SIZE)?;
        let (side, bit) = if index >= 512 {
            (0usize, (index - 512) as usize)
        } else {
            (1usize, (-index - 513) as usize)
        };
        Ok(u64_at(&raw, 40 + side * 768 + bit / 64 * 8) & (1u64 << (bit % 64)) != 0)
    }
    pub fn internal_accounts(&self, pool: &Pool) -> Result<Vec<String>> {
        let d = self.pair()?;
        let mut result = vec![
            pool.pool_id.clone(),
            self.programdata.clone(),
            PROGRAM.into(),
            key(&d, 88),
            key(&d, 120),
            key(&d, 152),
            key(&d, 184),
            key(&d, 552),
        ];
        result.extend(self.bin_arrays.iter().map(|a| a.address.clone()));
        result.extend(self.bitmap_extension.iter().map(|e| e.address.clone()));
        if let Some(t) = &self.token2022 {
            result.extend([
                crate::meteora_token2022::PROGRAM.into(),
                t.programdata.clone(),
            ]);
        }
        if pool.protocol == REWARDS_PROFILE {
            for i in 0..2 {
                let o = 264 + i * 144;
                if d[o..o + 32].iter().any(|b| *b != 0) {
                    result.extend([key(&d, o), key(&d, o + 32)]);
                }
            }
        }
        Ok(result)
    }
}
/// Mutable account reuse across physical pools needs an explicit combined ledger.
/// Reject it instead of treating those pools as independent allocation capacity.
pub fn validate_dependencies<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let rows = pools.into_iter().collect::<Vec<_>>();
    if !rows.iter().any(|p| p.meteora.is_some()) {
        return Ok(());
    }
    let mut owners = std::collections::BTreeMap::new();
    let mut token_identity = None;
    for p in rows {
        let mut mutable = vec![p.pool_id.clone()];
        if let Some(m) = &p.meteora {
            let d = m.pair()?;
            mutable.extend([key(&d, 152), key(&d, 184), key(&d, 552)]);
            mutable.extend(m.bin_arrays.iter().map(|a| a.address.clone()));
            let identity = (&m.token_program_code_sha256, m.deployed_slot);
            if token_identity.is_some_and(|old| old != identity) {
                return Err(err("METEORA_SHARED_PROGRAM_VERSION"));
            }
            token_identity = Some(identity);
        }
        if let Some(m) = &p.orca_whirlpool {
            mutable.extend([m.vault0.clone(), m.vault1.clone(), m.oracle.clone()]);
        }
        if let Some(m) = &p.raydium_cpmm {
            mutable.extend([m.vault0.clone(), m.vault1.clone()]);
        }
        if let Some(m) = &p.raydium_clmm {
            mutable.extend([m.vault0.clone(), m.vault1.clone(), m.observation.clone()]);
        }
        for account in mutable {
            if owners
                .insert(account, &p.pool_id)
                .is_some_and(|owner| owner != &p.pool_id)
            {
                return Err(err("METEORA_SHARED_MUTABLE_ACCOUNT"));
            }
        }
    }
    Ok(())
}

pub fn bin_price(id: i32, step: u16) -> Result<u128> {
    if id == 0 {
        return Ok(1u128 << 64);
    }
    if id.unsigned_abs() >= 0x80000 {
        return Err(err("METEORA_PRICE"));
    }
    let base = (1u128 << 64) + u128::from(step) * (1u128 << 64) / 10000;
    let mut squared = u128::MAX / base;
    let mut result = 1u128 << 64;
    for bit in 0..19 {
        if id.unsigned_abs() & (1 << bit) != 0 {
            result = result
                .checked_mul(squared)
                .ok_or_else(|| err("METEORA_PRICE"))?
                >> 64;
        }
        if bit != 18 {
            squared = squared
                .checked_mul(squared)
                .ok_or_else(|| err("METEORA_PRICE"))?
                >> 64;
        }
    }
    if result == 0 {
        return Err(err("METEORA_PRICE"));
    }
    if id > 0 {
        Ok(u128::MAX / result)
    } else {
        Ok(result)
    }
}
