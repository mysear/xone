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
            "/../../test/contracts/v3-exact-out.json"
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
    c.token = "v3-original-output-regression".into();
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
fn install(api: &Api, s: &mut Snapshot, pools: Vec<Pool>, block: &Value) {
    s.reference.block.number_or_slot = block["number"].as_u64().unwrap();
    s.reference.block.hash = block["hash"].as_str().unwrap().into();
    s.reference.block.parent_hash = block["parent_hash"].as_str().unwrap().into();
    s.reference.block.timestamp_ms = block["timestamp_ms"].as_u64().unwrap();
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
    s.source = "CONTROLLED_ORIGINAL_EVM_V3_TARGET".into();
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
fn request(f: &Value, tokens: &[String], target: &Value, budget: Value) -> Value {
    json!({"chainKey":"evm:31337","kind":"EXACT_OUT","sellToken":tokens[0],"buyToken":tokens[tokens.len()-1],"buyAmount":target,"maxSellAmount":budget,"caller":f["caller"],"recipient":f["recipient"],"slippageBps":0,"verification":"FAST"})
}
fn record(i: usize, value: Value) {
    if let Ok(dir) = std::env::var("XONE_V3_EXACT_OUT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("v3-{i}-api.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}
#[tokio::test]
async fn standard_v3_http_exact_targets_plans_views_and_input_limits_match_original_contracts() {
    let f = corpus();
    let (api, mut snapshot) = setup();
    let (mut full, mut bad) = (0, 0);
    let mut seen = BTreeSet::new();
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        install(&api, &mut snapshot, vec![p.clone()], &c["block"]);
        let tokens: Vec<String> = serde_json::from_value(c["routeTokens"].clone()).unwrap();
        let req = request(&f, &tokens, &c["target"], json!(MAX));
        let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
        if c["passed"] == false {
            assert_ne!(status, StatusCode::OK, "{i}: {q}");
            record(
                i,
                json!({"index":i,"passed":false,"request":req,"response":q}),
            );
            bad += 1;
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{i} {}: {q}", c["protocol"]);
        assert_eq!(q["requiredSellAmount"], c["requiredInput"]);
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
        record(
            i,
            json!({"index":i,"passed":true,"request":req,"response":q,"evidence":e}),
        );
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
            let (status, b) = call(&api, &format!("/v1/quotes/{id}/build"), json!({})).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(b["error"], "EXECUTION_ADAPTER_UNSUPPORTED");
        }
        let mut low = req;
        low["maxSellAmount"] = (c["requiredInput"]
            .as_str()
            .unwrap()
            .parse::<u128>()
            .unwrap()
            - 1)
        .to_string()
        .into();
        assert_ne!(
            call(&api, "/v1/quote-exact-out", low).await.0,
            StatusCode::OK
        );
        full += 1;
    }
    assert_eq!((full, bad), (80, 16));
    assert_eq!(seen.len(), 2);
}
#[tokio::test]
async fn standard_v3_http_two_hop_and_two_pool_grid_match_real_recursive_callbacks() {
    let f = corpus();
    let (api, mut snapshot) = setup();
    for c in f["paths"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(c["routePools"].clone()).unwrap();
        install(&api, &mut snapshot, pools, &c["block"]);
        let tokens: Vec<String> = serde_json::from_value(c["routeTokens"].clone()).unwrap();
        let (status, q) = call(
            &api,
            "/v1/quote-exact-out",
            request(&f, &tokens, &c["target"], json!(MAX)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], c["requiredInput"]);
        assert_eq!(q["routes"][0]["edges"].as_array().unwrap().len(), 2);
        let (_, e) = call(
            &api,
            &format!(
                "/internal/quotes/{}/evidence",
                q["quoteId"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        xone_execution_plan::exact_out::verify(
            &serde_json::from_value(e["execution_plan"].clone()).unwrap(),
        )
        .unwrap();
    }
    for c in f["grids"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(c["pools"].clone()).unwrap();
        let tokens = vec![pools[0].token0.clone(), pools[0].token1.clone()];
        install(&api, &mut snapshot, pools.clone(), &c["block"]);
        let req = request(&f, &tokens, &c["target"], json!(MAX));
        let (status, q) = call(&api, "/v1/quote-exact-out", req).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], c["bestRequiredInput"]);
        let id = q["quoteId"].as_str().unwrap();
        install(&api, &mut snapshot, pools, &c["block"]);
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
