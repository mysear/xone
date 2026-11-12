use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/meteora-only-y.json")).unwrap()
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
    json!({"chainKey":p.chain_key,"kind":"EXACT_IN","sellToken":a,"buyToken":b,"sellAmount":step["specified_amount"],"caller":"6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n","recipient":"HBjwLMmNpKkXR2w2qoyG6TUXeFC4r6DREQs1n3bYLj3S","slippageBps":50,"verification":"FAST"})
}
#[tokio::test]
async fn only_y_exact_input_and_output_http_use_the_declared_fee_policy() {
    let f = data();
    let mut full = 0;
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let step = &c["steps"][0];
        if step["passed"] == false {
            continue;
        }
        let (api, s) = setup(&f, index);
        let mut req = request(&s, step);
        req["sellAmount"] = step["specified_amount"].clone();
        let url = if step["exact_out"] == true {
            req["kind"] = "EXACT_OUT".into();
            req["buyAmount"] = req["sellAmount"].clone();
            req.as_object_mut().unwrap().remove("sellAmount");
            req["maxSellAmount"] = "1000000000000000".into();
            "/v1/quote-exact-out"
        } else {
            "/v1/quote"
        };
        let (code, q) = call(&api, url, req).await;
        assert_eq!(code, StatusCode::OK, "case {index}: {q}");
        assert_eq!(q["grossBuyAmount"], step["actual_output"]);
        if step["exact_out"] == true {
            assert_eq!(q["requiredSellAmount"], step["actual_input"]);
        }
        full += 1;
    }
    assert_eq!(full, 197);
}
