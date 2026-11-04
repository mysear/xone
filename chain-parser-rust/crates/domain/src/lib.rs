use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt, str::FromStr};

/// Validate dependencies that can be mutated across physical pool identities.
pub fn validate_pool_dependencies<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let rows = pools.into_iter().collect::<Vec<_>>();
    managed_v2_dependencies::validate(rows.iter().copied())?;
    curve_meta_state::validate_meta_dependencies(rows.iter().copied())?;
    curve_crypto_state::validate_crypto_dependencies(rows.iter().copied())?;
    infinity_bin::validate_dependencies(rows.iter().copied())
}

pub mod curve_crypto_state;
pub mod curve_meta_state;
pub mod curve_ng_state;
pub mod facts;
pub mod infinity_bin;
pub mod managed_fee_state;
/// Known unavailable pool IDs for candidate selection; the source bank stays complete.
pub fn unavailable_pool_reasons<'a>(
    pools: impl IntoIterator<Item = &'a Pool>,
) -> Result<std::collections::BTreeMap<String, String>> {
    managed_v2_dependencies::blocked_pools(pools)
}

pub mod managed_v2_dependencies;
pub mod managed_v2_state;
pub mod multi_asset;
pub mod orca;
pub mod orca_adaptive_state;
pub mod orca_math;
pub mod protocols;
pub mod rayclmm_math;
pub mod raydium;
pub mod raydium_clmm;
pub mod singleton;
mod singleton_pool;
pub mod stable;
pub mod v2_swap;
pub mod v3;
pub mod weighted;
pub use xone_proto as wire;
pub const SCHEMA_VERSION: u32 = 1;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct Error(pub String);
pub fn err(code: &str) -> Error {
    Error(code.to_owned())
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Amount(pub BigUint);
impl FromStr for Amount {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        if s.is_empty()
            || s.len() > 78
            || !s.bytes().all(|v| v.is_ascii_digit())
            || (s.len() > 1 && s.starts_with('0'))
        {
            return Err(err("INVALID_AMOUNT"));
        }
        let n = BigUint::from_str(s).map_err(|_| err("INVALID_AMOUNT"))?;
        if n.bits() > 256 {
            return Err(err("AMOUNT_OVERFLOW"));
        }
        Ok(Self(n))
    }
}
impl Amount {
    pub fn from_be_bytes(b: &[u8]) -> Result<Self> {
        if b.len() != 32 {
            return Err(err("UINT256_REQUIRES_32_BE_BYTES"));
        }
        Ok(Self(BigUint::from_bytes_be(b)))
    }
    pub fn to_be_bytes(&self) -> Result<[u8; 32]> {
        if self.0.bits() > 256 {
            return Err(err("AMOUNT_OVERFLOW"));
        }
        let mut b = [0; 32];
        let v = self.0.to_bytes_be();
        b[32 - v.len()..].copy_from_slice(&v);
        Ok(b)
    }
    pub fn checked(n: BigUint) -> Result<Self> {
        if n.bits() > 256 {
            Err(err("AMOUNT_OVERFLOW"))
        } else {
            Ok(Self(n))
        }
    }
    pub fn zero() -> Self {
        Self(BigUint::zero())
    }
}
impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        if self.0.bits() > 256 {
            return Err(serde::ser::Error::custom("AMOUNT_OVERFLOW"));
        }
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
pub fn hash<T: Serialize>(v: &T) -> Result<String> {
    let value = serde_json::to_value(v).map_err(|_| err("HASH_ENCODING"))?;
    let b = serde_json::to_vec(&value).map_err(|_| err("HASH_ENCODING"))?;
    let canonical = String::from_utf8(b)
        .map_err(|_| err("HASH_ENCODING"))?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}
