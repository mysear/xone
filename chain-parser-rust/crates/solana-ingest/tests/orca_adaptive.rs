mod orca_support;
use orca_support::*;
use xone_solana_ingest::{orca, raydium::RpcAccount};
fn put16(d: &mut [u8], o: usize, v: u16) {
    d[o..o + 2].copy_from_slice(&v.to_le_bytes())
}
fn put32(d: &mut [u8], o: usize, v: u32) {
    d[o..o + 4].copy_from_slice(&v.to_le_bytes())
}
fn adaptive() -> Fixture {
    let mut f = Fixture::new(false);
    let old = f.pool.id.clone();
    let (new, bump) = pda(&[
        b"whirlpool",
        &bytes(&f.pool.accounts[3]),
        &bytes(&f.pool.token0),
        &bytes(&f.pool.token1),
        &65u16.to_le_bytes(),
    ]);
    let mut state = f.accounts.remove(&old).unwrap();
    state.data[40] = bump;
    put16(&mut state.data, 43, 65);
    f.accounts.insert(new.clone(), state);
    let old_bytes = bytes(&old);
    let new_bytes = bytes(&new);
    for a in f.accounts.values_mut() {
        let mut i = 0;
        while i + 32 <= a.data.len() {
            if a.data[i..i + 32] == old_bytes {
                a.data[i..i + 32].copy_from_slice(&new_bytes);
                i += 32
            } else {
                i += 1
            }
        }
    }
    f.pool.id = new.clone();
    f.pool.accounts[0] = new.clone();
    f.pool.protocol = "orca-whirlpools-adaptive".into();
    let starts = f
        .pool
        .orca_whirlpool
        .as_ref()
        .unwrap()
        .tick_array_starts
        .clone();
    for (i, start) in starts.iter().enumerate() {
        let a = f.accounts.remove(&f.arrays[i]).unwrap();
        f.arrays[i] = pda(&[b"tick_array", &bytes(&new), start.to_string().as_bytes()]).0;
        f.accounts.insert(f.arrays[i].clone(), a);
    }
    f.accounts.remove(&f.oracle);
    f.oracle = orca::oracle_address(&f.pool).unwrap();
    let mut d = vec![0; 254];
    d[..8].copy_from_slice(&disc("Oracle"));
    put_key(&mut d, 8, &new);
    put64(&mut d, 40, 90);
    put16(&mut d, 48, 10);
    put16(&mut d, 50, 100);
    put16(&mut d, 52, 5000);
    put32(&mut d, 54, 1000);
    put32(&mut d, 58, 100000);
    put16(&mut d, 62, 8);
    put16(&mut d, 64, 64);
    d[66..82].fill(3);
    put64(&mut d, 82, 90);
    put64(&mut d, 90, 90);
    put32(&mut d, 98, 20000);
    put32(&mut d, 106, 40000);
    d[110..126].fill(5);
    d[126..254].fill(7);
    f.accounts
        .insert(f.oracle.clone(), account(PROGRAM, d, false));
    f
}
#[test]
fn initialized_oracle_is_parsed_only_for_its_own_adaptive_version() {
    let f = adaptive();
    let p = orca::decode_orca_whirlpool(&f.chain, &f.pool, &f.accounts, 100).unwrap();
    let s = p.orca_whirlpool.unwrap();
    assert_eq!(s.fee_tier_index, 65);
    let a = s.adaptive.unwrap();
    assert_eq!(a.whirlpool, p.pool_id);
    assert_eq!(a.filter_period, 10);
    assert_eq!(a.volatility_reference, 20000);
    assert_eq!(a.constants_reserved, vec![3; 16]);
    assert_eq!(a.variables_reserved, vec![5; 16]);
    assert_eq!(a.reserved, vec![7; 128]);
    assert_eq!(s.reward_infos.len(), 3);
    let mut old = f.pool;
    old.protocol = "orca-whirlpools-static".into();
    assert!(orca::decode_orca_whirlpool(&f.chain, &old, &f.accounts, 100).is_err());
}
#[test]
fn adaptive_oracle_binding_time_layout_and_combinations_fail_closed() {
    for bad in [
        "length",
        "disc",
        "owner",
        "executable",
        "binding",
        "gate",
        "future-reference",
        "future-major",
        "constant",
        "variables",
        "tier",
        "reward",
        "dynamic-array",
        "context",
    ] {
        let mut f = adaptive();
        let oracle = f.accounts.get_mut(&f.oracle).unwrap();
        match bad {
            "length" => {
                oracle.data.pop();
            }
            "disc" => oracle.data[0] ^= 1,
            "owner" => oracle.owner = SYSTEM.into(),
            "executable" => oracle.executable = true,
            "binding" => put_key(&mut oracle.data, 8, &key(33)),
            "gate" => put64(&mut oracle.data, 40, 1001),
            "future-reference" => put64(&mut oracle.data, 82, 1001),
            "future-major" => put64(&mut oracle.data, 90, 1001),
            "constant" => put16(&mut oracle.data, 62, 3),
            "variables" => put32(&mut oracle.data, 106, 100001),
            "tier" => put16(&mut f.accounts.get_mut(&f.pool.id).unwrap().data, 43, 64),
            "reward" => put_key(
                &mut f.accounts.get_mut(&f.pool.id).unwrap().data,
                269,
                &key(22),
            ),
            "dynamic-array" => {
                f.accounts.get_mut(&f.arrays[0]).unwrap().data[..8]
                    .copy_from_slice(&disc("DynamicTickArray"));
            }
            _ => put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 0, 101),
        };
        assert!(
            orca::decode_orca_whirlpool(&f.chain, &f.pool, &f.accounts, 100).is_err(),
            "{bad}"
        )
    }
}
#[test]
fn old_rpc_oracle_decoder_keeps_initialized_oracles_closed() {
    let f = adaptive();
    let a = &f.accounts[&f.oracle];
    let value = serde_json::json!({"owner":a.owner,"lamports":a.lamports,"executable":a.executable,"data":[base64::Engine::encode(&base64::engine::general_purpose::STANDARD,&a.data),"base64"]});
    assert!(orca::decode_rpc_account(&f.oracle, &f.oracle, &value).is_err());
    let _: Option<RpcAccount> = None;
}

