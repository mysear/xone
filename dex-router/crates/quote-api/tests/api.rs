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

#[tokio::test]
async fn actual_quote_uses_three_physical_routes_and_reports_grid_search() {
    let mut cfg = config();
    cfg.config.routing.max_splits = 3;
    cfg.config.routing.split_parts = 6;
    cfg.config.routing.max_expansions = 1_000;
    cfg.config.api.quote_timeout_ms = 5_000;
    let api = Api::new(cfg).unwrap();
    let mut snapshot = baseline();
    let mut direct = snapshot.pools.last().unwrap().clone();
    direct.reserve0 = "1000".parse().unwrap();
    direct.reserve1 = "1000".parse().unwrap();
    snapshot.pools = (0..3)
        .map(|i| {
            let mut p = direct.clone();
            p.pool_id = format!("ac-{i}");
            p
        })
        .collect();
    snapshot.reference.scope_hash = xone_domain::scope_hash(&snapshot.pools).unwrap();
    api.replica.install_baseline(snapshot).unwrap();
    let mut req = request();
    req["sellAmount"] = json!("903");
    let quote = api
        .quote(serde_json::from_value(req).unwrap())
        .await
        .unwrap();
    assert_eq!(quote["routes"].as_array().unwrap().len(), 3);
    assert_eq!(quote["consumedIn"], "903");
    assert_eq!(quote["refund"], "0");
    assert_eq!(quote["grossBuyAmount"], "690");
    assert_eq!(
        quote["quality"]["allocationSearch"]["exhaustiveWithinGrid"],
        true
    );
    assert_eq!(quote["quality"]["allocationSearch"]["maxSplits"], 3);
    assert_eq!(quote["quality"]["budgetExhausted"], false);
    // Gross mathematical output does not establish chain-fee-adjusted returns.
    assert!(quote["netBuyAmountEstimate"].is_null());
    assert_eq!(quote["verification"]["level"], "INDICATIVE");
}

#[tokio::test]
async fn allocation_work_limit_is_visible_even_when_candidate_search_finishes() {
    let mut cfg = config();
    cfg.config.routing.max_splits = 4;
    cfg.config.routing.max_expansions = 20;
    cfg.config.api.quote_timeout_ms = 5_000;
    let api = Api::new(cfg).unwrap();
    api.replica.install_baseline(baseline()).unwrap();
    let quote = api
        .quote(serde_json::from_value(request()).unwrap())
        .await
        .unwrap();
    assert_eq!(quote["quality"]["allocationSearch"]["evaluations"], 20);
    assert_eq!(
        quote["quality"]["allocationSearch"]["exhaustiveWithinGrid"],
        false
    );
    assert_eq!(quote["quality"]["budgetExhausted"], true);
    assert_eq!(quote["consumedIn"], "10003");
    assert_eq!(quote["refund"], "0");
}

#[tokio::test]
async fn unsupported_math_and_token_behavior_are_excluded_from_public_candidate_scope() {
    let api = Api::new(config()).unwrap();
    let mut s = baseline();
    let original_ids = s.pools.iter().map(|p| json!(p.pool_id)).collect::<Vec<_>>();
    let mut unknown = s.pools[0].clone();
    unknown.pool_id = "unsupported-math-pool".into();
    unknown.protocol = "unknown-swap-math".into();
    unknown.adapter_version = "unknown-swap-math/1".into();
    unknown.validate().unwrap();
    let mut token = s.pools[0].clone();
    token.pool_id = "unsupported-token-pool".into();
    token.token_behavior = "simulation-only".into();
    token.validate().unwrap();
    s.pools.extend([unknown, token]);
    s.reference.scope_hash = xone_domain::scope_hash(&s.pools).unwrap();
    let mut adapters = s
        .pools
        .iter()
        .map(|p| p.adapter_version.clone())
        .collect::<Vec<_>>();
    adapters.sort();
    adapters.dedup();
    s.reference.adapter_set_hash = xone_domain::hash(&adapters).unwrap();
    api.replica.install_baseline(s).unwrap();
    let result = api
        .quote(serde_json::from_value(request()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        result["quality"]["candidateScope"],
        json!(original_ids),
        "unsupported source rows are not searchable physical edges"
    );
    assert_eq!(
        result["quality"]["excludedPools"]["unsupported-math-pool"],
        "PROTOCOL_MATH_UNSUPPORTED"
    );
    assert_eq!(
        result["quality"]["excludedPools"]["unsupported-token-pool"],
        "TOKEN_BEHAVIOR_UNSUPPORTED"
    );
    assert_eq!(result["consumedIn"], "10003");
    assert_eq!(result["refund"], "0");
}
