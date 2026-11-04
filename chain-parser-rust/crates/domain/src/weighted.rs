use crate::{
    err,
    protocols::{validate_address, validate_identity},
    wire, Amount, Pool, Result,
};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WeightedState {
    pub code_sha256: String,
    pub vault: String,
    pub vault_code_sha256: String,
    pub pool_id: String,
    pub scaling0: Amount,
    pub scaling1: Amount,
    pub weight0: Amount,
    pub weight1: Amount,
    pub token0_code_sha256: String,
    pub token1_code_sha256: String,
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
impl Pool {
    pub(crate) fn validate_weighted(&self) -> Result<()> {
        let s = self
            .weighted
            .as_ref()
            .ok_or_else(|| err("WEIGHTED_STATE_REQUIRED"))?;
        if self.stable.is_some()
            || self.v2.is_some()
            || self.v3.is_some()
            || self.singleton_cl.is_some()
            || self.raydium_cpmm.is_some()
            || self.orca_whirlpool.is_some()
            || self.raydium_clmm.is_some()
        {
            return Err(err("WEIGHTED_STATE_REQUIRED"));
        }
        crate::validate_chain(&self.chain_key)?;
        for a in [&self.pool_id, &self.token0, &self.token1] {
            validate_address(a)?;
        }
        if !self.chain_key.starts_with("evm:")
            || self.token0 >= self.token1
            || self.token0 == self.pool_id
            || self.token1 == self.pool_id
            || self.token0 == s.vault
            || self.token1 == s.vault
            || self.pool_id == s.vault
        {
            return Err(err("WEIGHTED_IDENTITY"));
        }
        let profile =
            crate::protocols::protocol(&self.protocol).ok_or_else(|| err("WEIGHTED_PROFILE"))?;
        if profile.family != "weighted"
            || self.adapter_version != profile.adapter
            || self.token_behavior != "standard"
            || self.fee_denominator != 1_000_000_000_000_000_000
            || !(1_000_000_000_000..=100_000_000_000_000_000).contains(&self.fee_numerator)
        {
            return Err(err("WEIGHTED_PROFILE"));
        }
        validate_identity(&s.vault, &s.vault_code_sha256)?;
        if !digest(&s.code_sha256)
            || !digest(&s.token0_code_sha256)
            || !digest(&s.token1_code_sha256)
            || s.pool_id.len() != 66
            || !s.pool_id.starts_with(&self.pool_id)
            || !digest(&s.pool_id[2..])
            || &s.pool_id[42..46] != "0002"
        {
            return Err(err("WEIGHTED_CODE_OR_POOL_ID"));
        }
        let half = BigUint::from(500_000_000_000_000_000u64);
        let minimum = BigUint::from(10_000_000_000_000_000u64);
        let one = BigUint::from(1_000_000_000_000_000_000u64);
        if s.weight0.0 < minimum || s.weight1.0 < minimum || &s.weight0.0 + &s.weight1.0 != one {
            return Err(err("WEIGHTED_PARAMETERS"));
        }
        if (self.protocol == "balancer-v2-weighted2-equal"
            && (s.weight0.0 != half || s.weight1.0 != half))
            || [&s.scaling0, &s.scaling1]
                .iter()
                .any(|a| !(18..=36).any(|d| a.0 == BigUint::from(10u8).pow(d)))
        {
            return Err(err("WEIGHTED_PARAMETERS"));
        }
        if self.reserve0.0.bits() > 112 || self.reserve1.0.bits() > 112 {
            return Err(err("WEIGHTED_VAULT_BALANCE"));
        }
        Ok(())
    }
}
impl WeightedState {
    pub fn to_proto(&self) -> Result<wire::WeightedState> {
        let raw =
            |s: &str| hex::decode(s.trim_start_matches("0x")).map_err(|_| err("WEIGHTED_BYTES"));
        let amount = |a: &Amount| -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        };
        Ok(wire::WeightedState {
            code_sha256: raw(&self.code_sha256)?,
            vault: self.vault.clone(),
            vault_code_sha256: raw(&self.vault_code_sha256)?,
            pool_id: raw(&self.pool_id)?,
            scaling0: amount(&self.scaling0)?,
            scaling1: amount(&self.scaling1)?,
            weight0: amount(&self.weight0)?,
            weight1: amount(&self.weight1)?,
            token0_code_sha256: raw(&self.token0_code_sha256)?,
            token1_code_sha256: raw(&self.token1_code_sha256)?,
        })
    }
    pub fn from_proto(s: wire::WeightedState) -> Result<Self> {
        let digest = |v: Vec<u8>| -> Result<String> {
            if v.len() != 32 {
                return Err(err("WEIGHTED_BYTES_WIDTH"));
            }
            Ok(hex::encode(v))
        };
        let amount = |v: Option<wire::Amount>| -> Result<Amount> {
            Amount::from_be_bytes(&v.ok_or_else(|| err("WEIGHTED_AMOUNT_REQUIRED"))?.uint256_be)
        };
        Ok(Self {
            code_sha256: digest(s.code_sha256)?,
            vault: s.vault,
            vault_code_sha256: digest(s.vault_code_sha256)?,
            pool_id: format!("0x{}", digest(s.pool_id)?),
            scaling0: amount(s.scaling0)?,
            scaling1: amount(s.scaling1)?,
            weight0: amount(s.weight0)?,
            weight1: amount(s.weight1)?,
            token0_code_sha256: digest(s.token0_code_sha256)?,
            token1_code_sha256: digest(s.token1_code_sha256)?,
        })
    }
}
