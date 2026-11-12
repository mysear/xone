use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/pump-bonding.json")).unwrap()
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
    let m = p.pump.as_ref().unwrap();
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
    let zero = step["kind"] == "sell";
    let (a, b) = if zero {
        (&p.token0, &p.token1)
    } else {
        (&p.token1, &p.token0)
    };
    json!({"chainKey":p.chain_key,"kind":"EXACT_IN","sellToken":a,"buyToken":b,"sellAmount":step["amount"],"caller":p.pump.as_ref().unwrap().user,"recipient":p.pump.as_ref().unwrap().user,"slippageBps":50,"verification":"FAST"})
}
#[tokio::test]
async fn pump_original_classic_sol_quotes_are_exact_and_complete_curves_are_excluded() {
    let f = data();
    let mut full = 0;
    for (index, c) in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, c)| c["kind"] != "buy")
    {
        let s = &c["steps"][0];
        if c["complete"] == true {
            let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
            let g = xone_routing::Graph::build(std::slice::from_ref(&p)).unwrap();
            assert!(g.adjacency.is_empty());
            assert_eq!(g.excluded_pools[&p.pool_id], "PUMP_CURVE_COMPLETE");
            continue;
        }
        let (api, snapshot) = setup(&f, index);
        let req = request(&snapshot, s);
        let (code, q) = call(&api, "/v1/quote", req.clone()).await;
        if s["passed"] == false {
            assert_ne!(code, StatusCode::OK);
            continue;
        }
        assert_eq!(code, StatusCode::OK, "case {index}: {q}");
        let expected = if c["kind"] == "sell" {
            s["user_sol_delta"].as_i64().unwrap()
        } else {
            s["user_token_delta"].as_i64().unwrap()
        };
        assert_eq!(q["grossBuyAmount"], expected.to_string());
        assert_eq!(q["verification"]["level"], "INDICATIVE");
        assert!(q["netBuyAmountEstimate"].is_null());
        let mut bad = req;
        bad["recipient"] = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE".into();
        assert_ne!(call(&api, "/v1/quote", bad).await.0, StatusCode::OK);
        full += 1;
    }
    assert_eq!(full, 9);
}

#[tokio::test]
async fn pump_exact_token_output_http_preserves_original_sol_limit_and_completion_boundary() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-exact-out.json")).unwrap();
    let mut full = 0;
    for c in f["cases"].as_array().unwrap() {
        if c["complete"] == true {
            let pool: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
            assert!(xone_routing::Graph::build(&[pool])
                .unwrap()
                .adjacency
                .is_empty());
            continue;
        }
        let selected = json!({"cases":[{"steps":[{"pool":c["pool"]}]}]});
        let (api, s) = setup(&selected, 0);
        let p = &s.pools[0];
        let user = &p.pump.as_ref().unwrap().user;
        let req = json!({"chainKey":p.chain_key,"kind":"EXACT_OUT","sellToken":p.token1,"buyToken":p.token0,"buyAmount":c["target"],"maxSellAmount":if c["passed"]==true {c["required_input"].clone()}else{json!("1000000000000000")},"caller":user,"recipient":user,"slippageBps":50,"verification":"FAST"});
        let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
        if c["passed"] == false {
            assert_ne!(status, StatusCode::OK);
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], c["required_input"]);
        assert_eq!(q["grossBuyAmount"], c["target"]);
        assert_eq!(q["unusedInputBudget"], "0");
        assert_eq!(q["outputSurplus"], "0");
        let (_, e) = call(
            &api,
            &format!(
                "/internal/quotes/{}/evidence",
                q["quoteId"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        let plan: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&plan).unwrap();
        assert_eq!(
            plan.allocation.routes[0].factory,
            xone_domain::pump::PROGRAM
        );
        let mut bad = req.clone();
        bad["maxSellAmount"] = json!((c["required_input"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            - 1)
        .to_string());
        assert_ne!(
            call(&api, "/v1/quote-exact-out", bad).await.0,
            StatusCode::OK
        );
        let mut bad = req.clone();
        bad["recipient"] = json!(p.pool_id);
        assert_ne!(
            call(&api, "/v1/quote-exact-out", bad).await.0,
            StatusCode::OK
        );
        let mut bad = req.clone();
        bad["caller"] = json!(xone_domain::pump::NATIVE_SOL);
        assert_ne!(
            call(&api, "/v1/quote-exact-out", bad).await.0,
            StatusCode::OK
        );
        full += 1;
    }
    assert_eq!(full, 7);
}

#[tokio::test]
async fn pump_amm_v1_exact_input_and_token_output_http_match_original_created_accounts() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-amm.json")).unwrap();
    let mut full = 0;
    for c in f["cases"].as_array().unwrap() {
        let step = &c["steps"][0];
        let selected = json!({"cases":[{"steps":[{"pool":step["pool"]}]}]});
        let (api, s) = setup(&selected, 0);
        let p = &s.pools[0];
        let user = &p.pump.as_ref().unwrap().user;
        let sell = c["kind"] == "sell";
        let exact = c["kind"] == "buy";
        let mut req = json!({"chainKey":p.chain_key,"kind":if exact{"EXACT_OUT"}else{"EXACT_IN"},"sellToken":if sell{&p.token0}else{&p.token1},"buyToken":if sell{&p.token1}else{&p.token0},"caller":user,"recipient":user,"slippageBps":50,"verification":"FAST"});
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
            assert_ne!(status, StatusCode::OK, "case {} accepted", c["index"]);
            continue;
        }
        assert_eq!(status, StatusCode::OK, "case {} {q}", c["index"]);
        assert_eq!(
            q["consumedIn"].as_str().unwrap(),
            (-step[if sell { "actual_base" } else { "actual_quote" }]
                .as_i64()
                .unwrap())
            .to_string()
        );
        assert_eq!(
            q["grossBuyAmount"].as_str().unwrap(),
            step[if sell { "actual_quote" } else { "actual_base" }]
                .as_i64()
                .unwrap()
                .to_string()
        );
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
            let plan: xone_execution_plan::exact_out::ExactOutPlan =
                serde_json::from_value(e["execution_plan"].clone()).unwrap();
            xone_execution_plan::exact_out::verify(&plan).unwrap();
            assert_eq!(
                plan.allocation.routes[0].factory,
                xone_domain::pump_amm::PROGRAM
            );
        }
        let mut bad = req.clone();
        bad["recipient"] = json!(p.pool_id);
        assert_ne!(call(&api, path, bad).await.0, StatusCode::OK);
        full += 1;
    }
    assert_eq!(full, 18);
}

