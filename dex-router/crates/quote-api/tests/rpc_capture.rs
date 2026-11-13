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
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 << 20).await.unwrap();
    let result = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&bytes)));
    (status, result)
}
#[tokio::test]
async fn captured_rpc_accounts_execute_without_preinstalled_wallet_banks_and_do_not_refetch() {
    for index in 0..6 {
        let mut bank = match index {
            0 | 3 | 5 => Bank::new(),
            1 => Bank::token2022(1),
            _ => Bank::raydium(1),
        };
        if index == 5 {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let d = std::sync::Arc::make_mut(&mut bank.data);
            d["pool"] = d["after"].clone();
            let state = d["pool"]["meteora"].clone();
            let pair = hex::decode(state["pair_data"].as_str().unwrap()).unwrap();
            let pool_id = d["pool"]["pool_id"].as_str().unwrap().to_owned();
            d["accounts"][pool_id]["data"][0] = STANDARD.encode(&pair).into();
            let oracle = bs58::encode(&pair[552..584]).into_string();
            d["accounts"][oracle]["data"][0] = STANDARD
                .encode(hex::decode(state["oracle_data"].as_str().unwrap()).unwrap())
                .into();
            for item in state["bin_arrays"].as_array().unwrap() {
                let key = item["address"].as_str().unwrap();
                d["accounts"][key]["data"][0] = STANDARD
                    .encode(hex::decode(item["data"].as_str().unwrap()).unwrap())
                    .into();
            }
            for (side, offset) in [(0, 152), (1, 184)] {
                let key = bs58::encode(&pair[offset..offset + 32]).into_string();
                let value = state[format!("raw_vault{side}")].as_u64().unwrap();
                let mut raw = STANDARD
                    .decode(d["accounts"][&key]["data"][0].as_str().unwrap())
                    .unwrap();
                raw[64..72].copy_from_slice(&value.to_le_bytes());
                if raw[109..113] == [1, 0, 0, 0] {
                    d["accounts"][&key]["lamports"] =
                        (u64::from_le_bytes(raw[113..121].try_into().unwrap()) + value).into();
                }
                d["accounts"][key]["data"][0] = STANDARD.encode(raw).into();
            }
        }
        if index >= 3 {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let d = std::sync::Arc::make_mut(&mut bank.data);
            let field = if matches!(index, 3 | 5) {
                "meteora"
            } else {
                "raydium_cpmm"
            };
            let key = if matches!(index, 3 | 5) {
                "slot"
            } else {
                "bank_slot"
            };
            let slot = d["pool"][field][key].as_u64().unwrap() + 1;
            d["pool"][field][key] = slot.into();
            let clock = "SysvarC1ock11111111111111111111111111111111";
            let mut raw = STANDARD
                .decode(d["accounts"][clock]["data"][0].as_str().unwrap())
                .unwrap();
            raw[..8].copy_from_slice(&slot.to_le_bytes());
            d["accounts"][clock]["data"][0] = STANDARD.encode(raw).into();
        }
        // This user account address does not exist in any runtime bank manifest.
        let source = bank.data["sourceTokenAccount"]
            .as_str()
            .unwrap()
            .to_string();
        let new_source = "6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n";
        if source != new_source {
            let data = std::sync::Arc::make_mut(&mut bank.data);
            let a = data["accounts"]
                .as_object_mut()
                .unwrap()
                .remove(&source)
                .unwrap();
            data["accounts"][new_source] = a;
            data["sourceTokenAccount"] = new_source.into();
        }
        let (url, task) = bank.serve().await;
        let (base, snapshot) = bank.api(&url);
        let mut configuration = (*base.config).clone();
        drop(base);
        configuration.config.api.solana_runtime_manifest = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/sbf-runtime/rpc-capture.json"
        )
        .into();
        let api = xone_quote_api::Api::new(configuration).unwrap();
        api.replica.install_baseline(snapshot.clone()).unwrap();
        let (status, q) = call(&api, "/v1/quote-exact-out", bank.request()).await;
        assert_eq!(status, StatusCode::OK, "{index}: {q}");
        let id = q["quoteId"].as_str().unwrap();
        let prepare = if index == 1 {
            json!({"createDestinationAta":true,"wrapSourceSol":true,"refundWrappedSource":true})
        } else {
            bank.preparation()
        };
        let (status, p) = call(&api, &format!("/v1/quotes/{id}/prepare"), prepare.clone()).await;
        assert_eq!(status, StatusCode::OK, "{index}: {p}");
        assert_eq!(p["ready"], true);
        assert_eq!(p["accountSnapshot"]["source"], "PREPARATION_RPC_CAPTURE");
        assert!(p["accountSnapshot"]["snapshotHash"].as_str().is_some());
        assert!(p.get("accounts").is_none());
        let reads = bank
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v.as_str() == "getMultipleAccounts")
            .count();
        assert_eq!(reads, 1);
        // A changed current RPC must never mutate a previously captured bank.
        *bank.mode.lock().unwrap() = "changed_pool".into();
        let path = format!(
            "/v1/quotes/{id}/preparations/{}/simulate-independent",
            p["preparationHash"].as_str().unwrap()
        );
        let (status, e) = call(&api, &path, Value::Null).await;
        assert_eq!(status, StatusCode::OK, "{index}: {e}");
        assert_eq!(e["level"], "RPC_CAPTURE_SBF_SIMULATED");
        assert_eq!(e["accountSource"], "PREPARATION_RPC_CAPTURE");
        assert_eq!(e["bankSha256"], p["accountSnapshot"]["snapshotHash"]);
        assert_eq!(e["actualInput"], q["requiredSellAmount"]);
        assert_eq!(e["actualOutput"], "1000");
        assert_eq!(e["historicalBankVerified"], false);
        assert_eq!(e["independentProductionVerification"], false);
        assert_eq!(e["sourceBankUnchanged"], true);
        assert_eq!(
            bank.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v.as_str() == "getMultipleAccounts")
                .count(),
            reads
        );
        let (status, p) = call(&api, &format!("/v1/quotes/{id}/prepare"), prepare).await;
        assert_ne!(status, StatusCode::OK, "{p}");
        assert_eq!(p["error"], "QUOTE_STATE_CHANGED");
        let mut newer = snapshot;
        newer.reference.online_generation += 1;
        newer.reference.journal_seq += 1;
        api.replica.install_baseline(newer).unwrap();
        let (status, e) = call(&api, &path, Value::Null).await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(e["error"], "SNAPSHOT_REORGED");
        task.abort();
    }
}

