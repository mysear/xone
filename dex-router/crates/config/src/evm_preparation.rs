use crate::{err, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(super) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && value.bytes().any(|b| b != b'0')
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V4ExecutionConfig {
    pub executor: String,
    pub executor_code_sha256: String,
    pub token_code_sha256: BTreeMap<String, String>,
}
impl V4ExecutionConfig {
    pub fn validate(&self) -> Result<()> {
        let address = |s: &str| {
            xone_domain::protocols::validate_address(s).is_ok()
                && s != xone_domain::singleton::ZERO_ADDRESS
        };
        if !address(&self.executor)
            || !valid_digest(&self.executor_code_sha256)
            || !(1..=128).contains(&self.token_code_sha256.len())
            || self.token_code_sha256.iter().any(|(token, digest)| {
                !address(token) || token == &self.executor || !valid_digest(digest)
            })
        {
            return Err(err("V4_EXECUTION_CONFIG"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvmPreparationConfig {
    pub router: String,
    pub router_code_sha256: String,
    pub factory: String,
    pub factory_code_sha256: String,
    pub token_code_sha256: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub max_calls: usize,
    pub max_response_bytes: usize,
}
impl EvmPreparationConfig {
    pub fn validate(&self) -> Result<()> {
        let address = |s: &str| {
            xone_domain::protocols::validate_address(s).is_ok()
                && s != format!("0x{}", "0".repeat(40))
        };
        if !address(&self.router)
            || !address(&self.factory)
            || self.router == self.factory
            || !valid_digest(&self.router_code_sha256)
            || self.factory_code_sha256 != xone_domain::v2_swap::FACTORY_CODE_SHA256
            || self.token_code_sha256.len() < 2
            || self.token_code_sha256.len() > 128
            || self.token_code_sha256.iter().any(|(token, code)| {
                !address(token)
                    || !valid_digest(code)
                    || token == &self.router
                    || token == &self.factory
            })
            || !(100..=15000).contains(&self.timeout_ms)
            || !(32..=256).contains(&self.max_calls)
            || !(65536..=16777216).contains(&self.max_response_bytes)
        {
            return Err(err("EVM_PREPARATION_CONFIG"));
        }
        Ok(())
    }
}
