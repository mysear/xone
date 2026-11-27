//! Exercise normal quote and simulation HTTP handlers with the same-executable worker.
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
use xone_domain::{Pool, Snapshot, SnapshotRef};
use xone_simulation::worker::LocalSimulationBundle;

#[path = "singleton_replay/node.rs"]
mod node;
#[path = "singleton_replay/recovery.rs"]
mod recovery;

#[tokio::main]
async fn main() {
    let path = std::env::args_os().nth(1).expect("replay case directory");
    if path == "--evm-simulation-worker" {
        xone_simulation::worker::run_stdio().unwrap();
        return;
    }
    if path == "--serve-recovery-fixture" {
        recovery::serve(std::env::args_os().skip(2)).await;
        return;
    }
    let dir = Path::new(&path);
    let anvil = std::env::var_os("XONE_SINGLETON_ANVIL");
    let geth = std::env::var_os("XONE_SINGLETON_GETH");
    let geth_last = std::env::var_os("XONE_SINGLETON_GETH_LAST");
    assert!(
        [&anvil, &geth, &geth_last].into_iter().flatten().count() <= 1,
        "select one receipt node"
    );
    let node = anvil
        .as_deref()
        .map(node::Executable::Anvil)
        .or_else(|| {
            geth.as_deref()
                .map(|path| node::Executable::Geth(path, false))
        })
        .or_else(|| {
            geth_last
                .as_deref()
                .map(|path| node::Executable::Geth(path, true))
        });
    let temporary = tempfile::tempdir().unwrap();
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let case: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_replay(&case, temporary.path(), false, node).await;
        assert_replay(&case, temporary.path(), true, None).await;
        count += 1;
    }
    assert_eq!(count, 24);
    println!("{count} unsigned and {count} signed HTTP replays passed, with {count} missing-state and {count} nonce rejections");
    if geth_last.is_some() {
        println!(
            "{count} Geth final-transaction receipts passed with withdrawal credits reconciled"
        );
    } else if geth.is_some() {
        println!("{count} Geth transaction-index receipts in blocks with withdrawals passed, with gas, log, balance, reorg, transaction index, beacon root, beacon storage and withdrawal list rejection cases");
    } else if anvil.is_some() {
        println!("{count} mined Cancun receipts passed, with wrong gas, logs, balance and reorg rejection per receipt");
    }
}

