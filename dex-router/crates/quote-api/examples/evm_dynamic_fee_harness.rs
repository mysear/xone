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
    assert!((2..=3).contains(&args.len()), "INFO_PATH [FRONTEND_ORIGIN]");
    let info_path = std::path::Path::new(&args[1]);
    let bundle_path = info_path.with_extension("state.json");
    let (bound, mut pools, mut rpc, mut original) = bank::fixture();
    let mut context = bound.plan.context;
    let mut reference = bound.plan.snapshot;
    let mut sell_amount = bound.plan.allocation.sell_amount;
    let mut buy_amount = bound.plan.allocation.gross_buy_amount;
    if let Ok(case_id) = std::env::var("XONE_EVM_PATH_CASE") {
        let raw = if let Ok(path) = std::env::var("XONE_CANDIDATE_COST_CORPUS") {
            std::fs::read_to_string(path).unwrap()
        } else {
            include_str!("../../../test/contracts/v2-path-execution.json").into()
        };
        let doc: Value = serde_json::from_str(&raw).unwrap();
        let c = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == case_id)
            .expect("declared path case");
        let f = &c["fixture"];
        original = f.clone();
        original["transaction_context"] = c["transaction_context"].clone();
        pools = serde_json::from_value(c["pools"].clone()).unwrap();
        rpc.executor = serde_json::from_value(c["executor"].clone()).unwrap();
        rpc.genesis_hash = f["state_anchor"]["genesis_hash"].as_str().unwrap().into();
        context.caller = c["transaction"]["from"].as_str().unwrap().into();
        context.recipient = c["recipient"].as_str().unwrap().into();
        let tokens = c["path"].as_array().unwrap();
        context.sell_token = tokens[0].as_str().unwrap().into();
        context.buy_token = tokens.last().unwrap().as_str().unwrap().into();
        sell_amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        buy_amount = c["amount_out"].as_str().unwrap().parse().unwrap();
        reference.block.number_or_slot = f["state_anchor"]["block_number"].as_u64().unwrap();
        reference.block.hash = f["state_anchor"]["block_hash"].as_str().unwrap().into();
        reference.block.parent_hash = f["state_anchor"]["parent_hash"].as_str().unwrap().into();
        reference.block.timestamp_ms = f["state_anchor"]["timestamp"].as_u64().unwrap() * 1000;
        reference.scope_hash = xone_domain::scope_hash(&pools).unwrap();
        reference.adapter_set_hash = hash(&vec![xone_domain::v2_swap::ADAPTER]).unwrap();
    }
    let mut fixture: FixtureState = serde_json::from_value(json!({
        "schema_version":1,"state_anchor":original["state_anchor"],
        "execution_environment":original["execution_environment"],"accounts":original["accounts"],"block_hashes":[]
    })).unwrap();
    let native_alias = std::env::var("XONE_EVM_COINBASE_ALIAS").ok().as_deref() == Some("1");
    if native_alias {
        fixture.execution_environment.coinbase = context.caller.clone();
        fixture.execution_environment.block_hash =
            format!("0x{}", hash(&json!({
            "kind":"COUNTERFACTUAL_LOCAL_COINBASE_ALIAS",
            "source":fixture.state_anchor.block_hash,"environment":fixture.execution_environment
        })).unwrap());
    }
    let original_tx: TransactionContext =
        serde_json::from_value(original["transaction_context"].clone()).unwrap();
    let prepared_http = std::env::var("XONE_EVM_PREPARED_HTTP").ok();
    let mut rpc_task = None;
    if prepared_http.is_some() {
        assert!(
            std::env::var("XONE_EVM_PATH_CASE").is_err(),
            "prepared harness uses retained complete single bank"
        );
        let value = original["evidence"]["preblock_header"]["baseFeePerGas"]
            .as_str()
            .unwrap();
        let amount = u64::from_str_radix(&value[2..], 16).unwrap();
        fixture.state_anchor.base_fee_per_gas = Some(amount.to_string().parse().unwrap());
        let (_, _, ready, _, task) = bank::serve(prepared_http.as_deref().unwrap()).await;
        rpc = ready;
        rpc_task = Some(task);
    }
    let max_fee = original_tx.gas_price.clone();
    let mut bundle = LocalSimulationBundle {
        schema_version: 1,
        op_attributes_calldata: None,
        executor: rpc.executor.clone(),
        fixture,
        transaction_context: TransactionContext {
            max_priority_fee_per_gas: if std::env::var("XONE_EVM_LEGACY_FEE").ok().as_deref()
                == Some("1")
            {
                None
            } else {
                Some("1".parse().unwrap())
            },
            ..original_tx
        },
    };
    if let Ok(path) = std::env::var("XONE_OP_API_SOURCE") {
        assert!(
            prepared_http.is_none()
                && !native_alias
                && std::env::var("XONE_EVM_PATH_CASE").is_err()
        );
        let source: xone_simulation::worker::WorkerRequest =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let (ctx, snap, input, output, executor) = match (&source.bound, &source.exact_out_bound) {
            (Some(b), None) => (
                &b.plan.context,
                &b.plan.snapshot,
                &b.plan.allocation.sell_amount,
                &b.plan.allocation.gross_buy_amount,
                &b.executor,
            ),
            (None, Some(b)) => (
                &b.plan.context,
                &b.plan.snapshot,
                &b.plan.allocation.required_input,
                &b.plan.allocation.target_output,
                &b.executor,
            ),
            _ => panic!("one BOUND mode required"),
        };
        context = ctx.clone();
        reference = snap.clone();
        sell_amount = input.clone();
        buy_amount = output.clone();
        pools = source.pools.clone();
        rpc.chain_key = context.chain_key.clone();
        rpc.genesis_hash = source.fixture.state_anchor.genesis_hash.clone();
        rpc.executor = executor.clone();
        bundle = LocalSimulationBundle {
            schema_version: 1,
            fixture: source.fixture,
            executor: executor.clone(),
            transaction_context: source.transaction_context,
            op_attributes_calldata: Some(source.op_user.unwrap().attributes_calldata),
        };
    }
    std::fs::write(&bundle_path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "dynamic-fee-http-local-test-20261117".into();
    if bundle.op_attributes_calldata.is_some() {
        let mut chain = config
            .config
            .chains
            .iter()
            .find(|c| c.key.starts_with("evm:"))
            .unwrap()
            .clone();
        chain.key = rpc.chain_key.clone();
        config.config.chains = vec![chain];
    } else {
        config.config.chains.retain(|c| c.key == rpc.chain_key);
    }
    config.config.chains[0].genesis_hash = rpc.genesis_hash;
    if prepared_http.is_some() {
        config.config.chains[0].rpc_urls = vec![rpc.url.clone()];
        config.config.api.evm_preparation.insert(
            config.config.chains[0].key.clone(),
            xone_config::EvmPreparationConfig {
                router: rpc.executor.target.clone(),
                router_code_sha256: rpc.executor.code_sha256.clone(),
                factory: rpc.executor.factory.clone(),
                factory_code_sha256: rpc.executor.factory_code_sha256.clone(),
                token_code_sha256: rpc.token_code_sha256.clone(),
                timeout_ms: rpc.timeout_ms,
                max_calls: rpc.max_calls,
                max_response_bytes: rpc.max_response_bytes,
            },
        );
    }
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
    if let Ok(path) = std::env::var("XONE_NATIVE_VALUATION_CONFIG") {
        config.config.simulation.native_valuation =
            Some(serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap());
    }
    if let Ok(max) = std::env::var("XONE_CANDIDATE_COST_MAX_PLANS") {
        config.config.simulation.candidate_cost_max_plans = max.parse().unwrap();
    }
    config.config.api.quote_timeout_ms = 15000;
    config.config.routing.quote_ttl_ms = 60000;
    config.config.routing.max_splits = 1;
    let request = json!({"chainKey":context.chain_key,"kind":"EXACT_IN",
        "sellToken":context.sell_token,"buyToken":context.buy_token,
        "sellAmount":sell_amount,"caller":context.caller,
        "recipient":context.recipient,"slippageBps":50,"verification":"FAST"});
    let exact_out_request = json!({"chainKey":context.chain_key,"kind":"EXACT_OUT",
        "sellToken":context.sell_token,"buyToken":context.buy_token,
        "buyAmount":buy_amount,
        "maxSellAmount":xone_domain::Amount::checked(&sell_amount.0*2u64).unwrap(),
        "caller":context.caller,"recipient":context.recipient,
        "slippageBps":50,"verification":"FAST"});
    let snapshot = Snapshot {
        reference,
        pools,
        source: "RETAINED_ORIGINAL_ROUTER_PROVIDED_BANK".into(),
        history_gaps: vec![],
    };
    if args.len() == 3 {
        config.config.api.cors_allowed_origins = vec![args[2].clone()];
    }
    let api = xone_quote_api::Api::new(config).unwrap();
    api.replica.install_baseline(snapshot).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let info: Value = json!({"api":format!("http://{}",listener.local_addr().unwrap()),
        "token":api.config.token,"request":request,"exactOutRequest":exact_out_request,"maxFeePerGas":max_fee,
        "executionBaseFee":bundle.fixture.execution_environment.base_fee,
        "executionCoinbase":bundle.fixture.execution_environment.coinbase,
        "counterfactualCoinbaseAlias":native_alias,
        "bundleHash":hash(&bundle).unwrap(),"unsigned":true,"submitted":false});
    std::fs::write(info_path, serde_json::to_vec_pretty(&info).unwrap()).unwrap();
    axum::serve(listener, api.handler())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
    if let Some(task) = rpc_task {
        task.abort();
    }
}
