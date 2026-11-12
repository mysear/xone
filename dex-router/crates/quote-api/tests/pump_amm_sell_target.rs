#[allow(dead_code)]
#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, Pool};
async fn call(api: &xone_quote_api::Api, url: &str, body: Value) -> (StatusCode, Value) {
    let r = api
        .handler()
        .oneshot(
            Request::builder()
                .method(if url.contains("evidence") {
                    "GET"
                } else {
                    "POST"
                })
                .uri(url)
                .header("authorization", format!("Bearer {}", execution_bank::AUTH))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let s = r.status();
    (
        s,
        serde_json::from_slice(&to_bytes(r.into_body(), 8 << 20).await.unwrap()).unwrap(),
    )
}
#[tokio::test]
async fn original_pump_swap_sell_targets_flow_through_router_optimizer_api_and_plan() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/pump-amm-sell-target.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let bank = execution_bank::Bank::new();
    let (url, task) = bank.serve().await;
    let (api, mut snapshot) = bank.api(&url);
    let (mut passed, mut surplus) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let user = p.pump.as_ref().unwrap().user.clone();
        snapshot.reference.block.number_or_slot = p.pump.as_ref().unwrap().slot;
        snapshot.reference.block.timestamp_ms = p.pump.as_ref().unwrap().timestamp * 1000;
        snapshot.reference.block.hash =
            bs58::encode(Sha256::digest(serde_json::to_vec(c).unwrap())).into_string();
        snapshot.pools = vec![p];
        snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
        snapshot.reference.adapter_set_hash =
            hash(&vec![snapshot.pools[0].adapter_version.clone()]).unwrap();
        snapshot.reference.online_generation += 1;
        snapshot.reference.journal_seq += 1;
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let p = &snapshot.pools[0];
        let request = json!({"chainKey":p.chain_key,"kind":"EXACT_OUT","sellToken":p.token0,"buyToken":p.token1,"buyAmount":c["target"],"maxSellAmount":"1000000000000000","caller":user,"recipient":user,"slippageBps":50,"verification":"FAST"});
        let (status, q) = call(&api, "/v1/quote-exact-out", request.clone()).await;
        if c["passed"] == false {
            assert_ne!(status, StatusCode::OK, "{q}");
            continue;
        }
        assert_eq!(status, StatusCode::OK, "case {} {q}", c["index"]);
        assert_eq!(q["grossBuyAmount"], c["actual_output"]);
        assert_eq!(q["requiredSellAmount"], c["required_input"]);
        assert_eq!(q["buyAmount"], c["target"]);
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
        if c["actual_output"] != c["target"] {
            assert!(plan.allocation.gross_buy_amount.is_some());
            assert!(plan.allocation.routes[0].target_output.is_some());
            surplus += 1;
        }
        let mut low = request;
        low["maxSellAmount"] = json!((c["required_input"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            - 1)
        .to_string());
        assert_ne!(
            call(&api, "/v1/quote-exact-out", low).await.0,
            StatusCode::OK
        );
        passed += 1;
    }
    assert_eq!((passed, surplus), (42, 14));
    task.abort();
}