#[tokio::test]
async fn captured_account_cache_evicts_by_byte_budget_before_entry_capacity() {
    let bank = Bank::new();
    let (url, task) = bank.serve().await;
    let (api, _) = bank.api(&url);
    let mut ids = Vec::new();
    for slippage in 0..24 {
        let mut request = bank.request();
        request["slippageBps"] = slippage.into();
        let (status, q) = call(&api, "/v1/quote-exact-out", request).await;
        assert_eq!(status, StatusCode::OK, "{q}");
        let id = q["quoteId"].as_str().unwrap().to_owned();
        let (status, p) = call(
            &api,
            &format!("/v1/quotes/{id}/prepare"),
            bank.preparation(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{p}");
        assert_eq!(p["ready"], true);
        ids.push((id, p["preparationHash"].as_str().unwrap().to_owned()));
    }
    let mut evicted = 0;
    for (id, prep) in &ids {
        let (status, value) = call(
            &api,
            &format!("/v1/quotes/{id}/preparations/{prep}/simulate-independent"),
            Value::Null,
        )
        .await;
        if value["error"] == "PREPARATION_NOT_FOUND" {
            assert_eq!(status, StatusCode::NOT_FOUND);
            evicted += 1;
        } else {
            assert_eq!(value["error"], "SBF_RUNTIME_NOT_CONFIGURED");
        }
    }
    assert!(
        evicted > 0,
        "24 large raw snapshots must hit the 32MiB budget before the 256-entry count limit"
    );
    let (id, prep) = ids.last().unwrap();
    let (_, value) = call(
        &api,
        &format!("/v1/quotes/{id}/preparations/{prep}/simulate-independent"),
        Value::Null,
    )
    .await;
    assert_eq!(value["error"], "SBF_RUNTIME_NOT_CONFIGURED");
    task.abort();
}
