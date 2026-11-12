#[path = "support/execution_bank.rs"]
mod execution_bank;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use execution_bank::{Bank, AUTH};
use serde_json::{json, Value};
use tower::ServiceExt;
async fn call(
    api: &xone_quote_api::Api,
    path: &str,
    body: Value,
    auth: bool,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if auth {
        builder = builder.header("authorization", format!("Bearer {AUTH}"));
    }
    let response = api
        .handler()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"error":String::from_utf8_lossy(&bytes)})),
    )
}
#[tokio::test]
async fn preparation_checks_current_program_accounts_funds_and_compiles_unsigned_transaction() {
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (api, snapshot) = bank.api(&url);
    let req: xone_quote_api::ExactOutRequest = serde_json::from_value(bank.request()).unwrap();
    req.validate()
        .unwrap_or_else(|e| panic!("request {} {e}", bank.request()));
    let (status, quote) = call(&api, "/v1/quote-exact-out", bank.request(), true).await;
    assert_eq!(status, StatusCode::OK, "{quote}");
    assert_eq!(quote["requiredSellAmount"], bank.data["requiredInput"]);
    let path = format!("/v1/quotes/{}/prepare", quote["quoteId"].as_str().unwrap());
    let (status, prepared) = call(&api, &path, bank.preparation(), true).await;
    assert_eq!(status, StatusCode::OK, "{prepared}");
    assert_eq!(prepared["ready"], true, "{prepared}");
    assert_eq!(prepared["quoteId"], quote["quoteId"]);
    assert_eq!(prepared["planHash"], quote["planHash"]);
    assert_eq!(prepared["simulationRequired"], true);
    assert_eq!(prepared["submitted"], false);
    assert_eq!(prepared["signed"], false);
    let tx = STANDARD
        .decode(prepared["unsignedTransaction"].as_str().unwrap())
        .unwrap();
    assert_eq!(tx[0], 1);
    assert!(tx[1..65].iter().all(|b| *b == 0));
    assert_eq!(
        &tx[65..],
        STANDARD
            .decode(prepared["messageBase64"].as_str().unwrap())
            .unwrap()
    );
    assert!(tx.len() <= 1232);
    let instruction = &prepared["instructions"][0];
    let data = STANDARD
        .decode(instruction["dataBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(&data[..8], &[43, 215, 247, 132, 137, 60, 243, 81]);
    assert_eq!(u64::from_le_bytes(data[16..24].try_into().unwrap()), 1000);
    assert_eq!(
        instruction["accounts"][4]["pubkey"],
        bank.data["sourceTokenAccount"]
    );
    if let Ok(output) = std::env::var("XONE_PREPARATION_EVIDENCE") {
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&json!({"quote":quote,"preparation":prepared})).unwrap(),
        )
        .unwrap();
    }
    for mode in [
        "missing_source",
        "missing_destination",
        "insufficient",
        "fee_balance",
    ] {
        *bank.mode.lock().unwrap() = mode.into();
        let (status, body) = call(&api, &path, bank.preparation(), true).await;
        assert_eq!(status, StatusCode::OK, "{mode} {body}");
        assert_eq!(body["ready"], false, "{mode} {body}");
        assert!(body["unsignedTransaction"].is_null());
        assert!(!body["blockers"].as_array().unwrap().is_empty());
    }
    for mode in [
        "wrong_owner",
        "frozen",
        "changed_pool",
        "changed_program",
        "wrong_chain",
        "stale",
        "changed_clock",
        "expired_blockhash",
    ] {
        *bank.mode.lock().unwrap() = mode.into();
        let (status, body) = call(&api, &path, bank.preparation(), true).await;
        assert_ne!(status, StatusCode::OK, "{mode} {body}");
        assert!(body.get("unsignedTransaction").is_none());
    }
    *bank.mode.lock().unwrap() = "ready".into();
    bank.calls.lock().unwrap().clear();
    assert_eq!(
        call(&api, &path, bank.preparation(), false).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(bank.calls.lock().unwrap().is_empty());
    assert_ne!(
        call(&api, &path, json!({"rpcUrl":"http://bad.example"}), true)
            .await
            .0,
        StatusCode::OK
    );
    let (status, default) = call(&api, &path, json!({}), true).await;
    assert_eq!(status, StatusCode::OK, "{default}");
    assert_eq!(default["ready"], false);
    assert_ne!(
        default["sourceTokenAccount"],
        bank.data["sourceTokenAccount"]
    );
    let mut exact_in = bank.request();
    exact_in["kind"] = json!("EXACT_IN");
    exact_in.as_object_mut().unwrap().remove("buyAmount");
    exact_in.as_object_mut().unwrap().remove("maxSellAmount");
    exact_in["sellAmount"] = json!("1000");
    let (status, quote_in) = call(&api, "/v1/quote", exact_in, true).await;
    assert_eq!(status, StatusCode::OK, "{quote_in}");
    let (status, prepared_in) = call(
        &api,
        &format!(
            "/v1/quotes/{}/prepare",
            quote_in["quoteId"].as_str().unwrap()
        ),
        bank.preparation(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{prepared_in}");
    assert_eq!(prepared_in["ready"], true);
    let raw = STANDARD
        .decode(
            prepared_in["instructions"][0]["dataBase64"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(&raw[..8], &[65, 75, 63, 76, 235, 91, 91, 136]);
    assert_eq!(u64::from_le_bytes(raw[8..16].try_into().unwrap()), 1000);
    assert_eq!(
        u64::from_le_bytes(raw[16..24].try_into().unwrap()).to_string(),
        quote_in["minBuyAmount"].as_str().unwrap()
    );
    let mut advanced = snapshot.clone();
    advanced.reference.online_generation += 1;
    advanced.reference.journal_seq += 1;
    advanced.reference.block.hash = bank.data["blockhash"].as_str().unwrap().into();
    api.replica.install_baseline(advanced).unwrap();
    bank.calls.lock().unwrap().clear();
    assert_ne!(
        call(&api, &path, bank.preparation(), true).await.0,
        StatusCode::OK
    );
    assert!(bank.calls.lock().unwrap().is_empty());
    api.replica.suspend(&snapshot.reference.block.chain_key);
    assert_ne!(
        call(&api, &path, bank.preparation(), true).await.0,
        StatusCode::OK
    );
    assert!(bank.calls.lock().unwrap().iter().all(|m| [
        "getGenesisHash",
        "getMultipleAccounts",
        "getLatestBlockhash",
        "getFeeForMessage"
    ]
    .contains(&m.as_str())));
    task.abort();
}
#[tokio::test]
async fn cors_accepts_configured_local_frontend_and_rejects_unknown_origins() {
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (api, _) = bank.api(&url);
    for (origin, allowed) in [
        ("http://127.0.0.1:5173", true),
        ("http://localhost:5173", true),
        ("https://unlisted.invalid", false),
    ] {
        let response = api
            .handler()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/v1/quote-options")
                    .header("origin", origin)
                    .header("access-control-request-method", "GET")
                    .header("access-control-request-headers", "authorization")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .is_some(),
            allowed
        );
    }
    task.abort();
}

#[tokio::test]
async fn destination_ata_creation_rent_and_original_rpc_simulation_are_bound_to_quote() {
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (api, _) = bank.api(&url);
    let (status, quote) = call(&api, "/v1/quote-exact-out", bank.request(), true).await;
    assert_eq!(status, StatusCode::OK, "{quote}");
    let path = format!("/v1/quotes/{}/prepare", quote["quoteId"].as_str().unwrap());
    let request = json!({"sourceTokenAccount":bank.data["sourceTokenAccount"],"createDestinationAta":true,"simulate":true});
    let (status, result) = call(&api, &path, request.clone(), true).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["ready"], true, "{result}");
    assert_eq!(result["accountCreation"], true);
    assert_eq!(result["rentLamports"], "2039280");
    assert_eq!(result["totalFundingLamports"], "2044280");
    assert_eq!(result["instructions"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["instructions"][0]["programId"],
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
    );
    assert_eq!(result["instructions"][0]["dataBase64"], "AQ==");
    assert_eq!(result["simulation"]["level"], "RPC_SIMULATED");
    assert_eq!(result["simulation"]["actualInput"], "699");
    assert_eq!(result["simulation"]["actualOutput"], "1000");
    assert_eq!(
        result["simulation"]["independentProductionVerification"],
        false
    );
    if let Ok(output) = std::env::var("XONE_ATA_EVIDENCE") {
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&json!({"quote":quote,"preparation":result})).unwrap(),
        )
        .unwrap();
    }
    *bank.mode.lock().unwrap() = "rent_balance".into();
    let (status, result) = call(&api, &path, request.clone(), true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["ready"], false);
    assert_eq!(result["blockers"][0], "INSUFFICIENT_RENT_BALANCE");
    assert!(result["unsignedTransaction"].is_null());
    for (mode, code) in [
        ("changed_associated_program", "PREPARATION_PROGRAM_CHANGED"),
        (
            "simulation_mismatch",
            "PREPARATION_SIMULATION_AMOUNT_MISMATCH",
        ),
        ("simulation_error", "PREPARATION_SIMULATION_FAILED"),
    ] {
        *bank.mode.lock().unwrap() = mode.into();
        let (status, result) = call(&api, &path, request.clone(), true).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(result["error"], code, "{result}");
    }
    *bank.mode.lock().unwrap() = "missing_destination".into();
    let mut invalid = request.clone();
    invalid["destinationTokenAccount"] = bank.data["destinationTokenAccount"].clone();
    let (status, result) = call(&api, &path, invalid, true).await;
    assert_ne!(status, StatusCode::OK);
    assert_eq!(result["error"], "PREPARATION_CREATE_REQUIRES_ATA");
    *bank.mode.lock().unwrap() = "ready".into();
    let mut existing = bank.preparation();
    existing["createDestinationAta"] = json!(true);
    existing["simulate"] = json!(true);
    let (status, result) = call(&api, &path, existing, true).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["accountCreation"], false);
    assert_eq!(result["rentLamports"], "0");
    assert_eq!(result["instructions"].as_array().unwrap().len(), 1);
    assert!(bank.calls.lock().unwrap().iter().all(|m| [
        "getGenesisHash",
        "getMultipleAccounts",
        "getLatestBlockhash",
        "getFeeForMessage",
        "getMinimumBalanceForRentExemption",
        "simulateTransaction"
    ]
    .contains(&m.as_str())));
    task.abort();
}

#[tokio::test]
async fn native_source_wrap_creates_only_missing_wsol_and_funds_only_shortfall() {
    for balance in [None, Some(100u64), Some(3000u64)] {
        let bank = Bank::wsol(balance);
        let (url, task) = bank.serve().await;
        let (api, _) = bank.api(&url);
        let mut request = bank.request();
        request["maxSellAmount"] = json!("2000");
        let (status, q) = call(&api, "/v1/quote-exact-out", request, true).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        assert_eq!(q["requiredSellAmount"], bank.data["requiredInput"]);
        let path = format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap());
        let request = json!({"destinationTokenAccount":bank.data["destinationTokenAccount"],"wrapSourceSol":true,"simulate":true});
        let (status, p) = call(&api, &path, request.clone(), true).await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], true, "{p}");
        assert_eq!(p["sourceAccountCreation"], balance.is_none());
        let wrap = 1472u64.saturating_sub(balance.unwrap_or(0));
        assert_eq!(p["wrapLamports"], wrap.to_string());
        assert_eq!(
            p["expectedResidualWsol"],
            (balance.unwrap_or(0) + wrap - 1464).to_string()
        );
        assert_eq!(p["sourceWsolRetained"], true);
        assert_eq!(p["simulation"]["actualInput"], "1464");
        assert_eq!(p["simulation"]["actualOutput"], "1000");
        let instructions = p["instructions"].as_array().unwrap();
        assert_eq!(
            instructions.len(),
            if balance.is_none() {
                4
            } else if wrap > 0 {
                3
            } else {
                1
            }
        );
        if wrap > 0 {
            let offset = usize::from(balance.is_none());
            let transfer = &instructions[offset];
            assert_eq!(transfer["programId"], "11111111111111111111111111111111");
            let raw = STANDARD
                .decode(transfer["dataBase64"].as_str().unwrap())
                .unwrap();
            assert_eq!(&raw[..4], &2u32.to_le_bytes());
            assert_eq!(u64::from_le_bytes(raw[4..12].try_into().unwrap()), wrap);
            assert_eq!(instructions[offset + 1]["dataBase64"], "EQ==");
        }
        if balance.is_none() {
            if let Ok(path) = std::env::var("XONE_WRAP_EVIDENCE") {
                std::fs::write(
                    path,
                    serde_json::to_vec_pretty(&json!({"quote":q,"preparation":p})).unwrap(),
                )
                .unwrap();
            }
            let (status, both) = call(
                &api,
                &path,
                json!({"wrapSourceSol":true,"createDestinationAta":true,"simulate":true}),
                true,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{both}");
            assert_eq!(both["ready"], true);
            assert_eq!(both["instructions"].as_array().unwrap().len(), 5);
            assert_eq!(both["rentLamports"], "4078560");
            assert_eq!(both["totalFundingLamports"], "4085032");
            *bank.mode.lock().unwrap() = "wrap_balance".into();
            let (status, p) = call(&api, &path, request, true).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(p["ready"], false);
            assert_eq!(p["blockers"][0], "INSUFFICIENT_WRAP_BALANCE");
            assert!(p["unsignedTransaction"].is_null());
            *bank.mode.lock().unwrap() = "ready".into();
            let (status, p) = call(
                &api,
                &path,
                json!({"destinationTokenAccount":bank.data["destinationTokenAccount"]}),
                true,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(p["blockers"][0], "SOURCE_ACCOUNT_MISSING");
            let(status,p)=call(&api,&path,json!({"wrapSourceSol":true,"sourceTokenAccount":bank.data["pool"]["token0"],"destinationTokenAccount":bank.data["destinationTokenAccount"]}),true).await;
            assert_ne!(status, StatusCode::OK, "{p}");
        }
        let mut exact = bank.request();
        exact["kind"] = json!("EXACT_IN");
        exact.as_object_mut().unwrap().remove("buyAmount");
        exact.as_object_mut().unwrap().remove("maxSellAmount");
        exact["sellAmount"] = json!("2000");
        let (status, q) = call(&api, "/v1/quote", exact, true).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let(status,p)=call(&api,&format!("/v1/quotes/{}/prepare",q["quoteId"].as_str().unwrap()),json!({"wrapSourceSol":true,"simulate":true,"destinationTokenAccount":bank.data["destinationTokenAccount"]}),true).await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], true);
        assert_eq!(p["simulation"]["actualInput"], "2000");
        task.abort();
    }
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (api, _) = bank.api(&url);
    let (_, q) = call(&api, "/v1/quote-exact-out", bank.request(), true).await;
    let (status, p) = call(
        &api,
        &format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap()),
        json!({"wrapSourceSol":true}),
        true,
    )
    .await;
    assert_ne!(status, StatusCode::OK);
    assert_eq!(p["error"], "PREPARATION_WRAP_REQUIRES_WSOL");
    task.abort();
}

