use crate::{err, PoolConfig};
use serde::{Deserialize, Serialize};
use xone_domain::{
    managed_v2_state::{validate_dependencies, ManagedV2Dependency},
    protocols::{validate_address, validate_identity},
    Amount,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedV2PoolConfig {
    pub expected_factory: String,
    pub expected_factory_code_sha256: String,
    pub expected_pool_runtime_template_sha256: String,
    pub expected_fee_provider: String,
    pub expected_fee_provider_code_sha256: String,
    pub expected_protocol_collector: String,
    pub expected_protocol_collector_code_sha256: String,
    pub expected_manager: String,
    pub expected_provider_fee_id: Amount,
    pub expected_management_aum_fee: Amount,
    pub dependencies: Vec<ManagedV2Dependency>,
}
impl PoolConfig {
    pub(crate) fn validate_managed_v2(&self, chain: &str) -> xone_domain::Result<()> {
        let p = self;
        let fail = || err("MANAGED_V2_CONFIG");
        let v = p.multi_asset.as_ref().ok_or_else(fail)?;
        let m = v.managed_v2.as_ref().ok_or_else(fail)?;
        xone_domain::validate_chain(chain)?;
        validate_identity(&p.id, &v.expected_code_sha256)?;
        validate_identity(&v.expected_vault, &v.expected_vault_code_sha256)?;
        validate_identity(&m.expected_factory, &m.expected_factory_code_sha256)?;
        validate_identity(
            &m.expected_fee_provider,
            &m.expected_fee_provider_code_sha256,
        )?;
        validate_identity(
            &m.expected_protocol_collector,
            &m.expected_protocol_collector_code_sha256,
        )?;
        validate_address(&m.expected_manager)?;
        validate_identity(&p.id, &m.expected_pool_runtime_template_sha256)?;
        validate_dependencies(&m.dependencies)?;
        // ProtocolFeeCache encodes each provider fee ID in 8 bits.
        if m.expected_provider_fee_id.0.bits() > 8
            || m.expected_management_aum_fee > "950000000000000000".parse::<Amount>()?
        {
            return Err(fail());
        }
        if !chain.starts_with("evm:")
            || p.infinity_bin.is_some()
            || p.singleton_advanced.is_some()
            || p.weighted.is_some()
            || p.stable.is_some()
            || p.v2.is_some()
            || p.v3.is_some()
            || p.singleton_cl.is_some()
            || p.raydium_cpmm.is_some()
            || p.orca_whirlpool.is_some()
            || p.raydium_clmm.is_some()
            || !p.program_id.is_empty()
            || !p.accounts.is_empty()
            || v.token_policy != "standard-no-bpt"
            || !v.expected_implementation.is_empty()
            || !v.expected_implementation_code_sha256.is_empty()
            || !v.expected_lp_token.is_empty()
            || !v.expected_lp_token_code_sha256.is_empty()
            || !v.expected_base_pool_code_sha256.is_empty()
            || !(2..=50).contains(&v.tokens.len())
            || p.token0 != v.tokens[0].token
            || p.token1 != v.tokens[1].token
            || v.pool_id.len() != 66
            || !v.pool_id.starts_with(&format!("{}0001", p.id))
            || validate_identity(&p.id, &v.pool_id[2..]).is_err()
            || p.fee_denominator != 1_000_000_000_000_000_000
            || !(1_000_000_000_000..=950_000_000_000_000_000).contains(&p.fee_numerator)
        {
            return Err(fail());
        }
        for (i, a) in v.tokens.iter().enumerate() {
            validate_identity(&a.token, &a.expected_code_sha256)?;
            if a.token == p.id
                || a.token == v.expected_vault
                || i > 0 && v.tokens[i - 1].token >= a.token
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
