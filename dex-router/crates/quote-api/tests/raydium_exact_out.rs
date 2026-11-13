use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
async fn call(api: &Api, path: &str, value: Value) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method(if path == "/v1/quote-options" {
                    "GET"
                } else {
                    "POST"
                })
                .uri(path)
                .header("authorization", format!("Bearer {}", api.config.token))
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
async fn raydium_output_target_http_plan_and_protocol_view_match_original() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/raydium-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut count = 0;
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["expected"]["reverted"] != true && c["target"] == "1000")
        .take(24)
    {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let mut config = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            "dex-router",
        )
        .unwrap();
        config.config.chains[1].key = p.chain_key.clone();
        config.config.routing.max_splits = 1;
        config.config.routing.max_state_age_ms = 315_360_000_000;
        config.config.api.requests_per_minute = 10000;
        let api = Api::new(config).unwrap();
        let mut snapshot: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        let state = p.raydium_cpmm.as_ref().unwrap();
        snapshot.reference.block.chain_key = p.chain_key.clone();
        snapshot.reference.block.number_or_slot = state.bank_slot;
        snapshot.reference.block.timestamp_ms = state.bank_unix_timestamp * 1000;
        snapshot.reference.block.hash = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d".into();
        snapshot.reference.block.parent_hash =
            "6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n".into();
        snapshot.reference.owner_service = "chain-parser-rust".into();
        snapshot.pools = vec![p.clone()];
        snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
        snapshot.reference.adapter_set_hash = hash(&vec![p.adapter_version.clone()]).unwrap();
        snapshot.source = "CONTROLLED_ORIGINAL_NATIVE_HELPER_NOT_SBF".into();
        api.replica.install_baseline(snapshot).unwrap();
        let zero = c["zero_for_one"].as_bool().unwrap();
        let request = json!({"chainKey":p.chain_key,"kind":"EXACT_OUT","sellToken":if zero{&p.token0}else{&p.token1},"buyToken":if zero{&p.token1}else{&p.token0},"buyAmount":"1000","maxSellAmount":u64::MAX.to_string(),"caller":"9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso","recipient":"4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE","slippageBps":50,"verification":"FAST"});
        let (status, q) = call(&api, "/v1/quote-exact-out", request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], c["expected"]["amount_in"]);
        assert_eq!(q["grossBuyAmount"], "1000");
        assert_eq!(q["outputSurplus"], "0");
        assert_eq!(q["verification"]["level"], "INDICATIVE");
        let (status, options) = call(&api, "/v1/quote-options", Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        let chain = options["chains"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["chainKey"] == p.chain_key)
            .unwrap();
        assert_eq!(
            chain["protocolViews"][0]["capabilities"]["quoteModes"],
            json!(["EXACT_IN", "EXACT_OUT"])
        );
        assert_eq!(chain["protocolViews"][0]["ready"], true);
        assert_eq!(
            chain["pools"][0]["protocolViewId"],
            chain["protocolViews"][0]["viewId"]
        );
        assert_eq!(chain["pools"][0]["execution"]["preparation"], false);
        let mut low = request;
        low["maxSellAmount"] = (c["expected"]["amount_in"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            - 1)
        .to_string()
        .into();
        let (status, _) = call(&api, "/v1/quote-exact-out", low).await;
        assert_ne!(status, StatusCode::OK);
        count += 1;
    }
    assert_eq!(count, 24);
}
