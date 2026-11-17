//! Actual HTTP and same-executable worker over a retained original Router bank.
//! No external RPC, signing, or transaction submission.
#[path = "../../execution-preparation/tests/support/evm_rpc_fixture.rs"]
mod bank;
use serde_json::{json, Value};
use xone_domain::{hash, Snapshot};
use xone_simulation::{evm_fixture::*, worker::LocalSimulationBundle};

#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 2 && args[1] == "--evm-simulation-worker" {
        if let Err(error) = xone_simulation::worker::run_stdio() {
            eprintln!("{}", error.0);
            std::process::exit(1);
        }
        return;
    }
    assert_eq!(args.len(), 2, "INFO_PATH");
    let info_path = std::path::Path::new(&args[1]);
    let bundle_path = info_path.with_extension("state.json");
    let (bound, pools, rpc, original) = bank::fixture();
    let fixture: FixtureState = serde_json::from_value(json!({
        "schema_version":1,"state_anchor":original["state_anchor"],
        "execution_environment":original["execution_environment"],"accounts":original["accounts"],"block_hashes":[]
    })).unwrap();
    let original_tx: TransactionContext =
        serde_json::from_value(original["transaction_context"].clone()).unwrap();
    let max_fee = original_tx.gas_price.clone();
    let bundle = LocalSimulationBundle {
        schema_version: 1,
        executor: rpc.executor.clone(),
        fixture,
        transaction_context: TransactionContext {
            max_priority_fee_per_gas: Some("1".parse().unwrap()),
            ..original_tx
        },
    };
    std::fs::write(&bundle_path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "dynamic-fee-http-local-test-20261117".into();
    config.config.chains.retain(|c| c.key == rpc.chain_key);
    config.config.chains[0].genesis_hash = rpc.genesis_hash;
    config.config.simulation.backend = "revm-fixture".into();
    config.config.simulation.state_file = bundle_path.to_str().unwrap().into();
    config.config.simulation.allowed_targets = vec![rpc.executor.target];
    config.config.simulation.max_state_bytes = 8 * 1024 * 1024;
    config.config.simulation.max_state_reads = 8192;
    config.config.simulation.max_instructions = 2_000_000;
    config.config.simulation.max_memory_bytes = 64 * 1024 * 1024;
    config.config.simulation.timeout_ms = 15000;
    config.config.simulation.max_gas = 15_000_000;
    config.config.simulation.concurrency = 1;
    config.config.api.quote_timeout_ms = 15000;
    config.config.routing.quote_ttl_ms = 60000;
    config.config.routing.max_splits = 1;
    let request = json!({"chainKey":bound.plan.context.chain_key,"kind":"EXACT_IN",
        "sellToken":bound.plan.context.sell_token,"buyToken":bound.plan.context.buy_token,
        "sellAmount":bound.plan.allocation.sell_amount,"caller":bound.plan.context.caller,
        "recipient":bound.plan.context.recipient,"slippageBps":50,"verification":"FAST"});
    let snapshot = Snapshot {
        reference: bound.plan.snapshot,
        pools,
        source: "RETAINED_ORIGINAL_ROUTER_PROVIDED_BANK".into(),
        history_gaps: vec![],
    };
    let api = xone_quote_api::Api::new(config).unwrap();
    api.replica.install_baseline(snapshot).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let info: Value = json!({"api":format!("http://{}",listener.local_addr().unwrap()),
        "token":api.config.token,"request":request,"maxFeePerGas":max_fee,
        "executionBaseFee":bundle.fixture.execution_environment.base_fee,
        "bundleHash":hash(&bundle).unwrap(),"unsigned":true,"submitted":false});
    std::fs::write(info_path, serde_json::to_vec_pretty(&info).unwrap()).unwrap();
    axum::serve(listener, api.handler())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
