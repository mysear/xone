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
        "../../../test/contracts/slipstream-exact-out.json"
    ))
    .unwrap()
}
fn setup(f: &Value, index: usize) -> (Api, Snapshot) {
    let mut c = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    c.token = "slipstream-exact-output-http-regression-credential".into();
    c.config.routing.max_state_age_ms = 315_360_000_000;
    c.config.api.quote_timeout_ms = 10000;
    c.config.routing.max_hops = 3;
    c.config.routing.max_splits = 1;
    c.config.routing.split_parts = 4;
    let api = Api::new(c).unwrap();
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let p: Pool = serde_json::from_value(f["cases"][index]["pool"].clone()).unwrap();
    let ledger = &p.slipstream_context.as_ref().unwrap();
    s.reference.block.number_or_slot = ledger.block_number;
    s.reference.block.hash = ledger.block_hash.clone();
    s.reference.block.timestamp_ms = ledger.timestamp * 1000;
    s.pools = vec![p];
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    s.reference.adapter_set_hash = hash(&vec![s.pools[0].adapter_version.clone()]).unwrap();
    api.replica.install_baseline(s.clone()).unwrap();
    (api, s)
}
fn req(f: &Value, p: &Pool, zero: bool, target: &str) -> Value {
    let (sell, buy) = if zero {
        (&p.token0, &p.token1)
    } else {
        (&p.token1, &p.token0)
    };
    json!({"chainKey":"evm:31337","kind":"EXACT_OUT","sellToken":sell,"buyToken":buy,"buyAmount":target,"maxSellAmount":"1000000000000000000000000","caller":f["deployment"]["sender"],"recipient":"0x000000000000000000000000000000000000beef","slippageBps":50,"verification":"FAST","constraints":{"maxHops":3}})
}
async fn call(api: &Api, url: &str, body: Value) -> (StatusCode, Value) {
    let response = api
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
    let status = response.status();
    let value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4 << 20).await.unwrap()).unwrap();
    (status, value)
}
#[tokio::test]
async fn slipstream_exact_output_http_matches_original_input_and_complete_plan() {
    let f = data();
    for index in (2usize..108).step_by(6) {
        let c = &f["cases"][index];
        assert_eq!(c["expected"]["partial"], false);
        let (api, s) = setup(&f, index);
        let request = req(
            &f,
            &s.pools[0],
            c["zero_for_one"].as_bool().unwrap(),
            c["target_output"].as_str().unwrap(),
        );
        let (code, q) = call(&api, "/v1/quote-exact-out", request.clone()).await;
        assert_eq!(code, StatusCode::OK, "case {index}: {q}");
        assert_eq!(q["requiredSellAmount"], c["expected"]["consumed_in"]);
        assert_eq!(q["minBuyAmount"], request["buyAmount"]);
        assert_eq!(q["quality"]["candidateScope"], json!([s.pools[0].pool_id]));
        assert_eq!(q["quality"]["excludedPools"], json!({}));
        assert_eq!(q["quality"]["candidateSearch"]["maxHops"], 3);
        assert_eq!(
            q["quality"]["candidateSearch"]["maxCandidates"],
            api.config.config.routing.max_candidates
        );
        assert_eq!(
            q["quality"]["candidateSearch"]["maxExpansions"],
            api.config.config.routing.max_expansions
        );
        assert_eq!(q["quality"]["candidateSearch"]["probeParts"], 1);

        assert_eq!(q["refund"], "0");
        assert_eq!(q["verification"]["level"], "INDICATIVE");
        assert!(q["netSellAmountEstimate"].is_null());
        let id = q["quoteId"].as_str().unwrap();
        let (code, e) = call(
            &api,
            &format!("/internal/quotes/{id}/evidence"),
            Value::Null,
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        let plan: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&plan).unwrap();
        let mut state =
            std::collections::BTreeMap::from([(s.pools[0].pool_id.clone(), s.pools[0].clone())]);
        let route = &plan.allocation.routes[0];
        xone_routing::exact_out::evaluate_path(
            &mut state,
            &route.edges,
            &route.amount_out,
            xone_routing::Funds {
                sender: request["caller"].as_str().unwrap(),
                recipient: request["recipient"].as_str().unwrap(),
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(state.values().next().unwrap()).unwrap(),
            c["expected"]["expected_pool"]
        );
        let mut short = request.clone();
        short["maxSellAmount"] = (plan.allocation.required_input.0 - 1u8).to_string().into();
        assert_eq!(
            call(&api, "/v1/quote-exact-out", short).await.1["error"],
            "EXACT_OUT_MAX_INPUT"
        );
        assert_eq!(
            call(&api, &format!("/v1/quotes/{id}/build"), json!({}))
                .await
                .1["error"],
            "EXECUTION_ADAPTER_UNSUPPORTED"
        );
        assert_eq!(
            call(&api, &format!("/v1/quotes/{id}/simulate"), json!({}))
                .await
                .1["error"],
            "INDEPENDENT_SIMULATION_UNAVAILABLE"
        );
    }
}
#[tokio::test]
async fn slipstream_exact_output_http_rejects_partial_and_internal_funds_and_revokes_old_plan() {
    let f = data();
    let (api, mut s) = setup(&f, 86);
    let p = &s.pools[0];
    let request = req(&f, p, true, "1000000000");
    let (_, old) = call(&api, "/v1/quote-exact-out", request.clone()).await;
    assert!(old["quoteId"].is_string());
    for owner in [
        &p.pool_id,
        &p.slipstream_context
            .as_ref()
            .unwrap()
            .swap_fee_module
            .address,
        &p.slipstream_context.as_ref().unwrap().factory_voter,
        &p.slipstream_context.as_ref().unwrap().core.factory,
    ] {
        let mut bad = request.clone();
        bad["recipient"] = owner.clone().into();
        assert_eq!(
            call(&api, "/v1/quote-exact-out", bad).await.1["error"],
            "EXACT_OUT_UNMODELED_SELF_FUNDING"
        );
    }
    let mut partial = request.clone();
    partial["buyAmount"] = "1000000000000".into();
    assert_eq!(
        call(&api, "/v1/quote-exact-out", partial).await.1["error"],
        "NO_ROUTE"
    );
    let mut unknown = request.clone();
    unknown["caller"] = "0x0000000000000000000000000000000000000001".into();
    assert_eq!(
        call(&api, "/v1/quote-exact-out", unknown).await.1["error"],
        "NO_ROUTE"
    );
    let mut verified = request;
    verified["verification"] = "VERIFIED".into();
    assert_eq!(
        call(&api, "/v1/quote-exact-out", verified).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    s.reference.online_generation += 1;
    s.reference.canonical_epoch += 1;
    s.reference.journal_seq += 1;
    api.replica.install_baseline(s).unwrap();
    assert_eq!(
        call(
            &api,
            &format!("/v1/quotes/{}/build", old["quoteId"].as_str().unwrap()),
            json!({})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn slipstream_rehashed_multihop_plan_is_rejected_by_its_single_pool_template() {
    let f = data();
    let (api, s) = setup(&f, 2);
    let request = req(&f, &s.pools[0], true, "1000000000");
    let (code, q) = call(&api, "/v1/quote-exact-out", request).await;
    assert_eq!(code, StatusCode::OK);
    let (_, evidence) = call(
        &api,
        &format!(
            "/internal/quotes/{}/evidence",
            q["quoteId"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    let p: xone_execution_plan::exact_out::ExactOutPlan =
        serde_json::from_value(evidence["execution_plan"].clone()).unwrap();
    let mut allocation = p.allocation.clone();
    let route = &mut allocation.routes[0];
    let third = "0x000000000000000000000000000000000000aabb";
    route.tokens.push(third.into());
    route.amounts.push(route.amount_out.clone());
    let mut edge = route.edges[0].clone();
    edge.pool_id = "0x000000000000000000000000000000000000aacc".into();
    edge.to = third.into();
    route.edges.push(edge);
    let mut context = p.context.clone();
    context.buy_token = third.into();
    assert_eq!(
        xone_execution_plan::exact_out::build(
            &context,
            &allocation,
            &p.snapshot,
            &p.economic_policy_hash
        )
        .unwrap_err()
        .0,
        "EXACT_OUT_INVALID_PLAN"
    );
    // A V2 suffix cannot upgrade the callback template into a mixed route.
    allocation.routes[0].edges[1].protocol = xone_domain::v2_swap::PROFILE.into();
    assert!(xone_execution_plan::exact_out::build(
        &context,
        &allocation,
        &p.snapshot,
        &p.economic_policy_hash
    )
    .is_err());
}
