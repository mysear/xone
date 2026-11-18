//! Real HTTP handlers, original SBF and signed receipts for native-unit economics.
#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use execution_bank::{wallet_worker, Bank, AUTH};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(api: &xone_quote_api::Api, path: &str, value: Value) -> Value {
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
    let value: Value =
        serde_json::from_slice(&to_bytes(r.into_body(), 8 << 20).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{path}: {value}");
    value
}
#[tokio::test]
async fn failed_signed_swaps_verify_actual_fee_rollback_metadata_and_durable_retry() {
    for (case, bank) in [
        Bank::token2022(1),
        Bank::raydium_funding(1),
        Bank::pump_curve(5),
    ]
    .into_iter()
    .enumerate()
    {
        let (url, task) = bank.serve().await;
        let (initial, snapshot) = bank.api_with_manifest(
            &url,
            Some(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/sbf-runtime/rpc-capture.json"
            )),
        );
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("receipts");
        let mut config = initial.config.as_ref().clone();
        drop(initial);
        config.config.api.transaction_submission_enabled = true;
        config.config.api.transaction_submission_chains =
            vec![snapshot.reference.block.chain_key.clone()];
        config.config.api.transaction_max_pending = 2;
        config.config.api.transaction_receipt_journal = journal.to_str().unwrap().into();
        let api = xone_quote_api::Api::new(config.clone()).unwrap();
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let request = bank.request();
        let q = call(&api, "/v1/quote-exact-out", request.clone()).await;
        let id = q["quoteId"].as_str().unwrap();
        let mut options = if bank.data["preparationOptions"].is_object() {
            bank.data["preparationOptions"].clone()
        } else {
            bank.preparation()
        };
        options["createDestinationAta"] = true.into();
        options["simulate"] = false.into();
        let p = call(&api, &format!("/v1/quotes/{id}/prepare"), options).await;
        let prefix = format!(
            "/v1/quotes/{id}/preparations/{}",
            p["preparationHash"].as_str().unwrap()
        );
        let e = call(&api, &format!("{prefix}/simulate-independent"), Value::Null).await;
        let signed = wallet_worker(
            &bank,
            "sign",
            p["unsignedTransaction"].as_str().unwrap(),
            "ready",
        )
        .await;
        *bank.mode.lock().unwrap() = "send_fail".into();
        let submission = json!({"signedTransaction":signed["signedTransaction"],"evidenceHash":e["evidenceHash"],"confirm":true});
        let sent = call(&api, &format!("{prefix}/submit"), submission.clone()).await;
        assert_eq!(sent["state"], "SUBMITTED");
        let repeated = call(&api, &format!("{prefix}/submit"), submission).await;
        assert_eq!(repeated["signature"], sent["signature"]);
        drop(api);
        let intent = std::fs::read(&journal).unwrap();
        let path = format!("/v1/transactions/{}", signed["signature"].as_str().unwrap());
        let mut probes = Vec::new();
        if case == 0 {
            for (fault, expected, code) in [
                ("failure_missing", "RECEIPT_PENDING", "RECEIPT_META_MISSING"),
                (
                    "failure_meta_missing",
                    "RECEIPT_PENDING",
                    "RECEIPT_META_MISSING",
                ),
                (
                    "failure_fee_missing",
                    "RECEIPT_PENDING",
                    "RECEIPT_META_MISSING",
                ),
                (
                    "failure_tokens_missing",
                    "RECEIPT_PENDING",
                    "RECEIPT_META_MISSING",
                ),
                (
                    "failure_wire",
                    "SETTLEMENT_MISMATCH",
                    "SIGNED_TRANSACTION_INVALID",
                ),
                (
                    "failure_slot",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_SLOT_MISMATCH",
                ),
                (
                    "failure_fee",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_BALANCE_MISMATCH",
                ),
                (
                    "failure_fee_type",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FEE_INVALID",
                ),
                (
                    "failure_payer",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_BALANCE_MISMATCH",
                ),
                (
                    "failure_other_native",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_BALANCE_MISMATCH",
                ),
                (
                    "failure_balance_length",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_BALANCE_INVALID",
                ),
                (
                    "failure_meta_success",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_STATUS_MISMATCH",
                ),
                (
                    "failure_meta_error",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_STATUS_MISMATCH",
                ),
                (
                    "failure_error_type",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_UNSUPPORTED",
                ),
                (
                    "failure_token_amount",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_TOKEN_MISMATCH",
                ),
                (
                    "failure_token_owner",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_FAILURE_TOKEN_MISMATCH",
                ),
                (
                    "failure_token_duplicate",
                    "SETTLEMENT_MISMATCH",
                    "RECEIPT_TOKEN_IDENTITY",
                ),
                (
                    "failure_token_omit_both",
                    "RECEIPT_PENDING",
                    "RECEIPT_META_MISSING",
                ),
            ] {
                let mut fork = config.clone();
                let local = dir.path().join(fault);
                std::fs::write(&local, &intent).unwrap();
                fork.config.api.transaction_receipt_journal = local.to_str().unwrap().into();
                let api = xone_quote_api::Api::new(fork).unwrap();
                *bank.mode.lock().unwrap() = fault.into();
                let r = call(&api, &path, Value::Null).await;
                assert_eq!(r["state"], expected, "{fault}: {r}");
                assert_eq!(r["error"]["feeVerificationError"], code, "{fault}: {r}");
                assert!(
                    r["settlement"].is_null() && r["failureFee"].is_null(),
                    "{fault}"
                );
                if expected == "RECEIPT_PENDING" {
                    *bank.mode.lock().unwrap() = "ready".into();
                    let recovered = call(&api, &path, Value::Null).await;
                    assert_eq!(recovered["state"], "FAILED", "{fault}: {recovered}");
                    assert_eq!(recovered["failureFee"]["networkFeeNative"], "5000");
                }
                probes.push(json!({"mode":fault,"receipt":r}));
            }
        }
        let api = xone_quote_api::Api::new(config.clone()).unwrap();
        *bank.mode.lock().unwrap() = "failure_processed".into();
        let processed = call(&api, &path, Value::Null).await;
        assert_eq!(processed["state"], "RECEIPT_PENDING");
        assert!(processed["failureFee"].is_null());
        *bank.mode.lock().unwrap() = "failure_confirmed".into();
        let confirmed = call(&api, &path, Value::Null).await;
        assert_eq!(confirmed["state"], "RECEIPT_PENDING");
        assert_eq!(confirmed["failureFee"]["networkFeeNative"], "5000");
        *bank.mode.lock().unwrap() = "failure_disappeared".into();
        let disappeared = call(&api, &path, Value::Null).await;
        assert_eq!(disappeared["state"], "UNKNOWN");
        assert!(disappeared["failureFee"].is_null());
        assert!(disappeared["confirmationStatus"].is_null() && disappeared["slot"].is_null());
        *bank.mode.lock().unwrap() = "failure_confirmed".into();
        assert_eq!(
            call(&api, &path, Value::Null).await["failureFee"],
            confirmed["failureFee"]
        );
        drop(api);
        let api = xone_quote_api::Api::new(config.clone()).unwrap();
        *bank.mode.lock().unwrap() = "ready".into();
        let r = call(&api, &path, Value::Null).await;
        assert_eq!(r["state"], "FAILED", "case {case}: {r}");
        assert_eq!(r["error"]["code"], "ONCHAIN_TRANSACTION_FAILED");
        assert!(r["settlement"].is_null());
        let fee = &r["failureFee"];
        assert_eq!(fee["networkFeeNative"], "5000");
        assert_eq!(fee["payerLamportDelta"], "-5000");
        assert_eq!(fee["retainedRentDepositNative"], "0");
        assert_eq!(fee["observedTokenRollbackMatched"], true);
        assert_eq!(fee["observedNativeRollbackMatched"], true);
        assert_eq!(fee["transactionSha256"], r["transactionSha256"]);
        assert!(fee["netBuyAmountEstimate"].is_null() && fee["netSellAmountEstimate"].is_null());
        drop(api);
        let api = xone_quote_api::Api::new(config).unwrap();
        *bank.mode.lock().unwrap() = "wrong_chain".into();
        let restored = call(&api, &path, Value::Null).await;
        assert_eq!(restored, r);
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|s| s.as_str() == "sendTransaction")
                .count(),
            1
        );
        if let Ok(folder) = std::env::var("XONE_SOLANA_FAILURE_FEE_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            let tx =
                bank.transactions.lock().unwrap()[signed["signature"].as_str().unwrap()].clone();
            std::fs::write(std::path::Path::new(&folder).join(format!("case-{case}.json")),serde_json::to_vec_pretty(&json!({"request":request,"preparation":p,"evidence":e,"signed":signed,"executed":tx,"processed":processed,"confirmed":confirmed,"receipt":r,"restored":restored,"probes":probes})).unwrap()).unwrap();
        }
        drop(api);
        task.abort();
    }
}
