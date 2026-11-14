//! Versioned capabilities describe admitted kernels, not live RPC/account readiness.
use serde::{Deserialize, Serialize};
use xone_domain::{Pool, Result};
pub const VERSION: &str = "xone-protocol-capabilities/10";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtocolCapabilities {
    pub schema_version: u32,
    pub model_version: String,
    pub protocol: String,
    pub adapter_version: String,
    pub family: String,
    pub quote_modes: Vec<String>,
    pub token_behavior: String,
    pub requires_funds_context: bool,
    pub exact_out_route_scope: Option<String>,
    pub execution_preparation: bool,
    pub execution_route_scope: Option<String>,
    pub unsigned_transaction: bool,
    pub rpc_simulation: bool,
    pub account_creation: bool,
    pub native_funding: bool,
    pub native_sol_trading: bool,
    pub independent_simulation: bool,
    pub wallet_submission: bool,
    pub independent_production_verification: bool,
    pub production_ready: bool,
    pub execution_blocker: Option<String>,
}
pub fn describe(pool: &Pool) -> Result<ProtocolCapabilities> {
    pool.validate()?;
    Ok(describe_model(
        &pool.protocol,
        &pool.adapter_version,
        &pool.token_behavior,
    ))
}
pub fn catalog() -> std::collections::BTreeMap<String, ProtocolCapabilities> {
    xone_domain::protocols::NAMES
        .iter()
        .map(|name| {
            let adapter =
                xone_domain::protocols::protocol(name).map_or("unregistered", |p| p.adapter);
            ((*name).into(), describe_model(name, adapter, "standard"))
        })
        .collect()
}
fn describe_model(protocol: &str, adapter: &str, token_behavior: &str) -> ProtocolCapabilities {
    let profile = xone_domain::protocols::protocol(protocol);
    let exact_out = crate::exact_out::supports_protocol(protocol);
    let mut quote_modes = if profile.is_some() {
        vec!["EXACT_IN".into()]
    } else {
        Vec::new()
    };
    if exact_out {
        quote_modes.push("EXACT_OUT".into());
    }
    let meteora = xone_domain::meteora::supports_profile(protocol);
    let prepared = meteora
        || protocol == xone_domain::raydium::PROFILE
        || xone_domain::pump_amm::supports_profile(protocol)
        || protocol == xone_domain::pump::PROFILE;
    let contextual = profile.is_some_and(|p| {
        matches!(
            p.family,
            "pancake-lm" | "pancake-bsc4" | "slipstream-context"
        )
    }) || xone_domain::pump::supports_curve_profile(protocol)
        || xone_domain::pump_amm::supports_profile(protocol);
    ProtocolCapabilities {
        schema_version: 1,
        model_version: VERSION.into(),
        protocol: protocol.into(),
        adapter_version: adapter.into(),
        family: profile.map_or("unknown", |p| p.family).into(),
        quote_modes,
        token_behavior: token_behavior.into(),
        requires_funds_context: contextual,
        exact_out_route_scope: exact_out.then(|| {
            if protocol == xone_domain::v2_swap::PROFILE || crate::v3::supports_exact_out(protocol)
            {
                "SIMPLE_PATH_SINGLE_FACTORY"
            } else {
                "SINGLE_POOL_ORDERED_ALLOCATION"
            }
            .into()
        }),
        execution_preparation: prepared,
        execution_route_scope: prepared.then(|| "SINGLE_POOL_SINGLE_ROUTE".into()),
        unsigned_transaction: prepared,
        rpc_simulation: prepared,
        account_creation: meteora
            || xone_domain::pump_amm::supports_profile(protocol)
            || protocol == xone_domain::pump::PROFILE,
        native_funding: meteora || xone_domain::pump_amm::supports_profile(protocol),
        native_sol_trading: protocol == xone_domain::pump::PROFILE,
        independent_simulation: prepared,
        wallet_submission: prepared,
        independent_production_verification: false,
        production_ready: false,
        execution_blocker: if profile.is_none() {
            Some("PROTOCOL_UNSUPPORTED".into())
        } else {
            (!prepared).then(|| "EXECUTION_PREPARATION_ADAPTER_NOT_IMPLEMENTED".into())
        },
    }
}
