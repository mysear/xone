use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/meteora-exact-out.json"
    ))
    .unwrap()
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
    json!({"chainKey":p.chain_key,"kind":"EXACT_OUT","sellToken":a,"buyToken":b,"buyAmount":step["target"],"maxSellAmount":"1000000000000000","caller":"9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso","recipient":"4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE","slippageBps":50,"verification":"FAST"})
}
#[tokio::test]
async fn meteora_exact_output_http_matches_original_inputs_and_exact_recipient_credits() {
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
        let request = request(&s, &c["steps"][0]);
        let (code, q) = call(&api, "/v1/quote-exact-out", request.clone()).await;
        assert_eq!(code, StatusCode::OK, "case {index}: {q}");
        assert_eq!(q["requiredSellAmount"], c["steps"][0]["required_input"]);
        assert_eq!(q["grossBuyAmount"], request["buyAmount"]);
        assert_eq!(q["minBuyAmount"], request["buyAmount"]);
        assert_eq!(q["outputSurplus"], "0");
        assert_eq!(q["refund"], "0");
        assert_eq!(q["verification"]["level"], "INDICATIVE");
        assert!(q["netSellAmountEstimate"].is_null());
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
        let plan: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&plan).unwrap();
        assert_eq!(
            plan.allocation.routes[0].factory,
            xone_domain::meteora::PROGRAM
        );
        count += 1;
    }
    assert_eq!(count, 142);
}
#[tokio::test]
async fn meteora_exact_output_rejects_budget_width_funding_wrong_chain_and_stale_view() {
    let f = data();
    let (api, s) = setup(&f, 8);
    let req = request(&s, &f["cases"][8]["steps"][0]);
    let (code, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
    assert_eq!(code, StatusCode::OK, "{q}");
    for (key, value) in [
        ("maxSellAmount", "1"),
        ("maxSellAmount", "18446744073709551616"),
        ("buyAmount", "18446744073709551616"),
        ("caller", "0x0000000000000000000000000000000000000001"),
    ] {
        let mut bad = req.clone();
        bad[key] = value.into();
        assert_ne!(
            call(&api, "/v1/quote-exact-out", bad).await.0,
            StatusCode::OK
        );
    }
    let mut bad = req.clone();
    bad["recipient"] = s.pools[0].pool_id.clone().into();
    assert_ne!(
        call(&api, "/v1/quote-exact-out", bad).await.0,
        StatusCode::OK
    );
    api.replica.suspend("solana:mainnet-beta");
    assert_ne!(
        call(&api, "/v1/quote-exact-out", req).await.0,
        StatusCode::OK
    );
    assert_ne!(
        call(
            &api,
            &format!("/v1/quotes/{}/simulate", q["quoteId"].as_str().unwrap()),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
}
#[tokio::test]
async fn meteora_rehashed_multihop_program_namespace_and_cross_family_plans_fail() {
    let f = data();
    let (api, s) = setup(&f, 8);
    let req = request(&s, &f["cases"][8]["steps"][0]);
    let (_, q) = call(&api, "/v1/quote-exact-out", req).await;
    let (_, e) = call(
        &api,
        &format!(
            "/internal/quotes/{}/evidence",
            q["quoteId"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    let original: xone_execution_plan::exact_out::ExactOutPlan =
        serde_json::from_value(e["execution_plan"].clone()).unwrap();
    for mode in 0..4 {
        let mut p = original.clone();
        let route = &mut p.allocation.routes[0];
        if mode == 0 {
            route.factory = p.context.caller.clone();
        } else if mode == 1 {
            let intermediate = p.context.caller.clone();
            let mut second = route.edges[0].clone();
            second.pool_id = p.context.recipient.clone();
            route.edges[0].to = intermediate.clone();
            route.edges.push(second);
            route.tokens.insert(1, intermediate);
            route.amounts.insert(1, "3".parse().unwrap());
        } else if mode == 2 {
            route.edges[0].protocol = xone_domain::v2_swap::PROFILE.into();
        } else {
            p.context.recipient = route.edges[0].pool_id.clone();
        }
        p.plan_hash=hash(&json!({"engine":p.engine,"context":p.context,"snapshot":p.snapshot,"allocation":p.allocation,"min_buy_amount":p.min_buy_amount,"max_sell_amount":p.max_sell_amount,"economic_policy_hash":p.economic_policy_hash})).unwrap();
        assert!(xone_execution_plan::exact_out::verify(&p).is_err());
    }
}
