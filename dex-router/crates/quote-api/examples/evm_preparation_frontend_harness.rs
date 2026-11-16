//! Loopback RPC using retained original state; no signing, sending or production qualification.
#[path = "../tests/support/evm_bank.rs"]
mod bank;
use serde_json::json;
use std::sync::Arc;
#[tokio::main]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "INFO FRONTEND_ORIGIN");
    let (mut api, snapshot, request, _, rpc_task) = bank::setup("ready").await;
    Arc::make_mut(&mut api.config)
        .config
        .api
        .cors_allowed_origins = vec![args[2].clone()];
    let token = api.config.token.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::fs::write(
        &args[1],
        serde_json::to_vec(
            &json!({"api":url,"token":token,"request":request,"snapshotRef":snapshot.reference}),
        )
        .unwrap(),
    )
    .unwrap();
    let result = axum::serve(listener, api.handler())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    rpc_task.abort();
    result.unwrap();
}
