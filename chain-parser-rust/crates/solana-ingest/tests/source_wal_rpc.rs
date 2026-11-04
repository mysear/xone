use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
};
use xone_config::Chain;
use xone_solana_ingest::{
    rpc::Rpc,
    source_wal::{Recorder, SourceWal, WalLimits},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn directory() -> PathBuf {
    std::env::temp_dir().join(format!(
        "xone-recorded-rpc-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
#[derive(Clone)]
struct Mock {
    calls: Arc<Mutex<Vec<Value>>>,
    genesis: String,
}
async fn serve(State(s): State<Mock>, Json(q): Json<Value>) -> Json<Value> {
    s.calls.lock().unwrap().push(q.clone());
    let result = match q["method"].as_str().unwrap() {
        "getGenesisHash" => json!(s.genesis),
        "getMultipleAccounts" => {
            json!({"context":{"slot":100},"value":q["params"][0].as_array().unwrap().iter().map(|_|Value::Null).collect::<Vec<_>>()})
        }
        _ => panic!("unexpected {q}"),
    };
    Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
fn chain(url: String) -> Chain {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/solana/rayclmm-dynamic-base.json"
    ))
    .unwrap();
    let mut pool = raw["pool_config"].clone();
    pool["protocol"] = json!("raydium-clmm-dynamic");
    serde_json::from_value(json!({"key":"solana:localnet","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"localnet","rpc_urls":[url],"confirmation_tag":"finalized","auto_reorg_window":32,"pools":[pool]})).unwrap()
}
#[tokio::test]
async fn rpc_raw_account_response_is_durable_before_decoder_rejects_it() {
    let path = directory();
    let recorder = Recorder::new(
        SourceWal::open(
            &path,
            "solana:localnet",
            WalLimits {
                max_response_bytes: 1_000_000,
                max_records: 8,
            },
        )
        .unwrap(),
    );
    let calls = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mock = Mock {
        calls: calls.clone(),
        genesis: "localnet".into(),
    };
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", post(serve)).with_state(mock),
        )
        .await
        .unwrap()
    });
    let rpc = Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    let result = rpc
        .current_baseline_recorded(&chain(format!("http://{address}/")), Some(&recorder))
        .await;
    assert!(result.is_err());
    assert_eq!(recorder.len().unwrap(), 2);
    let captured = recorder.read(2).unwrap();
    assert_eq!(captured.method, "getMultipleAccounts");
    assert!(
        serde_json::from_slice::<Value>(&captured.response).unwrap()["result"]["value"][0]
            .is_null()
    );
    assert_eq!(captured.params[1]["commitment"], "finalized");
    task.abort();
    drop(recorder);
    let wal = SourceWal::open(
        &path,
        "solana:localnet",
        WalLimits {
            max_response_bytes: 1_000_000,
            max_records: 8,
        },
    )
    .unwrap();
    assert_eq!(wal.read(2).unwrap().response, captured.response);
    drop(wal);
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn wal_capacity_failure_cannot_fall_through_to_another_provider() {
    let path = directory();
    let recorder = Recorder::new(
        SourceWal::open(
            &path,
            "solana:localnet",
            WalLimits {
                max_response_bytes: 1_000_000,
                max_records: 1,
            },
        )
        .unwrap(),
    );
    let calls = Arc::new(Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mock = Mock {
        calls: calls.clone(),
        genesis: "localnet".into(),
    };
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/", post(serve)).with_state(mock),
        )
        .await
        .unwrap()
    });
    let mut ch = chain(format!("http://{address}/"));
    ch.rpc_urls.push("http://127.0.0.1:1/".into());
    let rpc = Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    assert_eq!(
        rpc.current_baseline_recorded(&ch, Some(&recorder))
            .await
            .unwrap_err()
            .0,
        "SOURCE_WAL_CAPACITY"
    );
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_eq!(recorder.len().unwrap(), 1);
    task.abort();
    drop(recorder);
    std::fs::remove_dir_all(path).unwrap();
}
