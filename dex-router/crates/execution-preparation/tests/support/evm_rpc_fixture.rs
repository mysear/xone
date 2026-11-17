#![allow(dead_code)]
use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use xone_domain::{hash, scope_hash, v2_swap, Amount, BlockRef, Pool, SnapshotRef};
use xone_execution_plan::{bind_v2_execution, build, BoundEvmExecution, Context, ExecutorIdentity};
use xone_execution_preparation::evm_rpc::{Request, RpcConfig};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};

fn corpus() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/contracts/v2-complete.json"
    )))
    .unwrap()
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn word(address: &str) -> String {
    format!("0x{}{}", "0".repeat(24), &address[2..])
}
pub fn fixture() -> (BoundEvmExecution, Vec<Pool>, RpcConfig, Value) {
    let doc = corpus();
    let c = &doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true && c["pool"]["protocol"] == v2_swap::PROFILE)
        .unwrap();
    let pool: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let pools = vec![pool.clone()];
    let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
    let route = AllocatedRoute {
        path: PathQuote {
            amount_in: input.clone(),
            amount_out: Amount::zero(),
            edges: vec![Edge {
                pool_id: pool.pool_id.clone(),
                protocol: pool.protocol.clone(),
                from_token: None,
                zero_for_one: c["token_in"] == pool.token0,
                to: c["token_out"].as_str().unwrap().into(),
            }],
            tokens: vec![
                c["token_in"].as_str().unwrap().into(),
                c["token_out"].as_str().unwrap().into(),
            ],
        },
        amount_in: input,
        amount_out: Amount::zero(),
    };
    let allocation = evaluate_allocation(&pools, &[route], &Amount::zero()).unwrap();
    let anchor = &c["state_anchor"];
    let reference = SnapshotRef {
        block: BlockRef {
            chain_key: pool.chain_key.clone(),
            number_or_slot: anchor["block_number"].as_u64().unwrap(),
            hash: anchor["block_hash"].as_str().unwrap().into(),
            parent_hash: anchor["parent_hash"].as_str().unwrap().into(),
            timestamp_ms: anchor["timestamp"].as_u64().unwrap() * 1000,
        },
        canonical_epoch: 1,
        journal_seq: 1,
        schema_version: 1,
        adapter_set_hash: hash(&vec![&pool.adapter_version]).unwrap(),
        online_generation: 1,
        scope_hash: scope_hash(&pools).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let context = Context {
        chain_key: pool.chain_key.clone(),
        caller: c["transaction"]["from"].as_str().unwrap().into(),
        recipient: c["recipient"].as_str().unwrap().into(),
        sell_token: c["token_in"].as_str().unwrap().into(),
        buy_token: c["token_out"].as_str().unwrap().into(),
        slippage_bps: 50,
        deadline_ms: now() + 60000,
    };
    let code = |address: &str| {
        c["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["address"] == address)
            .unwrap()["code"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let target = c["transaction"]["to"].as_str().unwrap();
    let executor = ExecutorIdentity {
        target: target.into(),
        code_sha256: format!(
            "{:x}",
            Sha256::digest(hex::decode(&code(target)[2..]).unwrap())
        ),
        factory: pool.v2.as_ref().unwrap().factory.clone(),
        factory_code_sha256: v2_swap::FACTORY_CODE_SHA256.into(),
        adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
    };
    let plan = build(&context, &allocation, &reference, "rpc-test").unwrap();
    let bound = bind_v2_execution(
        &plan,
        &pools,
        &executor,
        std::slice::from_ref(&executor.target),
    )
    .unwrap();
    let config = RpcConfig {
        url: String::new(),
        chain_key: pool.chain_key.clone(),
        genesis_hash: anchor["genesis_hash"].as_str().unwrap().into(),
        executor,
        token_code_sha256: [pool.token0.clone(), pool.token1.clone()]
            .into_iter()
            .map(|token| {
                let digest = format!(
                    "{:x}",
                    Sha256::digest(hex::decode(&code(&token)[2..]).unwrap())
                );
                (token, digest)
            })
            .collect(),
        timeout_ms: 15000,
        max_calls: 128,
        max_response_bytes: 2 * 1024 * 1024,
    };
    (bound, pools, config, (*c).clone())
}
pub fn request() -> Request {
    Request {
        gas_limit: 700000,
        max_fee_per_gas: "2000000000".parse().unwrap(),
        max_priority_fee_per_gas: "1000000000".parse().unwrap(),
        max_network_fee: "1400000000000000".parse().unwrap(),
    }
}
#[derive(Clone)]
struct Server {
    case: Value,
    config: RpcConfig,
    mode: String,
    requests: Arc<Mutex<Vec<Value>>>,
}
async fn respond(State(s): State<Server>, Json(q): Json<Value>) -> Json<Value> {
    s.requests.lock().unwrap().push(q.clone());
    let count = s.requests.lock().unwrap().len();
    if s.mode == "timeout" {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let method = q["method"].as_str().unwrap();
    let params = &q["params"];
    let block = s.case["state_anchor"]["block_hash"].as_str().unwrap();
    let anchor = params
        .as_array()
        .unwrap()
        .last()
        .filter(|p| p.is_object() && p.get("blockHash").is_some());
    let ignore_selector = s.mode == "ignored_selector"
        || s.mode == "ignored_code" && method == "eth_getCode"
        || s.mode == "ignored_storage" && method == "eth_getStorageAt"
        || s.mode == "ignored_call" && method == "eth_call"
        || s.mode == "ignored_nonce" && method == "eth_getTransactionCount";
    let result = if anchor.is_some_and(|p| p["blockHash"] != block) && !ignore_selector {
        return Json(
            json!({"jsonrpc":"2.0","id":q["id"],"error":{"code":-32000,"message":"unknown block"}}),
        );
    } else {
        match method {
            "eth_chainId" => json!(if s.mode == "wrong_chain" {
                "0x1"
            } else {
                "0x7a69"
            }),
            "eth_getBlockByNumber" => {
                if params[0] == "0x0" {
                    json!({"number":"0x0","hash":if s.mode=="wrong_genesis"{format!("0x{}","11".repeat(32))}else{s.config.genesis_hash.clone()}})
                } else {
                    let mut h = s.case["evidence"]["preblock_header"].clone();
                    if s.mode == "reorg" && count > 15 {
                        h["hash"] = format!("0x{}", "22".repeat(32)).into();
                    }
                    h
                }
            }
            "eth_getCode" => {
                if s.mode == "caller_contract" && params[0] == s.case["transaction"]["from"] {
                    json!("0x6000")
                } else {
                    let mut code = s.case["accounts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|a| a["address"] == params[0])
                        .map(|a| a["code"].clone())
                        .unwrap_or(json!("0x"));
                    if s.mode == "code" && params[0] == s.config.executor.target {
                        code = "0x6000".into();
                    }
                    code
                }
            }
            "eth_getStorageAt" => json!(if s.mode == "lock" {
                format!("0x{}", "0".repeat(64))
            } else {
                format!("0x{}1", "0".repeat(63))
            }),
            "eth_getTransactionCount" => {
                let caller = s.case["accounts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|a| a["address"] == s.case["transaction"]["from"])
                    .unwrap();
                let mut nonce = caller["nonce"].as_u64().unwrap();
                if s.mode == "pending" && params[1] == "pending" {
                    nonce += 1;
                }
                if s.mode == "nonce_changed"
                    && params[1] == "pending"
                    && s.requests
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|q| {
                            q["method"] == "eth_getTransactionCount" && q["params"][1] == "pending"
                        })
                        .count()
                        > 1
                {
                    nonce += 1;
                }
                json!(format!("0x{nonce:x}"))
            }
            "eth_getBalance" => json!(if s.mode == "funding" {
                "0x0"
            } else {
                "0x3635c9adc5dea00000"
            }),
            "eth_call" => {
                let call = &params[0];
                let mut found = None;
                for group in s.case["evidence"]["pre_getters"].as_array().unwrap() {
                    for entry in group["calls"].as_array().unwrap() {
                        if entry["to"] == call["to"] && entry["data"] == call["data"] {
                            found = Some(entry["result"].clone());
                        }
                    }
                }
                if call["to"] == s.config.executor.target && call["data"] == "0xc45a0155" {
                    found = Some(json!(word(&s.config.executor.factory)));
                }
                if found.is_none() && call["data"].as_str().unwrap().starts_with("0x70a08231") {
                    found = Some(json!(format!("0x{}", "f".repeat(64))));
                }
                if call["data"].as_str().unwrap().starts_with("0xdd62ed3e") {
                    found = Some(json!(if s.mode == "allowance" {
                        format!("0x{}", "0".repeat(64))
                    } else {
                        format!("0x{}", "f".repeat(64))
                    }));
                }
                let mut value = found.expect("All RPC calls must be covered by the test source");
                if s.mode == "reserves" && call["data"] == "0x0902f1ac" {
                    value = format!("0x{}", "0".repeat(192)).into();
                }
                value
            }
            _ => panic!("Unexpected method {method}"),
        }
    };
    let id = if s.mode == "id" {
        json!(999)
    } else {
        q["id"].clone()
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result}))
}
pub async fn serve(
    mode: &str,
) -> (
    BoundEvmExecution,
    Vec<Pool>,
    RpcConfig,
    Arc<Mutex<Vec<Value>>>,
    tokio::task::JoinHandle<()>,
) {
    let (b, p, mut c, case) = fixture();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let state = Server {
        case,
        config: c.clone(),
        mode: mode.into(),
        requests: requests.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    c.url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", post(respond)).with_state(state),
        )
        .await
        .unwrap();
    });
    (b, p, c, requests, task)
}
