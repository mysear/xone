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
fn input(bank: &Bank) -> Value {
    bank.data["preparationOptions"].clone()
}
async fn fresh_path(api: &xone_quote_api::Api, bank: &Bank) -> String {
    let (endpoint, request) = quote_request(bank);
    let (status, q) = call(api, endpoint, request).await;
    assert_eq!(status, StatusCode::OK, "{q}");
    format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap())
}
#[tokio::test]
async fn raydium_funding_accounts_execute_original_and_independent_vm() {
    for index in 0..10 {
        let bank = Bank::raydium_funding(index);
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
        let mut input = input(&bank);
        input["simulate"] = true.into();
        let (status, p) = call(
            &api,
            &format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap()),
            input,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{index} {}: {p}",
            bank.data["fundingScenario"]
        );
        assert_eq!(p["ready"], true, "{index}: {p}");
        assert_eq!(p["simulation"]["actualInput"], bank.data["requiredInput"]);
        assert_eq!(p["simulation"]["actualOutput"], bank.data["actualOutput"]);
        assert_eq!(p["requiredSigners"], json!([bank.data["caller"]]));
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.as_str() == "getMultipleAccounts")
                .count(),
            1
        );
        let (status, e) = call(
            &api,
            &format!(
                "/v1/quotes/{}/preparations/{}/simulate-independent",
                q["quoteId"].as_str().unwrap(),
                p["preparationHash"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{index}: {e}");
        assert_eq!(e["actualInput"], p["quotedInput"]);
        assert_eq!(e["actualOutput"], p["quotedOutput"]);
        assert_eq!(e["sourceBankUnchanged"], true);
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.as_str() == "getMultipleAccounts")
                .count(),
            1,
            "Independent simulation must use the captured batch"
        );
        if let Ok(folder) = std::env::var("XONE_RAYDIUM_FUNDING_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!("raydium-{index}-preparation.json")),
                serde_json::to_vec_pretty(&json!({"quote":q,"preparation":p,"independent":e}))
                    .unwrap(),
            )
            .unwrap();
        }
        task.abort();
    }
}
#[tokio::test]
async fn raydium_funding_wallet_receipts_reconcile_actual_rent_and_resume_without_resending() {
    for index in 0..10 {
        let bank = Bank::raydium_funding(index);
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
        let (status, p) = call(&api, &format!("/v1/quotes/{id}/prepare"), input(&bank)).await;
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
        if let Ok(folder) = std::env::var("XONE_RAYDIUM_FUNDING_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!("raydium-{index}-receipt.json")),
                serde_json::to_vec_pretty(
                    &json!({"quote":q,"preparation":p,"independent":e,"receipt":r}),
                )
                .unwrap(),
            )
            .unwrap();
        }

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
        assert_eq!(
            r["settlement"]["actualPersistentRentLamports"],
            (p["rentLamports"].as_str().unwrap().parse::<u64>().unwrap()
                - p["rentRefundLamports"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap())
            .to_string()
        );
        assert_eq!(
            r["settlement"]["payerLamportDelta"],
            p["expectedPayerLamportDelta"]
        );
        task.abort();
    }
}

