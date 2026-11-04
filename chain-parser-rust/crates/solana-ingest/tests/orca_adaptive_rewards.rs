use xone_solana_ingest::{orca, raydium::RpcAccount};
#[test]
fn adaptive_rewards_twenty_native_packed_banks_and_old_profile_rejections() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use xone_config::{Chain, PoolConfig};
    use xone_domain::Pool;
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-service.json"
    ))
    .unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-pools.json"
    ))
    .unwrap();
    let mut count = 0;
    for scenario in native["scenarios"].as_array().unwrap() {
        for (index, bank) in scenario["banks"].as_array().unwrap().iter().enumerate() {
            let name = format!("{}/{index}", scenario["name"].as_str().unwrap());
            let case = expected["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == name)
                .unwrap();
            let cfg: PoolConfig = serde_json::from_value(case["config"].clone()).unwrap();
            let ch:Chain=serde_json::from_value(serde_json::json!({"key":"solana:localnet","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"localnet","rpc_urls":[],"pools":[],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
            let accounts = bank["accounts"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, v)| {
                    (
                        id.clone(),
                        RpcAccount {
                            owner: v["owner"].as_str().unwrap().into(),
                            data: STANDARD.decode(v["data"][0].as_str().unwrap()).unwrap(),
                            executable: v["executable"].as_bool().unwrap(),
                            lamports: v["lamports"].as_u64().unwrap(),
                        },
                    )
                })
                .collect();
            let p =
                orca::decode_orca_whirlpool(&ch, &cfg, &accounts, bank["slot"].as_u64().unwrap())
                    .unwrap();
            let wanted: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
            assert_eq!(p, wanted, "{name}");
            for old in [
                "orca-whirlpools-adaptive",
                "orca-whirlpools-static",
                "orca-whirlpools-rewards",
            ] {
                let mut q = cfg.clone();
                q.protocol = old.into();
                assert!(
                    orca::decode_orca_whirlpool(&ch, &q, &accounts, bank["slot"].as_u64().unwrap())
                        .is_err(),
                    "{old}"
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 20);
}

use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
#[derive(Clone)]
struct Server {
    bank: Value,
    calls: Arc<Mutex<Vec<Value>>>,
    change_anchor: bool,
}
async fn serve(State(s): State<Server>, Json(q): Json<Value>) -> Json<Value> {
    let mut calls = s.calls.lock().unwrap();
    calls.push(q.clone());
    let slot = s.bank["slot"].as_u64().unwrap();
    let changed = s.change_anchor && calls.iter().filter(|c| c["method"] == "getBlock").count() > 1;
    let result = match q["method"].as_str().unwrap() {
        "getGenesisHash" => json!("localnet"),
        "getMultipleAccounts" => {
            json!({"context":{"slot":slot},"value":q["params"][0].as_array().unwrap().iter().map(|k|s.bank["accounts"][k.as_str().unwrap()].clone()).collect::<Vec<_>>()})
        }
        "getBlock" => {
            json!({"blockTime":s.bank["timestamp"],"blockhash":bs58::encode([if changed {72} else {70};32]).into_string(),"previousBlockhash":bs58::encode([71;32]).into_string(),"parentSlot":slot-1})
        }
        _ => panic!("unexpected RPC {q}"),
    };
    Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
async fn normal_rpc(
    bank: Value,
    case: &Value,
    change_anchor: bool,
) -> (xone_domain::Result<xone_domain::Snapshot>, Vec<Value>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let calls = Arc::new(Mutex::new(vec![]));
    let state = Server {
        bank,
        calls: calls.clone(),
        change_anchor,
    };
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", post(serve)).with_state(state),
        )
        .await
        .unwrap()
    });
    let ch = serde_json::from_value(json!({"key":"solana:localnet","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"localnet","rpc_urls":[format!("http://{addr}/")],"pools":[case["config"]],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
    let result = xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    }
    .current_baseline(&ch)
    .await;
    task.abort();
    let requests = calls.lock().unwrap().clone();
    (result, requests)
}
#[tokio::test]
async fn adaptive_rewards_normal_rpc_complete_finalized_banks_and_fail_closed_oracle() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let native: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-service.json"
    ))
    .unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-pools.json"
    ))
    .unwrap();
    let mut n = 0;
    for scenario in native["scenarios"].as_array().unwrap() {
        for bank in scenario["banks"].as_array().unwrap() {
            let case = &expected["cases"][n];
            let (s, calls) = normal_rpc(bank.clone(), case, false).await;
            assert_eq!(
                s.unwrap().pools,
                vec![serde_json::from_value::<xone_domain::Pool>(case["pool"].clone()).unwrap()],
                "{}",
                case["name"]
            );
            assert_eq!(calls.len(), 4);
            assert_eq!(calls[1]["params"][1]["commitment"], "finalized");
            assert!(calls[1]["params"][0]
                .as_array()
                .unwrap()
                .contains(&case["pool"]["orca_whirlpool"]["oracle"]));
            n += 1;
        }
    }
    assert_eq!(n, 20);
    let bank = &native["scenarios"][0]["banks"][0];
    let case = &expected["cases"][0];
    let oracle = case["pool"]["orca_whirlpool"]["oracle"].as_str().unwrap();
    for bad in 0..5 {
        let mut b = bank.clone();
        match bad {
            0 => b["accounts"][oracle] = Value::Null,
            1 => {
                b["accounts"][oracle] = json!({"owner":"11111111111111111111111111111111","data":["","base64"],"lamports":0,"executable":false})
            }
            2 | 3 => {
                let mut d = STANDARD
                    .decode(b["accounts"][oracle]["data"][0].as_str().unwrap())
                    .unwrap();
                if bad == 2 {
                    d.pop();
                } else {
                    d[8] ^= 1;
                }
                b["accounts"][oracle]["data"][0] = json!(STANDARD.encode(d));
            }
            _ => (),
        }
        assert!(normal_rpc(b, case, bad == 4).await.0.is_err(), "bad {bad}");
    }
}