#[tokio::test]
async fn output_unwrap_and_input_refund_only_close_new_temporary_accounts() {
    for refund in [false, true] {
        let bank = if refund {
            Bank::wsol(Some(3000))
        } else {
            Bank::new()
        };
        let (url, task) = bank.serve().await;
        let (api, _) = bank.api(&url);
        let mut request = bank.request();
        request["maxSellAmount"] = json!("2000");
        if !refund {
            request["recipient"] = bank.data["caller"].clone();
        }
        let (status, q) = call(&api, "/v1/quote-exact-out", request.clone(), true).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let path = format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap());
        let prep = if refund {
            json!({"wrapSourceSol":true,"refundWrappedSource":true,"destinationTokenAccount":bank.data["destinationTokenAccount"],"simulate":true})
        } else {
            json!({"unwrapDestinationSol":true,"sourceTokenAccount":bank.data["sourceTokenAccount"],"simulate":true})
        };
        let (status, p) = call(&api, &path, prep.clone(), true).await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], true, "{p}");
        assert_eq!(p["simulation"]["temporaryAccountClosed"], true);
        assert_eq!(p["rentRefundLamports"], "2039280");
        assert_eq!(
            p["instructions"][0]["programId"],
            "11111111111111111111111111111111"
        );
        assert_eq!(
            p["instructions"].as_array().unwrap().last().unwrap()["dataBase64"],
            "CQ=="
        );
        if refund {
            assert_eq!(p["wrapLamports"], "1472");
            assert_eq!(p["expectedNativeRefundLamports"], "2039288");
            assert_eq!(p["expectedPayerLamportDelta"], "-6464");
            assert_eq!(p["sourceWsolRetained"], false);
            assert_ne!(p["sourceTokenAccount"], bank.data["sourceTokenAccount"]);
            assert_eq!(p["simulation"]["actualInput"], "1464");
        } else {
            assert_eq!(p["settlementAsset"], "native:solana:sol");
            assert_eq!(p["expectedNativeRefundLamports"], "2040280");
            assert_eq!(p["expectedPayerLamportDelta"], "-4000");
            assert_eq!(p["simulation"]["actualInput"], "699");
        }
        assert_eq!(p["simulation"]["actualOutput"], "1000");
        if let Ok(folder) = std::env::var("XONE_TEMP_EVIDENCE_DIR") {
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(
                std::path::Path::new(&folder).join(if refund {
                    "refund-preparation.json"
                } else {
                    "unwrap-preparation.json"
                }),
                serde_json::to_vec_pretty(&json!({"quote":q,"preparation":p})).unwrap(),
            )
            .unwrap();
        }
        for (mode, code) in [
            ("temporary_occupied", "PREPARATION_TEMP_ACCOUNT_OCCUPIED"),
            (
                "native_credit_mismatch",
                "PREPARATION_SIMULATION_AMOUNT_MISMATCH",
            ),
            ("temp_not_closed", "PREPARATION_TEMP_ACCOUNT_NOT_CLOSED"),
        ] {
            *bank.mode.lock().unwrap() = mode.into();
            let (status, p) = call(&api, &path, prep.clone(), true).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(p["error"], code, "{mode} {p}");
            assert!(p.get("unsignedTransaction").is_none());
        }
        *bank.mode.lock().unwrap() = "ready".into();
        let mut invalid = prep.clone();
        invalid[if refund {
            "sourceTokenAccount"
        } else {
            "destinationTokenAccount"
        }] = bank.data[if refund {
            "sourceTokenAccount"
        } else {
            "destinationTokenAccount"
        }]
        .clone();
        let (status, p) = call(&api, &path, invalid, true).await;
        assert_ne!(status, StatusCode::OK, "{p}");
        if refund {
            let (status, p) = call(&api, &path, json!({"refundWrappedSource":true}), true).await;
            assert_ne!(status, StatusCode::OK);
            assert_eq!(p["error"], "PREPARATION_REFUND_REQUIRES_TEMP_SOURCE");
            let (status, p) = call(
                &api,
                &path,
                json!({"wrapSourceSol":true,"refundWrappedSource":true,"createDestinationAta":true,"simulate":true}),
                true,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{p}");
            assert_eq!(p["accountCreation"], true);
            assert_eq!(p["rentLamports"], "4078560");
            assert_eq!(p["rentRefundLamports"], "2039280");
            assert_eq!(p["totalFundingLamports"], "4085032");
            assert_eq!(p["expectedPayerLamportDelta"], "-2045744");
            assert_eq!(p["simulation"]["actualInput"], "1464");
            assert_eq!(p["simulation"]["actualOutput"], "1000");
            assert_eq!(p["instructions"].as_array().unwrap().len(), 7);
        }
        // ExactIn is composed with the same isolated closure templates.
        request["kind"] = json!("EXACT_IN");
        request.as_object_mut().unwrap().remove("buyAmount");
        request.as_object_mut().unwrap().remove("maxSellAmount");
        request["sellAmount"] = json!("1000");
        let (status, q) = call(&api, "/v1/quote", request, true).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let (status, p) = call(
            &api,
            &format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap()),
            prep,
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], true);
        assert_eq!(p["simulation"]["actualInput"], "1000");
        task.abort();
    }
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (api, _) = bank.api(&url);
    let (_, q) = call(&api, "/v1/quote-exact-out", bank.request(), true).await;
    let path = format!("/v1/quotes/{}/prepare", q["quoteId"].as_str().unwrap());
    let (status, p) = call(&api, &path, json!({"unwrapDestinationSol":true}), true).await;
    assert_ne!(status, StatusCode::OK);
    assert_eq!(p["error"], "PREPARATION_UNWRAP_CONTEXT_UNSUPPORTED");
    task.abort();
}
