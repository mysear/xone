//! Local original-account integration only; never a production daemon.
#[path = "../tests/support/execution_bank.rs"]
mod execution_bank;
use axum::{extract::State, routing::post, Json, Router};
use execution_bank::Bank;
use serde_json::{json, Value};
use std::sync::Arc;
fn browser_info(original: &Value) -> Value {
    fn prune(value: &mut Value) {
        match value {
            Value::Object(items) => {
                items.remove("accounts");
                items.remove("after");
                for v in items.values_mut() {
                    prune(v);
                }
            }
            Value::Array(items) => {
                for v in items {
                    prune(v);
                }
            }
            _ => {}
        }
    }
    let mut value = original.clone();
    prune(&mut value);
    value["sourceBankFixtureHash"] = xone_domain::hash(original).unwrap().into();
    value
}
async fn sign(State(bank): State<Bank>, Json(body): Json<Value>) -> Json<Value> {
    Json(
        execution_bank::wallet_worker(
            &bank,
            "sign",
            body["transaction"].as_str().unwrap(),
            "ready",
        )
        .await,
    )
}
async fn stats(State(bank): State<Bank>) -> Json<Value> {
    let calls = bank.calls.lock().unwrap();
    Json(
        json!({"sendCalls": calls.iter().filter(|m|m.as_str()=="sendTransaction").count(), "accountReads": calls.iter().filter(|m|m.as_str()=="getMultipleAccounts").count()}),
    )
}
#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        3,
        "rayclmm_advanced_frontend_harness INFO FRONTEND_ORIGIN"
    );
    let manifest = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/contracts/sbf-runtime/rpc-capture.json"
    );
    let mut rows = Vec::new();
    let mut tasks = Vec::new();
    for index in [
        0, 3, 9, 10, 17, 18, 21, 27, 28, 35, 36, 39, 45, 46, 53, 54, 57, 63, 64, 71, 72, 75, 81,
        82, 89, 90, 93, 99, 100, 107, 108, 111, 117, 118, 125, 126, 129, 135, 136, 143, 144, 147,
        153, 154, 161,
    ] {
        let bank = Bank::rayclmm_advanced(index);
        let (rpc, rpc_task) = bank.serve().await;
        tasks.push(rpc_task);
        let (mut api, _) = bank.api_with_manifest(&rpc, Some(manifest));
        Arc::make_mut(&mut api.config)
            .config
            .api
            .cors_allowed_origins = vec![args[2].clone()];
        let test = Router::new()
            .route("/test/sign", post(sign))
            .route("/test/stats", post(stats))
            .with_state(bank.clone())
            .layer(
                tower_http::cors::CorsLayer::new()
                    .allow_origin(args[2].parse::<axum::http::HeaderValue>().unwrap())
                    .allow_methods([axum::http::Method::POST])
                    .allow_headers([axum::http::header::CONTENT_TYPE]),
            );
        let routes = api.handler().merge(test);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        rows.push(json!({"index":index,"api":format!("http://{address}"),"bank":browser_info(&bank.data)}));
        tasks.push(tokio::spawn(async move {
            axum::serve(listener, routes).await.unwrap();
        }));
    }
    std::fs::write(
        &args[1],
        serde_json::to_vec(&json!({"token":execution_bank::AUTH,"cases":rows})).unwrap(),
    )
    .unwrap();
    tokio::signal::ctrl_c().await.unwrap();
    for task in tasks {
        task.abort();
    }
}
