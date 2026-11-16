use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_quote_api::Api;
const MAX: &str = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/stable-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn setup() -> (Api, Snapshot) {
    let mut c = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    c.token = "stable-original-output-regression".into();
    c.config.routing.max_state_age_ms = 315360000000;
    c.config.routing.max_hops = 3;
    c.config.routing.max_splits = 2;
    c.config.routing.split_parts = 4;
    c.config.routing.max_candidates = 16;
    c.config.routing.max_expansions = 10000;
    c.config.api.requests_per_minute = 10000;
    c.config.api.quote_timeout_ms = 30000;
    let api = Api::new(c).unwrap();
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    (api, snapshot)
}
fn install(api: &Api, s: &mut Snapshot, pools: Vec<Pool>) {
    s.reference.online_generation += 1;
    s.reference.journal_seq += 1;
    s.reference.scope_hash = scope_hash(&pools).unwrap();
    s.reference.adapter_set_hash = hash(
        &pools
            .iter()
            .map(|p| p.adapter_version.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
    )
    .unwrap();
    s.pools = pools;
    s.source = "CONTROLLED_STABLE_GIVEN_OUT_MATH".into();
    api.replica.install_baseline(s.clone()).unwrap();
}
async fn call(api: &Api, url: &str, body: Value) -> (StatusCode, Value) {
    let r = api
        .handler()
        .oneshot(
            Request::builder()
                .method(
                    if url.starts_with("/internal/") || url.ends_with("quote-options") {
                        "GET"
                    } else {
                        "POST"
                    },
                )
                .uri(url)
                .header("authorization", format!("Bearer {}", api.config.token))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        r.status(),
        serde_json::from_slice(&to_bytes(r.into_body(), 8 << 20).await.unwrap()).unwrap(),
    )
}
fn tokens(p: &Pool, i: usize, j: usize) -> Vec<String> {
    if let Some(s) = &p.multi_asset {
        vec![s.assets[i].token.clone(), s.assets[j].token.clone()]
    } else {
        let t = [&p.token0, &p.token1];
        vec![t[i].clone(), t[j].clone()]
    }
}
fn request(pair: &[String], target: &Value, budget: Value) -> Value {
    json!({"chainKey":"evm:31337","kind":"EXACT_OUT","sellToken":pair[0],"buyToken":pair[1],"buyAmount":target,"maxSellAmount":budget,"caller":"0x000000000000000000000000000000000000dead","recipient":"0x000000000000000000000000000000000000beef","slippageBps":0,"verification":"FAST"})
}
fn record(index: usize, value: Value) {
    if let Ok(dir) = std::env::var("XONE_STABLE_API_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("case-{index}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}
#[tokio::test]
async fn stable_http_targets_plans_views_and_limits_match_original_solver() {
    let f = corpus();
    let (api, mut snapshot) = setup();
    let mut seen = BTreeSet::new();
    let (mut success, mut rejected) = (0, 0);
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        install(&api, &mut snapshot, vec![p.clone()]);
        let pair = tokens(
            &p,
            c["i"].as_u64().unwrap() as usize,
            c["j"].as_u64().unwrap() as usize,
        );
        let req = request(&pair, &c["target"], json!(MAX));
        let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
        if c["expected"]["reverted"] == true {
            assert_ne!(status, StatusCode::OK, "{q}");
            record(
                index,
                json!({"index":index,"passed":false,"request":req,"response":q,"status":status.as_u16()}),
            );
            rejected += 1;
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{}: {q}", p.protocol);
        assert_eq!(q["requiredSellAmount"], c["expected"]["amount_in"]);
        assert_eq!(q["grossBuyAmount"], c["target"]);
        assert_eq!(q["outputSurplus"], "0");
        let id = q["quoteId"].as_str().unwrap();
        let (_, e) = call(
            &api,
            &format!("/internal/quotes/{id}/evidence"),
            Value::Null,
        )
        .await;
        let plan: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&plan).unwrap();
        let mut broken = plan.clone();
        broken.allocation.routes[0].edges[0].from_token = Some(pair[0].clone());
        assert!(xone_execution_plan::exact_out::build(
            &broken.context,
            &broken.allocation,
            &broken.snapshot,
            &broken.economic_policy_hash
        )
        .is_err());
        if seen.insert(p.protocol.clone()) {
            let (_, options) = call(&api, "/v1/quote-options", Value::Null).await;
            let chain = options["chains"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["chainKey"] == "evm:31337")
                .unwrap();
            let view = &chain["protocolViews"][0];
            assert!(view["capabilities"]["quoteModes"]
                .as_array()
                .unwrap()
                .contains(&json!("EXACT_OUT")));
            assert_eq!(view["capabilities"]["executionPreparation"], false);
            assert_eq!(
                q["protocolViewIds"][format!("{}@{}", p.protocol, p.adapter_version)],
                view["viewId"]
            );
            assert_ne!(
                call(&api, &format!("/v1/quotes/{id}/build"), json!({}))
                    .await
                    .0,
                StatusCode::OK
            );
        }
        success += 1;
        let mut low = req.clone();
        let n: xone_domain::Amount = c["expected"]["amount_in"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        low["maxSellAmount"] = (n.0 - 1u8).to_string().into();
        let (low_status, low_response) = call(&api, "/v1/quote-exact-out", low.clone()).await;
        assert_ne!(low_status, StatusCode::OK);
        record(
            index,
            json!({"index":index,"passed":true,"request":req,"response":q,"evidence":e,"status":status.as_u16(),"budgetControl":{"request":low,"status":low_status.as_u16(),"response":low_response}}),
        );
    }
    assert_eq!((success, rejected), (396, 324));
    assert_eq!(seen.len(), 1);
}
#[tokio::test]
async fn stable_http_two_pool_grid_and_old_view_revocation() {
    let f = corpus();
    let (api, mut snapshot) = setup();
    for grid in f["grids"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(grid["pools"].clone()).unwrap();
        install(&api, &mut snapshot, pools.clone());
        let pair = tokens(&pools[0], 0, 1);
        let req = request(&pair, &grid["target"], json!(MAX));
        let (status, q) = call(&api, "/v1/quote-exact-out", req).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], grid["bestRequiredInput"]);
        let id = q["quoteId"].as_str().unwrap();
        install(&api, &mut snapshot, pools);
        let (_, e) = call(
            &api,
            &format!("/internal/quotes/{id}/evidence"),
            Value::Null,
        )
        .await;
        assert_eq!(e["snapshot_valid"], false);
        let (_, old) = call(&api, &format!("/v1/quotes/{id}/build"), json!({})).await;
        assert_eq!(old["error"], "SNAPSHOT_REORGED");
    }
}
