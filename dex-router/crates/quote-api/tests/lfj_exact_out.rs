use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/lfj-exact-out.json")).unwrap()
}
fn setup(f: &Value, index: usize) -> (Api, Snapshot) {
    let mut c = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    c.token = "lfj-exact-output-http-regression-credential".into();
    c.config.routing.max_state_age_ms = 315_360_000_000;
    c.config.api.quote_timeout_ms = 10000;
    c.config.routing.max_hops = 3;
    c.config.routing.max_splits = 1;
    c.config.routing.split_parts = 4;
    let api = Api::new(c).unwrap();
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let p: Pool = serde_json::from_value(f["cases"][index]["pool"].clone()).unwrap();
    let ledger = &p.lfj.as_ref().unwrap();
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
async fn lfj_http_reports_original_minimum_input_actual_output_and_surplus() {
    let f = data();
    let mut count = 0;
    let mut extra = 0;
    for (index, c) in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, c)| c["full_target"] == true)
    {
        let (api, s) = setup(&f, index);
        let request = req(
            &f,
            &s.pools[0],
            c["zero_for_one"].as_bool().unwrap(),
            c["target_output"].as_str().unwrap(),
        );
        let (code, q) = call(&api, "/v1/quote-exact-out", request.clone()).await;
        assert_eq!(code, StatusCode::OK, "case {index}: {q}");
        assert_eq!(q["requiredSellAmount"], c["required_input"]);
        assert_eq!(q["grossBuyAmount"], c["actual_output"]);
        assert_eq!(q["minBuyAmount"], request["buyAmount"]);
        assert_eq!(q["outputSurplus"], c["output_surplus"]);
        assert_eq!(q["refund"], "0");
        assert_eq!(q["verification"]["level"], "INDICATIVE");
        assert!(q["netSellAmountEstimate"].is_null());
        let (_, e) = call(
            &api,
            &format!(
                "/internal/quotes/{}/evidence",
                q["quoteId"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        let p: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&p).unwrap();
        assert_eq!(
            p.allocation.gross_output().to_string(),
            c["actual_output"].as_str().unwrap()
        );
        assert_eq!(
            p.allocation.routes[0].output_target().to_string(),
            c["target_output"].as_str().unwrap()
        );
        let proposals = p
            .allocation
            .routes
            .iter()
            .map(|r| xone_optimizer::exact_out::ExactOutProposal {
                edges: r.edges.clone(),
                amount_out: r.output_target().clone(),
            })
            .collect::<Vec<_>>();
        let recomputed = xone_optimizer::exact_out::evaluate_allocation(
            &s.pools,
            &proposals,
            &p.allocation.target_output,
            &p.allocation.input_budget,
            xone_routing::Funds {
                sender: request["caller"].as_str().unwrap(),
                recipient: request["recipient"].as_str().unwrap(),
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(recomputed).unwrap(),
            serde_json::to_value(&p.allocation).unwrap()
        );
        if c["output_surplus"] != "0" {
            assert!(p.allocation.gross_buy_amount.is_some());
            assert!(p.allocation.routes[0].target_output.is_some());
            extra += 1;
        } else {
            assert!(p.allocation.gross_buy_amount.is_none());
            assert!(p.allocation.routes[0].target_output.is_none());
            assert!(e["execution_plan"]["allocation"]
                .get("gross_buy_amount")
                .is_none());
        }
        count += 1;
    }
    assert_eq!((count, extra), (216, 60));
}
#[tokio::test]
async fn lfj_http_rejects_partial_targets_short_budget_internal_funds_and_false_execution() {
    let f = data();
    let (api, mut s) = setup(&f, 0);
    let request = req(&f, &s.pools[0], true, "10000");
    let (code, q) = call(&api, "/v1/quote-exact-out", request.clone()).await;
    assert_eq!(code, StatusCode::OK, "{q}");
    let required: xone_domain::Amount = q["requiredSellAmount"].as_str().unwrap().parse().unwrap();
    let mut short = request.clone();
    short["maxSellAmount"] = (required.0 - 1u8).to_string().into();
    assert_eq!(
        call(&api, "/v1/quote-exact-out", short).await.1["error"],
        "EXACT_OUT_MAX_INPUT"
    );
    let mut partial = request.clone();
    partial["buyAmount"] = "100000000".into();
    assert_eq!(
        call(&api, "/v1/quote-exact-out", partial).await.1["error"],
        "NO_ROUTE"
    );
    for address in [
        &s.pools[0].pool_id,
        &s.pools[0].lfj.as_ref().unwrap().factory,
        &s.pools[0].lfj.as_ref().unwrap().implementation,
    ] {
        let mut bad = request.clone();
        bad["recipient"] = address.clone().into();
        assert_eq!(
            call(&api, "/v1/quote-exact-out", bad).await.1["error"],
            "EXACT_OUT_UNMODELED_SELF_FUNDING"
        );
    }
    for (op, error) in [
        ("build", "EXECUTION_ADAPTER_UNSUPPORTED"),
        ("simulate", "INDEPENDENT_SIMULATION_UNAVAILABLE"),
    ] {
        assert_eq!(
            call(
                &api,
                &format!("/v1/quotes/{}/{op}", q["quoteId"].as_str().unwrap()),
                json!({})
            )
            .await
            .1["error"],
            error
        );
    }
    let mut verified = request;
    verified["verification"] = "VERIFIED".into();
    assert_eq!(
        call(&api, "/v1/quote-exact-out", verified).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    s.reference.canonical_epoch += 1;
    s.reference.online_generation += 1;
    s.reference.journal_seq += 1;
    api.replica.install_baseline(s).unwrap();
    assert_eq!(
        call(
            &api,
            &format!("/v1/quotes/{}/build", q["quoteId"].as_str().unwrap()),
            json!({})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn lfj_rehashed_plan_cannot_hide_surplus_or_move_it_to_an_unmodeled_template() {
    let f = data();
    let (index, c) = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find(|(_, c)| c["full_target"] == true && c["output_surplus"] != "0")
        .unwrap();
    let (api, s) = setup(&f, index);
    let request = req(
        &f,
        &s.pools[0],
        c["zero_for_one"].as_bool().unwrap(),
        c["target_output"].as_str().unwrap(),
    );
    let (code, q) = call(&api, "/v1/quote-exact-out", request).await;
    assert_eq!(code, StatusCode::OK);
    let (_, e) = call(
        &api,
        &format!(
            "/internal/quotes/{}/evidence",
            q["quoteId"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    let p: xone_execution_plan::exact_out::ExactOutPlan =
        serde_json::from_value(e["execution_plan"].clone()).unwrap();
    for mode in 0..4 {
        let mut a = p.allocation.clone();
        match mode {
            0 => a.gross_buy_amount = None,
            1 => a.routes[0].target_output = None,
            2 => a.routes[0].target_output = Some(a.routes[0].amount_out.clone()),
            _ => a.routes[0].edges[0].protocol = xone_domain::v2_swap::PROFILE.into(),
        };
        assert!(
            xone_execution_plan::exact_out::build(
                &p.context,
                &a,
                &p.snapshot,
                &p.economic_policy_hash
            )
            .is_err(),
            "mode {mode}"
        );
    }
}
