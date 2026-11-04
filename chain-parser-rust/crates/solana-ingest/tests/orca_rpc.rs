mod orca_support;
use axum::{extract::State, routing::post, Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine};
use orca_support::*;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use xone_solana_ingest::{orca, rpc::Rpc};

#[derive(Clone)]
struct Mock {
    accounts: Arc<BTreeMap<String, Value>>,
    calls: Arc<Mutex<Vec<Value>>>,
    change_anchor: bool,
}
async fn serve(State(s): State<Mock>, Json(q): Json<Value>) -> Json<Value> {
    let mut calls = s.calls.lock().unwrap();
    calls.push(q.clone());
    let result = match q["method"].as_str().unwrap() {
        "getGenesisHash" => json!("test-genesis"),
        "getMultipleAccounts" => {
            json!({"context":{"slot":100},"value":q["params"][0].as_array().unwrap().iter().map(|k|s.accounts.get(k.as_str().unwrap()).cloned().unwrap_or(Value::Null)).collect::<Vec<_>>()})
        }
        "getBlock" => {
            let count = calls.iter().filter(|c| c["method"] == "getBlock").count();
            json!({"blockTime":1000,"blockhash":key(if s.change_anchor&&count>1{72}else{70}),"previousBlockhash":key(71),"parentSlot":99})
        }
        _ => panic!("unexpected method {q}"),
    };
    Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
async fn run(
    mut f: Fixture,
    oracle_null: bool,
    missing: Option<String>,
    changed: bool,
) -> (xone_domain::Result<xone_domain::Snapshot>, Vec<Value>) {
    let mut accounts:BTreeMap<_,_>=f.accounts.iter().map(|(id,a)|(id.clone(),json!({"owner":a.owner,"data":[STANDARD.encode(&a.data),"base64"],"executable":a.executable,"lamports":a.lamports}))).collect();
    if oracle_null {
        accounts.insert(f.oracle.clone(), Value::Null);
    }
    if let Some(missing) = missing {
        accounts.remove(&missing);
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let state = Mock {
        accounts: Arc::new(accounts),
        calls: calls.clone(),
        change_anchor: changed,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", post(serve)).with_state(state),
        )
        .await
        .unwrap()
    });
    f.chain.rpc_urls = vec![format!("http://{address}/")];
    f.chain.pools = vec![f.pool];
    let rpc = Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    let result = rpc.current_baseline(&f.chain).await;
    task.abort();
    let requests = calls.lock().unwrap().clone();
    (result, requests)
}
#[tokio::test]
async fn single_finalized_batch_handles_both_empty_oracle_forms_and_rechecks_bank() {
    for dynamic in [false, true] {
        for oracle_null in [false, true] {
            let f = Fixture::new(dynamic);
            let expected = orca::required_accounts(&f.pool).unwrap();
            let (result, calls) = run(f, oracle_null, None, false).await;
            let snapshot = result.unwrap();
            assert_eq!(snapshot.pools.len(), 1);
            assert_eq!(
                snapshot.pools[0].orca_whirlpool.as_ref().unwrap().bank_slot,
                100
            );
            assert_eq!(calls.len(), 4);
            assert_eq!(calls[0]["method"], "getGenesisHash");
            assert_eq!(calls[1]["method"], "getMultipleAccounts");
            let ids = calls[1]["params"][0].as_array().unwrap();
            assert_eq!(ids.len(), 14);
            for id in expected {
                assert!(ids.contains(&json!(id)));
            }
            assert_eq!(calls[1]["params"][1]["commitment"], "finalized");
            assert_eq!(calls[2]["method"], "getBlock");
            assert_eq!(calls[3]["method"], "getBlock");
        }
    }
}
#[tokio::test]
async fn null_business_dependency_is_rejected_and_changed_anchor_invalidates_snapshot() {
    let f = Fixture::new(false);
    let missing = f.arrays[1].clone();
    let (result, _) = run(f, true, Some(missing), false).await;
    assert!(result.is_err());
    let (result, _) = run(Fixture::new(true), true, None, true).await;
    assert_eq!(result.unwrap_err().0, "ACCOUNT_BANK_ANCHOR_CHANGED");
}
#[tokio::test]
async fn orca_requires_finalized_before_trying_providers() {
    let mut f = Fixture::new(false);
    f.chain.pools = vec![f.pool];
    f.chain.confirmation_tag = "confirmed".into();
    let rpc = Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    assert_eq!(
        rpc.current_baseline(&f.chain).await.unwrap_err().0,
        "ORCA_REQUIRES_FINALIZED"
    );
}
#[tokio::test]
async fn sharded_scope_without_clock_is_rejected_before_anchor() {
    let f = Fixture::new(false);
    let mut ch = f.chain.clone();
    // Distinct, valid pool windows on the same program can exceed 100 addresses.
    for i in 0..14u8 {
        let mut p = f.pool.clone();
        p.accounts[3] = key(100 + i);
        p.accounts[1] = key(120 + i);
        p.accounts[2] = key(140 + i);
        let pool = pda(&[
            b"whirlpool",
            &bytes(&p.accounts[3]),
            &bytes(&p.token0),
            &bytes(&p.token1),
            &64u16.to_le_bytes(),
        ])
        .0;
        p.id = pool.clone();
        p.accounts[0] = pool;
        ch.pools.push(p);
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    ch.rpc_urls = vec![format!("http://{}/", listener.local_addr().unwrap())];
    let calls = Arc::new(Mutex::new(Vec::new()));
    let state = Mock {
        accounts: Arc::new(BTreeMap::new()),
        calls: calls.clone(),
        change_anchor: false,
    };
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", post(serve)).with_state(state),
        )
        .await
        .unwrap()
    });
    let rpc = Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    let e = rpc.current_baseline(&ch).await.unwrap_err();
    task.abort();
    assert_eq!(e.0, "ACCOUNT_SHARD_CLOCK_INVALID");
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1]["method"], "getMultipleAccounts");
    assert!(calls[1]["params"][0].as_array().unwrap().len() <= 100);
}
