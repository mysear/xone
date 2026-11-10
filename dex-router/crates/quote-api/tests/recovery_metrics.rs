use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;
use xone_domain::Snapshot;
use xone_quote_api::Api;
async fn text(api: &Api, path: &str) -> String {
    let response = api
        .handler()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}
#[tokio::test]
async fn recovery_metrics_require_qualified_scope_and_a_real_successful_quote() {
    let cfg = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    let api = Api::new(cfg).unwrap();
    let before = text(&api, "/metrics").await;
    assert!(!before.contains("xone_first_qualified_snapshot_seconds"));
    assert!(!before.contains("xone_first_successful_quote_seconds"));
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    api.replica.install_baseline(snapshot).unwrap();
    let ready = api
        .handler()
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let scoped = text(&api, "/metrics").await;
    assert!(
        scoped.contains("xone_first_qualified_snapshot_seconds{chain=\"evm:31337\"}"),
        "{scoped}"
    );
    assert!(!scoped.contains("xone_first_successful_quote_seconds"));
    let body = json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":"A","buyToken":"C","sellAmount":"10003","caller":"0x0000000000000000000000000000000000000001","recipient":"0x0000000000000000000000000000000000000002","slippageBps":50,"verification":"FAST"});
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/quote")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let quoted = text(&api, "/metrics").await;
    assert!(
        quoted.contains("xone_first_successful_quote_seconds{chain=\"evm:31337\"}"),
        "{quoted}"
    );
}
