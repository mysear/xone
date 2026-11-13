#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use execution_bank::{wallet_worker, Bank, AUTH};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(api: &xone_quote_api::Api, path: &str, value: Value) -> (StatusCode, Value) {
    let r = api
        .handler()
        .oneshot(
            Request::builder()
                .method(if path.starts_with("/v1/transactions/") {
                    "GET"
                } else {
                    "POST"
                })
                .uri(path)
                .header("authorization", format!("Bearer {AUTH}"))
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status();
    let bytes = to_bytes(r.into_body(), 4 << 20).await.unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&bytes)));
    (status, value)
}
fn quote_request(bank: &Bank) -> (&str, Value) {
    let mut r = bank.request();
    if bank.data["exactOut"] == true {
        return ("/v1/quote-exact-out", r);
    }
    r["kind"] = "EXACT_IN".into();
    r.as_object_mut().unwrap().remove("buyAmount");
    r.as_object_mut().unwrap().remove("maxSellAmount");
    r["sellAmount"] = bank.data["sellAmount"].clone();
    ("/v1/quote", r)
}
#[tokio::test]
async fn pump_swap_canonical_boost_all_instruction_kinds_prepare_and_execute_independently() {
    for index in 0..9 {
        let bank = Bank::pump_amm(index);
        let (url, task) = bank.serve().await;
        let (api, _) = bank.api_with_manifest(
            &url,
            Some(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/sbf-runtime/rpc-capture.json"
            )),
        );
        let (endpoint, request) = quote_request(&bank);
        let (status, q) = call(&api, endpoint, request).await;
        assert_eq!(status, StatusCode::OK, "{index}: {q}");
        assert_eq!(q["grossBuyAmount"], bank.data["actualOutput"]);
        assert_eq!(q["consumedIn"], bank.data["requiredInput"]);
        if index == 8 {
            assert_eq!(q["outputSurplus"], "47185");
            assert_eq!(q["minBuyAmount"], "1000");
        }
        let id = q["quoteId"].as_str().unwrap();
        let path = format!("/v1/quotes/{id}/prepare");
        let mut input = bank.preparation();
        input["simulate"] = true.into();
        let (status, p) = call(&api, &path, input.clone()).await;
        assert_eq!(status, StatusCode::OK, "{index}: {p}");
        assert_eq!(p["ready"], true);
        assert_eq!(p["simulation"]["actualInput"], bank.data["requiredInput"]);
        assert_eq!(p["simulation"]["actualOutput"], bank.data["actualOutput"]);
        assert_eq!(p["totalFundingLamports"], "5000");
        assert_eq!(p["requiredSigners"], json!([bank.data["caller"]]));
        assert_eq!(p["instructions"].as_array().unwrap().len(), 1);
        if let Ok(folder) = std::env::var("XONE_PUMP_PREPARATION_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!("pump-amm-{index}-preparation.json")),
                serde_json::to_vec_pretty(&json!({"quote":q,"preparation":p})).unwrap(),
            )
            .unwrap();
        }
        let (status, e) = call(
            &api,
            &format!(
                "/v1/quotes/{id}/preparations/{}/simulate-independent",
                p["preparationHash"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{index}: {e}");
        assert_eq!(e["accountSource"], "PREPARATION_RPC_CAPTURE");
        assert_eq!(e["actualOutput"], p["quotedOutput"]);
        assert_eq!(e["sourceBankUnchanged"], true);
        for (mode, error) in [
            ("changed_pool", "QUOTE_STATE_CHANGED"),
            ("changed_program", "PREPARATION_PROGRAM_CHANGED"),
            ("changed_pump_fee_program", "PREPARATION_PROGRAM_CHANGED"),
            ("changed_clock", "QUOTE_CLOCK_CHANGED"),
            ("frozen", "PREPARATION_ACCOUNT_INVALID"),
            (
                "simulation_mismatch",
                "PREPARATION_SIMULATION_AMOUNT_MISMATCH",
            ),
        ] {
            *bank.mode.lock().unwrap() = mode.into();
            let (status, p) = call(&api, &path, input.clone()).await;
            assert_ne!(status, StatusCode::OK, "{index} {mode}: {p}");
            assert_eq!(p["error"], error, "{index} {mode}: {p}");
            assert!(p.get("unsignedTransaction").is_none());
        }
        *bank.mode.lock().unwrap() = "ready".into();
        let mut wrong = input.clone();
        wrong["sourceTokenAccount"] = bank.data["destinationTokenAccount"].clone();
        let (status, p) = call(&api, &path, wrong).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(p["error"], "PREPARATION_ACCOUNT_INVALID");
        for mode in [
            "missing_source",
            "missing_destination",
            "insufficient",
            "fee_balance",
        ] {
            *bank.mode.lock().unwrap() = mode.into();
            let (status, p) = call(&api, &path, input.clone()).await;
            assert_eq!(status, StatusCode::OK, "{index} {mode}: {p}");
            assert_eq!(p["ready"], false);
            assert!(p["unsignedTransaction"].is_null());
        }
        *bank.mode.lock().unwrap() = "ready".into();
        for option in [
            "createDestinationAta",
            "wrapSourceSol",
            "refundWrappedSource",
            "unwrapDestinationSol",
        ] {
            let mut invalid = input.clone();
            invalid[option] = true.into();
            let (_, p) = call(&api, &path, invalid).await;
            assert_eq!(p["error"], "PREPARATION_OPTIONS_UNSUPPORTED");
        }
        task.abort();
    }
}
#[tokio::test]
async fn pump_swap_signed_submission_receipt_matches_original_user_and_fee_accounts() {
    for index in [1, 3, 5, 7, 8] {
        let bank = Bank::pump_amm(index);
        let (url, task) = bank.serve().await;
        let (base, snapshot) = bank.api(&url);
        let mut config = (*base.config).clone();
        drop(base);
        let directory = tempfile::tempdir().unwrap();
        config.config.api.solana_runtime_manifest = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/sbf-runtime/rpc-capture.json"
        )
        .into();
        config.config.api.transaction_submission_enabled = true;
        config.config.api.transaction_submission_chains =
            vec![snapshot.reference.block.chain_key.clone()];
        config.config.api.transaction_max_pending = 2;
        config.config.api.transaction_receipt_journal =
            directory.path().join("receipts").to_str().unwrap().into();
        let api = xone_quote_api::Api::new(config.clone()).unwrap();
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let (endpoint, r) = quote_request(&bank);
        let (status, q) = call(&api, endpoint, r).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let id = q["quoteId"].as_str().unwrap();
        let (status, p) = call(
            &api,
            &format!("/v1/quotes/{id}/prepare"),
            bank.preparation(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{index}: {p}");
        let prefix = format!(
            "/v1/quotes/{id}/preparations/{}",
            p["preparationHash"].as_str().unwrap()
        );
        let (status, e) = call(&api, &format!("{prefix}/simulate-independent"), Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{e}");
        let signed = wallet_worker(
            &bank,
            "sign",
            p["unsignedTransaction"].as_str().unwrap(),
            "ready",
        )
        .await;
        let payload = json!({"signedTransaction":signed["signedTransaction"],"evidenceHash":e["evidenceHash"],"confirm":true});
        let (status, r) = call(&api, &format!("{prefix}/submit"), payload.clone()).await;
        assert_eq!(status, StatusCode::OK, "{index}: {r}");
        assert_eq!(r["state"], "SUBMITTED");
        let (status, r) = call(&api, &format!("{prefix}/submit"), payload).await;
        assert_eq!(status, StatusCode::OK, "{r}");
        drop(api);
        let api = xone_quote_api::Api::new(config).unwrap();
        api.replica.install_baseline(snapshot).unwrap();
        let (status, r) = call(
            &api,
            &format!("/v1/transactions/{}", signed["signature"].as_str().unwrap()),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{r}");
        assert_eq!(r["state"], "FINALIZED");
        assert_eq!(r["settlement"]["actualInput"], bank.data["requiredInput"]);
        assert_eq!(r["settlement"]["actualOutput"], bank.data["actualOutput"]);
        assert_eq!(r["settlement"]["feeLamports"], "5000");
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.as_str() == "sendTransaction")
                .count(),
            1
        );
        task.abort();
    }
}
