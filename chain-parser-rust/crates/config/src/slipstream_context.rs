use super::PoolConfig;
use serde::{Deserialize, Serialize};
use xone_domain::{err, Result};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamModuleConfig {
    pub kind: String,
    pub address: String,
    pub expected_code_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamContextConfig {
    pub expected_code_sha256: String,
    pub expected_implementation: String,
    pub expected_implementation_code_sha256: String,
    pub expected_factory: String,
    pub expected_factory_code_sha256: String,
    pub expected_voter: String,
    pub expected_voter_code_sha256: String,
    pub expected_token0_code_sha256: String,
    pub expected_token1_code_sha256: String,
    pub token_policy: String,
    pub origins: Vec<String>,
    pub swap_fee_module: SlipstreamModuleConfig,
    pub unstaked_fee_module: SlipstreamModuleConfig,
    pub tick_word_radius: u32,
    pub max_initialized_ticks: u32,
    pub max_steps: u32,
    pub max_observations: u32,
}
impl SlipstreamModuleConfig {
    fn validate(&self, swap: bool) -> Result<()> {
        if self.kind == "none" {
            if self.address != xone_domain::singleton::ZERO_ADDRESS
                || !self.expected_code_sha256.is_empty()
            {
                return Err(err("SLIPSTREAM_ABSENT_MODULE_CONFIG"));
            }
            return Ok(());
        }
        if self.kind != "custom" && (self.kind != "dynamic" || !swap) {
            return Err(err("SLIPSTREAM_MODULE_KIND_CONFIG"));
        }
        xone_domain::protocols::validate_identity(&self.address, &self.expected_code_sha256)?;
        if self.address == xone_domain::singleton::ZERO_ADDRESS {
            return Err(err("SLIPSTREAM_MODULE_IDENTITY_CONFIG"));
        }
        Ok(())
    }
}
impl SlipstreamContextConfig {
    pub(super) fn validate(&self, p: &PoolConfig, chain: &str) -> Result<()> {
        if p.protocol != xone_domain::slipstream_context::PROFILE
            || !chain.starts_with("evm:")
            || xone_domain::validate_chain(chain).is_err()
            || self.token_policy != "standard"
            || p.fee_numerator != 0
            || p.fee_denominator != 1000000
            || !p.program_id.is_empty()
            || !p.accounts.is_empty()
            || p.lfj.is_some()
            || p.infinity_bin.is_some()
            || p.singleton_advanced.is_some()
            || p.multi_asset.is_some()
            || p.weighted.is_some()
            || p.raydium_clmm.is_some()
            || p.orca_whirlpool.is_some()
            || p.raydium_cpmm.is_some()
            || p.singleton_cl.is_some()
            || p.stable.is_some()
            || p.v2.is_some()
            || p.v3.is_some()
        {
            return Err(err("SLIPSTREAM_CONTEXT_CONFIG"));
        }
        let mut distinct = std::collections::BTreeSet::new();
        for address in [
            &p.id,
            &p.token0,
            &p.token1,
            &self.expected_factory,
            &self.expected_implementation,
        ] {
            xone_domain::protocols::validate_address(address)?;
            if address == xone_domain::singleton::ZERO_ADDRESS || !distinct.insert(address) {
                return Err(err("SLIPSTREAM_IDENTITY_ALIAS_CONFIG"));
            }
        }
        if p.token0 >= p.token1 {
            return Err(err("SLIPSTREAM_TOKEN_ORDER_CONFIG"));
        }
        xone_domain::protocols::validate_identity(
            &self.expected_voter,
            &self.expected_voter_code_sha256,
        )?;
        if self.expected_voter_code_sha256 != xone_domain::slipstream_context::VOTER_RUNTIME_SHA256
        {
            return Err(err("SLIPSTREAM_VOTER_VERSION_CONFIG"));
        }
        for digest in [
            &self.expected_code_sha256,
            &self.expected_implementation_code_sha256,
            &self.expected_factory_code_sha256,
            &self.expected_token0_code_sha256,
            &self.expected_token1_code_sha256,
        ] {
            xone_domain::protocols::validate_identity(&p.id, digest)?;
        }
        if self.tick_word_radius > 16
            || !(1..=4096).contains(&self.max_initialized_ticks)
            || !(1..=8192).contains(&self.max_steps)
            || !(1..=4096).contains(&self.max_observations)
            || self.origins.len() > 16
            || self.swap_fee_module.kind == "dynamic" && self.origins.is_empty()
        {
            return Err(err("SLIPSTREAM_CONTEXT_BUDGET_CONFIG"));
        }
        self.swap_fee_module.validate(true)?;
        self.unstaked_fee_module.validate(false)?;
        let mut origins = std::collections::BTreeSet::new();
        for address in &self.origins {
            xone_domain::protocols::validate_address(address)?;
            if address == xone_domain::singleton::ZERO_ADDRESS || !origins.insert(address) {
                return Err(err("SLIPSTREAM_ORIGIN_CONFIG"));
            }
        }
        Ok(())
    }
}
