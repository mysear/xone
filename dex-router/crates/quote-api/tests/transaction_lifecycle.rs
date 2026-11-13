#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use execution_bank::{wallet_worker, Bank, AUTH};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(api: &xone_quote_api::Api, path: &str, value: Value) -> (StatusCode, Value) {
    let response = api
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
    (
        response.status(),
        serde_json::from_slice(&to_bytes(response.into_body(), 4 << 20).await.unwrap()).unwrap(),
    )
}
#[tokio::test]
async fn wallet_submission_requires_signature_evidence_confirmation_and_durable_reconciliation() {
    for mode in [
        "ready",
        "send_uncertain",
        "submit_pending",
        "send_fail",
        "expire_after_send",
        "receipt_mismatch",
        "submit_confirmed",
        "receipt_missing",
    ] {
        let bank = Bank::token2022(1);
        let (url, task) = bank.serve().await;
        let (base, snapshot) = bank.api(&url);
        let mut config = (*base.config).clone();
        drop(base);
        let directory = tempfile::tempdir().unwrap();
        config.config.api.solana_runtime_manifest = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/sbf-runtime/token2022.json"
        )
        .into();
        config.config.api.transaction_submission_enabled = true;
        config.config.api.transaction_submission_chains =
            vec![snapshot.reference.block.chain_key.clone()];
        config.config.api.transaction_max_pending = 2;
        config.config.api.transaction_receipt_journal = directory
            .path()
            .join("receipts.ndjson")
            .to_str()
            .unwrap()
            .into();
        let api = xone_quote_api::Api::new(config.clone()).unwrap();
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let (status, q) = call(&api, "/v1/quote-exact-out", bank.request()).await;
        assert_eq!(status, StatusCode::OK, "{mode}: {q}");
        let id = q["quoteId"].as_str().unwrap();
        let(status,p)=call(&api,&format!("/v1/quotes/{id}/prepare"),json!({"sourceTokenAccount":bank.data["sourceTokenAccount"],"createDestinationAta":true})).await;
        assert_eq!(status, StatusCode::OK, "{mode}: {p}");
        let prep = p["preparationHash"].as_str().unwrap();
        let prefix = format!("/v1/quotes/{id}/preparations/{prep}");
        let mut disabled = config.clone();
        disabled.config.api.transaction_submission_enabled = false;
        disabled.config.api.transaction_receipt_journal.clear();
        disabled.config.api.transaction_max_pending = 0;
        disabled.config.api.transaction_submission_chains.clear();
        let no_send = xone_quote_api::Api::new(disabled).unwrap();
        no_send.replica.install_baseline(snapshot.clone()).unwrap();

        let signed = wallet_worker(
            &bank,
            "sign",
            p["unsignedTransaction"].as_str().unwrap(),
            "ready",
        )
        .await;
        let request = json!({"signedTransaction":signed["signedTransaction"],"evidenceHash":"missing","confirm":true});
        let (status, e) = call(&no_send, &format!("{prefix}/submit"), request.clone()).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(e["error"], "TRANSACTION_SUBMISSION_DISABLED");
        drop(no_send);

        let (status, e) = call(&api, &format!("{prefix}/submit"), request.clone()).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(e["error"], "INDEPENDENT_SIMULATION_REQUIRED");
        assert!(bank.transactions.lock().unwrap().is_empty());
        let (status, e) = call(&api, &format!("{prefix}/simulate-independent"), Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{mode}: {e}");
        let mut request = request;
        request["evidenceHash"] = e["evidenceHash"].clone();
        let mut tampered = request.clone();
        let mut raw = STANDARD
            .decode(tampered["signedTransaction"].as_str().unwrap())
            .unwrap();
        raw[1] ^= 1;
        tampered["signedTransaction"] = STANDARD.encode(raw).into();
        let (status, e) = call(&api, &format!("{prefix}/verify-signature"), tampered).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(e["error"], "WALLET_SIGNATURE_INVALID");
        let mut no_confirm = request.clone();
        no_confirm["confirm"] = false.into();
        let (status, e) = call(&api, &format!("{prefix}/submit"), no_confirm).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(e["error"], "TRANSACTION_CONFIRMATION_REQUIRED");
        let (status, e) = call(&api, &format!("{prefix}/verify-signature"), request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{e}");
        assert_eq!(e["signature"], signed["signature"]);
        assert_eq!(e["submitted"], false);
        if mode == "ready" {
            *bank.mode.lock().unwrap() = "simulation_mismatch".into();
            let (status, e) = call(&api, &format!("{prefix}/submit"), request.clone()).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(e["error"], "PREPARATION_SIMULATION_AMOUNT_MISMATCH");
            assert_eq!(
                bank.calls
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|v| v.as_str() == "sendTransaction")
                    .count(),
                0
            );
            *bank.mode.lock().unwrap() = "wrong_chain".into();
            let (status, e) = call(&api, &format!("{prefix}/submit"), request.clone()).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(e["error"], "PREPARATION_WRONG_CHAIN");
            *bank.mode.lock().unwrap() = "ready".into();
            let mut wrong = request.clone();
            wrong["evidenceHash"] = "other".into();
            let (status, e) = call(&api, &format!("{prefix}/submit"), wrong).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(e["error"], "INDEPENDENT_SIMULATION_REQUIRED");
        }
        *bank.mode.lock().unwrap() = mode.into();
        let (status, receipt) = call(&api, &format!("{prefix}/submit"), request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{mode}: {receipt}");
        assert_eq!(receipt["signature"], signed["signature"]);
        assert_eq!(
            receipt["state"],
            if mode == "send_uncertain" {
                "UNKNOWN"
            } else {
                "SUBMITTED"
            }
        );
        let (status, duplicate) = call(&api, &format!("{prefix}/submit"), request).await;
        assert_eq!(status, StatusCode::OK, "{duplicate}");
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.as_str() == "sendTransaction")
                .count(),
            1,
            "{mode}"
        );
        // Simulate server restart: only the fsynced journal survives, no quote/prep cache.
        drop(api);
        let api = xone_quote_api::Api::new(config).unwrap();
        api.replica.install_baseline(snapshot).unwrap();
        let path = format!("/v1/transactions/{}", signed["signature"].as_str().unwrap());
        let (status, receipt) = call(&api, &path, Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{mode}: {receipt}");
        let expected = match mode {
            "submit_pending" => "PROCESSED",
            "send_fail" => "FAILED",
            "expire_after_send" => "EXPIRED",
            "receipt_mismatch" => "SETTLEMENT_MISMATCH",
            "submit_confirmed" => "CONFIRMED",
            "receipt_missing" => "RECEIPT_PENDING",
            _ => "FINALIZED",
        };
        assert_eq!(receipt["state"], expected, "{mode}: {receipt}");
        if matches!(mode, "ready" | "send_uncertain") {
            assert_eq!(receipt["settlement"]["verified"], true);
            assert_eq!(receipt["settlement"]["actualInput"], "1502");
            assert_eq!(receipt["settlement"]["actualOutput"], "1000");
        }
        if matches!(
            mode,
            "submit_pending" | "submit_confirmed" | "receipt_missing"
        ) {
            *bank.mode.lock().unwrap() = "ready".into();
            let (status, receipt) = call(&api, &path, Value::Null).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(receipt["state"], "FINALIZED");
        }
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.as_str() == "sendTransaction")
                .count(),
            1
        );
        drop(api);
        task.abort();
    }
}