#[tokio::test]
async fn pump_sol_target_http_preserves_ceil_fee_minimum_and_explicit_output_surplus() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/pump-sell-exact-out.json"
    ))
    .unwrap();
    let (mut full, mut surplus) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let selected = json!({"cases":[{"steps":[{"pool":c["pool"]}]}]});
        let (api, s) = setup(&selected, 0);
        let p = &s.pools[0];
        let user = &p.pump.as_ref().unwrap().user;
        let req = json!({"chainKey":p.chain_key,"kind":"EXACT_OUT","sellToken":p.token0,"buyToken":p.token1,"buyAmount":c["target"],"maxSellAmount":if c["passed"]==true{c["required_input"].clone()}else{json!("18446744073709551615")},"caller":user,"recipient":user,"slippageBps":50,"verification":"FAST"});
        let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
        if c["passed"] == false {
            assert_ne!(status, StatusCode::OK);
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["grossBuyAmount"], c["actual_output"]);
        assert_eq!(q["requiredSellAmount"], c["required_input"]);
        let delta = c["actual_output"].as_str().unwrap().parse::<u64>().unwrap()
            - c["target"].as_str().unwrap().parse::<u64>().unwrap();
        assert_eq!(q["outputSurplus"].as_str().unwrap(), delta.to_string());
        surplus += usize::from(delta > 0);
        let (_, e) = call(
            &api,
            &format!(
                "/internal/quotes/{}/evidence",
                q["quoteId"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        let plan: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&plan).unwrap();
        let mut bad = req.clone();
        bad["maxSellAmount"] = json!((c["required_input"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            - 1)
        .to_string());
        assert_ne!(
            call(&api, "/v1/quote-exact-out", bad).await.0,
            StatusCode::OK
        );
        full += 1;
    }
    assert_eq!((full, surplus), (18, 9));
}

#[tokio::test]
async fn actual_migrated_boost_pool_http_prices_virtual_inventory_but_limits_real_payout() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-migration.json")).unwrap();
    for c in f["cases"].as_array().unwrap() {
        let selected = json!({"cases":[{"steps":[{"pool":c["pool"]}]}]});
        let (api, s) = setup(&selected, 0);
        let p = &s.pools[0];
        let user = &p.pump.as_ref().unwrap().user;
        let zero = c["kind"] == "sell";
        let exact = c["kind"] == "buy";
        let mut req = json!({"chainKey":p.chain_key,"kind":if exact{"EXACT_OUT"}else{"EXACT_IN"},"sellToken":if zero{&p.token0}else{&p.token1},"buyToken":if zero{&p.token1}else{&p.token0},"caller":user,"recipient":user,"slippageBps":50,"verification":"FAST"});
        let path = if exact {
            req["buyAmount"] = c["amount"].clone();
            req["maxSellAmount"] = json!("1000000000000000");
            "/v1/quote-exact-out"
        } else {
            req["sellAmount"] = c["amount"].clone();
            "/v1/quote"
        };
        let (status, q) = call(&api, path, req).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(
            q["consumedIn"].as_str().unwrap(),
            (-c[if zero { "actual_base" } else { "actual_quote" }]
                .as_i64()
                .unwrap())
            .to_string()
        );
        assert_eq!(
            q["grossBuyAmount"].as_str().unwrap(),
            c[if zero { "actual_quote" } else { "actual_base" }]
                .as_i64()
                .unwrap()
                .to_string()
        );
    }
    let c = &f["virtual_reserve_control"];
    let selected = json!({"cases":[{"steps":[{"pool":c["pool"]}]}]});
    let (api, s) = setup(&selected, 0);
    let p = &s.pools[0];
    let user = &p.pump.as_ref().unwrap().user;
    let req = json!({"chainKey":p.chain_key,"kind":"EXACT_IN","sellToken":p.token0,"buyToken":p.token1,"sellAmount":c["amount"],"caller":user,"recipient":user,"slippageBps":50,"verification":"FAST"});
    assert_ne!(call(&api, "/v1/quote", req).await.0, StatusCode::OK);
}
