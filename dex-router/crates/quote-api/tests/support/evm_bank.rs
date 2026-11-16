#[path = "../../../execution-preparation/tests/support/evm_rpc_fixture.rs"]
pub mod source;
use serde_json::{json, Value};
use xone_config::EvmPreparationConfig;
use xone_domain::Snapshot;
use xone_quote_api::Api;
pub async fn setup(
    mode: &str,
) -> (
    Api,
    Snapshot,
    Value,
    std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    tokio::task::JoinHandle<()>,
) {
    let (bound, pools, rpc, observations, task) = source::serve(mode).await;
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "evm-rpc-local-test-20261117".into();
    config.config.chains.retain(|c| c.key == rpc.chain_key);
    let chain = &mut config.config.chains[0];
    chain.rpc_urls = vec![rpc.url];
    chain.genesis_hash = rpc.genesis_hash;
    config.config.api.evm_preparation.insert(
        chain.key.clone(),
        EvmPreparationConfig {
            router: rpc.executor.target,
            router_code_sha256: rpc.executor.code_sha256,
            factory: rpc.executor.factory,
            factory_code_sha256: rpc.executor.factory_code_sha256,
            token_code_sha256: rpc.token_code_sha256,
            timeout_ms: rpc.timeout_ms,
            max_calls: rpc.max_calls,
            max_response_bytes: rpc.max_response_bytes,
        },
    );
    config.config.api.quote_timeout_ms = 10000;
    config.config.routing.quote_ttl_ms = 60000;
    config.config.routing.max_splits = 1;
    let snapshot = Snapshot {
        reference: bound.plan.snapshot,
        pools,
        source: "RETAINED_ORIGINAL_POOL_LOCAL_RPC".into(),
        history_gaps: Vec::new(),
    };
    let request = json!({"chainKey":bound.plan.context.chain_key,"kind":"EXACT_IN","sellToken":bound.plan.context.sell_token,"buyToken":bound.plan.context.buy_token,"sellAmount":bound.plan.allocation.sell_amount,"caller":bound.plan.context.caller,"recipient":bound.plan.context.recipient,"slippageBps":50,"verification":"FAST"});
    let api = Api::new(config).unwrap();
    api.replica.install_baseline(snapshot.clone()).unwrap();
    (api, snapshot, request, observations, task)
}
