use xone_solana_ingest::{orca, raydium::RpcAccount};
#[test]
fn adaptive_dynamic_twenty_native_packed_banks_and_old_profile_rejections() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use xone_config::{Chain, PoolConfig};
    use xone_domain::Pool;
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-service.json"
    ))
    .unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
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
                "orca-whirlpools-adaptive-rewards",
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
async fn adaptive_dynamic_normal_rpc_requests_and_decodes_supplemental_arrays() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use xone_config::PoolConfig;

    let native: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-service.json"
    ))
    .unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
    ))
    .unwrap();
    let mut bank = native["scenarios"][0]["banks"][0].clone();
    let mut case = expected["cases"][0].clone();
    let original: PoolConfig = serde_json::from_value(case["config"].clone()).unwrap();
    let original_accounts = orca::required_accounts(&original).unwrap();
    let template = case["pool"]["orca_whirlpool"]["tick_arrays"][0]["address"]
        .as_str()
        .unwrap();
    let mut account = bank["accounts"][template].clone();
    let mut bytes = STANDARD
        .decode(account["data"][0].as_str().unwrap())
        .unwrap();
    bytes.resize(60 + 88, 0);
    bytes[44..].fill(0);
    account["space"] = json!(bytes.len());
    let mut extra_accounts = Vec::new();
    for start in [11264i32, 16896, 22528, 28160] {
        case["config"]["orca_whirlpool"]["tick_array_starts"]
            .as_array_mut()
            .unwrap()
            .push(json!(start));
        let config: PoolConfig = serde_json::from_value(case["config"].clone()).unwrap();
        let key = orca::required_accounts(&config)
            .unwrap()
            .into_iter()
            .find(|key| !original_accounts.contains(key) && !extra_accounts.contains(key))
            .unwrap();
        bytes[8..12].copy_from_slice(&start.to_le_bytes());
        account["data"][0] = json!(STANDARD.encode(&bytes));
        bank["accounts"][&key] = account.clone();
        case["pool"]["orca_whirlpool"]["tick_arrays"]
            .as_array_mut()
            .unwrap()
            .push(
                json!({"address": key, "start_tick_index": start, "layout": "dynamic",
                        "initialized_bitmap": "0", "ticks": []}),
            );
        extra_accounts.push(key);
    }
    let (snapshot, calls) = normal_rpc(bank.clone(), &case, false).await;
    let wanted = serde_json::from_value::<xone_domain::Pool>(case["pool"].clone()).unwrap();
    assert_eq!(snapshot.unwrap().pools, vec![wanted.clone()]);
    let requested = calls[1]["params"][0].as_array().unwrap();
    assert!(extra_accounts
        .iter()
        .all(|key| requested.contains(&json!(key))));

    let address = &extra_accounts[3];
    bank["accounts"][address] = json!({
        "owner": "11111111111111111111111111111111",
        "data": ["", "base64"],
        "lamports": 1_000_000,
        "executable": false,
        "space": 0
    });
    let mut wanted = wanted;
    wanted.orca_whirlpool.as_mut().unwrap().tick_arrays[6].layout = "sparse".into();
    let (snapshot, _) = normal_rpc(bank.clone(), &case, false).await;
    assert_eq!(snapshot.unwrap().pools, vec![wanted]);

    for invalid in [
        json!({"owner": "11111111111111111111111111111111",
               "data": ["AQ==", "base64"], "lamports": 1_000_000, "executable": false}),
        json!({"owner": "11111111111111111111111111111111",
               "data": ["", "base64"], "lamports": 1_000_000, "executable": true}),
        Value::Null,
    ] {
        bank["accounts"][address] = invalid;
        assert!(normal_rpc(bank.clone(), &case, false).await.0.is_err());
    }
}

