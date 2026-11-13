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
    if account_bytes > 8 * 1024 * 1024 {
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
pub(crate) async fn simulate(
    State(api): State<Api>,
    headers: HeaderMap,
    Path((id, preparation)): Path<(String, String)>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure(err("RATE_LIMITED")))?;
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
    let mut evidence = if engine.uses_rpc_capture() {
        let captured = p
            .accounts
            .as_ref()
            .ok_or_else(|| failure(err("SBF_CAPTURE_REQUIRED")))?;
        let value = serde_json::to_value(&**captured)
            .map_err(|_| failure(err("SBF_CAPTURE_BINDING_INVALID")))?;
        engine
            .simulate_captured(
                &p.pool,
                &p.context.caller,
                &p.context.recipient,
                &p.context.sell_token,
                &p.context.buy_token,
                &p.result,
                &value,
            )
            .await
            .map_err(failure)?
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
            .map_err(failure)?
    };
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
    if let Some(cached) = api
        .preparations
        .lock()
        .map_err(|_| failure(err("CACHE_LOCK")))?
        .get_mut(&preparation)
    {
        cached.evidence = Some(evidence.clone());
    }
    Ok(Json(evidence))
}
