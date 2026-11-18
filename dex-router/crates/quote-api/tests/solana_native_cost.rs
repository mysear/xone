//! Actual quote/preparation HTTP and original SBF execution across Solana families.
#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use execution_bank::{Bank, AUTH};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
async fn call(api: &xone_quote_api::Api, path: &str, input: Value) -> Value {
    let response = api
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {AUTH}"))
                .header("content-type", "application/json")
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 8 << 20).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{path}: {value}");
    value
}
fn number(value: &Value, key: &str) -> u64 {
    value[key].as_str().unwrap().parse().unwrap()
}
#[tokio::test]
async fn solana_native_cost_actual_http_binds_message_fee_funding_and_original_sbf() {
    let mut banks = vec![
        Bank::new(),
        Bank::token2022(0),
        Bank::raydium(0),
        Bank::raydium_funding(0),
        Bank::pump_amm(0),
        Bank::pump_curve(0),
        Bank::orca(0),
        Bank::orca_adaptive(0),
        Bank::rayclmm(0),
        Bank::rayclmm_advanced(0),
    ];
    banks.extend((1..10).map(Bank::raydium_funding));
    let ahead = Bank::new();
    *ahead.mode.lock().unwrap() = "fee_context_ahead".into();
    banks.push(ahead);
    assert_eq!(banks.len(), 20);
    for (i, bank) in banks.into_iter().enumerate() {
        let (url, task) = bank.serve().await;
        let (api, _) = bank.api_with_manifest(
            &url,
            Some(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/sbf-runtime/rpc-capture.json"
            )),
        );
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
            request,
        )
        .await;
        let mut input = if bank.data["preparationOptions"].is_object() {
            bank.data["preparationOptions"].clone()
        } else {
            bank.preparation()
        };
        input["simulate"] = false.into();
        let p = call(
            &api,
            &format!("/v1/quotes/{}/prepare", quote["quoteId"].as_str().unwrap()),
            input,
        )
        .await;
        assert_eq!(p["ready"], true, "{i}: {p}");
        let c = &p["costs"];
        assert_eq!(c["status"], "MESSAGE_BOUND_RPC_ESTIMATE");
        assert_eq!(c["sourceAuthority"], "PREPARATION_RPC_OBSERVATION");
        assert_eq!(c["nativeUnit"], "LAMPORTS");
        assert_eq!(c["networkFeeNative"], p["feeLamports"]);
        assert_eq!(c["nativeKnownSubtotal"], p["feeLamports"]);
        assert_eq!(c["nativeFundingRequired"], p["totalFundingLamports"]);
        let principal = if p["sourceNativeSol"] == true {
            number(&p, "requiredInput")
        } else {
            number(&p, "wrapLamports")
        };
        assert_eq!(
            number(&p, "totalFundingLamports"),
            number(&p, "feeLamports") + number(&p, "rentLamports") + principal
        );
        assert_eq!(
            c["expectedRetainedRentDepositNative"],
            (number(&p, "rentLamports") - number(&p, "rentRefundLamports")).to_string()
        );
        let message = STANDARD
            .decode(p["messageBase64"].as_str().unwrap())
            .unwrap();
        assert_eq!(c["messageSha256"], format!("{:x}", Sha256::digest(message)));
        assert_eq!(
            c["accountSnapshotHash"],
            p["accountSnapshot"]["snapshotHash"]
        );
        assert!(c["feeObservedSlot"].as_u64().unwrap() >= p["observedSlot"].as_u64().unwrap());
        if i == 19 {
            assert_eq!(
                c["feeObservedSlot"].as_u64().unwrap(),
                p["observedSlot"].as_u64().unwrap() + 1
            );
        }
        assert_eq!(c["feeContextPolicy"], "MIN_CONTEXT_SLOT_NOT_SAME_BANK");
        assert_eq!(c["historicalBankVerified"], false);
        assert!(c["totalNativeEstimate"].is_null());
        assert!(c["netBuyAmountEstimate"].is_null());
        assert_eq!(p["submitted"], false);
        let evidence = call(
            &api,
            &format!(
                "/v1/quotes/{}/preparations/{}/simulate-independent",
                quote["quoteId"].as_str().unwrap(),
                p["preparationHash"].as_str().unwrap()
            ),
            Value::Null,
        )
        .await;
        assert_eq!(evidence["actualInput"], p["quotedInput"], "{i}: {evidence}");
        assert_eq!(
            evidence["actualOutput"], p["quotedOutput"],
            "{i}: {evidence}"
        );
        assert_eq!(evidence["sourceBankUnchanged"], true);
        assert_eq!(evidence["feeLamports"], c["networkFeeNative"]);
        if let Ok(folder) = std::env::var("XONE_SOLANA_NATIVE_COST_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(format!("case-{i}.json")),
                serde_json::to_vec_pretty(
                    &json!({"quote":quote,"preparation":p,"evidence":evidence}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        task.abort();
    }
}
