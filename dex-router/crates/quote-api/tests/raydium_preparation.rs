#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use execution_bank::{Bank, AUTH};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(api: &xone_quote_api::Api, path: &str, value: Value) -> (StatusCode, Value) {
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {AUTH}"))
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status(),
        serde_json::from_slice(&to_bytes(response.into_body(), 4 << 20).await.unwrap()).unwrap(),
    )
}
#[tokio::test]
async fn raydium_preparation_checks_real_account_closure_and_original_sbf_net_credits() {
    for index in 0..2 {
        let bank = Bank::raydium(index);
        let (url, task) = bank.serve().await;
        let (api, _) = bank.api(&url);
        let request = bank.request();
        let (status, q) = call(&api, "/v1/quote-exact-out", request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], bank.data["requiredInput"]);
        let path = format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap());
        let mut prepare = bank.preparation();
        prepare["simulate"] = true.into();
        let (status, p) = call(&api, &path, prepare.clone()).await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], true);
        assert_eq!(p["simulation"]["actualInput"], bank.data["requiredInput"]);
        assert_eq!(p["simulation"]["actualOutput"], "1000");
        assert_eq!(p["instructions"].as_array().unwrap().len(), 1);
        assert_eq!(
            p["instructions"][0]["programId"],
            "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C"
        );
        assert_eq!(p["totalFundingLamports"], "5000");
        assert_eq!(p["signed"], false);
        assert_eq!(p["submitted"], false);
        if let Ok(folder) = std::env::var("XONE_RAYDIUM_PREPARATION_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!("raydium-{index}-preparation.json")),
                serde_json::to_vec_pretty(&json!({"quote":q,"preparation":p})).unwrap(),
            )
            .unwrap();
        }
        for (mode, error) in [
            ("changed_pool", "QUOTE_STATE_CHANGED"),
            ("changed_program", "PREPARATION_PROGRAM_CHANGED"),
            ("changed_clock", "QUOTE_CLOCK_CHANGED"),
            ("frozen", "PREPARATION_ACCOUNT_INVALID"),
            (
                "simulation_mismatch",
                "PREPARATION_SIMULATION_AMOUNT_MISMATCH",
            ),
        ] {
            *bank.mode.lock().unwrap() = mode.into();
            let (status, p) = call(&api, &path, prepare.clone()).await;
            assert_ne!(status, StatusCode::OK, "{mode} {p}");
            assert_eq!(p["error"], error, "{mode} {p}");
            assert!(p.get("unsignedTransaction").is_none());
        }
        *bank.mode.lock().unwrap() = "ready".into();
        for option in [
            "createDestinationAta",
            "wrapSourceSol",
            "refundWrappedSource",
            "unwrapDestinationSol",
        ] {
            let mut bad = prepare.clone();
            bad[option] = true.into();
            let (status, p) = call(&api, &path, bad).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(p["error"], "PREPARATION_OPTIONS_UNSUPPORTED");
        }
        let mut exact = request;
        exact["kind"] = "EXACT_IN".into();
        exact.as_object_mut().unwrap().remove("buyAmount");
        exact.as_object_mut().unwrap().remove("maxSellAmount");
        exact["sellAmount"] = "10000".into();
        let (status, q) = call(&api, "/v1/quote", exact).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let (status, p) = call(
            &api,
            &format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap()),
            prepare,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["simulation"]["actualInput"], "10000");
        assert_eq!(p["simulation"]["actualOutput"], q["grossBuyAmount"]);
        task.abort();
    }
}
