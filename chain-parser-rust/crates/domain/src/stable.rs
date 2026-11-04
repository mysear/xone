use crate::{
    err,
    protocols::{validate_address, validate_identity},
    wire, Amount, Result,
};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StableState {
    pub code_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub implementation: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub implementation_code_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub factory: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub factory_code_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vault: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vault_code_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pool_id: String,
    pub rate0: Amount,
    pub rate1: Amount,
    pub amplification: Amount,
    pub admin_fee: u64,
    pub max_iterations: u32,
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
fn power10(n: &BigUint, low: u32, high: u32) -> bool {
    (low..=high).any(|p| n == &BigUint::from(10u8).pow(p))
}
impl StableState {
    pub fn validate(&self, family: &str) -> Result<()> {
        if !digest(&self.code_sha256) || self.max_iterations != 255 {
            return Err(err("STABLE_CODE_OR_BUDGET"));
        }
        if !self.implementation.is_empty() {
            validate_address(&self.implementation)?;
            if !digest(&self.implementation_code_sha256) {
                return Err(err("STABLE_IMPLEMENTATION_IDENTITY"));
            }
        } else if !self.implementation_code_sha256.is_empty() {
            return Err(err("STABLE_IMPLEMENTATION_IDENTITY"));
        }
        match family {
            "aero" => {
                validate_identity(&self.factory, &self.factory_code_sha256)?;
                if !power10(&self.rate0.0, 0, 18)
                    || !power10(&self.rate1.0, 0, 18)
                    || !self.amplification.0.is_zero()
                    || self.admin_fee != 0
                    || !self.vault.is_empty()
                    || !self.vault_code_sha256.is_empty()
                    || !self.pool_id.is_empty()
                {
                    return Err(err("AERO_STATE_PARAMETERS"));
                }
            }
            "curve" => {
                if !power10(&self.rate0.0, 18, 36)
                    || !power10(&self.rate1.0, 18, 36)
                    || self.amplification.0 < BigUint::from(100u64)
                    || self.amplification.0 >= BigUint::from(100000000u64)
                    || self.admin_fee != 5000000000
                    || !self.factory.is_empty()
                    || !self.factory_code_sha256.is_empty()
                    || !self.vault.is_empty()
                    || !self.vault_code_sha256.is_empty()
                    || !self.pool_id.is_empty()
                {
                    return Err(err("CURVE_STATE_PARAMETERS"));
                }
            }
            "balancer" => {
                validate_identity(&self.vault, &self.vault_code_sha256)?;
                if !power10(&self.rate0.0, 18, 36)
                    || !power10(&self.rate1.0, 18, 36)
                    || self.amplification.0 < BigUint::from(1000u64)
                    || self.amplification.0 > BigUint::from(5000000u64)
                    || self.admin_fee != 0
                    || !self.factory.is_empty()
                    || !self.factory_code_sha256.is_empty()
                    || self.pool_id.len() != 66
                    || !self.pool_id.starts_with("0x")
                    || self.pool_id != self.pool_id.to_ascii_lowercase()
                    || hex::decode(&self.pool_id[2..]).is_err()
                {
                    return Err(err("BALANCER_STATE_PARAMETERS"));
                }
            }
            _ => return Err(err("STABLE_PROTOCOL_UNSUPPORTED")),
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::StableState> {
        fn raw(s: &str) -> Result<Vec<u8>> {
            hex::decode(s.trim_start_matches("0x")).map_err(|_| err("STABLE_DIGEST"))
        }
        fn amount(a: &Amount) -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        }
        Ok(wire::StableState {
            code_sha256: raw(&self.code_sha256)?,
            implementation: self.implementation.clone(),
            implementation_code_sha256: raw(&self.implementation_code_sha256)?,
            factory: self.factory.clone(),
            factory_code_sha256: raw(&self.factory_code_sha256)?,
            vault: self.vault.clone(),
            vault_code_sha256: raw(&self.vault_code_sha256)?,
            pool_id: raw(&self.pool_id)?,
            rate0: amount(&self.rate0)?,
            rate1: amount(&self.rate1)?,
            amplification: amount(&self.amplification)?,
            admin_fee: self.admin_fee,
            max_iterations: self.max_iterations,
        })
    }
    pub fn from_proto(s: wire::StableState) -> Result<Self> {
        fn digest(b: Vec<u8>, optional: bool) -> Result<String> {
            if b.len() != 32 && !(optional && b.is_empty()) {
                return Err(err("STABLE_DIGEST_WIDTH"));
            }
            Ok(hex::encode(b))
        }
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(&a.ok_or_else(|| err("STABLE_AMOUNT_REQUIRED"))?.uint256_be)
        }
        let id = if s.pool_id.is_empty() {
            String::new()
        } else {
            format!("0x{}", digest(s.pool_id, false)?)
        };
        Ok(Self {
            code_sha256: digest(s.code_sha256, false)?,
            implementation: s.implementation,
            implementation_code_sha256: digest(s.implementation_code_sha256, true)?,
            factory: s.factory,
            factory_code_sha256: digest(s.factory_code_sha256, true)?,
            vault: s.vault,
            vault_code_sha256: digest(s.vault_code_sha256, true)?,
            pool_id: id,
            rate0: amount(s.rate0)?,
            rate1: amount(s.rate1)?,
            amplification: amount(s.amplification)?,
            admin_fee: s.admin_fee,
            max_iterations: s.max_iterations,
        })
    }
}
