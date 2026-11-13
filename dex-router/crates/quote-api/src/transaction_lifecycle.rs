//! Explicit opt-in wallet submission, signed preflight and durable receipt updates.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use execution_receipts::{Journal, Receipt, State as TransactionState};
use sha2::{Digest, Sha256};
use xone_execution_plan::solana_signed::{verify, SignedTransaction};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SubmissionRequest {
    signed_transaction: String,
    evidence_hash: String,
    #[serde(default)]
    confirm: bool,
}
fn decode(value: &str) -> Result<Vec<u8>> {
    if value.len() > 1644 {
        return Err(err("SIGNED_TRANSACTION_INVALID"));
    }
    STANDARD
        .decode(value)
        .map_err(|_| err("SIGNED_TRANSACTION_INVALID"))
}
fn journal(api: &Api) -> Result<&Arc<Mutex<Journal>>> {
    api.receipts
        .as_ref()
        .ok_or_else(|| err("TRANSACTION_SUBMISSION_DISABLED"))
}
fn policy(api: &Api, chain: &str) -> Result<()> {
    let c = &api.config.config.api;
    if !c.transaction_submission_enabled
        || !c.transaction_submission_chains.iter().any(|v| v == chain)
    {
        return Err(err("TRANSACTION_SUBMISSION_DISABLED"));
    }
    Ok(())
}
async fn rpc(api: &Api, chain: &str, method: &str, params: Value) -> Result<Value> {
    let configured = api
        .config
        .config
        .chains
        .iter()
        .find(|v| v.key == chain)
        .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
    let url = configured
        .rpc_urls
        .first()
        .ok_or_else(|| err("PREPARATION_RPC_NOT_CONFIGURED"))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(
            api.config.config.api.quote_timeout_ms.clamp(1000, 15000),
        ))
        .build()
        .map_err(|_| err("TRANSACTION_RPC_UNAVAILABLE"))?;
    let mut response = client
        .post(url)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .map_err(|_| err("TRANSACTION_RPC_UNAVAILABLE"))?;
    if !response.status().is_success() {
        return Err(err("TRANSACTION_RPC_UNAVAILABLE"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| err("TRANSACTION_RPC_UNAVAILABLE"))?
    {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err(err("TRANSACTION_RPC_LIMIT"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| err("TRANSACTION_RPC_INVALID"))?;
    if value["id"] != 1 || value.get("error").is_some() {
        return Err(err("TRANSACTION_RPC_INVALID"));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| err("TRANSACTION_RPC_INVALID"))
}
async fn genesis(api: &Api, chain: &str, expected: &str) -> Result<()> {
    if rpc(api, chain, "getGenesisHash", json!([])).await?.as_str() != Some(expected) {
        return Err(err("PREPARATION_WRONG_CHAIN"));
    }
    Ok(())
}
fn signed(
    api: &Api,
    id: &str,
    preparation: &str,
    request: &SubmissionRequest,
) -> Result<(solana_simulation::Prepared, SignedTransaction, Vec<u8>)> {
    let p = solana_simulation::selected(api, id, preparation)?;
    policy(api, &p.context.chain_key)?;
    let engine = api
        .solana_sbf
        .as_ref()
        .ok_or_else(|| err("INDEPENDENT_SIMULATION_REQUIRED"))?;
    let chain = api
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == p.context.chain_key)
        .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
    if !engine.available(&p.pool, &chain.genesis_hash) {
        return Err(err("SBF_BANK_NOT_COVERED"));
    }
    let evidence = p
        .evidence
        .as_ref()
        .ok_or_else(|| err("INDEPENDENT_SIMULATION_REQUIRED"))?;
    if evidence["success"] != true
        || evidence["evidenceHash"].as_str() != Some(request.evidence_hash.as_str())
        || evidence["preparationHash"] != p.result["preparationHash"]
    {
        return Err(err("INDEPENDENT_SIMULATION_REQUIRED"));
    }
    let raw = decode(&request.signed_transaction)?;
    let unsigned = decode(
        p.result["unsignedTransaction"]
            .as_str()
            .ok_or_else(|| err("SIGNED_TRANSACTION_INVALID"))?,
    )?;
    let info = verify(&unsigned, &raw, &p.context.caller)?;
    Ok((p, info, raw))
}
pub(crate) async fn verify_wallet(
    State(api): State<Api>,
    headers: HeaderMap,
    Path((id, preparation)): Path<(String, String)>,
    Json(request): Json<SubmissionRequest>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let (_, info, _) = signed(&api, &id, &preparation, &request).map_err(failure)?;
    Ok(Json(
        json!({"signature":info.signature,"transactionSha256":info.transaction_sha256,"messageSha256":info.message_sha256,"preparationHash":preparation,"evidenceHash":request.evidence_hash,"signed":true,"submitted":false}),
    ))
}
async fn preflight(
    api: &Api,
    p: &solana_simulation::Prepared,
    raw: &[u8],
    expected_genesis: &str,
) -> Result<()> {
    let chain = &p.context.chain_key;
    genesis(api, chain, expected_genesis).await?;
    let last = p.result["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    let height = rpc(
        api,
        chain,
        "getBlockHeight",
        json!([{"commitment":"confirmed"}]),
    )
    .await?
    .as_u64()
    .ok_or_else(|| err("TRANSACTION_RPC_INVALID"))?;
    if height > last {
        return Err(err("PREPARATION_BLOCKHASH_EXPIRED"));
    }
    let blockhash = p.result["recentBlockhash"]
        .as_str()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    let slot = p.result["observedSlot"]
        .as_u64()
        .ok_or_else(|| err("TRANSACTION_RPC_INVALID"))?;
    let valid = rpc(
        api,
        chain,
        "isBlockhashValid",
        json!([blockhash,{"commitment":"confirmed","minContextSlot":slot}]),
    )
    .await?;
    if valid["value"] != true || valid["context"]["slot"].as_u64().is_none_or(|v| v < slot) {
        return Err(err("PREPARATION_BLOCKHASH_EXPIRED"));
    }
    let fee = rpc(
        api,
        chain,
        "getFeeForMessage",
        json!([p.result["messageBase64"],{"commitment":"confirmed","minContextSlot":slot}]),
    )
    .await?;
    let cost = fee["value"]
        .as_u64()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    let addresses = json!([
        p.result["sourceTokenAccount"],
        p.result["destinationTokenAccount"],
        p.context.caller
    ]);
    let before = rpc(
        api,
        chain,
        "getMultipleAccounts",
        json!([addresses,{"encoding":"base64","commitment":"confirmed","minContextSlot":slot}]),
    )
    .await?;
    let observed = before["context"]["slot"]
        .as_u64()
        .filter(|n| *n >= slot)
        .ok_or_else(|| err("PREPARATION_STALE_RPC"))?;
    let simulation=rpc(api,chain,"simulateTransaction",json!([STANDARD.encode(raw),{"encoding":"base64","commitment":"confirmed","minContextSlot":observed,"sigVerify":true,"replaceRecentBlockhash":false,"accounts":{"encoding":"base64","addresses":addresses}}])).await?;
    if simulation["value"].get("err") != Some(&Value::Null) {
        return Err(err("PREPARATION_SIMULATION_FAILED"));
    }
    if simulation["context"]["slot"]
        .as_u64()
        .is_none_or(|n| n < observed)
    {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let before = before["value"]
        .as_array()
        .ok_or_else(|| err("TRANSACTION_RPC_INVALID"))?;
    let after = simulation["value"]["accounts"]
        .as_array()
        .ok_or_else(|| err("TRANSACTION_RPC_INVALID"))?;
    xone_execution_preparation::verify_simulated_settlement(
        &p.context, &p.result, before, after, cost,
    )?;
    Ok(())
}
pub(crate) async fn submit(
    State(api): State<Api>,
    headers: HeaderMap,
    Path((id, preparation)): Path<(String, String)>,
    Json(request): Json<SubmissionRequest>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    if !request.confirm {
        return Err(failure(err("TRANSACTION_CONFIRMATION_REQUIRED")));
    }
    let raw = decode(&request.signed_transaction).map_err(failure)?;
    if raw.len() < 65 {
        return Err(failure(err("SIGNED_TRANSACTION_INVALID")));
    }
    let signature = bs58::encode(&raw[1..65]).into_string();
    if let Some(existing) = journal(&api)
        .map_err(failure)?
        .lock()
        .map_err(|_| failure(err("RECEIPT_LOCK")))?
        .get(&signature)
    {
        if existing.quote_id != id || existing.preparation["preparationHash"] != preparation {
            return Err(failure(err("PREPARATION_QUOTE_MISMATCH")));
        }
        verify(
            &decode(
                existing.preparation["unsignedTransaction"]
                    .as_str()
                    .ok_or_else(|| failure(err("SIGNED_TRANSACTION_INVALID")))?,
            )
            .map_err(failure)?,
            &raw,
            &existing.context.caller,
        )
        .map_err(failure)?;
        return Ok(Json(existing.public()));
    }
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure(err("RATE_LIMITED")))?;
    let (p, info, raw) = signed(&api, &id, &preparation, &request).map_err(failure)?;
    let chain = api
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == p.context.chain_key)
        .ok_or_else(|| failure(err("UNKNOWN_CHAIN")))?;
    preflight(&api, &p, &raw, &chain.genesis_hash)
        .await
        .map_err(failure)?;
    solana_simulation::current(&api, &p).map_err(failure)?;
    let now = now_ms();
    let receipt = Receipt {
        schema_version: 1,
        signature: info.signature.clone(),
        transaction_sha256: info.transaction_sha256,
        signed_transaction: request.signed_transaction,
        context: p.context,
        genesis_hash: chain.genesis_hash.clone(),
        quote_id: id,
        preparation: p.result,
        evidence_hash: request.evidence_hash,
        state: TransactionState::Submitting,
        created_at_ms: now,
        updated_at_ms: now,
        confirmation_status: None,
        slot: None,
        settlement: None,
        error: None,
    };
    let (mut receipt, reserved) = journal(&api)
        .map_err(failure)?
        .lock()
        .map_err(|_| failure(err("RECEIPT_LOCK")))?
        .reserve(receipt)
        .map_err(failure)?;
    if !reserved {
        return Ok(Json(receipt.public()));
    }
    // A durable SUBMITTING intent precedes this call. An uncertain result is
    // reconciled by signature; never automatically send it again.
    let response=rpc(&api,&receipt.context.chain_key,"sendTransaction",json!([STANDARD.encode(raw),{"encoding":"base64","skipPreflight":false,"preflightCommitment":"confirmed","maxRetries":0,"minContextSlot":receipt.preparation["observedSlot"]}])).await;
    receipt.updated_at_ms = now_ms();
    match response {
        Ok(value) if value.as_str() == Some(receipt.signature.as_str()) => {
            receipt.state = TransactionState::Submitted;
        }
        Ok(_) => {
            receipt.state = TransactionState::Unknown;
            receipt.error = Some(json!({"code":"TRANSACTION_RPC_SIGNATURE_MISMATCH"}));
        }
        Err(error) => {
            receipt.state = TransactionState::Unknown;
            receipt.error = Some(json!({"code":error.0}));
        }
    }
    journal(&api)
        .map_err(failure)?
        .lock()
        .map_err(|_| failure(err("RECEIPT_LOCK")))?
        .update(receipt.clone())
        .map_err(failure)?;
    Ok(Json(receipt.public()))
}
fn p_slot(prepared: &Value) -> Result<u64> {
    prepared["observedSlot"]
        .as_u64()
        .ok_or_else(|| err("RECEIPT_META_MISSING"))
}
fn amount(prepared: &Value, name: &str) -> Result<u64> {
    prepared[name]
        .as_str()
        .ok_or_else(|| err("RECEIPT_META_MISSING"))?
        .parse()
        .map_err(|_| err("RECEIPT_META_MISSING"))
}
fn token_balance(
    meta: &Value,
    phase: &str,
    index: usize,
    mint: &str,
    owner: &str,
    missing_zero: bool,
) -> Result<u64> {
    let values = meta[phase]
        .as_array()
        .ok_or_else(|| err("RECEIPT_META_MISSING"))?;
    if let Some(item) = values
        .iter()
        .find(|v| v["accountIndex"].as_u64() == Some(index as u64))
    {
        if item["mint"] != mint || item["owner"] != owner {
            return Err(err("RECEIPT_TOKEN_IDENTITY"));
        }
        item["uiTokenAmount"]["amount"]
            .as_str()
            .ok_or_else(|| err("RECEIPT_META_MISSING"))?
            .parse()
            .map_err(|_| err("RECEIPT_META_MISSING"))
    } else if missing_zero {
        Ok(0)
    } else {
        Err(err("RECEIPT_META_MISSING"))
    }
}
fn settlement(receipt: &Receipt, tx: &Value) -> Result<Value> {
    let wire = tx["transaction"][0]
        .as_str()
        .ok_or_else(|| err("RECEIPT_META_MISSING"))?;
    if tx["transaction"][1] != "base64" || decode(wire)? != decode(&receipt.signed_transaction)? {
        return Err(err("RECEIPT_WIRE_MISMATCH"));
    }
    let info = verify(
        &decode(
            receipt.preparation["unsignedTransaction"]
                .as_str()
                .ok_or_else(|| err("RECEIPT_META_MISSING"))?,
        )?,
        &decode(wire)?,
        &receipt.context.caller,
    )?;
    if tx["slot"].as_u64() != receipt.slot {
        return Err(err("RECEIPT_SLOT_MISMATCH"));
    }
    let p = &receipt.preparation;
    let c = &receipt.context;
    let meta = &tx["meta"];
    if meta.get("err") != Some(&Value::Null) {
        return Err(err("RECEIPT_ONCHAIN_FAILURE"));
    }
    let index = |key: &str| {
        info.account_keys
            .iter()
            .position(|v| v == key)
            .ok_or_else(|| err("RECEIPT_META_MISSING"))
    };
    let source = index(
        p["sourceTokenAccount"]
            .as_str()
            .ok_or_else(|| err("RECEIPT_META_MISSING"))?,
    )?;
    let destination = index(
        p["destinationTokenAccount"]
            .as_str()
            .ok_or_else(|| err("RECEIPT_META_MISSING"))?,
    )?;
    let native_before = meta["preBalances"][0]
        .as_u64()
        .ok_or_else(|| err("RECEIPT_META_MISSING"))?;
    let native_after = meta["postBalances"][0]
        .as_u64()
        .ok_or_else(|| err("RECEIPT_META_MISSING"))?;
    let fee = meta["fee"]
        .as_u64()
        .ok_or_else(|| err("RECEIPT_META_MISSING"))?;
    let persistent = amount(p, "rentLamports")?
        .checked_sub(amount(p, "rentRefundLamports")?)
        .ok_or_else(|| err("RECEIPT_AMOUNT_MISMATCH"))?;
    let paid = if p["refundWrappedSource"] == true
        || (p["sourceNativeSol"] == true && c.sell_token == xone_domain::pump::NATIVE_SOL)
    {
        u128::from(native_before)
            .checked_sub(u128::from(native_after) + u128::from(fee) + u128::from(persistent))
            .and_then(|v| u64::try_from(v).ok())
            .ok_or_else(|| err("RECEIPT_AMOUNT_MISMATCH"))?
    } else {
        let start = token_balance(
            meta,
            "preTokenBalances",
            source,
            &c.sell_token,
            &c.caller,
            p["sourceAccountCreation"] == true,
        )?;
        let end = token_balance(
            meta,
            "postTokenBalances",
            source,
            &c.sell_token,
            &c.caller,
            false,
        )?;
        start
            .checked_add(amount(p, "wrapLamports")?)
            .and_then(|v| v.checked_sub(end))
            .ok_or_else(|| err("RECEIPT_AMOUNT_MISMATCH"))?
    };
    let received = if p["unwrapDestinationSol"] == true
        || (p["destinationNativeSol"] == true && c.buy_token == xone_domain::pump::NATIVE_SOL)
    {
        (u128::from(native_after) + u128::from(fee) + u128::from(persistent))
            .checked_sub(u128::from(native_before))
            .and_then(|v| u64::try_from(v).ok())
            .ok_or_else(|| err("RECEIPT_AMOUNT_MISMATCH"))?
    } else {
        let start = token_balance(
            meta,
            "preTokenBalances",
            destination,
            &c.buy_token,
            &c.recipient,
            p["accountCreation"] == true,
        )?;
        let end = token_balance(
            meta,
            "postTokenBalances",
            destination,
            &c.buy_token,
            &c.recipient,
            false,
        )?;
        end.checked_sub(start)
            .ok_or_else(|| err("RECEIPT_AMOUNT_MISMATCH"))?
    };
    let minimum = if p.get("minBuyAmount").is_some() {
        amount(p, "minBuyAmount")?
    } else if p["quoteKind"] == "EXACT_OUT" {
        amount(p, "quotedOutput")?
    } else {
        u64::try_from(
            u128::from(amount(p, "quotedOutput")?) * u128::from(10000 - c.slippage_bps) / 10000,
        )
        .map_err(|_| err("RECEIPT_AMOUNT_MISMATCH"))?
    };
    if paid == 0
        || (p["quoteKind"] == "EXACT_IN" && paid != amount(p, "quotedInput")?)
        || paid > amount(p, "requiredInput")?
        || received < minimum
    {
        return Err(err("RECEIPT_AMOUNT_MISMATCH"));
    }
    Ok(
        json!({"verified":true,"actualInput":paid.to_string(),"actualOutput":received.to_string(),"minBuyAmount":minimum.to_string(),"maxSellAmount":p["requiredInput"],"feeLamports":fee.to_string(),"payerLamportDelta":(i128::from(native_after)-i128::from(native_before)).to_string(),"settlementAsset":p["settlementAsset"],"transactionSha256":format!("{:x}",Sha256::digest(decode(wire)?))}),
    )
}
pub(crate) async fn receipt(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(signature): Path<String>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let mut record = journal(&api)
        .map_err(failure)?
        .lock()
        .map_err(|_| failure(err("RECEIPT_LOCK")))?
        .get(&signature)
        .ok_or_else(|| failure(err("RECEIPT_NOT_FOUND")))?;
    if matches!(
        record.state,
        TransactionState::Finalized
            | TransactionState::Failed
            | TransactionState::SettlementMismatch
    ) {
        return Ok(Json(record.public()));
    }
    genesis(&api, &record.context.chain_key, &record.genesis_hash)
        .await
        .map_err(failure)?;
    let response = rpc(
        &api,
        &record.context.chain_key,
        "getSignatureStatuses",
        json!([[signature],{"searchTransactionHistory":true}]),
    )
    .await
    .map_err(failure)?;
    let values = response["value"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or_else(|| failure(err("TRANSACTION_RPC_INVALID")))?;
    let value = &values[0];
    record.updated_at_ms = now_ms();
    record.settlement = None;
    if value.is_null() {
        let height = rpc(
            &api,
            &record.context.chain_key,
            "getBlockHeight",
            json!([{"commitment":"confirmed"}]),
        )
        .await
        .map_err(failure)?
        .as_u64()
        .ok_or_else(|| failure(err("TRANSACTION_RPC_INVALID")))?;
        record.state = if height
            > record.preparation["lastValidBlockHeight"]
                .as_u64()
                .ok_or_else(|| failure(err("TRANSACTION_RPC_INVALID")))?
        {
            TransactionState::Expired
        } else {
            TransactionState::Unknown
        };
        record.error = Some(json!({"code":"SIGNATURE_NOT_OBSERVED","height":height}));
    } else {
        let status = value["confirmationStatus"]
            .as_str()
            .filter(|v| ["processed", "confirmed", "finalized"].contains(v))
            .ok_or_else(|| failure(err("TRANSACTION_RPC_INVALID")))?;
        record.confirmation_status = Some(status.into());
        record.slot = Some(
            value["slot"]
                .as_u64()
                .filter(|n| p_slot(&record.preparation).is_ok_and(|minimum| *n >= minimum))
                .ok_or_else(|| failure(err("TRANSACTION_RPC_INVALID")))?,
        );
        record.error = None;
        if value.get("err") != Some(&Value::Null) {
            record.state = if status == "finalized" {
                TransactionState::Failed
            } else {
                TransactionState::ReceiptPending
            };
            record.error = Some(json!({"code":"ONCHAIN_TRANSACTION_FAILED","detail":value["err"]}));
        } else if status == "processed" {
            record.state = TransactionState::Processed;
        } else {
            let tx=rpc(&api,&record.context.chain_key,"getTransaction",json!([signature,{"encoding":"base64","commitment":status,"maxSupportedTransactionVersion":0}])).await.map_err(failure)?;
            if tx.is_null() {
                record.state = TransactionState::ReceiptPending;
                record.error = Some(json!({"code":"RECEIPT_META_MISSING"}));
            } else {
                match settlement(&record, &tx) {
                    Ok(value) => {
                        record.settlement = Some(value);
                        record.state = if status == "finalized" {
                            TransactionState::Finalized
                        } else {
                            TransactionState::Confirmed
                        };
                    }
                    Err(error) => {
                        record.state = if error.0.ends_with("MISSING") {
                            TransactionState::ReceiptPending
                        } else {
                            TransactionState::SettlementMismatch
                        };
                        record.error = Some(json!({"code":error.0}));
                    }
                }
            }
        }
    }
    journal(&api)
        .map_err(failure)?
        .lock()
        .map_err(|_| failure(err("RECEIPT_LOCK")))?
        .update(record.clone())
        .map_err(failure)?;
    Ok(Json(record.public()))
}
