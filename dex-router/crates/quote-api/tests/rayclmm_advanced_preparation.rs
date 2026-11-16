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
async fn rayclmm_advanced_funding_accounts_execute_original_and_independent_vm() {
    for index in 0..162 {
        let bank = Bank::rayclmm_advanced(index);
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
            bank.data["executionScenario"]
        );
        assert_eq!(p["ready"], true, "{index}: {p}");
        assert_eq!(p["computeUnitLimit"], 1_400_000);
        assert_eq!(
            p["instructions"][0]["programId"],
            "ComputeBudget111111111111111111111111111111"
        );
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
        if let Ok(folder) = std::env::var("XONE_RAYCLMM_ADVANCED_EXECUTION_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder)
                    .join(format!("rayclmm-advanced-{index}-preparation.json")),
                serde_json::to_vec_pretty(&json!({"quote":q,"preparation":p,"independent":e}))
                    .unwrap(),
            )
            .unwrap();
        }
        task.abort();
    }
}
#[tokio::test]
async fn rayclmm_advanced_funding_wallet_receipts_reconcile_actual_rent_and_resume_without_resending(
) {
    for index in 0..162 {
        let bank = Bank::rayclmm_advanced(index);
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
        assert_eq!(status, StatusCode::OK, "{index}: {e}");
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
        if let Ok(folder) = std::env::var("XONE_RAYCLMM_ADVANCED_EXECUTION_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder)
                    .join(format!("rayclmm-advanced-{index}-receipt.json")),
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
async fn rayclmm_advanced_funding_guards_require_explicit_options_program_identity_and_funds() {
    for index in [
        8, 10, 12, 26, 28, 30, 44, 46, 48, 62, 64, 66, 80, 82, 84, 98, 100, 102, 116, 118, 120,
        134, 136, 138, 152, 154, 156,
    ] {
        let bank = Bank::rayclmm_advanced(index);
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
            if index % 18 == 12 {
                "PREPARATION_WRAP_REQUIRES_WSOL"
            } else {
                "PREPARATION_REFUND_REQUIRES_TEMP_SOURCE"
            }
        );
        if index % 18 == 10 {
            *bank.mode.lock().unwrap() = "temporary_occupied".into();
            let path = fresh_path(&api, &bank).await;
            let (_, p) = call(&api, &path, input(&bank)).await;
            assert_eq!(p["error"], "PREPARATION_TEMP_ACCOUNT_OCCUPIED", "{p}");
        }
        task.abort();
    }
}

#[tokio::test]
async fn rayclmm_advanced_funding_failed_swap_receipts_never_resubmit() {
    for index in [
        0, 8, 10, 12, 18, 26, 28, 30, 36, 44, 46, 48, 54, 62, 64, 66, 72, 80, 82, 84, 90, 98, 100,
        102, 108, 116, 118, 120, 126, 134, 136, 138, 144, 152, 154, 156,
    ] {
        let bank = Bank::rayclmm_advanced(index);
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
        assert_eq!(status, StatusCode::OK, "{index}: {e}");
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
async fn rayclmm_advanced_creation_and_wrapping_opt_out_remain_blocked_without_unsigned_wire() {
    let bank = Bank::rayclmm_advanced(8);
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

#[tokio::test]
async fn rayclmm_advanced_complete_source_state_and_runtime_closure_reject_mutations() {
    for index in [
        0, 4, 18, 22, 36, 40, 54, 58, 72, 76, 90, 94, 108, 112, 126, 130, 144, 148,
    ] {
        let bank = Bank::rayclmm_advanced(index);
        let (url, task) = bank.serve().await;
        let (api, _) = bank.api(&url);
        for (mode, error) in [
            ("changed_pool", "QUOTE_STATE_CHANGED"),
            ("changed_rayclmm_array", "QUOTE_STATE_CHANGED"),
            (
                "changed_rayclmm_order",
                "PREPARATION_RAYCLMM_ORDERS_UNSUPPORTED",
            ),
            (
                "changed_rayclmm_version",
                "PREPARATION_RAYCLMM_VERSION_UNSUPPORTED",
            ),
            ("changed_rayclmm_observation", "QUOTE_STATE_CHANGED"),
            ("changed_rayclmm_config", "QUOTE_STATE_CHANGED"),
            ("changed_rayclmm_mint", "PREPARATION_ACCOUNT_INVALID"),
            ("underbacked_rayclmm_vault", "PREPARATION_ACCOUNT_INVALID"),
            ("changed_rayclmm_budget", "PREPARATION_PROGRAM_CHANGED"),
            (
                "changed_rayclmm_token2022_program",
                "PREPARATION_PROGRAM_CHANGED",
            ),
            ("changed_rayclmm_memo", "PREPARATION_PROGRAM_CHANGED"),
            ("missing_rayclmm_observation", "PREPARATION_ACCOUNT_INVALID"),
            ("changed_rayclmm_extension", "QUOTE_STATE_CHANGED"),
            ("missing_rayclmm_extension", "PREPARATION_ACCOUNT_INVALID"),
            ("changed_rayclmm_seed", "QUOTE_STATE_CHANGED"),
            ("changed_rayclmm_dynamic", "QUOTE_STATE_CHANGED"),
            (
                "changed_rayclmm_dynamic_reserved",
                "PREPARATION_RAYCLMM_VERSION_UNSUPPORTED",
            ),
            ("changed_epoch", "QUOTE_CLOCK_CHANGED"),
        ] {
            if mode == "underbacked_rayclmm_vault"
                && bank.data["pool"]["token0"] != xone_domain::pump_amm::WSOL
            {
                continue;
            }
            if mode == "changed_rayclmm_dynamic"
                && bank.data["pool"]["raydium_clmm"]["dynamic_fee"].is_null()
            {
                continue;
            }
            *bank.mode.lock().unwrap() = mode.into();
            let path = fresh_path(&api, &bank).await;
            let (status, result) = call(&api, &path, input(&bank)).await;
            assert_ne!(status, StatusCode::OK, "{index} {mode}: {result}");
            assert_eq!(result["error"], error, "{index} {mode}: {result}");
            assert!(result.get("unsignedTransaction").is_none());
        }
        task.abort();
    }
}

#[tokio::test]
async fn rayclmm_advanced_declared_compute_budget_requires_a_compatible_pinned_runtime() {
    let bank = Bank::rayclmm_advanced(0);
    let (url, task) = bank.serve().await;
    let mut manifest: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/sbf-runtime/rpc-capture.json"
        ))
        .unwrap(),
    )
    .unwrap();
    manifest["max_compute_units"] = 200_000.into();
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("runtime.json");
    std::fs::write(&file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let (api, _) = bank.api_with_manifest(&url, Some(file.to_str().unwrap()));
    let (endpoint, request) = quote_request(&bank);
    let (status, q) = call(&api, endpoint, request).await;
    assert_eq!(status, StatusCode::OK, "{q}");
    let (status, p) = call(
        &api,
        &format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap()),
        input(&bank),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!(p["computeUnitLimit"], 1_400_000);
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
    assert_ne!(status, StatusCode::OK, "{e}");
    assert_eq!(e["error"], "SBF_BANK_NOT_COVERED");
    task.abort();
}
