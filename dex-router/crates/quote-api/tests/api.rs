use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::Snapshot;
use xone_quote_api::{Api, QuoteRequest};
fn config() -> xone_config::ConfigSnapshot {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    cfg.token = "explicit-api-test-credential-123456".into();
    cfg
}
fn baseline() -> Snapshot {
    serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap()
}
fn request() -> Value {
    json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":"A","buyToken":"C","sellAmount":"10003","caller":"0x0000000000000000000000000000000000000001","recipient":"0x0000000000000000000000000000000000000002","slippageBps":50,"verification":"FAST"})
}
#[tokio::test]
async fn api_scope_string_amount_and_verification_semantics() {
    let api = Api::new(config()).unwrap();
    let handler = api.handler();
    let body = request();
    let response = handler
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/quote")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    api.replica.install_baseline(baseline()).unwrap();
    let token = api.config.token.clone();
    let call = |v: Value| {
        Request::builder()
            .method("POST")
            .uri("/v1/quote")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(v.to_string()))
            .unwrap()
    };
    let mut bad = body.clone();
    bad["sellAmount"] = json!(10003);
    assert_eq!(
        handler.clone().oneshot(call(bad)).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    let response = handler.clone().oneshot(call(body.clone())).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let v: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(v["consumedIn"], "10003");
    assert_eq!(v["verification"]["level"], "INDICATIVE");
    assert_eq!(v["refund"], "0");
    assert_eq!(v["quality"]["historyIndependent"], true);
    let mut verified = body;
    verified["verification"] = json!("VERIFIED");
    assert_eq!(
        handler.oneshot(call(verified)).await.unwrap().status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
#[tokio::test]
async fn duplicate_request_is_single_flight_and_reorg_rejects_old_build() {
    let api = Api::new(config()).unwrap();
    let mut snapshot = baseline();
    api.replica.install_baseline(snapshot.clone()).unwrap();
    let r: QuoteRequest = serde_json::from_value(request()).unwrap();
    let (a, b) = tokio::join!(api.quote(r.clone()), api.quote(r));
    let a = a.unwrap();
    assert_eq!(a["quoteId"], b.unwrap()["quoteId"]);
    snapshot.reference.online_generation += 1;
    snapshot.reference.canonical_epoch += 1;
    snapshot.reference.journal_seq += 1;
    api.replica.install_baseline(snapshot).unwrap();
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/quotes/{}/build",
                    a["quoteId"].as_str().unwrap()
                ))
                .header("authorization", format!("Bearer {}", api.config.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
#[tokio::test]
async fn per_tenant_quota_is_enforced() {
    let mut c = config();
    c.config.api.requests_per_minute = 1;
    let api = Api::new(c).unwrap();
    api.replica.install_baseline(baseline()).unwrap();
    for status in [StatusCode::OK, StatusCode::TOO_MANY_REQUESTS] {
        let response = api
            .handler()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/quote")
                    .header("authorization", format!("Bearer {}", api.config.token))
                    .body(Body::from(request().to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
}
