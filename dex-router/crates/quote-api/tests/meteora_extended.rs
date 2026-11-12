use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
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
                .method(if url.contains("/evidence") || url == "/v1/quote-options" {
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
#[tokio::test]
async fn extended_dlmm_profiles_http_use_original_net_credits_and_complete_amounts() {
    for (file, expected) in [
        ("meteora-bitmap.json", 34usize),
        ("meteora-rewards.json", 120),
        ("meteora-orders.json", 64),
        ("meteora-orders-exact-out.json", 161),
        ("meteora-token2022.json", 80),
    ] {
        let f: Value = serde_json::from_slice(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../test/contracts")
                    .join(file),
            )
            .unwrap(),
        )
        .unwrap();
        let mut full = 0;
        for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let step = &c["steps"][0];
            let (api, s) = setup(&f, index);
            let p = &s.pools[0];
            let zero = c["zero_for_one"].as_bool().unwrap();
            let exact = c["exact_out"].as_bool().unwrap();
            let mut req = json!({"chainKey":p.chain_key,"kind":if exact{"EXACT_OUT"}else{"EXACT_IN"},"sellToken":if zero{&p.token0}else{&p.token1},"buyToken":if zero{&p.token1}else{&p.token0},"caller":"6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n","recipient":"HBjwLMmNpKkXR2w2qoyG6TUXeFC4r6DREQs1n3bYLj3S","slippageBps":50,"verification":"FAST"});
            let path = if exact {
                req["buyAmount"] = step["amount"].clone();
                req["maxSellAmount"] = json!("1000000000000000");
                "/v1/quote-exact-out"
            } else {
                req["sellAmount"] = step["amount"].clone();
                "/v1/quote"
            };
            let (status, q) = call(&api, path, req.clone()).await;
            if step["passed"] == false {
                assert_ne!(status, StatusCode::OK, "{file} {index} accepted");
                continue;
            }
            assert_eq!(status, StatusCode::OK, "{file} {index} {q}");
            assert_eq!(q["grossBuyAmount"], step["actual_output"]);
            assert_eq!(q["consumedIn"], step["actual_input"]);
            let (_, e) = call(
                &api,
                &format!(
                    "/internal/quotes/{}/evidence",
                    q["quoteId"].as_str().unwrap()
                ),
                Value::Null,
            )
            .await;
            if exact {
                let p: xone_execution_plan::exact_out::ExactOutPlan =
                    serde_json::from_value(e["execution_plan"].clone()).unwrap();
                xone_execution_plan::exact_out::verify(&p).unwrap();
                let mut bad = req.clone();
                bad["maxSellAmount"] = json!((step["actual_input"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
                    - 1)
                .to_string());
                assert_ne!(call(&api, path, bad).await.0, StatusCode::OK);
            }
            full += 1;
        }
        assert_eq!(full, expected, "{file}");
    }
}

#[tokio::test]
async fn orders_exact_out_rehashed_composed_plans_and_stale_views_are_rejected() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-orders-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let (api, s) = setup(&f, 87);
    let p = &s.pools[0];
    let req = json!({"chainKey":p.chain_key,"kind":"EXACT_OUT","sellToken":p.token0,"buyToken":p.token1,"buyAmount":"1000","maxSellAmount":"1000000000000000","caller":"6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n","recipient":"HBjwLMmNpKkXR2w2qoyG6TUXeFC4r6DREQs1n3bYLj3S","slippageBps":50,"verification":"FAST"});
    let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
    assert_eq!(status, StatusCode::OK, "{q}");
    let (_, evidence) = call(
        &api,
        &format!(
            "/internal/quotes/{}/evidence",
            q["quoteId"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    let original: xone_execution_plan::exact_out::ExactOutPlan =
        serde_json::from_value(evidence["execution_plan"].clone()).unwrap();
    for mode in 0..4 {
        let mut p = original.clone();
        let route = &mut p.allocation.routes[0];
        match mode {
            0 => route.factory = p.context.caller.clone(),
            1 => {
                let intermediate = p.context.caller.clone();
                let mut second = route.edges[0].clone();
                second.pool_id = p.context.recipient.clone();
                route.edges[0].to = intermediate.clone();
                route.edges.push(second);
                route.tokens.insert(1, intermediate);
                route.amounts.insert(1, "3".parse().unwrap());
            }
            2 => route.edges[0].protocol = xone_domain::v2_swap::PROFILE.into(),
            _ => p.context.recipient = route.edges[0].pool_id.clone(),
        }
        p.plan_hash = hash(&json!({"engine":p.engine,"context":p.context,"snapshot":p.snapshot,"allocation":p.allocation,"min_buy_amount":p.min_buy_amount,"max_sell_amount":p.max_sell_amount,"economic_policy_hash":p.economic_policy_hash})).unwrap();
        assert!(xone_execution_plan::exact_out::verify(&p).is_err());
    }
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
async fn quote_options_exposes_current_chain_pool_tokens_and_protocols() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-orders-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let (api, snapshot) = setup(&f, 87);
    let (status, options) = call(&api, "/v1/quote-options", Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{options}");
    let chain = options["chains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["chainKey"] == snapshot.reference.block.chain_key)
        .unwrap();
    assert_eq!(chain["ready"], true);
    assert!(chain["viewId"].as_str().is_some());
    assert!(chain["graphVersion"].as_u64().is_some());
    assert_eq!(chain["poolCount"], 1);
    assert!(chain["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .any(|protocol| protocol == &snapshot.pools[0].protocol));
    assert!(chain["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .any(|token| token.as_str() == Some(snapshot.pools[0].token0.as_str())));
    let pool = chain["pools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pool| pool["protocol"] == snapshot.pools[0].protocol)
        .unwrap();
    assert!(pool["quoteModes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|mode| mode == "EXACT_IN"));
    assert!(pool["quoteModes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|mode| mode == "EXACT_OUT"));
    assert_eq!(pool["execution"]["quote"], true);
    assert_eq!(pool["execution"]["unsignedPlan"], false);
}
