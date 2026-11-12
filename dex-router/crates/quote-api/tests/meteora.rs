use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/meteora-mm.json")).unwrap()
}
fn setup(f: &Value, index: usize) -> (Api, Snapshot) {
    let mut c = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    c.token = "meteora-http-local-test".into();
    c.config.chains[1].key = "solana:mainnet-beta".into();
    c.config.routing.max_state_age_ms = 315_360_000_000;
    c.config.api.quote_timeout_ms = 30000;
    c.config.routing.max_splits = 1;
    c.config.routing.split_parts = 4;
    let api = Api::new(c).unwrap();
    let p: Pool = serde_json::from_value(f["cases"][index]["steps"][0]["pool"].clone()).unwrap();
    let m = p.meteora.as_ref().unwrap();
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.reference.block.chain_key = p.chain_key.clone();
    s.reference.block.number_or_slot = m.slot;
    s.reference.block.timestamp_ms = m.timestamp * 1000;
    s.reference.block.hash = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d".into();
    s.reference.block.parent_hash = "6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n".into();
    s.reference.owner_service = "chain-parser-rust".into();
    s.pools = vec![p];
    s.source = "CONTROLLED_RETAINED_SBF_MM_BANK".into();
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    s.reference.adapter_set_hash = hash(&vec![s.pools[0].adapter_version.clone()]).unwrap();
    api.replica.install_baseline(s.clone()).unwrap();
    (api, s)
}
async fn call(api: &Api, url: &str, body: Value) -> (StatusCode, Value) {
    let r = api
        .handler()
        .oneshot(
            Request::builder()
                .method(if url.contains("/evidence") {
                    "GET"
                } else {
                    "POST"
                })
                .uri(url)
                .header("authorization", format!("Bearer {}", api.config.token))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status();
    let value = serde_json::from_slice(&to_bytes(r.into_body(), 4 << 20).await.unwrap()).unwrap();
    (status, value)
}
fn request(s: &Snapshot, step: &Value) -> Value {
    let p = &s.pools[0];
    let zero = step["zero_for_one"].as_bool().unwrap();
    let (a, b) = if zero {
        (&p.token0, &p.token1)
    } else {
        (&p.token1, &p.token0)
    };
    json!({"chainKey":p.chain_key,"kind":"EXACT_IN","sellToken":a,"buyToken":b,"sellAmount":step["input"],"caller":"6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n","recipient":"HBjwLMmNpKkXR2w2qoyG6TUXeFC4r6DREQs1n3bYLj3S","slippageBps":50,"verification":"FAST"})
}
#[tokio::test]
async fn http_quote_amounts_plans_and_source_scope_match_original_meteora_banks() {
    let f = data();
    let mut count = 0;
    for (index, c) in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, c)| c["steps"][0]["passed"] == true)
    {
        let (api, s) = setup(&f, index);
        let (code, q) = call(&api, "/v1/quote", request(&s, &c["steps"][0])).await;
        assert_eq!(code, StatusCode::OK, "case {index}: {q}");
        assert_eq!(q["grossBuyAmount"], c["steps"][0]["output"]);
        assert_eq!(q["refund"], "0");
        assert_eq!(q["verification"]["level"], "INDICATIVE");
        assert_eq!(q["quality"]["optimalityClaim"], "HEURISTIC_WITHIN_SCOPE");
        assert!(q["netBuyAmountEstimate"].is_null());
        let (code, e) = call(
            &api,
            &format!(
                "/internal/quotes/{}/evidence",
                q["quoteId"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        let plan: xone_execution_plan::Plan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::verify_plan_hash(&plan).unwrap();
        let replay = xone_optimizer::evaluate_allocation(
            &s.pools,
            &plan.allocation.routes,
            &xone_domain::Amount::zero(),
        )
        .unwrap();
        assert_eq!(
            replay.gross_buy_amount.to_string(),
            c["steps"][0]["output"].as_str().unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 35);
}
#[tokio::test]
async fn pool_funding_and_reorg_cache_invalidation_fail_closed() {
    let f = data();
    let (api, s) = setup(&f, 3);
    let req = request(&s, &f["cases"][3]["steps"][0]);
    let (code, q) = call(&api, "/v1/quote", req.clone()).await;
    assert_eq!(code, StatusCode::OK, "{q}");
    let mut invalid = req.clone();
    invalid["recipient"] = s.pools[0].pool_id.clone().into();
    let (code, _) = call(&api, "/v1/quote", invalid).await;
    assert_ne!(code, StatusCode::OK);
    api.replica.suspend("solana:mainnet-beta");
    let (code, _) = call(&api, "/v1/quote", req).await;
    assert_ne!(code, StatusCode::OK);
    let (code, _) = call(
        &api,
        &format!("/v1/quotes/{}/simulate", q["quoteId"].as_str().unwrap()),
        json!({}),
    )
    .await;
    assert_ne!(code, StatusCode::OK);
}
