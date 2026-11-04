mod orca_support;
use axum::{extract::State, routing::post, Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine};
use orca_support::{bytes, key, pda, put_key, Fixture, CLOCK, PROGRAM};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use xone_config::Chain;
use xone_solana_ingest::{orca, rpc::Rpc};

fn scope(count: usize) -> (Chain, BTreeMap<String, Value>) {
    let mut f = Fixture::new(false);
    let base = f.pool.clone();
    let pool_bytes = f.accounts[&base.id].data.clone();
    let vault0 = f.accounts[&base.accounts[1]].clone();
    let vault1 = f.accounts[&base.accounts[2]].clone();
    let mut pools = Vec::new();
    for i in 0..count {
        let cfg = key(30 + i as u8);
        let vaults = [key(100 + i as u8), key(160 + i as u8)];
        let (id, bump) = pda(&[
            b"whirlpool",
            &bytes(&cfg),
            &bytes(&base.token0),
            &bytes(&base.token1),
            &64u16.to_le_bytes(),
        ]);
        let mut p = base.clone();
        p.id = id.clone();
        p.accounts[0] = id.clone();
        p.accounts[1] = vaults[0].clone();
        p.accounts[2] = vaults[1].clone();
        p.accounts[3] = cfg.clone();
        let mut state = pool_bytes.clone();
        put_key(&mut state, 8, &cfg);
        state[40] = bump;
        put_key(&mut state, 133, &vaults[0]);
        put_key(&mut state, 213, &vaults[1]);
        let mut account = f.accounts[&base.id].clone();
        account.data = state;
        f.accounts.insert(id.clone(), account);
        f.accounts
            .insert(cfg, f.accounts[&base.accounts[3]].clone());
        for (address, template) in vaults.iter().zip([&vault0, &vault1]) {
            let mut account = template.clone();
            put_key(&mut account.data, 32, &id);
            f.accounts.insert(address.clone(), account);
        }
        let arrays = p.orca_whirlpool.as_ref().unwrap().tick_array_starts.clone();
        for (start, old) in arrays.into_iter().zip(&f.arrays) {
            let address = pda(&[b"tick_array", &bytes(&id), start.to_string().as_bytes()]).0;
            let mut account = f.accounts[old].clone();
            put_key(&mut account.data, 9956, &id);
            f.accounts.insert(address, account);
        }
        let oracle = pda(&[b"oracle", &bytes(&id)]).0;
        f.accounts.insert(oracle, f.accounts[&f.oracle].clone());
        pools.push(p);
    }
    f.chain.pools = pools;
    let required = f
        .chain
        .pools
        .iter()
        .flat_map(|p| orca::required_accounts(p).unwrap())
        .collect::<BTreeSet<_>>();
    let accounts=required.into_iter().map(|id|{
        let a=&f.accounts[&id];(id,json!({"owner":a.owner,"data":[STANDARD.encode(&a.data),"base64"],"executable":a.executable,"lamports":a.lamports}))
    }).collect();
    (f.chain, accounts)
}
#[derive(Clone)]
struct Mock {
    accounts: Arc<BTreeMap<String, Value>>,
    calls: Arc<Mutex<Vec<Value>>>,
    fault: &'static str,
}
async fn serve(State(s): State<Mock>, Json(q): Json<Value>) -> Json<Value> {
    let mut calls = s.calls.lock().unwrap();
    calls.push(q.clone());
    let n = calls
        .iter()
        .filter(|q| q["method"] == "getMultipleAccounts")
        .count();
    let block_n = calls.iter().filter(|q| q["method"] == "getBlock").count();
    let result = match q["method"].as_str().unwrap() {
        "getGenesisHash" => json!("test-genesis"),
        "getMultipleAccounts" => {
            let mut values = q["params"][0]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| {
                    s.accounts
                        .get(k.as_str().unwrap())
                        .cloned()
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>();
            let slot = if n > 1 && s.fault == "slot" {
                101
            } else if n > 1 && s.fault == "old-slot" {
                99
            } else {
                100
            };
            if n > 1 {
                match s.fault {
                    "short" => {
                        values.pop();
                    }
                    "extra" => values.push(Value::Null),
                    "missing" => {
                        values[0] = Value::Null;
                    }
                    fault if fault.starts_with("clock-") => {
                        let last = values.last_mut().unwrap();
                        match fault {
                            "clock-null" => *last = Value::Null,
                            "clock-owner" => last["owner"] = json!(PROGRAM),
                            "clock-executable" => last["executable"] = json!(true),
                            _ => {
                                let mut data =
                                    STANDARD.decode(last["data"][0].as_str().unwrap()).unwrap();
                                match fault {
                                    "clock-size" => {
                                        data.pop();
                                    }
                                    "clock-slot" => {
                                        data[0..8].copy_from_slice(&101u64.to_le_bytes())
                                    }
                                    _ => data[16] ^= 1,
                                }
                                last["data"][0] = json!(STANDARD.encode(data));
                            }
                        }
                    }
                    _ => {}
                }
            }
            json!({"context":{"slot":slot},"value":values})
        }
        "getBlock" => {
            json!({"blockTime":1000,"blockhash":key(if s.fault=="anchor"&&block_n>1{72}else{70}),"previousBlockhash":key(71),"parentSlot":if s.fault=="initial-anchor"{100}else{99}})
        }
        _ => panic!("unexpected {q}"),
    };
    let response = json!({"jsonrpc":"2.0","id":1,"result":result});
    calls.last_mut().unwrap()["response_bytes"] =
        json!(serde_json::to_vec(&response).unwrap().len());
    Json(response)
}
async fn start(
    accounts: BTreeMap<String, Value>,
    fault: &'static str,
) -> (String, tokio::task::JoinHandle<()>, Arc<Mutex<Vec<Value>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mock = Mock {
        accounts: Arc::new(accounts),
        calls: calls.clone(),
        fault,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", post(serve)).with_state(mock),
        )
        .await
        .unwrap()
    });
    (format!("http://{address}/"), task, calls)
}
async fn execute(
    mut chain: Chain,
    accounts: BTreeMap<String, Value>,
    fault: &'static str,
    max_bytes: usize,
    recorder: Option<&xone_solana_ingest::source_wal::Recorder>,
) -> (xone_domain::Result<xone_domain::Snapshot>, Vec<Value>) {
    let (url, task, calls) = start(accounts, fault).await;
    chain.rpc_urls.insert(0, url);
    let rpc = Rpc {
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap(),
        max_bytes,
    };
    let result = rpc.current_baseline_recorded(&chain, recorder).await;
    task.abort();
    let calls = calls.lock().unwrap().clone();
    (result, calls)
}
async fn run(
    count: usize,
    fault: &'static str,
) -> (xone_domain::Result<xone_domain::Snapshot>, Vec<Value>) {
    let (chain, accounts) = scope(count);
    execute(chain, accounts, fault, 4_000_000, None).await
}
#[tokio::test]
async fn same_finalized_context_shards_decode_all_pools_without_cross_slot_stitching() {
    let (chain, accounts) = scope(16);
    assert!(accounts.len() > 100);
    assert_eq!(chain.pools.len(), 16);
    let (result, calls) = run(16, "").await;
    let snapshot = result.unwrap();
    assert_eq!(snapshot.pools.len(), 16);
    assert_eq!(snapshot.reference.block.number_or_slot, 100);
    assert_eq!(snapshot.source, "RPC_SAME_FINALIZED_CONTEXT_SHARDS");
    let batches = calls
        .iter()
        .filter(|q| q["method"] == "getMultipleAccounts")
        .collect::<Vec<_>>();
    assert!(batches.len() > 1);
    let mut seen = BTreeSet::new();
    for q in batches {
        let keys = q["params"][0].as_array().unwrap();
        assert!(keys.len() <= 100);
        assert!(keys.contains(&json!(CLOCK)));
        assert_eq!(q["params"][1]["commitment"], "finalized");
        assert!(q["params"][1].get("minContextSlot").is_none());
        for id in keys {
            if id.as_str() != Some(CLOCK) {
                assert!(seen.insert(id.as_str().unwrap()));
            }
        }
    }
    assert_eq!(seen.len() + 1, accounts.len());
    assert!(snapshot
        .pools
        .iter()
        .all(|p| p.orca_whirlpool.as_ref().unwrap().bank_slot == 100));
    assert_eq!(
        calls.iter().filter(|q| q["method"] == "getBlock").count(),
        2
    );
    assert_eq!(
        snapshot.pools[0]
            .orca_whirlpool
            .as_ref()
            .unwrap()
            .program_id,
        PROGRAM
    );
}
#[tokio::test]
async fn different_shard_context_fails_the_whole_snapshot() {
    let (result, _) = run(16, "slot").await;
    assert_eq!(result.unwrap_err().0, "ACCOUNT_SHARD_CONTEXT_MISMATCH");
}