#[tokio::test]
async fn raydium_funding_guards_require_explicit_options_program_identity_and_funds() {
    for index in [0, 1, 2] {
        let bank = Bank::raydium_funding(index);
        let (url, task) = bank.serve().await;
        let (api, _) = bank.api(&url);
        for (mode, error) in [
            ("changed_associated_program", "PREPARATION_PROGRAM_CHANGED"),
            ("changed_system_program", "PREPARATION_ACCOUNT_INVALID"),
            ("changed_program", "PREPARATION_PROGRAM_CHANGED"),
            ("changed_clock", "QUOTE_CLOCK_CHANGED"),
        ] {
            *bank.mode.lock().unwrap() = mode.into();
            let path = fresh_path(&api, &bank).await;
            let (status, p) = call(&api, &path, input(&bank)).await;
            assert_ne!(status, StatusCode::OK, "{index} {mode}: {p}");
            assert_eq!(p["error"], error, "{index} {mode}: {p}");
            assert!(p.get("unsignedTransaction").is_none());
        }
        *bank.mode.lock().unwrap() = "fee_balance".into();
        let path = fresh_path(&api, &bank).await;
        let (status, p) = call(&api, &path, input(&bank)).await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], false);
        assert!(p["blockers"]
            .as_array()
            .unwrap()
            .contains(&json!("INSUFFICIENT_FEE_BALANCE")));
        assert!(p["unsignedTransaction"].is_null());
        *bank.mode.lock().unwrap() = "ready".into();
        let path = fresh_path(&api, &bank).await;
        let mut bad = input(&bank);
        bad["refundWrappedSource"] = true.into();
        bad["wrapSourceSol"] = true.into();
        bad["sourceTokenAccount"] = bank.data["sourceTokenAccount"].clone();
        let (_, p) = call(&api, &path, bad).await;
        assert_eq!(
            p["error"],
            if index == 2 {
                "PREPARATION_WRAP_REQUIRES_WSOL"
            } else {
                "PREPARATION_REFUND_REQUIRES_TEMP_SOURCE"
            }
        );
        if index == 1 {
            *bank.mode.lock().unwrap() = "temporary_occupied".into();
            let path = fresh_path(&api, &bank).await;
            let (_, p) = call(&api, &path, input(&bank)).await;
            assert_eq!(p["error"], "PREPARATION_TEMP_ACCOUNT_OCCUPIED", "{p}");
        }
        task.abort();
    }
}

#[tokio::test]
async fn raydium_funding_failed_swap_receipts_never_resubmit() {
    for index in [0, 1, 2, 4] {
        let bank = Bank::raydium_funding(index);
        let (url, task) = bank.serve().await;
        let (initial, snapshot) = bank.api_with_manifest(
            &url,
            Some(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/sbf-runtime/rpc-capture.json"
            )),
        );
        let directory = tempfile::tempdir().unwrap();
        let mut config = initial.config.as_ref().clone();
        drop(initial);
        config.config.api.transaction_submission_enabled = true;
        config.config.api.transaction_submission_chains = vec!["solana:mainnet-beta".into()];
        config.config.api.transaction_max_pending = 2;
        config.config.api.transaction_receipt_journal =
            directory.path().join("receipts").to_str().unwrap().into();
        let api = xone_quote_api::Api::new(config.clone()).unwrap();
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let (endpoint, request) = quote_request(&bank);
        let (status, q) = call(&api, endpoint, request).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let id = q["quoteId"].as_str().unwrap();
        let request = input(&bank);
        let (status, p) = call(&api, &format!("/v1/quotes/{id}/prepare"), request).await;
        assert_eq!(status, StatusCode::OK, "{p}");
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
        *bank.mode.lock().unwrap() = "send_fail".into();
        let(status,r)=call(&api,&format!("{prefix}/submit"),json!({"signedTransaction":signed["signedTransaction"],"evidenceHash":e["evidenceHash"],"confirm":true})).await;
        assert_eq!(status, StatusCode::OK, "{r}");
        drop(api);
        let restored = xone_quote_api::Api::new(config).unwrap();
        restored.replica.install_baseline(snapshot).unwrap();
        let (status, r) = call(
            &restored,
            &format!("/v1/transactions/{}", signed["signature"].as_str().unwrap()),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{r}");
        assert_eq!(r["state"], "FAILED");
        assert_ne!(r["settlement"]["verified"], true);
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.as_str() == "sendTransaction")
                .count(),
            1
        );
        task.abort();
    }
}

#[tokio::test]
async fn raydium_creation_and_wrapping_opt_out_remain_blocked_without_unsigned_wire() {
    let bank = Bank::raydium_funding(0);
    let (url, task) = bank.serve().await;
    let (api, _) = bank.api(&url);
    for (field, blocker) in [
        ("wrapSourceSol", "SOURCE_ACCOUNT_MISSING"),
        ("createDestinationAta", "DESTINATION_ACCOUNT_MISSING"),
    ] {
        let path = fresh_path(&api, &bank).await;
        let mut request = input(&bank);
        request[field] = false.into();
        let (status, p) = call(&api, &path, request).await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], false);
        assert!(p["blockers"].as_array().unwrap().contains(&json!(blocker)));
        assert!(p["unsignedTransaction"].is_null());
    }
    task.abort();
}
