//! Server-approved oracle authorities. Authorization is configuration, never
//! accepted from a quote or simulation request. Rates describe atomic units.
use crate::{err, hash, protocols::validate_address, validate_chain, Amount, Result};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_feed: Option<ChainlinkUsdPairPolicy>,
}

/// Explicitly approved deployments, never discovered from an HTTP request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChainlinkFeedPolicy {
    pub proxy: String,
    pub proxy_code_sha256: String,
    pub aggregator: String,
    pub aggregator_code_sha256: String,
    pub description: String,
    pub decimals: u8,
    pub min_answer: Amount,
    pub max_answer: Amount,
    pub max_age_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainlinkUsdPairPolicy {
    pub schema_version: u32,
    pub native_decimals: u8,
    pub token_decimals: u8,
    pub native_usd: ChainlinkFeedPolicy,
    pub token_usd: ChainlinkFeedPolicy,
}
pub const ETHEREUM_GENESIS: &str =
    "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3";
impl ChainlinkUsdPairPolicy {
    pub fn validate(&self, chain: &str, genesis: &str) -> Result<()> {
        let bad = || err("NATIVE_VALUATION_MARKET_POLICY");
        if self.schema_version != 1
            || chain != "evm:1"
            || genesis != ETHEREUM_GENESIS
            || self.native_decimals != 18
            || self.token_decimals > 36
            || self.native_usd.description != "ETH / USD"
            || (self.native_usd.proxy == self.token_usd.proxy && self.native_usd != self.token_usd)
        {
            return Err(bad());
        }
        for f in [&self.native_usd, &self.token_usd] {
            validate_address(&f.proxy)?;
            validate_address(&f.aggregator)?;
            if f.proxy == format!("0x{}", "0".repeat(40))
                || f.aggregator == format!("0x{}", "0".repeat(40))
                || f.decimals > 18
                || !f.description.ends_with(" / USD")
                || f.description.len() < 7
                || f.description.len() > 128
                || !f.description.bytes().all(|b| (32..=126).contains(&b))
                || !(1..=86_400_000).contains(&f.max_age_ms)
                || f.min_answer == Amount::zero()
                || f.min_answer > f.max_answer
                || f.max_answer.0.bits() > 255
                || [&f.proxy_code_sha256, &f.aggregator_code_sha256]
                    .iter()
                    .any(|h| !canonical_hex(h, 32) || h.bytes().all(|b| b == b'0'))
            {
                return Err(bad());
            }
        }
        Ok(())
    }
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
        if let Some(feed) = &self.market_feed {
            feed.validate(&self.chain_key, &self.genesis_hash)?;
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