#[tokio::test]
async fn shard_clock_completeness_and_bank_barriers_reject_partial_snapshots() {
    for (fault, expected) in [
        ("old-slot", "ACCOUNT_SHARD_CONTEXT_MISMATCH"),
        ("short", "ACCOUNT_BATCH_INCOMPLETE"),
        ("extra", "ACCOUNT_BATCH_INCOMPLETE"),
        ("clock-null", "ACCOUNT_SHARD_CLOCK_INVALID"),
        ("clock-owner", "ACCOUNT_SHARD_CLOCK_INVALID"),
        ("clock-executable", "ACCOUNT_SHARD_CLOCK_INVALID"),
        ("clock-size", "ACCOUNT_SHARD_CLOCK_INVALID"),
        ("clock-slot", "ACCOUNT_SHARD_CLOCK_INVALID"),
        ("clock-drift", "ACCOUNT_SHARD_CLOCK_CHANGED"),
        ("initial-anchor", "BLOCK_PARENT_SLOT_INVALID"),
        ("anchor", "ACCOUNT_BANK_ANCHOR_CHANGED"),
    ] {
        let (result, calls) = run(16, fault).await;
        assert_eq!(result.unwrap_err().0, expected, "{fault}");
        if fault == "initial-anchor" {
            assert_eq!(
                calls
                    .iter()
                    .filter(|q| q["method"] == "getMultipleAccounts")
                    .count(),
                1
            );
        }
    }
    let (result, _) = run(16, "missing").await;
    assert!(result.is_err());
}
#[tokio::test]
async fn aggregate_account_wire_budget_is_not_multiplied_by_the_number_of_shards() {
    let (result, calls) = run(16, "").await;
    result.unwrap();
    let sizes = calls
        .iter()
        .filter(|q| q["method"] == "getMultipleAccounts")
        .map(|q| q["response_bytes"].as_u64().unwrap() as usize)
        .collect::<Vec<_>>();
    let total = sizes.iter().sum::<usize>();
    let largest = *sizes.iter().max().unwrap();
    assert!(total > largest);
    for budget in [largest, total - 1] {
        let (chain, accounts) = scope(16);
        let (result, _) = execute(chain, accounts, "", budget, None).await;
        assert_eq!(result.unwrap_err().0, "ACCOUNT_SHARD_RESPONSE_BUDGET");
    }
    let (chain, accounts) = scope(16);
    assert!(execute(chain, accounts, "", total, None).await.0.is_ok());
}
#[tokio::test]
async fn single_batch_compatibility_is_retained_and_nonfinalized_shards_rejected() {
    let (result, calls) = run(1, "").await;
    let snapshot = result.unwrap();
    assert_eq!(snapshot.source, "RPC_CONTEXT_SLOT_ACCOUNTS");
    assert_eq!(
        calls
            .iter()
            .map(|q| q["method"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "getGenesisHash",
            "getMultipleAccounts",
            "getBlock",
            "getBlock"
        ]
    );
    let (mut chain, accounts) = scope(16);
    chain.confirmation_tag = "confirmed".into();
    assert!(execute(chain, accounts, "", 4_000_000, None)
        .await
        .0
        .is_err());
}
fn numbered(n: usize) -> String {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&(n as u64).to_le_bytes());
    bs58::encode(bytes).into_string()
}
fn archive_scope(count: usize, share_last: bool) -> (Chain, BTreeMap<String, Value>) {
    let (mut chain, _) = scope(1);
    chain.pools.clear();
    let mut accounts = BTreeMap::new();
    for i in 0..count {
        let mut ids = (0..6).map(|j| numbered(i * 6 + j + 1)).collect::<Vec<_>>();
        if share_last && i == count - 1 {
            ids[4] = numbered(5);
            ids[5] = numbered(6);
        }
        let pool=serde_json::from_value(json!({"id":ids[0],"protocol":"raydium-cpmm","token0":ids[4],"token1":ids[5],"fee_numerator":2500,"fee_denominator":1000000,"program_id":xone_solana_ingest::rpc::CPMM_PROGRAM,"accounts":ids})).unwrap();
        let mut state = vec![0u8; 637];
        state[..8].copy_from_slice(&[247, 237, 227, 245, 215, 195, 222, 70]);
        for (at, id) in [
            (8, &ids[3]),
            (72, &ids[1]),
            (104, &ids[2]),
            (168, &ids[4]),
            (200, &ids[5]),
            (232, &xone_solana_ingest::rpc::TOKEN_PROGRAM.to_string()),
            (264, &xone_solana_ingest::rpc::TOKEN_PROGRAM.to_string()),
        ] {
            put_key(&mut state, at, id);
        }
        let mut cfg = vec![0u8; 236];
        cfg[..8].copy_from_slice(&[218, 244, 33, 104, 203, 203, 43, 111]);
        cfg[12..20].copy_from_slice(&2500u64.to_le_bytes());
        for (id, owner, data) in [
            (ids[0].clone(), xone_solana_ingest::rpc::CPMM_PROGRAM, state),
            (ids[3].clone(), xone_solana_ingest::rpc::CPMM_PROGRAM, cfg),
        ] {
            accounts.insert(id,json!({"owner":owner,"data":[STANDARD.encode(data),"base64"],"executable":false,"lamports":1}));
        }
        for side in 0..2 {
            let mut mint = vec![0u8; 82];
            mint[45] = 1;
            let mut vault = vec![0u8; 165];
            put_key(&mut vault, 0, &ids[4 + side]);
            vault[108] = 1;
            vault[64..72].copy_from_slice(&10_000_000u64.to_le_bytes());
            for (id, data) in [
                (ids[4 + side].clone(), mint),
                (ids[1 + side].clone(), vault),
            ] {
                accounts.insert(id,json!({"owner":xone_solana_ingest::rpc::TOKEN_PROGRAM,"data":[STANDARD.encode(data),"base64"],"executable":false,"lamports":1}));
            }
        }
        chain.pools.push(pool);
    }
    let (_, orca) = scope(1);
    accounts.insert(CLOCK.into(), orca[CLOCK].clone());
    (chain, accounts)
}
#[tokio::test]
async fn archive_only_scopes_cannot_bypass_shard_guards_and_exact_100_stays_single() {
    let (chain, accounts) = archive_scope(20, false);
    let (result, calls) = execute(chain, accounts, "", 4_000_000, None).await;
    assert_eq!(result.unwrap().pools.len(), 20);
    assert_eq!(
        calls.iter().filter(|q| q["method"] == "getBlock").count(),
        2
    );
    let (mut chain, accounts) = archive_scope(20, false);
    chain.confirmation_tag = "confirmed".into();
    let (result, calls) = execute(chain, accounts, "", 4_000_000, None).await;
    assert_eq!(result.unwrap_err().0, "ACCOUNT_SHARDS_REQUIRES_FINALIZED");
    assert_eq!(calls.len(), 1);
    let (chain, accounts) = archive_scope(17, true);
    let (result, calls) = execute(chain, accounts, "", 4_000_000, None).await;
    let snapshot = result.unwrap();
    assert_eq!(snapshot.source, "RPC_CONTEXT_SLOT_ACCOUNTS");
    let batches = calls
        .iter()
        .filter(|q| q["method"] == "getMultipleAccounts")
        .collect::<Vec<_>>();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["params"][0].as_array().unwrap().len(), 100);
    let (chain, accounts) = archive_scope(600, false);
    let (result, calls) = execute(chain, accounts, "", 4_000_000, None).await;
    assert_eq!(result.unwrap_err().0, "ACCOUNT_SHARD_COUNT_LIMIT");
    assert_eq!(calls.len(), 1);
}
#[tokio::test]
async fn mixed_decoder_dependencies_and_decoder_specific_null_oracles_remain_exact() {
    let (mut chain, mut accounts) = scope(16);
    let (archive, extra) = archive_scope(20, false);
    chain.pools.extend(archive.pools);
    accounts.extend(extra);
    for pool in chain
        .pools
        .iter()
        .filter(|p| p.protocol == "orca-whirlpools-static")
    {
        let address = pda(&[b"oracle", &bytes(&pool.id)]).0;
        accounts.insert(address, Value::Null);
    }
    let (result, calls) = execute(chain, accounts, "", 4_000_000, None).await;
    assert_eq!(result.unwrap().pools.len(), 36);
    assert!(
        calls
            .iter()
            .filter(|q| q["method"] == "getMultipleAccounts")
            .count()
            >= 3
    );
}
#[tokio::test]
async fn export_account_shard_fixture_only_when_requested() {
    if let Some(path) = std::env::var_os("XONE_ACCOUNT_SHARD_FIXTURE") {
        let (chain, accounts) = scope(16);
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"chain":chain,"accounts":accounts,"synthetic":true,"source_slot":100}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

#[tokio::test]
async fn maximum_shard_plan_is_32_requests_and_35_total_calls() {
    let (chain, accounts) = archive_scope(528, false);
    let (result, calls) = execute(chain, accounts, "", 4_000_000, None).await;
    assert_eq!(result.unwrap().pools.len(), 528);
    assert_eq!(
        calls
            .iter()
            .filter(|q| q["method"] == "getMultipleAccounts")
            .count(),
        32
    );
    assert_eq!(calls.len(), 35);
}
#[tokio::test]
async fn provider_fallback_restarts_the_entire_frozen_manifest() {
    let (mut chain, accounts) = scope(16);
    let mut backup = accounts.clone();
    for pool in &chain.pools {
        let value = backup.get_mut(&pool.accounts[1]).unwrap();
        let mut data = STANDARD.decode(value["data"][0].as_str().unwrap()).unwrap();
        data[64..72].copy_from_slice(&7_000_000u64.to_le_bytes());
        value["data"][0] = json!(STANDARD.encode(data));
    }
    let (url, task, calls) = start(backup, "").await;
    chain.rpc_urls = vec![url];
    let (result, primary) = execute(chain, accounts, "slot", 4_000_000, None).await;
    let snapshot = result.unwrap();
    assert!(snapshot
        .pools
        .iter()
        .all(|p| p.orca_whirlpool.as_ref().unwrap().raw_vault0 == 7_000_000));
    let backup = calls.lock().unwrap().clone();
    task.abort();
    assert_eq!(
        primary
            .iter()
            .find(|q| q["method"] == "getMultipleAccounts")
            .unwrap()["params"],
        backup
            .iter()
            .find(|q| q["method"] == "getMultipleAccounts")
            .unwrap()["params"]
    );
    assert_eq!(backup[0]["method"], "getGenesisHash");
}
#[tokio::test]
async fn incomplete_providers_never_fill_each_others_missing_accounts() {
    let (mut chain, mut accounts) = scope(16);
    let mut backup = accounts.clone();
    accounts.insert(chain.pools[0].accounts[1].clone(), Value::Null);
    backup.insert(chain.pools[15].accounts[2].clone(), Value::Null);
    let (url, task, backup_calls) = start(backup, "").await;
    chain.rpc_urls = vec![url];
    let (result, primary) = execute(chain, accounts, "", 4_000_000, None).await;
    task.abort();
    assert!(result.is_err());
    for calls in [primary, backup_calls.lock().unwrap().clone()] {
        assert!(
            calls
                .iter()
                .filter(|q| q["method"] == "getMultipleAccounts")
                .count()
                > 1
        );
    }
}
#[tokio::test]
async fn rejected_shards_are_recorded_before_context_decode_and_wal_failure_never_falls_back() {
    use xone_solana_ingest::source_wal::{Recorder, SourceWal, WalLimits};
    let base = std::env::temp_dir().join(format!(
        "xone-shards-wal-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for (capacity, fault) in [(10, "slot"), (3, "")] {
        let path = base.join(capacity.to_string());
        let limits = WalLimits {
            max_response_bytes: 4_000_000,
            max_records: capacity,
        };
        let recorder = Recorder::new(SourceWal::open(&path, "solana:test", limits).unwrap());
        let (mut chain, accounts) = scope(16);
        let (url, task, backup) = start(accounts.clone(), "").await;
        chain.rpc_urls = vec![url];
        // The malformed-context attempt gets no usable fallback so its error remains visible.
        if fault == "slot" {
            chain.rpc_urls.clear();
        }
        let (result, _) = execute(chain, accounts, fault, 4_000_000, Some(&recorder)).await;
        task.abort();
        if capacity == 3 {
            assert_eq!(result.unwrap_err().0, "SOURCE_WAL_CAPACITY");
            assert!(backup.lock().unwrap().is_empty());
            assert_eq!(recorder.len().unwrap(), 3);
        } else {
            assert_eq!(result.unwrap_err().0, "ACCOUNT_SHARD_CONTEXT_MISMATCH");
            assert_eq!(recorder.len().unwrap(), 4);
            let record = recorder.read(4).unwrap();
            assert_eq!(record.method, "getMultipleAccounts");
            assert_eq!(
                serde_json::from_slice::<Value>(&record.response).unwrap()["result"]["context"]
                    ["slot"],
                101
            );
        }
        drop(recorder);
        let reopened = SourceWal::open(&path, "solana:test", limits).unwrap();
        assert!(reopened.status()["records"].as_u64().unwrap() > 0);
        drop(reopened);
    }
    std::fs::remove_dir_all(base).unwrap();
}
