//! In-process HTTP quote scope; actual process/VM execution belongs to B5.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Snapshot};
use xone_quote_api::Api;
fn api_source(id: &str) -> (Api, Snapshot, Value) {
    let v: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-breaker-pools.json"
    ))
    .unwrap();
    let c = v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
        .clone();
    let b = v["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == c["before"])
        .unwrap();
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.pools = b["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| serde_json::from_value(p["pool"].clone()).unwrap())
        .collect();
    let selected_pool = b["pools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == c["pool_name"])
        .unwrap()["pool"]["pool_id"]
        .as_str()
        .unwrap();
    // This explicit original single-pool scope tests callback rejection;
    // multi-pool alternative quotes are separately replayed from actual plans.
    s.pools.retain(|p| p.pool_id == selected_pool);
    let number =
        |v: &Value| u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    s.reference.block.number_or_slot = number(&b["header"]["number"]);
    s.reference.block.timestamp_ms = number(&b["header"]["timestamp"]) * 1000;
    s.reference.block.hash = b["header"]["hash"].as_str().unwrap().into();
    s.reference.block.parent_hash = b["header"]["parentHash"].as_str().unwrap().into();
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    let mut adapters = s
        .pools
        .iter()
        .map(|p| p.adapter_version.clone())
        .collect::<Vec<_>>();
    adapters.sort();
    adapters.dedup();
    s.reference.adapter_set_hash = hash(&adapters).unwrap();
    let mut config = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    config.token = "local-breaker-scope-explicit-credential".into();
    config.config.routing.max_hops = 2;
    config.config.routing.max_splits = 2;
    config.config.routing.split_parts = 10;
    config.config.api.quote_timeout_ms = 10000;
    let api = Api::new(config).unwrap();
    api.replica.install_baseline(s.clone()).unwrap();
    (api, s, c)
}
async fn quote(api: &Api, c: &Value, caller: &str) -> (StatusCode, Value) {
    let response=api.handler().oneshot(Request::builder().method("POST").uri("/v1/quote").header("authorization",format!("Bearer {}",api.config.token)).header("content-type","application/json").body(Body::from(json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":c["token_in"],"buyToken":c["token_out"],"sellAmount":c["amount_in"],"caller":caller,"recipient":caller,"slippageBps":50,"verification":"FAST","constraints":{"maxHops":1}}).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let body =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    (status, body)
}
#[tokio::test]
async fn breaker_http_preserves_amount_dependent_success_and_business_rejection() {
    let v: Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/contracts/managed-v2-breaker-pools.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for label in [
        "cached-original:A:asset-0:join",
        "cached-original:A:asset-0:exit",
        "start+1:A:asset-1:join",
        "subunit-upper-small-join-trips",
        "subunit-upper-large-join-recovers",
        "dust-BPT-exit-internal-bound-trips",
        "dust-regular-internal-bound-trips",
        "dust-regular-disabled-zero-credit",
    ] {
        let c = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["label"] == label)
            .unwrap();
        let (api, _, c) = api_source(c["id"].as_str().unwrap());
        let (status, body) = quote(&api, &c, c["actor"].as_str().unwrap()).await;
        if c["expected_native"]["status"] == 1 && c["expected_native"]["actual_amount_out"] != "0" {
            assert_eq!(status, StatusCode::OK, "{label}: {body}");
            assert_eq!(
                body["grossBuyAmount"],
                c["expected_native"]["actual_amount_out"]
            );
            assert_eq!(body["verification"]["level"], "INDICATIVE");
            assert_eq!(body["verification"]["success"], false);
        } else {
            assert!(
                (status == StatusCode::NOT_FOUND && body["error"] == "NO_ROUTE")
                    || (status == StatusCode::BAD_REQUEST
                        && body["error"] == "NO_EXECUTABLE_ALLOCATION"),
                "{label}: {status} {body}"
            );
        }
    }
}