async fn assert_replay(
    case: &Value,
    dir: &Path,
    incomplete: bool,
    node: Option<node::Executable<'_>>,
) {
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
    let mined = match node {
        Some(executable) => {
            let node = node::Node::start(executable, dir, &bundle, now).await;
            Some(
                node.mine(case, &mut bundle, &mut reference, &mut pool, now)
                    .await,
            )
        }
        None => None,
    };
    if let Some(state) = &mut pool.infinity_bin {
        state.timestamp = bundle.fixture.state_anchor.timestamp;
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
    let rpc = match &mined {
        Some(mined) => mined.router(),
        None => replay_rpc(&bundle, &reference, &case["vault"]),
    };
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
    if let Some(mined) = &mined {
        config.config.routing.quote_ttl_ms = mined.quote_ttl_ms();
        if mined.transaction_index() != 0 {
            config.config.simulation.evm_cancun_verified_chain = pool.chain_key.clone();
            config.config.api.quote_timeout_ms = 15_000;
        }
    }
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
    let (status, quote) = call(path.into(), request.clone()).await;
    assert_eq!(status, 200, "{quote}");
    if let Some(mined) = mined.as_ref().filter(|node| node.transaction_index() != 0) {
        if request["sellToken"] != xone_domain::singleton::ZERO_ADDRESS
            && request["buyToken"] != xone_domain::singleton::ZERO_ADDRESS
        {
            request["verification"] = "VERIFIED".into();
            mined.inject(node::Fault::SimulationGas);
            let (status, rejected) = call(path.into(), request.clone()).await;
            assert_eq!(status, 422, "{rejected}");
            assert_eq!(rejected["error"], "SIMULATION_FAILED");
            mined.clear_fault();
            let (status, verified) = call(path.into(), request.clone()).await;
            assert_eq!(status, 200, "{verified}");
            assert_eq!(verified["verification"]["level"], "VERIFIED");
            assert_eq!(verified["verification"]["success"], true);
            assert_eq!(verified["snapshotRef"], quote["snapshotRef"]);
            assert_eq!(verified["grossBuyAmount"], quote["grossBuyAmount"]);
            let status_request = Request::get(format!(
                "/internal/chains/{}/status",
                request["chainKey"].as_str().unwrap()
            ))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
            let timing = api.handler().oneshot(status_request).await.unwrap();
            assert_eq!(timing.status(), 200);
            let bytes = axum::body::to_bytes(timing.into_body(), 1 << 20)
                .await
                .unwrap();
            let timing: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(
                timing["recovery"]["first_verified_quote_ms"]
                    .as_u64()
                    .unwrap()
                    >= timing["recovery"]["first_successful_quote_ms"]
                        .as_u64()
                        .unwrap()
            );
            assert_eq!(
                timing["recovery"]["first_verified_quote_generation"],
                verified["onlineGeneration"]
            );
            let (status, cached) = call(path.into(), request.clone()).await;
            assert_eq!(status, 200, "{cached}");
            assert_eq!(cached, verified);
            let verified_id = verified["quoteId"].as_str().unwrap();
            let (status, build) = call(format!("/v1/quotes/{verified_id}/build"), json!({})).await;
            assert_eq!(status, 200, "{build}");
            assert_eq!(build["independentExecutionVerified"], true);
            mined.inject(node::Fault::SourceReorg);
            let (status, rejected) = call(path.into(), request.clone()).await;
            assert_eq!(status, 409, "{rejected}");
            assert_eq!(rejected["error"], "SNAPSHOT_REORGED");
            mined.inject(node::Fault::SimulationAnchorChanged);
            let (status, rejected) = call(path.into(), request).await;
            assert_eq!(status, 409, "{rejected}");
            assert_eq!(rejected["error"], "SNAPSHOT_REORGED");
            let (status, rejected) =
                call(format!("/v1/quotes/{verified_id}/build"), json!({})).await;
            assert_eq!(status, 409, "{rejected}");
            assert_eq!(rejected["error"], "SNAPSHOT_REORGED");
            mined.clear_fault();
        }
    }
    let path = format!("/v1/quotes/{}", quote["quoteId"].as_str().unwrap());
    let (status, build) = call(format!("{path}/build"), json!({})).await;
    assert_eq!(status, 200, "{build}");
    let (status, replay) = call(format!("{path}/simulate"), json!({})).await;
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
    let sign = |nonce| sign_transaction(&build["transaction"], &bundle, nonce);
    let nonce = bundle.transaction_context.nonce;
    let signed = sign(nonce);
    let (status, replay) = call(
        format!("{path}/simulate"),
        json!({"signedEvmTransaction":signed}),
    )
    .await;
    assert_eq!(status, 200, "{replay}");
    assert_eq!(replay["verification"]["level"], "LOCAL_SIGNED_EVM_EXECUTED");
    assert_eq!(
        replay["outcome"]["signed_transaction_hash"],
        signed["transactionHash"]
    );
    assert_eq!(replay["outcome"]["actual_input_consumed"], case["input"]);
    assert_eq!(replay["outcome"]["actual_buy_amount"], case["output"]);
    if let Some(mined) = &mined {
        assert_eq!(signed, mined.signed);
        if mined.transaction_index() != 0 {
            let request = json!({"checkEvmSimulation":true});
            let (status, simulated) = call(format!("{path}/simulate"), request.clone()).await;
            assert_eq!(status, 200, "{simulated}");
            assert_eq!(
                simulated["nodeSimulation"]["sourceStateRoot"],
                mined.source_state_root
            );
            assert_eq!(
                simulated["nodeSimulation"]["gasUsed"],
                replay["outcome"]["execution"]["gas_used"]
            );
            assert_eq!(simulated["verified"], false);
            if context["sell_token"] != xone_domain::singleton::ZERO_ADDRESS
                && context["buy_token"] != xone_domain::singleton::ZERO_ADDRESS
            {
                assert_eq!(
                    simulated["nodeSimulation"]["settlement"]["actualInputConsumed"],
                    case["input"]
                );
                assert_eq!(
                    simulated["nodeSimulation"]["settlement"]["actualBuyAmount"],
                    case["output"]
                );
                mined.inject(node::Fault::SimulationBalance);
                let (status, rejected) = call(format!("{path}/simulate"), request.clone()).await;
                assert_eq!(status, 422, "{rejected}");
                assert_eq!(rejected["error"], "EVM_NODE_SIMULATION_MISMATCH");
            }
            mined.inject(node::Fault::SimulationGas);
            let (status, rejected) = call(format!("{path}/simulate"), request).await;
            assert_eq!(status, 422, "{rejected}");
            assert_eq!(rejected["error"], "EVM_NODE_SIMULATION_MISMATCH");
        }
        let receipt_request = json!({"signedEvmTransaction":signed,"checkEvmReceipt":true});
        let (status, observed) = call(format!("{path}/simulate"), receipt_request.clone()).await;
        assert_eq!(status, 200, "{observed}");
        assert_eq!(
            observed["nodeReceipt"]["authority"],
            "CANONICAL_RPC_OBSERVED"
        );
        assert_eq!(observed["nodeReceipt"]["scope"], mined.scope());
        assert_eq!(
            observed["nodeReceipt"]["transactionIndex"],
            mined.transaction_index()
        );
        assert_eq!(observed["nodeReceipt"]["success"], true);
        assert_eq!(observed["nodeReceipt"]["receiptsRoot"], mined.receipts_root);
        assert_eq!(
            observed["nodeReceipt"]["transactionsRoot"],
            mined.transactions_root
        );
        assert_eq!(
            observed["nodeReceipt"]["sourceStateRoot"],
            mined.source_state_root
        );
        assert_eq!(
            observed["nodeReceipt"]["executionStateRoot"],
            mined.execution_state_root
        );
        assert_eq!(
            observed["nodeReceipt"]["transactionHash"],
            signed["transactionHash"]
        );
        assert_eq!(
            observed["nodeReceipt"]["actualInputConsumed"],
            case["input"]
        );
        assert_eq!(observed["nodeReceipt"]["actualBuyAmount"], case["output"]);
        assert_eq!(
            observed["nodeReceipt"]["payerNativeBefore"],
            observed["outcome"]["native_fees"]["caller_before"]
        );
        assert_eq!(
            observed["nodeReceipt"]["payerNativeAfter"],
            observed["outcome"]["native_fees"]["caller_after"]
        );
        assert_eq!(observed["verified"], false);
        let mut faults = vec![
            node::Fault::Gas,
            node::Fault::Logs,
            node::Fault::ReceiptsMissing,
            node::Fault::ReceiptBloom,
            node::Fault::TransactionMissing,
            node::Fault::TransactionWire,
            node::Fault::SourceReorg,
            node::Fault::SourceRoot,
            node::Fault::SourceProofMissing,
            node::Fault::SourceStorage,
            node::Fault::AncestorMissing,
            node::Fault::AncestorParent,
            node::Fault::Balance,
            node::Fault::PayerBalance,
            node::Fault::Reorg,
        ];
        if mined.transaction_index() != 0 {
            assert_eq!(
                observed["nodeReceipt"]["withdrawalsRoot"],
                mined.withdrawals_root.as_deref().unwrap()
            );
            assert_eq!(
                observed["nodeReceipt"]["parentBeaconBlockRoot"],
                bundle
                    .fixture
                    .execution_environment
                    .parent_beacon_block_root
                    .as_ref()
                    .unwrap()
                    .as_str()
            );
            faults.extend([
                node::Fault::ReceiptsOrder,
                node::Fault::TransactionBody,
                node::Fault::IgnoreTraceIndex,
                node::Fault::BeaconRoot,
                node::Fault::BeaconStorage,
                node::Fault::BeaconStorageProof,
                node::Fault::WithdrawalsList,
                node::Fault::WithdrawalsAmount,
                node::Fault::WithdrawalsRoot,
            ]);
        }
        if mined.scope() == "TRANSACTION_BEFORE_WITHDRAWALS" {
            faults.push(node::Fault::WithdrawalBalance);
        }
        for fault in faults {
            mined.inject(fault);
            let (status, rejected) =
                call(format!("{path}/simulate"), receipt_request.clone()).await;
            assert_eq!(status, 422, "{fault:?}: {rejected}");
            assert_eq!(rejected["phase"], "node_receipt", "{fault:?}: {rejected}");
            let expected = match fault {
                node::Fault::Reorg | node::Fault::SourceReorg => "EVM_NODE_RECEIPT_REORGED",
                node::Fault::WithdrawalBalance => {
                    "EVM_NODE_RECEIPT_PRE_WITHDRAWAL_STATE_UNAVAILABLE"
                }
                node::Fault::SourceProofMissing | node::Fault::SourceStorage => {
                    "EVM_NODE_RECEIPT_SOURCE_STATE_MISMATCH"
                }
                node::Fault::AncestorMissing => "EVM_NODE_RECEIPT_ANCESTOR_MISMATCH",
                node::Fault::BeaconStorage | node::Fault::BeaconStorageProof => {
                    "EVM_NODE_RECEIPT_BEACON_STATE_MISMATCH"
                }
                _ => "EVM_NODE_RECEIPT_MISMATCH",
            };
            assert_eq!(rejected["error"], expected);
        }
    }
    let (status, rejected) = call(
        format!("{path}/simulate"),
        json!({"signedEvmTransaction":sign(nonce+1)}),
    )
    .await;
    assert_eq!(status, 422, "{rejected}");
    assert_eq!(rejected["phase"], "singleton_signed");
    rpc_task.abort();
}

fn sign_transaction(transaction: &Value, bundle: &LocalSimulationBundle, nonce: u64) -> Value {
    // The fixture funds the public test account derived from scalar 1.
    sign_transaction_with_key(transaction, bundle, nonce, 1)
}

fn sign_transaction_with_key(
    transaction: &Value,
    bundle: &LocalSimulationBundle,
    nonce: u64,
    scalar: u8,
) -> Value {
    use secp256k1::{Message, Secp256k1, SecretKey};
    use tiny_keccak::{Hasher, Keccak};

    fn encode(bytes: &[u8], list: bool) -> Vec<u8> {
        if !list && bytes.len() == 1 && bytes[0] < 128 {
            return bytes.to_vec();
        }
        let base = if list { 0xc0 } else { 0x80 };
        let mut result = if bytes.len() < 56 {
            vec![base + bytes.len() as u8]
        } else {
            let size = bytes.len().to_be_bytes();
            let start = size.iter().position(|&byte| byte != 0).unwrap();
            let mut result = vec![base + 55 + (size.len() - start) as u8];
            result.extend_from_slice(&size[start..]);
            result
        };
        result.extend_from_slice(bytes);
        result
    }
    let integer = |bytes: &[u8]| {
        let start = bytes
            .iter()
            .position(|&byte| byte != 0)
            .unwrap_or(bytes.len());
        encode(&bytes[start..], false)
    };
    let quantity = |amount: &xone_domain::Amount| integer(&amount.to_be_bytes().unwrap());
    let hex_data = |field: &str| hex::decode(&transaction[field].as_str().unwrap()[2..]).unwrap();
    let hash = |bytes: &[u8]| {
        let mut output = [0; 32];
        let mut keccak = Keccak::v256();
        keccak.update(bytes);
        keccak.finalize(&mut output);
        output
    };
    let context = &bundle.transaction_context;
    let mut body = [
        integer(&context.chain_id.to_be_bytes()),
        integer(&nonce.to_be_bytes()),
        quantity(context.max_priority_fee_per_gas.as_ref().unwrap()),
        quantity(&context.gas_price),
        integer(&context.gas_limit.to_be_bytes()),
        encode(&hex_data("to"), false),
        quantity(&serde_json::from_value(transaction["value"].clone()).unwrap()),
        encode(&hex_data("data"), false),
        encode(&[], true),
    ]
    .concat();
    let envelope = |body: &[u8]| [vec![2], encode(body, true)].concat();
    let mut key = [0; 32];
    key[31] = scalar;
    let signature = Secp256k1::new().sign_ecdsa_recoverable(
        &Message::from_digest(hash(&envelope(&body))),
        &SecretKey::from_slice(&key).unwrap(),
    );
    let (parity, signature) = signature.serialize_compact();
    for field in [
        &[parity.to_i32() as u8][..],
        &signature[..32],
        &signature[32..],
    ] {
        body.extend(integer(field));
    }
    let wire = envelope(&body);
    json!({"transactionHash":format!("0x{}",hex::encode(hash(&wire))),
           "signedWire":format!("0x{}",hex::encode(wire))})
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
