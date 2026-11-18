//! Public HTTP regression against a small independent integer allocation oracle.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::Snapshot;
use xone_quote_api::Api;

fn instance(cost: &str, max_splits: usize, evaluations: usize) -> Api {
    let mut cfg = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    cfg.token = "cost-fallback-http-regression-credential".into();
    cfg.config.routing.max_splits = max_splits;
    cfg.config.routing.split_parts = 6;
    cfg.config.routing.max_expansions = evaluations;
    cfg.config.routing.route_activation_cost = cost.parse().unwrap();
    cfg.config.api.quote_timeout_ms = 5000;
    let api = Api::new(cfg).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let mut pool = snapshot.pools.last().unwrap().clone();
    pool.reserve0 = "1000".parse().unwrap();
    pool.reserve1 = "1000".parse().unwrap();
    snapshot.pools = (0..3)
        .map(|i| {
            let mut p = pool.clone();
            p.pool_id = format!("fallback-ac-{i}");
            p
        })
        .collect();
    snapshot.reference.scope_hash = xone_domain::scope_hash(&snapshot.pools).unwrap();
    api.replica.install_baseline(snapshot).unwrap();
    api
}
async fn quote(api: &Api) -> Value {
    let req = json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":"A","buyToken":"C","sellAmount":"903","caller":"0x0000000000000000000000000000000000000001","recipient":"0x0000000000000000000000000000000000000002","slippageBps":50,"verification":"FAST"});
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/quote")
                .header("authorization", format!("Bearer {}", api.config.token))
                .body(Body::from(req.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}
fn oracle(cost: u128) -> (u128, usize) {
    let mut best = (0, 0, 0);
    for a in 0..=6 {
        for b in 0..=6 - a {
            let (mut used, mut gross, mut count) = (0u128, 0u128, 0usize);
            for units in [a, b, 6 - a - b] {
                let input = 903 * (used + units) / 6 - 903 * used / 6;
                used += units;
                if input > 0 {
                    gross += input * 997 * 1000 / (1000 * 1000 + input * 997);
                    count += 1;
                }
            }
            let score = gross.saturating_sub(cost * count as u128);
            if (score, gross) > (best.0, best.1) {
                best = (score, gross, count);
            }
        }
    }
    (best.1, best.2)
}
fn disclosure(q: &Value, cost: &str) {
    let costs = &q["costs"];
    let ranking = &q["quality"]["costRanking"];
    assert_eq!(costs["proposalActivationCostBuyToken"], "0");
    assert_eq!(costs["ignoredConfiguredActivationCostBuyToken"], cost);
    assert_eq!(costs["configuredActivationCostApplied"], false);
    assert_eq!(costs["chainFeeCompleteness"], "UNAVAILABLE");
    for field in [
        "gasUnits",
        "gasNative",
        "l1DataNative",
        "approvalNative",
        "totalNativeEstimate",
    ] {
        assert!(costs[field].is_null(), "{field}");
    }
    assert!(q["netBuyAmountEstimate"].is_null());
    assert_eq!(ranking["version"], "xone-cost-unavailable-ranking/1");
    assert_eq!(ranking["objective"], "MAXIMUM_GROSS_BUY_AMOUNT");
    assert_eq!(ranking["costAdjustmentApplied"], false);
    assert_eq!(q["verification"]["level"], "INDICATIVE");
}
#[tokio::test]
async fn unavailable_cost_exact_in_ignores_unvalued_penalty_and_matches_independent_grid() {
    assert_eq!(oracle(0), (690, 3));
    assert_eq!(oracle(100), (620, 2));
    for cost in ["0", "100", "340282366920938463463374607431768211456"] {
        let q = quote(&instance(cost, 3, 1000)).await;
        assert_eq!(q["grossBuyAmount"], "690");
        assert_eq!(q["routes"].as_array().unwrap().len(), 3);
        assert_eq!(q["consumedIn"], "903");
        assert_eq!(q["refund"], "0");
        assert_eq!(
            q["quality"]["allocationSearch"]["exhaustiveWithinGrid"],
            true
        );
        disclosure(&q, cost);
    }
}
#[tokio::test]
async fn unavailable_cost_fallback_preserves_user_constraints_and_budget_exhaustion() {
    let q = quote(&instance("100", 1, 1000)).await;
    assert_eq!(q["routes"].as_array().unwrap().len(), 1);
    assert_eq!(q["grossBuyAmount"], "473");
    disclosure(&q, "100");
    let q = quote(&instance("100", 3, 20)).await;
    assert_eq!(q["quality"]["budgetExhausted"], true);
    assert_eq!(
        q["quality"]["allocationSearch"]["exhaustiveWithinGrid"],
        false
    );
    disclosure(&q, "100");
}
