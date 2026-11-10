//! Independent ExactOut HTTP contract over original-program banks.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Snapshot};
use xone_execution_plan::exact_out::{verify, ExactOutPlan};
use xone_quote_api::Api;

fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/v2-exact-out.json")).unwrap()
}
fn source(f: &Value) -> Snapshot {
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.pools = serde_json::from_value(f["grid"]["pools"].clone()).unwrap();
    let clock = s.pools[0].v2.as_ref().unwrap().swap.as_ref().unwrap();
    s.reference.block.number_or_slot = clock.bank_block_number;
    s.reference.block.hash = clock.bank_block_hash.clone();
    s.reference.block.timestamp_ms = clock.bank_unix_timestamp * 1000;
    s.reference.block.parent_hash = f["cases"][0]["source"]["header"]["parent_hash"]
        .as_str()
        .unwrap()
        .into();
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    let mut adapters = s
        .pools
        .iter()
        .map(|p| p.adapter_version.clone())
        .collect::<Vec<_>>();
    adapters.sort();
    adapters.dedup();
    s.reference.adapter_set_hash = hash(&adapters).unwrap();
    s
}
fn api(s: &Snapshot) -> Api {
    let mut c = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    c.token = "exact-output-http-regression-credential".into();
    c.config.routing.max_state_age_ms = 315_360_000_000;
    c.config.routing.max_hops = 3;
    c.config.routing.max_splits = 3;
    c.config.routing.split_parts = 4;
    c.config.routing.max_expansions = 10000;
    c.config.routing.max_candidates = 10;
    c.config.api.quote_timeout_ms = 10000;
    let api = Api::new(c).unwrap();
    api.replica.install_baseline(s.clone()).unwrap();
    api
}
fn request(f: &Value) -> Value {
    json!({"chainKey":"evm:31337", "kind":"EXACT_OUT", "sellToken": f["grid"]["paths"][0][0],
        "buyToken": f["grid"]["paths"][0][1], "buyAmount": f["grid"]["target_output"],
        "maxSellAmount": f["grid"]["max_input"], "caller": f["context"]["sender"],
        "recipient": f["context"]["recipient"], "slippageBps": 50, "verification":"FAST"})
}
async fn call(
    api: &Api,
    method: &str,
    url: &str,
    body: Value,
    authorized: bool,
) -> (StatusCode, Value) {
    let mut r = Request::builder()
        .method(method)
        .uri(url)
        .header("content-type", "application/json");
    if authorized {
        r = r.header("authorization", format!("Bearer {}", api.config.token));
    }
    let response = api
        .handler()
        .oneshot(r.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let code = response.status();
    let value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4 << 20).await.unwrap()).unwrap();
    (code, value)
}
#[tokio::test]
async fn exact_out_http_matches_original_grid_and_has_distinct_amount_and_plan_contracts() {
    let f = data();
    let s = source(&f);
    let api = api(&s);
    let req = request(&f);
    let (code, q) = call(&api, "POST", "/v1/quote-exact-out", req.clone(), true).await;
    assert_eq!(code, StatusCode::OK, "{q}");
    assert_eq!(q["requiredSellAmount"], f["grid"]["best_required_input"]);
    assert_eq!(q["consumedIn"], q["requiredSellAmount"]);
    assert_eq!(q["grossBuyAmount"], req["buyAmount"]);
    assert_eq!(q["minBuyAmount"], req["buyAmount"]);
    assert_eq!(q["refund"], "0");
    assert_eq!(q["routes"].as_array().unwrap().len(), 3);
    assert_eq!(q["quality"]["allocationSearch"]["evaluations"], 39);
    assert_eq!(
        q["quality"]["allocationSearch"]["exhaustiveWithinGrid"],
        true
    );
    assert_eq!(q["costs"].as_object().unwrap().len(), 1);
    assert!(q["netSellAmountEstimate"].is_null());
    assert_eq!(q["verification"]["level"], "INDICATIVE");
    assert_eq!(
        q["snapshotRef"],
        serde_json::to_value(&s.reference).unwrap()
    );
    let id = q["quoteId"].as_str().unwrap();
    assert!(id.starts_with("xo-"));
    let (code, e) = call(
        &api,
        "GET",
        &format!("/internal/quotes/{id}/evidence"),
        Value::Null,
        true,
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(e["quote"], q);
    let plan: ExactOutPlan = serde_json::from_value(e["execution_plan"].clone()).unwrap();
    verify(&plan).unwrap();
    let required = &plan.allocation.required_input.0;
    assert_eq!(
        plan.max_sell_amount.0,
        (required * 10050u32 + 9999u32) / 10000u32
    );
    assert_eq!(
        plan.allocation.unused_input_budget.0,
        &plan.allocation.input_budget.0 - required
    );
    let (_, again) = call(&api, "POST", "/v1/quote-exact-out", req, true).await;
    assert_eq!(again, q);
    for (path, expected) in [
        ("build", "EXECUTION_ADAPTER_UNSUPPORTED"),
        ("simulate", "INDEPENDENT_SIMULATION_UNAVAILABLE"),
    ] {
        let (code, v) = call(
            &api,
            "POST",
            &format!("/v1/quotes/{id}/{path}"),
            json!({}),
            true,
        )
        .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(v["error"], expected);
    }
    // Bind amounts, deadlines, path metadata, engine, budget, and snapshot to the commitment.
    for field in ["plan_hash", "max_sell_amount", "min_buy_amount", "engine"] {
        let mut v = e["execution_plan"].clone();
        v[field] = if field.ends_with("amount") {
            json!("1")
        } else {
            json!("corrupt")
        };
        assert!(
            verify(&serde_json::from_value(v).unwrap()).is_err(),
            "{field}"
        );
    }
    let mut changed = plan.clone();
    changed.context.deadline_ms += 1;
    assert!(verify(&changed).is_err());
    let mut changed = plan.clone();
    changed.snapshot.canonical_epoch += 1;
    assert!(verify(&changed).is_err());
    let mut changed = plan.clone();
    changed.allocation.routes[0].amounts[0] = "1".parse().unwrap();
    assert!(verify(&changed).is_err());
    let mut overflow = plan.allocation.clone();
    overflow.input_budget.0 <<= 256usize;
    overflow.unused_input_budget.0 = &overflow.input_budget.0 - &overflow.required_input.0;
    assert!(xone_execution_plan::exact_out::build(
        &plan.context,
        &overflow,
        &plan.snapshot,
        &plan.economic_policy_hash
    )
    .is_err());
    let mut changed = plan;
    changed.allocation.unused_input_budget = "1".parse().unwrap();
    assert!(verify(&changed).is_err());
}
#[tokio::test]
async fn exact_out_http_rejects_invalid_targets_budgets_aliases_and_old_endpoint_conversion() {
    let f = data();
    let api = api(&source(&f));
    let req = request(&f);
    assert_eq!(
        call(&api, "POST", "/v1/quote-exact-out", req.clone(), false)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&api, "POST", "/v1/quote", req.clone(), true).await.0,
        StatusCode::BAD_REQUEST
    );
    for (field, value, error) in [
        (
            "buyAmount",
            json!(123),
            "INVALID_REQUEST_AMOUNT_MUST_BE_STRING",
        ),
        ("buyAmount", json!("0"), "INVALID_REQUEST"),
        ("maxSellAmount", json!("1"), "EXACT_OUT_MAX_INPUT"),
        ("kind", json!("EXACT_IN"), "QUOTE_KIND_UNSUPPORTED"),
        (
            "caller",
            f["identities"]["factory"]["address"].clone(),
            "EXACT_OUT_UNMODELED_SELF_FUNDING",
        ),
        (
            "verification",
            json!("VERIFIED"),
            "INDEPENDENT_SIMULATION_UNAVAILABLE",
        ),
        ("constraints", json!({"maxHops":0}), "INVALID_HOP_LIMIT"),
    ] {
        let mut bad = req.clone();
        bad[field] = value;
        let (code, v) = call(&api, "POST", "/v1/quote-exact-out", bad, true).await;
        assert!(!code.is_success(), "{field}: {v}");
        assert_eq!(v["error"], error, "{field}");
    }
    let mut bad = req.clone();
    bad["sellAmount"] = json!("123");
    assert_eq!(
        call(&api, "POST", "/v1/quote-exact-out", bad, true).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut capped = req;
    capped["maxSellAmount"] = f["grid"]["best_required_input"].clone();
    let (code, q) = call(&api, "POST", "/v1/quote-exact-out", capped, true).await;
    assert_eq!(code, StatusCode::OK, "{q}");
    assert_eq!(q["maxSellAmount"], q["requiredSellAmount"]);
    assert_eq!(q["unusedInputBudget"], "0");
}
#[tokio::test]
async fn exact_out_single_flight_reorg_invalidation_and_unsupported_scope_are_explicit() {
    let f = data();
    let mut s = source(&f);
    let api = api(&s);
    let req: xone_quote_api::ExactOutRequest = serde_json::from_value(request(&f)).unwrap();
    let (a, b) = tokio::join!(
        api.quote_exact_out(req.clone()),
        api.quote_exact_out(req.clone())
    );
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    s.reference.online_generation += 1;
    s.reference.canonical_epoch += 1;
    s.reference.journal_seq += 1;
    api.replica.install_baseline(s).unwrap();
    let id = a["quoteId"].as_str().unwrap();
    for path in ["build", "simulate"] {
        assert_eq!(
            call(
                &api,
                "POST",
                &format!("/v1/quotes/{id}/{path}"),
                json!({}),
                true
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }
    let b = api.quote_exact_out(req).await.unwrap();
    assert_ne!(a["quoteId"], b["quoteId"]);
    let mut legacy: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    legacy.pools[0].token0 = f["grid"]["paths"][0][0].as_str().unwrap().into();
    legacy.pools[0].token1 = f["grid"]["paths"][0][1].as_str().unwrap().into();
    legacy.reference.scope_hash = scope_hash(&legacy.pools).unwrap();
    let other = Api::new((*api.config).clone()).unwrap();
    other.replica.install_baseline(legacy).unwrap();
    assert_eq!(
        other
            .quote_exact_out(serde_json::from_value(request(&f)).unwrap())
            .await
            .unwrap_err()
            .0,
        "NO_ROUTE"
    );
}

#[tokio::test]
async fn exact_out_http_retains_a_feasible_whole_order_when_split_probe_fails() {
    // Structural, admitted-state regression only; no public history claim.
    let f = data();
    let mut s = source(&f);
    s.pools.truncate(1);
    let p = &mut s.pools[0];
    p.reserve0 = "997".parse().unwrap();
    p.reserve1 = "8".parse().unwrap();
    let v = p.v2.as_mut().unwrap().swap.as_mut().unwrap();
    v.raw_balance0 = "996".parse().unwrap();
    v.raw_balance1 = "8".parse().unwrap();
    let (sell, buy) = (p.token0.clone(), p.token1.clone());
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    let api = api(&s);
    let mut req = request(&f);
    req["sellToken"] = sell.into();
    req["buyToken"] = buy.into();
    req["buyAmount"] = "4".into();
    req["maxSellAmount"] = "1001".into();
    let (code, q) = call(&api, "POST", "/v1/quote-exact-out", req, true).await;
    assert_eq!(code, StatusCode::OK, "{q}");
    assert_eq!(q["requiredSellAmount"], "1001");
    assert_eq!(q["minBuyAmount"], "4");
    assert_eq!(q["maxSellAmount"], "1001");
    assert_eq!(q["routes"].as_array().unwrap().len(), 1);
    assert_eq!(q["quality"]["budgetExhausted"], false);
}