pub fn validate_chain(s: &str) -> Result<()> {
    let (family, id) = s.split_once(':').ok_or_else(|| err("INVALID_CHAIN_KEY"))?;
    let good = match family {
        "evm" => !id.starts_with('0') && !id.is_empty() && id.bytes().all(|v| v.is_ascii_digit()),
        "solana" => {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || v == b'-' || v == b'_')
        }
        _ => false,
    };
    if good {
        Ok(())
    } else {
        Err(err("INVALID_CHAIN_KEY"))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlockRef {
    pub chain_key: String,
    pub number_or_slot: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRef {
    pub block: BlockRef,
    pub canonical_epoch: u64,
    pub journal_seq: u64,
    pub schema_version: u32,
    pub adapter_set_hash: String,
    pub online_generation: u64,
    pub scope_hash: String,
    pub writer_epoch: u64,
    pub owner_service: String,
    pub owner_assignment_version: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pool {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infinity_bin: Option<infinity_bin::InfinityBinState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singleton_advanced: Option<singleton::SingletonAdvancedState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_asset: Option<multi_asset::MultiAssetState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted: Option<weighted::WeightedState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raydium_clmm: Option<raydium_clmm::RaydiumCLMMState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orca_whirlpool: Option<orca::OrcaWhirlpoolState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raydium_cpmm: Option<raydium::RaydiumCPMMState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub singleton_cl: Option<singleton::SingletonCLState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable: Option<stable::StableState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v2: Option<protocols::V2State>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v3: Option<v3::V3State>,
    pub chain_key: String,
    pub pool_id: String,
    pub protocol: String,
    pub token0: String,
    pub token1: String,
    pub reserve0: Amount,
    pub reserve1: Amount,
    pub fee_numerator: u64,
    pub fee_denominator: u64,
    pub adapter_version: String,
    pub token_behavior: String,
}
impl Pool {
    pub fn validate(&self) -> Result<()> {
        self.validate_v2_complete()?;
        if [
            self.singleton_advanced.is_some(),
            self.multi_asset.is_some(),
            self.weighted.is_some(),
            self.stable.is_some(),
            self.v2.is_some(),
            self.v3.is_some(),
            self.singleton_cl.is_some(),
            self.raydium_cpmm.is_some(),
            self.infinity_bin.is_some(),
            self.orca_whirlpool.is_some(),
            self.raydium_clmm.is_some(),
        ]
        .into_iter()
        .filter(|v| *v)
        .count()
            > 1
        {
            return Err(err("POOL_MODEL_AMBIGUOUS"));
        }

        if protocols::protocol(&self.protocol).is_some_and(|p| p.family == "singleton-advanced") {
            return self.validate_singleton_advanced();
        }
        if self.singleton_advanced.is_some() {
            return Err(err("SINGLETON_ADVANCED_PROTOCOL_MISMATCH"));
        }
        if crate::protocols::protocol(&self.protocol).is_some_and(|p| p.family == "multi") {
            return self.validate_multi_asset();
        }
        if self.multi_asset.is_some() {
            return Err(err("MULTI_ASSET_PROTOCOL_MISMATCH"));
        }
        if self.protocol == "balancer-v2-weighted2-equal"
            || self.protocol == "balancer-v2-weighted2-static"
        {
            return self.validate_weighted();
        }
        if self.weighted.is_some() {
            return Err(err("WEIGHTED_PROTOCOL_MISMATCH"));
        }
        if self.protocol == infinity_bin::PROFILE {
            return self
                .infinity_bin
                .as_ref()
                .ok_or_else(|| err("INFINITY_BIN_STATE_REQUIRED"))?
                .validate(self);
        }
        if self.infinity_bin.is_some() {
            return Err(err("INFINITY_BIN_PROFILE_MISMATCH"));
        }
        if self.protocol == "raydium-clmm-rewards"
            || self.protocol == "raydium-clmm-static"
            || self.protocol == "raydium-clmm-token-fee"
            || self.protocol == "raydium-clmm-bitmap"
            || self.protocol == "raydium-clmm-custom-seed"
            || self.protocol == "raydium-clmm-dynamic"
        {
            return self
                .raydium_clmm
                .as_ref()
                .ok_or_else(|| err("RAYCLMM_STATE_REQUIRED"))?
                .validate(self);
        }
        if self.raydium_clmm.is_some() {
            return Err(err("RAYCLMM_PROTOCOL_MISMATCH"));
        }
        if matches!(
            self.protocol.as_str(),
            "orca-whirlpools-static"
                | "orca-whirlpools-rewards"
                | "orca-whirlpools-adaptive"
                | "orca-whirlpools-adaptive-rewards"
                | "orca-whirlpools-adaptive-dynamic"
        ) {
            return self
                .orca_whirlpool
                .as_ref()
                .ok_or_else(|| err("ORCA_STATE_REQUIRED"))?
                .validate(self);
        }
        if self.orca_whirlpool.is_some() {
            return Err(err("ORCA_PROTOCOL_MISMATCH"));
        }
        if self.protocol == "raydium-cpmm-creator" {
            return self
                .raydium_cpmm
                .as_ref()
                .ok_or_else(|| err("RAYDIUM_STATE_REQUIRED"))?
                .validate(self);
        }
        if self.raydium_cpmm.is_some() {
            return Err(err("RAYDIUM_PROTOCOL_MISMATCH"));
        }

        if let Some(profile) = protocols::protocol(&self.protocol) {
            if profile.family != "singleton"
                && (profile.require_identity
                    || self.v2.is_some()
                    || self.pool_id.to_ascii_lowercase().starts_with("0x"))
            {
                protocols::validate_address(&self.pool_id)?;
            }
            if !self.chain_key.starts_with("evm:") {
                return Err(err("PROTOCOL_CHAIN_FAMILY_MISMATCH"));
            }
            if self.adapter_version != profile.adapter {
                return Err(err("ADAPTER_VERSION_MISMATCH"));
            }
            if profile.family == "singleton" {
                let s = self
                    .singleton_cl
                    .as_ref()
                    .ok_or_else(|| err("SINGLETON_STATE_REQUIRED"))?;
                if self.v2.is_some()
                    || self.v3.is_some()
                    || self.stable.is_some()
                    || self.reserve0 != Amount::zero()
                    || self.reserve1 != Amount::zero()
                    || self.fee_denominator != 1000000
                    || self.fee_numerator != u64::from(s.lp_fee)
                {
                    return Err(err("SINGLETON_STATE_REQUIRED"));
                }
                s.validate(&self.protocol, &self.token0, &self.token1)?;
                if self.pool_id != format!("{}/{}", s.manager, s.pool_id) {
                    return Err(err("SINGLETON_PHYSICAL_ID"));
                }
            } else if self.singleton_cl.is_some() {
                return Err(err("SINGLETON_PROTOCOL_MISMATCH"));
            } else if ["aero", "curve", "balancer"].contains(&profile.family) {
                if self.v2.is_some()
                    || self.v3.is_some()
                    || self.fee_denominator != profile.fee_denominator
                {
                    return Err(err("STABLE_STATE_REQUIRED"));
                }
                self.stable
                    .as_ref()
                    .ok_or_else(|| err("STABLE_STATE_REQUIRED"))?
                    .validate(profile.family)?;
                if profile.family == "curve" && !(4000000..=100000000).contains(&self.fee_numerator)
                {
                    return Err(err("CURVE_FEE_RANGE"));
                }
                if profile.family == "balancer"
                    && !(1000000000000..=100000000000000000).contains(&self.fee_numerator)
                {
                    return Err(err("BALANCER_FEE_RANGE"));
                }
                if profile.family == "aero" && self.fee_numerator > 300 {
                    return Err(err("AERO_FEE_RANGE"));
                }
            } else if self.stable.is_some() {
                return Err(err("STABLE_PROTOCOL_MISMATCH"));
            } else if profile.family == "v3" {
                if self.v2.is_some()
                    || !self.reserve0.0.is_zero()
                    || !self.reserve1.0.is_zero()
                    || self.fee_denominator != 1000000
                {
                    return Err(err("V3_STATE_REQUIRED"));
                }
                let v = self.v3.as_ref().ok_or_else(|| err("V3_STATE_REQUIRED"))?;
                v.validate()?;
                if self.protocol == "aerodrome-slipstream" {
                    protocols::validate_identity(&v.factory, &v.factory_code_sha256)?;
                    if v.fee_protocol != 0 || self.fee_numerator > 100000 {
                        return Err(err("SLIPSTREAM_STATE_IDENTITY"));
                    }
                }
                if profile.fee_protocol_bits == 8 && v.fee_protocol > 255 {
                    return Err(err("V3_FEE_PROTOCOL_WIDTH"));
                }
                if self.protocol == "pancake-v3"
                    && (v.fee_protocol & 65535 > 10000 || v.fee_protocol >> 16 > 10000)
                {
                    return Err(err("V3_PROTOCOL_FEE_RANGE"));
                }
            } else {
                if self.v3.is_some() {
                    return Err(err("V3_PROTOCOL_MISMATCH"));
                }
                if profile.require_identity && self.v2.is_none() {
                    return Err(err("V2_IDENTITY_REQUIRED"));
                }
                if let Some(v) = &self.v2 {
                    protocols::validate_identity(&v.factory, &v.code_sha256)?;
                }
                if profile.fee_denominator != 0
                    && (self.fee_numerator != profile.fee_numerator
                        || self.fee_denominator != profile.fee_denominator)
                {
                    return Err(err("PROTOCOL_FEE_MISMATCH"));
                }
            }
        } else if self.v2.is_some()
            || self.v3.is_some()
            || self.stable.is_some()
            || self.singleton_cl.is_some()
        {
            return Err(err("PROTOCOL_STATE_MISMATCH"));
        }
        validate_chain(&self.chain_key)?;
        if self.pool_id.is_empty()
            || self.token0.is_empty()
            || self.token1.is_empty()
            || self.token0 == self.token1
            || self.adapter_version.is_empty()
        {
            return Err(err("INVALID_POOL_IDENTITY"));
        }
        if self.fee_denominator == 0 || self.fee_numerator >= self.fee_denominator {
            return Err(err("INVALID_FEE"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    pub from: u64,
    pub to: u64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub reference: SnapshotRef,
    pub pools: Vec<Pool>,
    pub source: String,
    pub history_gaps: Vec<Gap>,
}
pub fn scope_hash(pools: &[Pool]) -> Result<String> {
    let mut ids = pools.iter().map(|p| &p.pool_id).collect::<Vec<_>>();
    ids.sort();
    hash(&ids)
}
impl Snapshot {
    pub fn validate(&self, owner: &str, assignment: u64) -> Result<()> {
        let r = &self.reference;
        if self.pools.is_empty() {
            return Err(err("EMPTY_POOL_SCOPE"));
        }
        if r.schema_version != SCHEMA_VERSION {
            return Err(err("UNKNOWN_SCHEMA"));
        }
        if r.owner_service != owner || r.owner_assignment_version != assignment {
            return Err(err("OWNER_MISMATCH"));
        }
        validate_chain(&r.block.chain_key)?;
        if r.writer_epoch == 0
            || r.canonical_epoch == 0
            || r.online_generation == 0
            || r.journal_seq == 0
            || r.block.hash.is_empty()
            || r.adapter_set_hash.is_empty()
        {
            return Err(err("INVALID_SNAPSHOT"));
        }
        let mut seen = BTreeSet::new();
        validate_pool_dependencies(self.pools.iter())?;
        for p in &self.pools {
            if let Some(v) = p.v2.as_ref().and_then(|v| v.swap.as_ref()) {
                if v.bank_block_number != r.block.number_or_slot
                    || v.bank_block_hash != r.block.hash
                    || v.bank_unix_timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
                {
                    return Err(err("V2_SWAP_SNAPSHOT_BANK"));
                }
            }
            if let Some(b) = &p.infinity_bin {
                if b.block_hash != r.block.hash
                    || b.block_number != r.block.number_or_slot
                    || b.timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
                {
                    return Err(err("INFINITY_BIN_SNAPSHOT_BANK"));
                }
            }
            if let Some(c) = p.multi_asset.as_ref().and_then(|s| s.curve_crypto.as_ref()) {
                if c.block_number != r.block.number_or_slot
                    || c.block_hash != r.block.hash
                    || c.timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
                {
                    return Err(err("CURVE_CRYPTO_SNAPSHOT_BANK"));
                }
            }
            if let Some(m) = p.multi_asset.as_ref().and_then(|s| s.curve_meta.as_ref()) {
                if m.block_number != r.block.number_or_slot
                    || m.block_hash != r.block.hash
                    || m.timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
                {
                    return Err(err("CURVE_META_SNAPSHOT_BANK"));
                }
            }

            if let Some(m) = p.multi_asset.as_ref().and_then(|s| s.managed_v2.as_ref()) {
                if m.execution_context.is_some() {
                    return Err(err("MANAGED_V2_DERIVED_STATE_NOT_CANONICAL"));
                }
                if u64::from(m.last_collection_timestamp) > m.timestamp {
                    return Err(err("MANAGED_V2_SNAPSHOT_COLLECTION_CLOCK"));
                }
                if m.token_info
                    .iter()
                    .any(|v| v.last_change_block > r.block.number_or_slot)
                {
                    return Err(err("MANAGED_V2_SNAPSHOT_TOKEN_BANK"));
                };
                if m.block_number != r.block.number_or_slot
                    || m.block_hash != r.block.hash
                    || m.timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
                {
                    return Err(err("MANAGED_V2_SNAPSHOT_BANK"));
                }
            }
            if let Some(m) = p.multi_asset.as_ref().and_then(|s| s.managed_fees.as_ref()) {
                if m.block_number != r.block.number_or_slot
                    || m.block_hash != r.block.hash
                    || m.timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
                {
                    return Err(err("MANAGED_FEE_SNAPSHOT_BANK"));
                }
            }
            if let Some(ng) = p.multi_asset.as_ref().and_then(|s| s.curve_ng.as_ref()) {
                if ng.block_hash != r.block.hash
                    || ng.block_number != r.block.number_or_slot
                    || ng.timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
                {
                    return Err(err("CURVE_NG_SNAPSHOT_BANK"));
                }
            }
            if p.singleton_advanced
                .as_ref()
                .is_some_and(|a| a.snapshot_block_hash != r.block.hash)
            {
                return Err(err("SINGLETON_ADVANCED_SNAPSHOT_BANK"));
            }
            if p.raydium_clmm
                .as_ref()
                .is_some_and(|s| s.bank_slot != r.block.number_or_slot)
            {
                return Err(err("RAYCLMM_SNAPSHOT_BANK"));
            }
            if p.orca_whirlpool.as_ref().is_some_and(|o| {
                o.bank_slot != r.block.number_or_slot
                    || o.adaptive.is_some()
                        && o.bank_unix_timestamp.checked_mul(1000) != Some(r.block.timestamp_ms)
            }) {
                return Err(err("ORCA_SNAPSHOT_BANK"));
            }
            if p.raydium_cpmm
                .as_ref()
                .is_some_and(|s| s.bank_slot != r.block.number_or_slot)
            {
                return Err(err("RAYDIUM_SNAPSHOT_BANK"));
            }
            p.validate()?;
            if p.chain_key != r.block.chain_key || !seen.insert(&p.pool_id) {
                return Err(err("POOL_SCOPE_MISMATCH"));
            }
        }
        if scope_hash(&self.pools)? != r.scope_hash {
            return Err(err("SCOPE_HASH_MISMATCH"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub reference: SnapshotRef,
    pub kind: String,
    pub record_id: String,
    pub payload_hash: String,
    pub pools: Vec<Pool>,
}
impl Record {
    pub fn validate_payload(&self) -> Result<()> {
        if hash(&self.pools)? != self.payload_hash {
            return Err(err("PAYLOAD_HASH_MISMATCH"));
        }
        Ok(())
    }
}

pub mod wire_conversion;