#[tokio::test]
async fn adaptive_token2022_normal_rpc_binds_program_mints_and_vaults() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use xone_domain::spl_token_2022::{PROGRAM, PROGRAMDATA};

    let native: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-service.json"
    ))
    .unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
    ))
    .unwrap();
    let wire: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-token2022-wire.json"
    ))
    .unwrap();
    let mut bank = native["scenarios"][0]["banks"][0].clone();
    let mut case = expected["cases"][0].clone();
    case["pool"] = wire["snapshot"]["pools"][0].clone();
    let pool = &case["pool"];
    let tokens = &pool["orca_whirlpool"]["token_2022"];
    let loader = "BPFLoaderUpgradeab1e11111111111111111111111";
    let account = |owner, executable, data: &[u8]| {
        json!({
            "owner": owner, "executable": executable, "lamports": 10_000_000,
            "data": [STANDARD.encode(data), "base64"],
        })
    };
    let mut program = vec![2, 0, 0, 0];
    program.extend(bs58::decode(PROGRAMDATA).into_vec().unwrap());
    bank["accounts"][PROGRAM] = account(loader, true, &program);
    let mut programdata = vec![0u8; 45];
    programdata[0] = 3;
    programdata[4..12].copy_from_slice(&300u64.to_le_bytes());
    programdata.extend(b"\x7fELFsynthetic Token-2022 proof");
    bank["accounts"][PROGRAMDATA] = account(loader, false, &programdata);
    for i in 0..2 {
        let asset = &tokens[&format!("token{i}")];
        let mint = pool[&format!("token{i}")].as_str().unwrap();
        let vault = pool["orca_whirlpool"][&format!("vault{i}")]
            .as_str()
            .unwrap();
        for (key, field) in [(mint, "mint_data"), (vault, "vault_data")] {
            let data = hex::decode(asset[field].as_str().unwrap()).unwrap();
            bank["accounts"][key] = account(PROGRAM, false, &data);
        }
    }
    let config = json!({
        "programdata": PROGRAMDATA, "deployed_slot": 300,
        "code_sha256": tokens["program_code_sha256"]
    });
    case["config"]["protocol"] = pool["protocol"].clone();
    case["config"]["orca_whirlpool"]["token_2022"] = config;
    let (snapshot, calls) = normal_rpc(bank.clone(), &case, false).await;
    let wanted = serde_json::from_value::<xone_domain::Pool>(case["pool"].clone()).unwrap();
    assert_eq!(snapshot.unwrap().pools, vec![wanted]);
    let requested = calls[1]["params"][0].as_array().unwrap();
    assert!(requested.contains(&json!(PROGRAM)));
    assert!(requested.contains(&json!(PROGRAMDATA)));

    let mut changed = bank.clone();
    changed["accounts"][PROGRAMDATA]["data"][0] = json!(STANDARD.encode([0; 46]));
    assert!(normal_rpc(changed, &case, false).await.0.is_err());
    bank["accounts"][case["pool"]["orca_whirlpool"]["vault0"].as_str().unwrap()]["owner"] =
        json!(xone_domain::raydium::TOKEN_PROGRAM);
    assert!(normal_rpc(bank, &case, false).await.0.is_err());
}

#[tokio::test]
async fn adaptive_dynamic_normal_rpc_complete_finalized_banks_and_fail_closed_oracle() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let native: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-service.json"
    ))
    .unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
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

#[tokio::test]
async fn adaptive_dynamic_normal_rpc_exact_compact_layout_and_array_faults() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let native: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-service.json"
    ))
    .unwrap();
    let expected: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
    ))
    .unwrap();
    let bank = &native["scenarios"][0]["banks"][0];
    let case = &expected["cases"][0];
    let address = case["pool"]["orca_whirlpool"]["tick_arrays"][0]["address"]
        .as_str()
        .unwrap();
    let original = STANDARD
        .decode(bank["accounts"][address]["data"][0].as_str().unwrap())
        .unwrap();
    let bitmap = u128::from_le_bytes(original[44..60].try_into().unwrap());
    assert_eq!(original.len(), 148 + 112 * bitmap.count_ones() as usize);
    for bad in 0..12 {
        let mut b = bank.clone();
        let mut data = original.clone();
        match bad {
            0 => {
                data.pop();
            }
            1 => data.push(0),
            2 => data.push(1),
            3 => data.resize(10004, 0),
            4 => data.resize(10005, 0),
            5 => data[44] ^= 1,
            6 => data[55] |= 1, // bitmap bit 88
            7 => data[60] = 2,
            8 => data[12] ^= 1, // Whirlpool binding
            9 => data[8] ^= 1,  // start/PDA binding
            10 => b["accounts"][address]["owner"] = json!("11111111111111111111111111111111"),
            _ => b["accounts"][address]["executable"] = json!(true),
        }
        b["accounts"][address]["data"][0] = json!(STANDARD.encode(data));
        assert!(normal_rpc(b, case, false).await.0.is_err(), "bad {bad}");
    }
    let mut missing = bank.clone();
    missing["accounts"][address] = Value::Null;
    assert!(normal_rpc(missing, case, false).await.0.is_err());
    // Empty and fully initialized compact arrays exercise the exact 148/10004-byte bounds.
    for initialized in [false, true] {
        let mut data = original[..60].to_vec();
        let bits = if initialized { (1u128 << 88) - 1 } else { 0 };
        data[44..60].copy_from_slice(&bits.to_le_bytes());
        for _ in 0..88 {
            data.push(u8::from(initialized));
            if initialized {
                data.extend_from_slice(&0i128.to_le_bytes());
                data.extend_from_slice(&1u128.to_le_bytes());
                data.extend_from_slice(&[0; 80]);
            }
        }
        assert_eq!(data.len(), if initialized { 10004 } else { 148 });
        let mut b = bank.clone();
        b["accounts"][address]["data"][0] = json!(STANDARD.encode(data));
        let result = normal_rpc(b, case, false).await.0.unwrap();
        let a = &result.pools[0].orca_whirlpool.as_ref().unwrap().tick_arrays[0];
        assert_eq!(a.layout, "dynamic");
        assert_eq!(a.ticks.len(), if initialized { 88 } else { 0 });
    }
}
