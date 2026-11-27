//! Quote/preparation commitment around the isolated local SBF worker.
use super::*;
#[derive(Clone)]
pub(crate) struct Prepared {
    pub view: Arc<QuoteView>,
    pub context: Context,
    pub pool: xone_domain::Pool,
    pub result: Value,
    pub evidence: Option<Value>,
    pub accounts: Option<Arc<xone_execution_preparation::AccountSnapshot>>,
    account_bytes: usize,
}
pub(crate) fn store(
    api: &Api,
    view: Arc<QuoteView>,
    context: Context,
    pool: xone_domain::Pool,
    result: &Value,
    accounts: Option<xone_execution_preparation::AccountSnapshot>,
) -> Result<()> {
    if result["ready"] != true {
        return Ok(());
    }
    let key = result["preparationHash"]
        .as_str()
        .ok_or_else(|| err("SBF_PREPARATION_INVALID"))?
        .to_owned();
    let mut cache = api.preparations.lock().map_err(|_| err("CACHE_LOCK"))?;
    cache.retain(|_, p| p.context.deadline_ms > now_ms() && p.view.is_valid());
    let account_bytes = accounts
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|_| err("PREPARATION_CAPTURE_INVALID"))?
        .map_or(0, |v| v.len());
    if account_bytes > 16 * 1024 * 1024 {
        return Err(err("PREPARATION_CAPTURE_LIMIT"));
    }
    let capacity = (api.config.config.api.requests_per_minute as usize).clamp(16, 256);
    while cache.len() >= capacity
        || cache
            .iter()
            .filter(|(id, _)| *id != &key)
            .map(|(_, p)| p.account_bytes)
            .sum::<usize>()
            + account_bytes
            > 32 * 1024 * 1024
    {
        let old = cache.keys().find(|id| *id != &key).cloned();
        if let Some(old) = old {
            cache.remove(&old);
        } else {
            break;
        }
    }
    let previous = cache.get(&key).and_then(|p| p.evidence.clone());
    cache.insert(
        key,
        Prepared {
            view,
            context,
            pool,
            result: result.clone(),
            evidence: previous,
            accounts: accounts.map(Arc::new),
            account_bytes,
        },
    );
    Ok(())
}
pub(crate) fn selected(api: &Api, id: &str, preparation: &str) -> Result<Prepared> {
    let p = api
        .preparations
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(preparation)
        .cloned()
        .ok_or_else(|| err("PREPARATION_NOT_FOUND"))?;
    if p.result["quoteId"] != id {
        return Err(err("PREPARATION_QUOTE_MISMATCH"));
    }
    current(api, &p)?;
    Ok(p)
}
pub(crate) fn current(api: &Api, p: &Prepared) -> Result<()> {
    api.check_view(&p.view)?;
    if now_ms() >= p.context.deadline_ms {
        return Err(err("QUOTE_EXPIRED"));
    }
    if api.replica.view(&p.context.chain_key)?.view_id != p.view.view_id {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VersionedBankProof {
    version: u32,
    snapshot_ref: xone_domain::SnapshotRef,
    source: Value,
    accounts: BTreeMap<String, BankAccountVersion>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BankAccountVersion {
    write_slot: u64,
    write_bank_id: u64,
    write_version: u64,
    account_sha256: String,
}

fn verify_bank_proof(
    p: &Prepared,
    capture: &xone_execution_preparation::AccountSnapshot,
    source: &Value,
    proof: VersionedBankProof,
) -> Result<Value> {
    let bad = || err("SBF_VERSIONED_BANK_MISMATCH");
    let bank_id = source["bank_id"].as_u64().ok_or_else(bad)?;
    let block = &p.view.reference.block;
    if proof.version != 1
        || proof.snapshot_ref != p.view.reference
        || proof.source != *source
        || capture.observed_slot != block.number_or_slot
        || !proof.accounts.contains_key(&p.pool.pool_id)
    {
        return Err(bad());
    }
    for (key, version) in &proof.accounts {
        let account = capture.accounts.get(key).ok_or_else(bad)?;
        let data = account["data"]
            .as_array()
            .filter(|data| data.len() == 2 && data[1] == "base64")
            .and_then(|data| data[0].as_str())
            .ok_or_else(bad)?;
        let digest = hash(&(
            account["owner"].as_str().ok_or_else(bad)?,
            account["lamports"].as_u64().ok_or_else(bad)?,
            account["executable"].as_bool().ok_or_else(bad)?,
            account["rentEpoch"].as_u64().ok_or_else(bad)?,
            data,
        ))?;
        if version.write_slot > block.number_or_slot
            || (version.write_slot == block.number_or_slot && version.write_bank_id != bank_id)
            || version.account_sha256 != digest
        {
            return Err(bad());
        }
    }
    Ok(
        json!({"version":proof.version,"snapshotRef":proof.snapshot_ref,
        "source":proof.source,"accounts":proof.accounts}),
    )
}

async fn versioned_bank(api: &Api, p: &Prepared) -> Result<Option<Value>> {
    let Ok(source) = serde_json::from_str::<Value>(&p.view.source) else {
        return Ok(None);
    };
    if source["kind"] != "YELLOWSTONE_INDEPENDENT_VERSIONED_BANK" {
        return Ok(None);
    }
    let capture = p
        .accounts
        .as_ref()
        .ok_or_else(|| err("SBF_CAPTURE_REQUIRED"))?;
    let block = &p.view.reference.block;
    let keys = capture
        .accounts
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",");
    let mut failure = err("SBF_VERSIONED_BANK_UNAVAILABLE");
    for base in &api.config.config.journal.source_urls {
        let Ok(mut url) = reqwest::Url::parse(&format!(
            "{}/internal/chains/{}/execution-bank",
            base.trim_end_matches('/'),
            p.context.chain_key,
        )) else {
            continue;
        };
        url.query_pairs_mut()
            .append_pair("slot", &block.number_or_slot.to_string())
            .append_pair("blockhash", &block.hash)
            .append_pair("keys", &keys);
        let Ok(response) = api.fetch(url.as_str()).await else {
            continue;
        };
        let proof = serde_json::from_value(response)
            .map_err(|_| err("SBF_VERSIONED_BANK_MISMATCH"))
            .and_then(|proof| verify_bank_proof(p, capture, &source, proof));
        match proof {
            Ok(proof) => return Ok(Some(proof)),
            Err(error) => failure = error,
        }
    }
    Err(failure)
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SimulationRequest {
    signed_transaction: Option<String>,
}

pub(crate) async fn simulate(
    State(api): State<Api>,
    headers: HeaderMap,
    Path((id, preparation)): Path<(String, String)>,
    body: Bytes,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure(err("RATE_LIMITED")))?;
    let request = if body.is_empty() {
        SimulationRequest::default()
    } else {
        serde_json::from_slice::<Option<SimulationRequest>>(&body)
            .map_err(|_| failure(err("INVALID_SIMULATION_REQUEST")))?
            .unwrap_or_default()
    };
    let p = selected(&api, &id, &preparation).map_err(failure)?;
    let engine = api
        .solana_sbf
        .as_ref()
        .ok_or_else(|| failure(err("SBF_RUNTIME_NOT_CONFIGURED")))?;
    let chain = api
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == p.context.chain_key)
        .ok_or_else(|| failure(err("UNKNOWN_CHAIN")))?;
    if !engine.available(&p.pool, &chain.genesis_hash) {
        return Err(failure(err("SBF_BANK_NOT_COVERED")));
    }
    let capture = if engine.uses_rpc_capture() {
        let captured = p
            .accounts
            .as_ref()
            .ok_or_else(|| failure(err("SBF_CAPTURE_REQUIRED")))?;
        Some(
            serde_json::to_value(&**captured)
                .map_err(|_| failure(err("SBF_CAPTURE_BINDING_INVALID")))?,
        )
    } else {
        None
    };
    let bank_proof = versioned_bank(&api, &p).await.map_err(failure)?;
    if bank_proof.is_some() && capture.is_none() {
        return Err(failure(err("SBF_CAPTURE_REQUIRED")));
    }
    let mut evidence = if let Some(signed) = &request.signed_transaction {
        engine
            .simulate_signed(
                &p.pool,
                &p.context.caller,
                &p.context.recipient,
                &p.context.sell_token,
                &p.context.buy_token,
                &p.result,
                capture.as_ref(),
                signed,
            )
            .await
    } else if let Some(value) = &capture {
        engine
            .simulate_captured(
                &p.pool,
                &p.context.caller,
                &p.context.recipient,
                &p.context.sell_token,
                &p.context.buy_token,
                &p.result,
                value,
            )
            .await
    } else {
        engine
            .simulate(
                &p.pool,
                &p.context.caller,
                &p.context.recipient,
                &p.context.sell_token,
                &p.context.buy_token,
                &p.result,
            )
            .await
    }
    .map_err(failure)?;
    if let Some(proof) = bank_proof {
        evidence["versionedBank"] = proof;
    }
    if request.signed_transaction.is_some() {
        transaction_lifecycle::genesis(&api, &p.context.chain_key, &chain.genesis_hash)
            .await
            .map_err(failure)?;
        let observation = transaction_lifecycle::observe_blockhash(&api, &p)
            .await
            .map_err(failure)?;
        evidence["blockhashValidity"] = json!(observation);
    }
    let cost_source = if engine.uses_rpc_capture() {
        native_economics::Source::CapturedSbf
    } else {
        native_economics::Source::LocalSbf
    };
    let costs = native_economics::evaluated(&p.context, &p.result, &evidence, cost_source)
        .map_err(failure)?;
    evidence["costs"] = costs;
    current(&api, &p).map_err(failure)?;
    evidence["quoteId"] = id.into();
    evidence["protocolViewIds"] = json!(p
        .view
        .protocol_bindings([p.pool.pool_id.as_str()])
        .map_err(failure)?);
    evidence
        .as_object_mut()
        .ok_or_else(|| failure(err("SBF_WORKER_RESPONSE")))?
        .remove("evidenceHash");
    evidence["evidenceHash"] = hash(&evidence).map_err(failure)?.into();
    // Signed simulation evidence is returned to the requester. The unsigned
    // evidence remains the preparation commitment used by wallet submission.
    if request.signed_transaction.is_none() {
        if let Some(cached) = api
            .preparations
            .lock()
            .map_err(|_| failure(err("CACHE_LOCK")))?
            .get_mut(&preparation)
        {
            cached.evidence = Some(evidence.clone());
        }
    }
    Ok(Json(evidence))
}
