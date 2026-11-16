use crate::{err, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
        let digest = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && s.bytes().any(|b| b != b'0')
        };
        let address = |s: &str| {
            xone_domain::protocols::validate_address(s).is_ok()
                && s != format!("0x{}", "0".repeat(40))
        };
        if !address(&self.router)
            || !address(&self.factory)
            || self.router == self.factory
            || !digest(&self.router_code_sha256)
            || self.factory_code_sha256 != xone_domain::v2_swap::FACTORY_CODE_SHA256
            || self.token_code_sha256.len() < 2
            || self.token_code_sha256.len() > 128
            || self.token_code_sha256.iter().any(|(token, code)| {
                !address(token) || !digest(code) || token == &self.router || token == &self.factory
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
