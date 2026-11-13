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
// Each guard is about its own RPC/account condition, so give it a fresh
// quote rather than sharing one deadline across repeated full-program reads.
async fn fresh_path(api: &xone_quote_api::Api, bank: &Bank) -> String {
    let (endpoint, request) = quote_request(bank);
    let (status, quote) = call(api, endpoint, request).await;
    assert_eq!(status, StatusCode::OK, "{quote}");
    format!("/v1/quotes/{}/prepare", quote["quoteId"].as_str().unwrap())
}
#[tokio::test]
async fn pump_native_curve_all_directions_prepare_simulate_and_reject_mutations() {
    for index in 0..5 {
        let bank = Bank::pump_curve(index);
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
        if index == 4 {
            assert_eq!(q["outputSurplus"], "9971");
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
        let required = p["requiredInput"].as_str().unwrap().parse::<u64>().unwrap();
        assert_eq!(
            p["totalFundingLamports"],
            (5000
                + if bank.data["zeroForOne"] == true {
                    0
                } else {
                    required
                })
            .to_string()
        );
        assert_eq!(p["sourceNativeSol"], bank.data["zeroForOne"] != true);
        assert_eq!(p["destinationNativeSol"], bank.data["zeroForOne"]);
        assert_eq!(p["requiredSigners"], json!([bank.data["caller"]]));
        assert_eq!(p["instructions"].as_array().unwrap().len(), 1);
        if let Ok(folder) = std::env::var("XONE_PUMP_CURVE_PREPARATION_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!("pump-curve-{index}-preparation.json")),
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
            let (status, p) = call(&api, &fresh_path(&api, &bank).await, input.clone()).await;
            assert_ne!(status, StatusCode::OK, "{index} {mode}: {p}");
            assert_eq!(p["error"], error, "{index} {mode}: {p}");
            assert!(p.get("unsignedTransaction").is_none());
        }
        *bank.mode.lock().unwrap() = "ready".into();
        let mut wrong = input.clone();
        wrong["sourceTokenAccount"] = bank.data["destinationTokenAccount"].clone();
        let (status, p) = call(&api, &fresh_path(&api, &bank).await, wrong).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(
            p["error"],
            if bank.data["zeroForOne"] == true {
                "PREPARATION_ACCOUNT_INVALID"
            } else {
                "PREPARATION_NATIVE_ACCOUNT_INVALID"
            }
        );
        let zero = bank.data["zeroForOne"] == true;
        for mode in [
            if zero {
                "missing_source"
            } else {
                "missing_destination"
            },
            if zero { "insufficient" } else { "fee_balance" },
        ] {
            *bank.mode.lock().unwrap() = mode.into();
            let (status, p) = call(&api, &fresh_path(&api, &bank).await, input.clone()).await;
            assert_eq!(status, StatusCode::OK, "{index} {mode}: {p}");
            assert_eq!(p["ready"], false);
            assert!(p["unsignedTransaction"].is_null());
        }
        *bank.mode.lock().unwrap() = "ready".into();
        for option in [
            "wrapSourceSol",
            "refundWrappedSource",
            "unwrapDestinationSol",
        ] {
            let mut invalid = input.clone();
            invalid[option] = true.into();
            let (_, p) = call(&api, &fresh_path(&api, &bank).await, invalid).await;
            assert_eq!(p["error"], "PREPARATION_OPTIONS_UNSUPPORTED");
        }
        let mut create_existing = bank.preparation();
        create_existing["createDestinationAta"] = true.into();
        let (status, existing) = call(&api, &fresh_path(&api, &bank).await, create_existing).await;
        if bank.data["zeroForOne"] == true {
            assert_ne!(status, StatusCode::OK);
            assert_eq!(existing["error"], "PREPARATION_DESTINATION_NOT_ATA");
        } else {
            assert_eq!(status, StatusCode::OK, "{existing}");
            assert_eq!(existing["ready"], true);
            assert_eq!(existing["accountCreation"], false);
            assert_eq!(existing["rentLamports"], "0");
            assert_eq!(existing["instructions"].as_array().unwrap().len(), 1);
        }
        task.abort();
    }
}
#[tokio::test]
async fn pump_native_curve_signed_submission_receipt_matches_real_sol_and_fee_accounts() {
    for index in [0, 1, 2, 3, 4] {
        let bank = Bank::pump_curve(index);
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
            r["settlement"]["payerLamportDelta"],
            p["expectedPayerLamportDelta"]
        );
        assert_eq!(r["settlement"]["minBuyAmount"], p["minBuyAmount"]);
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

#[tokio::test]
async fn native_curve_creates_missing_destination_ata_and_separates_trade_rent_fee_receipts() {
    for index in [5usize, 6, 7, 8] {
        let bank = Bank::pump_curve(index);
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
        let path = format!("/v1/quotes/{id}/prepare");
        let (status, blocked) = call(&api, &path, bank.preparation()).await;
        assert_eq!(status, StatusCode::OK, "{blocked}");
        assert_eq!(blocked["ready"], false);
        assert_eq!(blocked["blockers"], json!(["DESTINATION_ACCOUNT_MISSING"]));
        assert!(blocked["unsignedTransaction"].is_null());
        let mut request = bank.preparation();
        request["createDestinationAta"] = true.into();
        request["simulate"] = true.into();
        let mut wrong = request.clone();
        wrong["destinationTokenAccount"] = bank.data["caller"].clone();
        let (_, bad) = call(&api, &fresh_path(&api, &bank).await, wrong).await;
        assert_eq!(bad["error"], "PREPARATION_DESTINATION_NOT_ATA");
        *bank.mode.lock().unwrap() = if bank.data["destinationPrefundedLamports"]
            .as_u64()
            .unwrap_or(0)
            >= 2039280
        {
            "fee_balance"
        } else {
            "rent_balance"
        }
        .into();
        let (status, bad) = call(&api, &fresh_path(&api, &bank).await, request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{bad}");
        assert_eq!(bad["ready"], false);
        assert!(bad["unsignedTransaction"].is_null());
        *bank.mode.lock().unwrap() = "simulation_mismatch".into();
        let (_, bad) = call(&api, &fresh_path(&api, &bank).await, request.clone()).await;
        assert_eq!(bad["error"], "PREPARATION_SIMULATION_AMOUNT_MISMATCH");
        *bank.mode.lock().unwrap() = "ready".into();
        let (endpoint, query) = quote_request(&bank);
        let (status, q) = call(&api, endpoint, query).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let id = q["quoteId"].as_str().unwrap();
        let path = format!("/v1/quotes/{id}/prepare");
        let (status, p) = call(&api, &path, request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{index}: {p}");
        assert_eq!(p["ready"], true);
        assert_eq!(p["accountCreation"], true);
        let prefund = bank.data["destinationPrefundedLamports"]
            .as_u64()
            .unwrap_or(0);
        let rent = 2039280u64.saturating_sub(prefund);
        assert_eq!(p["rentLamports"], rent.to_string());
        assert_eq!(p["instructions"].as_array().unwrap().len(), 2);
        assert_eq!(
            p["instructions"][0]["programId"],
            "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        );
        assert_eq!(p["simulation"]["actualInput"], bank.data["requiredInput"]);
        assert_eq!(p["simulation"]["actualOutput"], bank.data["actualOutput"]);
        let required = p["requiredInput"].as_str().unwrap().parse::<u64>().unwrap();
        assert_eq!(
            p["totalFundingLamports"],
            (rent + 5000 + required).to_string()
        );
        if let Ok(folder) = std::env::var("XONE_PUMP_CURVE_PREPARATION_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!("pump-curve-{index}-preparation.json")),
                serde_json::to_vec_pretty(&json!({"quote":q,"preparation":p})).unwrap(),
            )
            .unwrap();
        }
        let prefix = format!(
            "/v1/quotes/{id}/preparations/{}",
            p["preparationHash"].as_str().unwrap()
        );
        let (status, e) = call(&api, &format!("{prefix}/simulate-independent"), Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{e}");
        assert_eq!(e["actualOutput"], bank.data["actualOutput"]);
        let signed = wallet_worker(
            &bank,
            "sign",
            p["unsignedTransaction"].as_str().unwrap(),
            "ready",
        )
        .await;
        let payload = json!({"signedTransaction":signed["signedTransaction"],"evidenceHash":e["evidenceHash"],"confirm":true});
        let (status, r) = call(&api, &format!("{prefix}/submit"), payload.clone()).await;
        assert_eq!(status, StatusCode::OK, "{r}");
        let (status, r) = call(&api, &format!("{prefix}/submit"), payload).await;
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
        assert_eq!(r["state"], "FINALIZED");
        assert_eq!(r["settlement"]["actualInput"], bank.data["requiredInput"]);
        assert_eq!(r["settlement"]["actualOutput"], bank.data["actualOutput"]);
        assert_eq!(r["settlement"]["feeLamports"], "5000");
        assert_eq!(
            r["settlement"]["payerLamportDelta"],
            p["expectedPayerLamportDelta"]
        );
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.as_str() == "sendTransaction")
                .count(),
            1
        );
        task.abort();
    }
}

#[tokio::test]
async fn native_created_ata_failed_swap_receipt_is_failed_and_never_resubmitted() {
    let bank = Bank::pump_curve(6);
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
    let mut request = bank.preparation();
    request["createDestinationAta"] = true.into();
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
