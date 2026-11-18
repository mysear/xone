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
fn number(v: &Value, key: &str) -> u64 {
    v[key].as_str().unwrap().parse().unwrap()
}
#[tokio::test]
async fn native_economics_original_sbf_and_signed_receipts_use_actual_amounts_and_separate_deposits(
) {
    let mut banks: Vec<(Bank, bool)> = (0..6).map(|i| (Bank::pump_curve(i), i == 5)).collect();
    banks.extend([1usize, 2, 4, 5].map(|i| (Bank::raydium_funding(i), false)));
    banks.push((Bank::new(), false));
    for (index, (bank, create)) in banks.into_iter().enumerate() {
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
        config.config.api.transaction_submission_chains =
            vec![snapshot.reference.block.chain_key.clone()];
        config.config.api.transaction_max_pending = 2;
        config.config.api.transaction_receipt_journal =
            directory.path().join("receipts").to_str().unwrap().into();
        let api = xone_quote_api::Api::new(config.clone()).unwrap();
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let mut request = bank.request();
        let exact = bank.data.get("exactOut").is_none_or(|v| v == true);
        if !exact {
            request["kind"] = "EXACT_IN".into();
            request.as_object_mut().unwrap().remove("buyAmount");
            request.as_object_mut().unwrap().remove("maxSellAmount");
            request["sellAmount"] = bank.data["sellAmount"].clone();
        }
        let quote = call(
            &api,
            if exact {
                "/v1/quote-exact-out"
            } else {
                "/v1/quote"
            },
            request.clone(),
        )
        .await;
        let id = quote["quoteId"].as_str().unwrap();
        let mut options = if bank.data["preparationOptions"].is_object() {
            bank.data["preparationOptions"].clone()
        } else {
            bank.preparation()
        };
        options["simulate"] = false.into();
        if create {
            options["createDestinationAta"] = true.into();
        }
        let p = call(&api, &format!("/v1/quotes/{id}/prepare"), options).await;
        assert_eq!(p["ready"], true);
        let prefix = format!(
            "/v1/quotes/{id}/preparations/{}",
            p["preparationHash"].as_str().unwrap()
        );
        let evidence = call(&api, &format!("{prefix}/simulate-independent"), Value::Null).await;
        let cost = &evidence["costs"];
        assert_eq!(cost["sourceAuthority"], "RPC_CAPTURE_SBF_EXECUTION");
        assert_eq!(cost["quoteRankingApplied"], false);
        assert_eq!(cost["independentProductionVerification"], false);
        let input = number(&evidence, "actualInput");
        let output = number(&evidence, "actualOutput");
        let fee = number(&evidence, "feeLamports");
        let rent = number(&p, "rentLamports") - number(&p, "rentRefundLamports");
        let native_input = p["sourceNativeSol"] == true || p["refundWrappedSource"] == true;
        let native_output = p["destinationNativeSol"] == true || p["unwrapDestinationSol"] == true;
        if native_input {
            assert_eq!(cost["netSellAmountEstimate"], (input + fee).to_string());
            assert!(cost["netBuyAmountEstimate"].is_null());
            assert_eq!(
                cost["actualPayerNativeDelta"],
                (-i128::from(input) - i128::from(fee) - i128::from(rent)).to_string()
            );
        } else if native_output {
            assert_eq!(
                cost["netBuyAmountEstimate"],
                output
                    .checked_sub(fee)
                    .map_or(Value::Null, |n| json!(n.to_string()))
            );
            assert_eq!(
                cost["actualPayerNativeDelta"],
                (i128::from(output) - i128::from(fee) - i128::from(rent)).to_string()
            );
        } else {
            assert_eq!(cost["tokenValuationStatus"], "UNAVAILABLE");
            assert!(cost["netBuyAmountEstimate"].is_null());
            assert!(cost["netSellAmountEstimate"].is_null());
        }
        if index == 4 {
            assert_eq!(evidence["actualOutput"], "10971");
            assert_eq!(cost["netBuyAmountEstimate"], "5971");
            assert_eq!(quote["minBuyAmount"], "1000");
        }
        if index == 5 {
            assert_eq!(cost["retainedRentDepositNative"], "2039280");
            assert_eq!(cost["netSellAmountEstimate"], "1005000");
        }
        let signed = wallet_worker(
            &bank,
            "sign",
            p["unsignedTransaction"].as_str().unwrap(),
            "ready",
        )
        .await;
        let payload = json!({"signedTransaction":signed["signedTransaction"],"evidenceHash":evidence["evidenceHash"],"confirm":true});
        let submitted = call(&api, &format!("{prefix}/submit"), payload.clone()).await;
        let repeated = call(&api, &format!("{prefix}/submit"), payload).await;
        assert_eq!(submitted["signature"], repeated["signature"]);
        drop(api);
        let restored = xone_quote_api::Api::new(config).unwrap();
        restored.replica.install_baseline(snapshot).unwrap();
        let receipt = call(
            &restored,
            &format!("/v1/transactions/{}", signed["signature"].as_str().unwrap()),
            Value::Null,
        )
        .await;
        assert_eq!(receipt["state"], "FINALIZED");
        let actual = &receipt["settlement"]["costs"];
        assert_eq!(actual["sourceAuthority"], "RPC_TRANSACTION_RECEIPT");
        for key in [
            "netBuyAmountEstimate",
            "netSellAmountEstimate",
            "networkFeeNative",
            "retainedRentDepositNative",
            "actualPayerNativeDelta",
            "tokenValuationStatus",
        ] {
            assert_eq!(actual[key], cost[key], "{index}: {key}");
        }
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.as_str() == "sendTransaction")
                .count(),
            1
        );
        if let Ok(folder) = std::env::var("XONE_SOLANA_NATIVE_ECONOMICS_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            let executed =
                bank.transactions.lock().unwrap()[signed["signature"].as_str().unwrap()].clone();
            std::fs::write(std::path::Path::new(&folder).join(format!("case-{index}.json")),serde_json::to_vec_pretty(&json!({"request":request,"quote":quote,"preparation":p,"evidence":evidence,"receipt":receipt,"signed":signed,"executed":executed})).unwrap()).unwrap();
        }
        task.abort();
    }
}
