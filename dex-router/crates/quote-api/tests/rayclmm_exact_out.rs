#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use execution_bank::{Bank, AUTH};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
const SENDER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
const RECIPIENT: &str = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
async fn call(api: &xone_quote_api::Api, url: &str, body: Value) -> (StatusCode, Value) {
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
                .header("authorization", format!("Bearer {AUTH}"))
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
fn record(index: usize, value: Value) {
    if let Ok(folder) = std::env::var("XONE_RAYCLMM_EXACT_OUT_DIR") {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            std::path::Path::new(&folder).join(format!("rayclmm-{index}-api.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
}
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/rayclmm-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn set_pools(s: &mut Snapshot, pools: Vec<Pool>) {
    let o = pools[0].raydium_clmm.as_ref().unwrap();
    s.reference.block.number_or_slot = o.bank_slot;
    s.reference.block.timestamp_ms = o.bank_unix_timestamp * 1000;
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
    s.source = "CONTROLLED_ORIGINAL_RAYCLMM_EXACT_OUT_NATIVE".into();
}
fn request(p: &Pool, zero: bool, target: &Value, budget: Value) -> Value {
    json!({"chainKey":p.chain_key,"kind":"EXACT_OUT","sellToken":if zero{&p.token0}else{&p.token1},"buyToken":if zero{&p.token1}else{&p.token0},"buyAmount":target,"maxSellAmount":budget,"caller":SENDER,"recipient":RECIPIENT,"slippageBps":0,"verification":"FAST"})
}
#[tokio::test]
async fn rayclmm_exact_output_api_quotes_plans_views_and_budget_rejections_match_native_corpus() {
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (api, mut snapshot) = bank.api(&url);
    let f = corpus();
    let mut seen = BTreeSet::new();
    let (mut full, mut rejected) = (0, 0);
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        set_pools(&mut snapshot, vec![p.clone()]);
        let installed = api.replica.install_baseline(snapshot.clone());
        if installed.is_err() {
            assert_eq!(c["passed"], false, "{i}: {installed:?}");
            record(
                i,
                json!({"caseIndex":i,"phase":"BASELINE_ADMISSION","passed":false,"error":installed.err().unwrap().0}),
            );
            rejected += 1;
            continue;
        }
        let req = request(
            &p,
            c["zeroForOne"].as_bool().unwrap(),
            &c["target"],
            json!("18446744073709551615"),
        );
        let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
        if c["passed"] == false {
            assert_ne!(status, StatusCode::OK, "{i}: {q}");
            record(
                i,
                json!({"caseIndex":i,"phase":"QUOTE","passed":false,"request":req,"response":q}),
            );
            rejected += 1;
            continue;
        }
        assert_eq!(status, StatusCode::OK, "{i} {}: {q}", c["name"]);
        assert_eq!(q["requiredSellAmount"], c["requiredInput"]);
        assert_eq!(q["grossBuyAmount"], c["target"]);
        assert_eq!(q["minBuyAmount"], c["target"]);
        assert_eq!(q["outputSurplus"], "0");
        let id = q["quoteId"].as_str().unwrap();
        let (status, e) = call(
            &api,
            &format!("/internal/quotes/{id}/evidence"),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{e}");
        let plan: xone_execution_plan::exact_out::ExactOutPlan =
            serde_json::from_value(e["execution_plan"].clone()).unwrap();
        xone_execution_plan::exact_out::verify(&plan).unwrap();
        record(
            i,
            json!({"caseIndex":i,"phase":"QUOTE","passed":true,"request":req,"response":q,"evidence":e}),
        );
        assert_eq!(
            plan.allocation.routes[0].factory,
            xone_domain::raydium_clmm::PROGRAM_ID
        );
        if seen.insert(p.protocol.clone()) {
            let (status, options) = call(&api, "/v1/quote-options", Value::Null).await;
            assert_eq!(status, StatusCode::OK);
            let chain = options["chains"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["chainKey"] == p.chain_key)
                .unwrap();
            let view = &chain["protocolViews"][0];
            assert!(view["capabilities"]["quoteModes"]
                .as_array()
                .unwrap()
                .contains(&json!("EXACT_OUT")));
            assert_eq!(view["capabilities"]["executionPreparation"], false);
            assert_eq!(view["capabilities"]["walletSubmission"], false);
            assert_eq!(
                q["protocolViewIds"][format!("{}@{}", p.protocol, p.adapter_version)],
                view["viewId"]
            );
            let (status, unsupported) =
                call(&api, &format!("/v1/quotes/{id}/prepare"), json!({})).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(unsupported["error"], "EXECUTION_ADAPTER_UNSUPPORTED");
            assert!(unsupported.get("unsignedTransaction").is_none());
        }
        let mut low = req;
        low["maxSellAmount"] = (c["requiredInput"].as_str().unwrap().parse::<u64>().unwrap() - 1)
            .to_string()
            .into();
        assert_ne!(
            call(&api, "/v1/quote-exact-out", low).await.0,
            StatusCode::OK
        );
        full += 1;
    }
    assert_eq!(seen.len(), 6);
    assert!(full >= 900 && rejected >= 300);
    assert!(
        bank.calls.lock().unwrap().is_empty(),
        "pure quotes must not fetch dynamic execution accounts"
    );
    task.abort();
}
#[tokio::test]
async fn rayclmm_api_selects_native_minimum_over_two_pool_grid_and_withdraws_old_views() {
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (initial, mut snapshot) = bank.api(&url);
    let mut config = initial.config.as_ref().clone();
    drop(initial);
    config.config.routing.max_splits = 2;
    let api = xone_quote_api::Api::new(config).unwrap();
    for pair in corpus()["pairs"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(pair["pools"].clone()).unwrap();
        set_pools(&mut snapshot, pools.clone());
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let req = request(
            &pools[0],
            pair["zeroForOne"].as_bool().unwrap(),
            &pair["target"],
            json!("18446744073709551615"),
        );
        let (status, q) = call(&api, "/v1/quote-exact-out", req.clone()).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], pair["bestRequiredInput"]);
        assert_eq!(q["grossBuyAmount"], pair["target"]);
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
        assert!(!plan.allocation.routes.is_empty() && plan.allocation.routes.len() <= 2);
        let mut low = req;
        low["maxSellAmount"] = (pair["bestRequiredInput"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            - 1)
        .to_string()
        .into();
        assert_ne!(
            call(&api, "/v1/quote-exact-out", low).await.0,
            StatusCode::OK
        );
        set_pools(&mut snapshot, pools);
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let (_, withdrawn) = call(
            &api,
            &format!("/internal/quotes/{id}/evidence"),
            Value::Null,
        )
        .await;
        assert_eq!(withdrawn["snapshot_valid"], false);
        let (status, old) = call(&api, &format!("/v1/quotes/{id}/build"), json!({})).await;
        assert_ne!(status, StatusCode::OK, "{old}");
        assert_eq!(old["error"], "SNAPSHOT_REORGED");
    }
    assert!(bank.calls.lock().unwrap().is_empty());
    task.abort();
}
