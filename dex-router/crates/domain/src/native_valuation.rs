//! Server-approved oracle authorities. Authorization is configuration, never
//! accepted from a quote or simulation request. Rates describe atomic units.
use crate::{err, hash, protocols::validate_address, validate_chain, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricePolicy {
    pub source_id: String,
    pub chain_key: String,
    pub genesis_hash: String,
    pub token: String,
    pub signers: BTreeMap<String, String>,
    pub threshold: usize,
    pub max_age_ms: u64,
    pub max_validity_ms: u64,
}
pub fn canonical_hex(s: &str, bytes: usize) -> bool {
    s.len() == bytes * 2
        && s.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
pub fn block_hash(s: &str) -> bool {
    s.strip_prefix("0x").is_some_and(|h| canonical_hex(h, 32))
}
impl PricePolicy {
    pub fn validate(&self) -> Result<()> {
        validate_chain(&self.chain_key)?;
        validate_address(&self.token)?;
        let name = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_/.:".contains(&b))
        };
        if !self.chain_key.starts_with("evm:")
            || !block_hash(&self.genesis_hash)
            || self.genesis_hash == format!("0x{}", "0".repeat(64))
            || self.token == format!("0x{}", "0".repeat(40))
            || !name(&self.source_id)
            || !(2..=16).contains(&self.signers.len())
            || !(2..=self.signers.len()).contains(&self.threshold)
            || !(1..=3_600_000).contains(&self.max_age_ms)
            || !(1..=3_600_000).contains(&self.max_validity_ms)
            || self.signers.iter().any(|(id, key)| {
                !name(id) || !canonical_hex(key, 32) || key.bytes().all(|b| b == b'0')
            })
            || self.signers.values().collect::<BTreeSet<_>>().len() != self.signers.len()
        {
            return Err(err("NATIVE_VALUATION_POLICY"));
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<String> {
        self.validate()?;
        hash(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeValuationConfig {
    pub observations_file: String,
    pub policies: Vec<PricePolicy>,
}
impl NativeValuationConfig {
    pub fn validate(&self) -> Result<()> {
        if self.observations_file.is_empty()
            || self.observations_file.len() > 4096
            || self.observations_file.contains('\0')
            || self.policies.is_empty()
            || self.policies.len() > 128
        {
            return Err(err("NATIVE_VALUATION_POLICY"));
        }
        let mut pairs = BTreeSet::new();
        for policy in &self.policies {
            policy.validate()?;
            if !pairs.insert((&policy.chain_key, &policy.genesis_hash, &policy.token)) {
                return Err(err("NATIVE_VALUATION_POLICY"));
            }
        }
        Ok(())
    }
}