#[derive(Clone)]
struct Server {
    accounts: std::sync::Arc<std::collections::BTreeMap<String, serde_json::Value>>,
    calls: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    bad: String,
}
async fn adaptive_rpc_serve(
    axum::extract::State(s): axum::extract::State<Server>,
    axum::Json(q): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    use serde_json::json;
    let mut calls = s.calls.lock().unwrap();
    calls.push(q.clone());
    let result = match q["method"].as_str().unwrap() {
        "getGenesisHash" => json!("test-genesis"),
        "getMultipleAccounts" => {
            json!({"context":{"slot":100},"value":q["params"][0].as_array().unwrap().iter().map(|k|s.accounts.get(k.as_str().unwrap()).cloned().unwrap_or(serde_json::Value::Null)).collect::<Vec<_>>()})
        }
        "getBlock" => {
            let count = calls.iter().filter(|v| v["method"] == "getBlock").count();
            json!({"blockTime":1000,"blockhash":key(if s.bad=="anchor"&&count>1{72}else{70}),"previousBlockhash":key(71),"parentSlot":99})
        }
        _ => panic!("unexpected {q}"),
    };
    axum::Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
#[tokio::test]
async fn adaptive_normal_rpc_uses_initialized_oracle_same_finalized_bank_and_reanchors() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;
    for bad in ["", "null", "system-empty", "anchor", "oracle-binding"] {
        let mut f = adaptive();
        if bad == "oracle-binding" {
            put_key(
                &mut f.accounts.get_mut(&f.oracle).unwrap().data,
                8,
                &key(33),
            )
        }
        let mut accounts:std::collections::BTreeMap<_,_>=f.accounts.iter().map(|(id,a)|(id.clone(),json!({"owner":a.owner,"data":[STANDARD.encode(&a.data),"base64"],"executable":a.executable,"lamports":a.lamports}))).collect();
        if bad == "null" {
            accounts.insert(f.oracle.clone(), serde_json::Value::Null);
        }
        if bad == "system-empty" {
            accounts.insert(
                f.oracle.clone(),
                json!({"owner":SYSTEM,"data":["","base64"],"executable":false,"lamports":0}),
            );
        }
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let server = Server {
            accounts: std::sync::Arc::new(accounts),
            calls: calls.clone(),
            bad: bad.into(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                axum::Router::new()
                    .route("/", axum::routing::post(adaptive_rpc_serve))
                    .with_state(server),
            )
            .await
            .unwrap()
        });
        f.chain.rpc_urls = vec![format!("http://{address}/")];
        f.chain.pools = vec![f.pool];
        let result = xone_solana_ingest::rpc::Rpc {
            http: reqwest::Client::new(),
            max_bytes: 1_000_000,
        }
        .current_baseline(&f.chain)
        .await;
        task.abort();
        let requests = calls.lock().unwrap();
        if !bad.is_empty() {
            assert!(result.is_err(), "{bad}");
            if bad == "anchor" {
                assert_eq!(
                    requests
                        .iter()
                        .filter(|c| c["method"] == "getBlock")
                        .count(),
                    2
                )
            }
            continue;
        }
        let snapshot = result.unwrap();
        let s = snapshot.pools[0].orca_whirlpool.as_ref().unwrap();
        assert_eq!(s.adaptive.as_ref().unwrap().reserved, vec![7; 128]);
        assert_eq!(s.bank_slot, 100);
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[1]["method"], "getMultipleAccounts");
        assert_eq!(requests[1]["params"][1]["commitment"], "finalized");
        assert!(requests[1]["params"][0]
            .as_array()
            .unwrap()
            .contains(&json!(f.oracle)));
        assert_eq!(requests[2]["method"], "getBlock");
        assert_eq!(requests[3]["method"], "getBlock");
    }
}

#[test]
fn original_program_packed_adaptive_banks_match_full_expected_service_state() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use xone_config::{Chain, PoolConfig};
    use xone_domain::Pool;
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-service.json"
    ))
    .unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-pools.json"
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
            count += 1;
        }
    }
    assert_eq!(count, 20);
}
