#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use execution_bank::{Bank, AUTH};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(api: &xone_quote_api::Api, path: &str, value: Value) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {AUTH}"))
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status(),
        serde_json::from_slice(&to_bytes(response.into_body(), 4 << 20).await.unwrap()).unwrap(),
    )
}
#[tokio::test]
async fn api_independent_sbf_binds_actual_preparation_and_rejects_quote_or_view_changes() {
    let bank = Bank::token2022(1);
    let (url, task) = bank.serve().await;
    let (mut config, _) = bank.api(&url);
    let config = std::sync::Arc::make_mut(&mut config.config);
    config.config.api.solana_runtime_manifest = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/contracts/sbf-runtime/token2022.json"
    )
    .into();
    let api = xone_quote_api::Api::new(config.clone()).unwrap();
    // Install the same immutable parser bank; no execution fixture is an RPC substitute.
    let (_, snapshot) = bank.api(&url);
    api.replica.install_baseline(snapshot.clone()).unwrap();
    let (status, q) = call(&api, "/v1/quote-exact-out", bank.request()).await;
    assert_eq!(status, StatusCode::OK, "{q}");
    let id = q["quoteId"].as_str().unwrap();
    let (status, p) = call(
        &api,
        &format!("/v1/quotes/{id}/prepare"),
        json!({"sourceTokenAccount":bank.data["sourceTokenAccount"],"createDestinationAta":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!(p["ready"], true);
    assert!(p.get("simulation").is_none());
    let prep = p["preparationHash"].as_str().unwrap();
    let path = format!("/v1/quotes/{id}/preparations/{prep}/simulate-independent");
    let (status, e) = call(&api, &path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    assert_eq!(e["level"], "LOCAL_SBF_SIMULATED");
    assert_eq!(e["actualInput"], "1502");
    assert_eq!(e["actualOutput"], "1000");
    assert_eq!(e["sourceBankUnchanged"], true);
    assert_eq!(e["preparationHash"], p["preparationHash"]);
    assert_eq!(e["planHash"], q["planHash"]);
    assert_eq!(e["protocolViewIds"], q["protocolViewIds"]);
    assert_eq!(e["independentProductionVerification"], false);
    assert_eq!(e["submitted"], false);
    let (status, e) = call(
        &api,
        &format!("/v1/quotes/other/preparations/{prep}/simulate-independent"),
        Value::Null,
    )
    .await;
    assert_ne!(status, StatusCode::OK);
    assert_eq!(e["error"], "PREPARATION_QUOTE_MISMATCH");
    let mut newer = snapshot;
    newer.reference.online_generation += 1;
    newer.reference.journal_seq += 1;
    api.replica.install_baseline(newer).unwrap();
    let (status, e) = call(&api, &path, Value::Null).await;
    assert_ne!(status, StatusCode::OK);
    assert_eq!(e["error"], "SNAPSHOT_REORGED");
    task.abort();
}
