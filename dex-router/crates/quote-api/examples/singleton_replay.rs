//! Exercise normal quote and simulation HTTP handlers with the same-executable worker.
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
use xone_domain::{Pool, Snapshot, SnapshotRef};
use xone_simulation::worker::LocalSimulationBundle;

#[tokio::main]
async fn main() {
    let path = std::env::args_os().nth(1).expect("replay case directory");
    if path == "--evm-simulation-worker" {
        xone_simulation::worker::run_stdio().unwrap();
        return;
    }
    let dir = Path::new(&path);
    let temporary = tempfile::tempdir().unwrap();
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let case: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_replay(&case, temporary.path(), false).await;
        assert_replay(&case, temporary.path(), true).await;
        count += 1;
    }
    assert_eq!(count, 24);
    println!("{count} HTTP replays and {count} incomplete-state rejections passed");
}

async fn assert_replay(case: &Value, dir: &Path, incomplete: bool) {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let mut bundle: LocalSimulationBundle = serde_json::from_value(case["bundle"].clone()).unwrap();
    let mut reference: SnapshotRef = serde_json::from_value(case["snapshot"].clone()).unwrap();
    let mut pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    bundle.fixture.state_anchor.timestamp = now;
    bundle.fixture.execution_environment.timestamp = now + 1;
    reference.block.timestamp_ms = now * 1000;
    if let Some(state) = &mut pool.infinity_bin {
        state.timestamp = now;
    }
    let manager = pool.pool_id.split_once('/').unwrap().0.to_owned();
    if incomplete {
        bundle
            .fixture
            .accounts
            .iter_mut()
            .find(|account| account.address == manager)
            .unwrap()
            .storage
            .clear();
    }
    let bundle_path = dir.join("state.json");
    std::fs::write(&bundle_path, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "singleton-replay-http-test".into();
    config
        .config
        .chains
        .retain(|chain| chain.key == pool.chain_key);
    config.config.chains[0].genesis_hash = bundle.fixture.state_anchor.genesis_hash.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    config.config.chains[0].rpc_urls = vec![format!("http://{}", listener.local_addr().unwrap())];
    let rpc = replay_rpc(&bundle, &reference, &case["vault"]);
    let rpc_task = tokio::spawn(async move { axum::serve(listener, rpc).await.unwrap() });
    let transaction = xone_config::V4ExecutionConfig {
        executor: case["executor"].as_str().unwrap().into(),
        executor_code_sha256: case["executorCodeSha256"].as_str().unwrap().into(),
        token_code_sha256: serde_json::from_value(case["tokenDigests"].clone()).unwrap(),
    };
    let sim = &mut config.config.simulation;
    sim.backend = "revm-fixture".into();
    sim.state_file = bundle_path.to_str().unwrap().into();
    sim.allowed_targets = vec![transaction.executor.clone()];
    sim.max_state_bytes = 8 * 1024 * 1024;
    sim.max_state_reads = 8192;
    sim.max_instructions = 2_000_000;
    sim.max_memory_bytes = 4 * 1024 * 1024;
    sim.max_gas = 2_500_000;
    sim.timeout_ms = 15_000;
    sim.concurrency = 1;
    if case["vault"].is_null() {
        config
            .config
            .api
            .v4_execution
            .insert(pool.chain_key.clone(), transaction);
    } else {
        let (vault, vault_code_sha256) = serde_json::from_value(case["vault"].clone()).unwrap();
        config.config.api.infinity_execution.insert(
            pool.chain_key.clone(),
            BTreeMap::from([(
                manager,
                xone_config::InfinityExecutionConfig {
                    vault,
                    vault_code_sha256,
                    transaction,
                },
            )]),
        );
    }
    let token = config.token.clone();
    let api = xone_quote_api::Api::new(config).unwrap();
    api.replica
        .install_baseline(Snapshot {
            reference,
            pools: vec![pool],
            source: "LOCAL_SINGLETON_REPLAY".into(),
            history_gaps: vec![],
        })
        .unwrap();
    let call = |path: String, body: Value| {
        let request = Request::post(path)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let handler = api.handler();
        async move {
            let resp = handler.oneshot(request).await.unwrap();
            let status = resp.status();
            let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
                .await
                .unwrap();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        }
    };
    let context = &case["context"];
    let kind = &case["quoteKind"];
    let mut request = json!({
        "chainKey":context["chain_key"],"kind":kind,"caller":context["caller"],
        "recipient":context["recipient"],"sellToken":context["sell_token"],"buyToken":context["buy_token"],
        "slippageBps":context["slippage_bps"],"verification":"FAST"
    });
    let exact_in = kind == "EXACT_IN";
    let path = if exact_in {
        request["sellAmount"] = case["input"].clone();
        "/v1/quote"
    } else {
        request["buyAmount"] = case["output"].clone();
        request["maxSellAmount"] = json!("1100000");
        "/v1/quote-exact-out"
    };
    let (status, quote) = call(path.into(), request).await;
    assert_eq!(status, 200, "{quote}");
    let path = format!("/v1/quotes/{}", quote["quoteId"].as_str().unwrap());
    let (status, build) = call(format!("{path}/build"), json!({})).await;
    assert_eq!(status, 200, "{build}");
    let (status, replay) = call(format!("{path}/simulate"), json!({})).await;
    rpc_task.abort();
    if incomplete {
        assert_eq!(status, 422, "{replay}");
        assert_eq!(
            replay["error"], "SIMULATION_EXECUTION_UNAVAILABLE",
            "{replay}"
        );
        return;
    }
    assert_eq!(status, 200, "{replay}");
    assert_eq!(replay["bindingHash"], build["binding"]["binding_hash"]);
    assert_eq!(replay["transaction"], build["transaction"]);
    assert_eq!(replay["outcome"]["actual_input_consumed"], case["input"]);
    assert_eq!(replay["outcome"]["actual_buy_amount"], case["output"]);
    let calldata = |data: &Value| hex::decode(&data.as_str().unwrap()[2..]).unwrap();
    let original = calldata(&case["calldata"]);
    let built = calldata(&build["transaction"]["data"]);
    assert_eq!(built.len(), original.len());
    let deadline = built.len() - 32;
    assert_eq!(built[..deadline], original[..deadline]);
    let intrinsic_data_gas = |data: &[u8]| -> u64 {
        data.iter()
            .map(|&byte| if byte == 0 { 4 } else { 16 })
            .sum()
    };
    assert_eq!(
        replay["outcome"]["execution"]["gas_used"].as_u64().unwrap() - intrinsic_data_gas(&built),
        case["gas"].as_u64().unwrap() - intrinsic_data_gas(&original),
    );
    assert_eq!(replay["verification"]["success"], true);
    assert_eq!(replay["verified"], false);
}

fn replay_rpc(
    bundle: &LocalSimulationBundle,
    reference: &SnapshotRef,
    vault: &Value,
) -> axum::Router {
    use std::sync::Arc;
    let state = Arc::new((
        bundle.fixture.state_anchor.genesis_hash.clone(),
        json!({
            "hash":reference.block.hash,"parentHash":reference.block.parent_hash,
            "number":format!("0x{:x}",reference.block.number_or_slot),
            "timestamp":format!("0x{:x}",reference.block.timestamp_ms / 1000),
            "stateRoot":format!("0x{}", "11".repeat(32)), "baseFeePerGas":"0xa",
        }),
        bundle
            .fixture
            .accounts
            .iter()
            .map(|account| (account.address.clone(), account.code.clone()))
            .collect::<BTreeMap<_, _>>(),
        vault[0].as_str().map(str::to_owned),
    ));
    axum::Router::new().route(
        "/",
        axum::routing::post(move |axum::Json(request): axum::Json<Value>| {
            let state = state.clone();
            async move {
                let (genesis, block, code, vault) = state.as_ref();
                let params = &request["params"];
                let result = match request["method"].as_str().unwrap() {
                    "eth_chainId" => json!("0x7a69"),
                    "eth_getBlockByNumber" if params[0] == "0x0" => json!({"hash":genesis}),
                    "eth_getBlockByNumber" => block.clone(),
                    "eth_getCode" | "eth_call" => {
                        assert_eq!(params[1]["blockHash"], block["hash"]);
                        assert_eq!(params[1]["requireCanonical"], true);
                        if request["method"] == "eth_getCode" {
                            json!(code[params[0].as_str().unwrap()])
                        } else {
                            let calldata = params[0]["data"].as_str().unwrap();
                            if calldata == "0xfbfa77cf" {
                                json!(format!("0x{:0>64}", &vault.as_ref().unwrap()[2..]))
                            } else {
                                assert!(calldata.starts_with("0x8403be91"));
                                assert_eq!(params[0]["to"].as_str(), vault.as_deref());
                                json!(format!("0x{:064x}", 1))
                            }
                        }
                    }
                    method => panic!("unexpected replay RPC method {method}"),
                };
                axum::Json(json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
            }
        }),
    )
}
