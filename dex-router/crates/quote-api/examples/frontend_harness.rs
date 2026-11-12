//! Explicit local integration harness; never used by the production daemon.
#[allow(dead_code)]
#[path = "../tests/support/execution_bank.rs"]
mod execution_bank;
use axum::{
    extract::State,
    middleware::{self, Next},
    response::Response,
    routing::post,
    Json, Router,
};
use execution_bank::Bank;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use xone_domain::Snapshot;
#[derive(Clone)]
struct Control {
    bank: Bank,
    api: xone_quote_api::Api,
    snapshot: Arc<Mutex<Snapshot>>,
}
#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "frontend-harness INFO_FILE FRONTEND_ORIGIN");
    let bank = Bank::new();
    let (rpc, rpc_task) = bank.serve().await;
    let (mut api, snapshot) = bank.api(&rpc);
    Arc::make_mut(&mut api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let control = Control {
        bank: bank.clone(),
        api: api.clone(),
        snapshot: Arc::new(Mutex::new(snapshot)),
    };
    let admin = Router::new()
        .route("/test/mode", post(mode))
        .with_state(control.clone());
    let routes = api
        .handler()
        .merge(admin)
        .layer(middleware::from_fn_with_state(control, delay));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::fs::write(&args[1],serde_json::to_vec_pretty(&json!({"api":format!("http://{}",listener.local_addr().unwrap()),"token":execution_bank::AUTH,"bank":&*bank.data,"amm":serde_json::from_slice::<Value>(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),"/../../test/contracts/pump-amm-sell-target.json")).unwrap()).unwrap()["cases"][33]})).unwrap()).unwrap();
    axum::serve(listener, routes).await.unwrap();
    rpc_task.abort();
}
async fn delay(State(c): State<Control>, req: axum::extract::Request, next: Next) -> Response {
    let slow = c.bank.mode.lock().unwrap().as_str() == "delay_quotes"
        && req.uri().path().starts_with("/v1/quote")
        && req.method() == "POST";
    if slow {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }
    next.run(req).await
}
async fn mode(State(c): State<Control>, Json(body): Json<Value>) -> Json<Value> {
    let mode = body["mode"].as_str().unwrap();
    *c.bank.mode.lock().unwrap() = mode.into();
    let mut s = c.snapshot.lock().unwrap();
    if mode == "suspend" {
        c.api.replica.suspend(&s.reference.block.chain_key);
    }
    if mode == "restore" {
        s.reference.online_generation += 1;
        s.reference.journal_seq += 1;
        c.api.replica.install_baseline(s.clone()).unwrap();
    }
    if mode == "advance" {
        s.reference.online_generation += 1;
        s.reference.journal_seq += 1;
        s.reference.block.number_or_slot += 1;
        s.pools[0].meteora.as_mut().unwrap().slot += 1;
        s.reference.block.parent_hash = s.reference.block.hash.clone();
        s.reference.block.hash = c.bank.data["blockhash"].as_str().unwrap().into();
        c.api.replica.install_baseline(s.clone()).unwrap();
    }
    Json(json!({"mode":mode}))
}
