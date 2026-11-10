use super::PoolConfig;
use serde::{Deserialize, Serialize};
use xone_domain::{err, Result};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PancakeLmConfig {
    pub expected_code_sha256: String,
    pub expected_factory: String,
    pub expected_factory_code_sha256: String,
    pub expected_lm_pool: String,
    pub expected_lm_pool_code_sha256: String,
    pub expected_master_chef: String,
    pub expected_master_chef_code_sha256: String,
    pub expected_cake: String,
    pub expected_weth: String,
    pub expected_position_manager: String,
    pub expected_token0_code_sha256: String,
    pub expected_token1_code_sha256: String,
    pub token_policy: String,
    pub tick_word_radius: u32,
    pub max_initialized_ticks: u32,
    pub max_steps: u32,
    pub max_observations: u32,
}
impl PancakeLmConfig {
    pub(super) fn validate(&self, p: &PoolConfig, chain: &str) -> Result<()> {
        if p.protocol != xone_domain::pancake_lm::PROFILE
            || !chain.starts_with("evm:")
            || xone_domain::validate_chain(chain).is_err()
            || self.token_policy != "standard"
            || p.fee_numerator >= 1000000
            || p.fee_denominator != 1000000
            || !p.program_id.is_empty()
            || !p.accounts.is_empty()
            || p.slipstream_context.is_some()
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
            return Err(err("PANCAKE_LM_CONFIG"));
        }
        let mut distinct = std::collections::BTreeSet::new();
        for a in [
            &p.id,
            &p.token0,
            &p.token1,
            &self.expected_factory,
            &self.expected_lm_pool,
            &self.expected_master_chef,
        ] {
            xone_domain::protocols::validate_address(a)?;
            if a == xone_domain::singleton::ZERO_ADDRESS || !distinct.insert(a) {
                return Err(err("PANCAKE_LM_IDENTITY_ALIAS_CONFIG"));
            }
        }
        if p.token0 >= p.token1 {
            return Err(err("PANCAKE_LM_TOKEN_ORDER_CONFIG"));
        }
        for a in [
            &self.expected_cake,
            &self.expected_weth,
            &self.expected_position_manager,
        ] {
            xone_domain::protocols::validate_address(a)?;
            if a == xone_domain::singleton::ZERO_ADDRESS {
                return Err(err("PANCAKE_LM_MASTER_CHEF_IMMUTABLE_CONFIG"));
            }
        }
        for d in [
            &self.expected_code_sha256,
            &self.expected_factory_code_sha256,
            &self.expected_lm_pool_code_sha256,
            &self.expected_master_chef_code_sha256,
            &self.expected_token0_code_sha256,
            &self.expected_token1_code_sha256,
        ] {
            xone_domain::protocols::validate_identity(&p.id, d)?;
        }
        if self.tick_word_radius > 16
            || !(1..=4096).contains(&self.max_initialized_ticks)
            || !(1..=8192).contains(&self.max_steps)
            || !(1..=4096).contains(&self.max_observations)
        {
            return Err(err("PANCAKE_LM_CONFIG_BUDGET"));
        }
        Ok(())
    }
}
