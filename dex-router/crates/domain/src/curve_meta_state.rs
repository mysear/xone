use crate::{err, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const RUNTIME_SHA256: &str = "c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d";
pub const BASE_POOL: &str = "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7";
pub const BASE_LP: &str = "0x6c3f90f043a72fa612cbac8115ee7e52bde6e490";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CurveMetaState {
    pub implementation: String,
    pub implementation_code_sha256: String,
    pub base_pool: String,
    pub base_pool_code_sha256: String,
    pub base_virtual_price: Amount,
    pub stored_balances: Vec<Amount>,
    pub admin_balances: Vec<Amount>,
    pub timestamp: u64,
    pub block_number: u64,
    pub block_hash: String,
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
impl Pool {
    pub(crate) fn validate_curve_meta(&self) -> Result<()> {
        let s = self
            .multi_asset
            .as_ref()
            .ok_or_else(|| err("CURVE_META_STATE_REQUIRED"))?;
        let m = s
            .curve_meta
            .as_ref()
            .ok_or_else(|| err("CURVE_META_STATE_REQUIRED"))?;
        if s.curve_ng.is_some()
            || s.managed_fees.is_some()
            || s.assets.len() != 2
            || !s.weights.is_empty()
            || !s.vault.is_empty()
            || !s.vault_code_sha256.is_empty()
            || !s.pool_id.is_empty()
            || m.stored_balances.len() != 2
            || m.admin_balances.len() != 2
            || s.assets[1].token != BASE_LP
            || s.assets[1].scaling.0 != BigUint::from(1_000_000_000_000_000_000u64)
            || m.base_pool != BASE_POOL
            || self.pool_id == m.base_pool
            || !digest(&m.base_pool_code_sha256)
            || m.timestamp == 0
            || m.timestamp.checked_mul(1000).is_none()
            || m.block_hash.len() != 66
            || !m.block_hash.starts_with("0x")
            || !digest(&m.block_hash[2..])
            || m.block_hash[2..].bytes().all(|b| b == b'0')
        {
            return Err(err("CURVE_META_PARAMETERS"));
        }
        if s.amplification.0 < BigUint::from(100u64)
            || s.amplification.0 >= BigUint::from(100_000_000u64)
            || s.admin_fee != 5_000_000_000
            || !(4_000_000..=100_000_000).contains(&self.fee_numerator)
        {
            return Err(err("CURVE_META_FEE_OR_AMP"));
        }
        crate::protocols::validate_address(&m.implementation)?;
        if m.implementation == self.pool_id || m.implementation_code_sha256 != RUNTIME_SHA256 {
            return Err(err("CURVE_META_IMPLEMENTATION"));
        }
        let code = hex::decode(format!(
            "363d3d373d3d3d363d73{}5af43d82803e903d91602b57fd5bf3",
            &m.implementation[2..]
        ))
        .map_err(|_| err("CURVE_META_CLONE_CODE"))?;
        if hex::encode(Sha256::digest(code)) != s.code_sha256 {
            return Err(err("CURVE_META_CLONE_CODE"));
        }
        if m.base_virtual_price.0.is_zero() || m.base_virtual_price.0.bits() > 256 {
            return Err(err("CURVE_META_VIRTUAL_PRICE"));
        }
        for i in 0..2 {
            let sum = &s.assets[i].balance.0 + &m.admin_balances[i].0;
            if sum.bits() > 256
                || m.stored_balances[i].0.bits() > 256
                || m.admin_balances[i].0.bits() > 256
                || sum != m.stored_balances[i].0
            {
                return Err(err("CURVE_META_BALANCE_ACCOUNTING"));
            }
        }
        Ok(())
    }
}
/// Current base VP can be reused through local swaps only if the base itself
/// cannot be mutated by the plan, and every read of that dependency agrees.
pub fn validate_meta_dependencies<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let rows = pools.into_iter().collect::<Vec<_>>();
    if !rows.iter().any(|p| {
        p.multi_asset
            .as_ref()
            .is_some_and(|s| s.curve_meta.is_some())
    }) {
        return Ok(());
    }
    let ids = rows
        .iter()
        .map(|p| p.pool_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut observed: Option<(&CurveMetaState, &str)> = None;
    for p in rows {
        let Some(s) = p.multi_asset.as_ref() else {
            continue;
        };
        let Some(m) = s.curve_meta.as_ref() else {
            continue;
        };
        if ids.contains(m.base_pool.as_str()) {
            return Err(err("CURVE_META_MUTABLE_BASE_DEPENDENCY"));
        }
        if s.assets.len() != 2 {
            return Err(err("CURVE_META_PARAMETERS"));
        }
        let code = s.assets[1].code_sha256.as_str();
        if let Some((prior, lp)) = observed {
            if prior.base_pool != m.base_pool
                || prior.base_pool_code_sha256 != m.base_pool_code_sha256
                || prior.base_virtual_price != m.base_virtual_price
                || prior.timestamp != m.timestamp
                || prior.block_number != m.block_number
                || prior.block_hash != m.block_hash
                || lp != code
            {
                return Err(err("CURVE_META_DEPENDENCY_CONFLICT"));
            }
        }
        observed = Some((m, code));
    }
    Ok(())
}
impl CurveMetaState {
    pub fn to_proto(&self) -> Result<wire::CurveMetaState> {
        fn bytes(v: &str) -> Result<Vec<u8>> {
            let b = hex::decode(v.trim_start_matches("0x"))
                .map_err(|_| err("CURVE_META_BYTES_WIDTH"))?;
            if b.len() != 32 {
                return Err(err("CURVE_META_BYTES_WIDTH"));
            }
            Ok(b)
        }
        fn amount(v: &Amount) -> Result<wire::Amount> {
            Ok(wire::Amount {
                uint256_be: v.to_be_bytes()?.to_vec(),
            })
        }
        Ok(wire::CurveMetaState {
            implementation: self.implementation.clone(),
            implementation_code_sha256: bytes(&self.implementation_code_sha256)?,
            base_pool: self.base_pool.clone(),
            base_pool_code_sha256: bytes(&self.base_pool_code_sha256)?,
            base_virtual_price: Some(amount(&self.base_virtual_price)?),
            stored_balances: self
                .stored_balances
                .iter()
                .map(amount)
                .collect::<Result<_>>()?,
            admin_balances: self
                .admin_balances
                .iter()
                .map(amount)
                .collect::<Result<_>>()?,
            timestamp: self.timestamp,
            block_number: self.block_number,
            block_hash: bytes(&self.block_hash)?,
        })
    }
    pub fn from_proto(s: wire::CurveMetaState) -> Result<Self> {
        fn amount(v: wire::Amount) -> Result<Amount> {
            Amount::from_be_bytes(&v.uint256_be)
        }
        if s.base_pool_code_sha256.len() != 32
            || s.block_hash.len() != 32
            || s.implementation_code_sha256.len() != 32
        {
            return Err(err("CURVE_META_BYTES_WIDTH"));
        }
        Ok(Self {
            implementation: s.implementation,
            implementation_code_sha256: hex::encode(s.implementation_code_sha256),
            base_pool: s.base_pool,
            base_pool_code_sha256: hex::encode(s.base_pool_code_sha256),
            base_virtual_price: amount(
                s.base_virtual_price
                    .ok_or_else(|| err("CURVE_META_AMOUNT_REQUIRED"))?,
            )?,
            stored_balances: s
                .stored_balances
                .into_iter()
                .map(amount)
                .collect::<Result<_>>()?,
            admin_balances: s
                .admin_balances
                .into_iter()
                .map(amount)
                .collect::<Result<_>>()?,
            timestamp: s.timestamp,
            block_number: s.block_number,
            block_hash: format!("0x{}", hex::encode(s.block_hash)),
        })
    }
}
