//! QuoteView/cache authorization boundary for explicit EVM RPC preparation.
use super::*;
use xone_execution_plan::{bind_v2_exact_out_execution, bind_v2_execution, ExecutorIdentity};
use xone_execution_preparation::evm_rpc::{self, Bound, Request, RpcConfig};
#[derive(Clone)]
pub(super) struct Prepared {
    pub hash: String,
    pub rpc_hash: String,
    pub input: xone_simulation::worker::WorkerPreparation,
    _budget: Arc<tokio::sync::OwnedSemaphorePermit>,
}
impl Prepared {
    pub(super) fn retain(
        api: &Api,
        hash: String,
        rpc_hash: String,
        input: xone_simulation::worker::WorkerPreparation,
    ) -> Result<Self> {
        let bytes = serde_json::to_vec(&input)
            .map_err(|_| err("HASH_ENCODING"))?
            .len();
        if bytes > 64 * 1024 {
            return Err(err("EVM_PREPARATION_CACHE_LIMIT"));
        }
        let permit = api
            .evm_preparation_budget
            .clone()
            .try_acquire_many_owned(bytes as u32)
            .map_err(|_| err("EVM_PREPARATION_CACHE_LIMIT"))?;
        Ok(Self {
            hash,
            rpc_hash,
            input,
            _budget: Arc::new(permit),
        })
    }
}
fn current(api: &Api, view: &QuoteView, context: &Context) -> Result<()> {
    api.check_view(view)?;
    if now_ms() >= context.deadline_ms {
        return Err(err("QUOTE_EXPIRED"));
    }
    if api.replica.view(&context.chain_key)?.view_id != view.view_id {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    Ok(())
}
fn config(api: &Api, chain: &str) -> Result<RpcConfig> {
    let declared = api
        .config
        .config
        .api
        .evm_preparation
        .get(chain)
        .ok_or_else(|| err("EVM_PREPARATION_NOT_CONFIGURED"))?;
    let chain = api
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == chain)
        .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
    Ok(RpcConfig {
        url: chain
            .rpc_urls
            .first()
            .ok_or_else(|| err("EVM_PREPARATION_NOT_CONFIGURED"))?
            .clone(),
        chain_key: chain.key.clone(),
        genesis_hash: chain.genesis_hash.clone(),
        executor: ExecutorIdentity {
            target: declared.router.clone(),
            code_sha256: declared.router_code_sha256.clone(),
            factory: declared.factory.clone(),
            factory_code_sha256: declared.factory_code_sha256.clone(),
            adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
        },
        token_code_sha256: declared.token_code_sha256.clone(),
        timeout_ms: declared.timeout_ms,
        max_calls: declared.max_calls,
        max_response_bytes: declared.max_response_bytes,
    })
}
pub(super) async fn prepare(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: Bytes,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let request: Request = serde_json::from_slice(&body)
        .map_err(|_| failure(err("EVM_PREPARATION_REQUEST_INVALID")))?;
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure(err("RATE_LIMITED")))?;
    let exact_in = api
        .cache
        .lock()
        .map_err(|_| failure(err("CACHE_LOCK")))?
        .get(&id)
        .cloned();
    let (view, context, capture, kind) = if let Some(cached) = exact_in {
        current(&api, &cached.view, &cached.plan.context).map_err(failure)?;
        let cfg = config(&api, &cached.plan.context.chain_key).map_err(failure)?;
        let bound = bind_v2_execution(
            &cached.plan,
            &cached.view.pools,
            &cfg.executor,
            std::slice::from_ref(&cfg.executor.target),
        )
        .map_err(failure)?;
        let capture = evm_rpc::prepare(&cfg, Bound::ExactIn(&bound), &cached.view.pools, request)
            .await
            .map_err(failure)?;
        (cached.view, cached.plan.context, capture, "EXACT_IN")
    } else {
        let cached = api
            .exact_out_cache
            .lock()
            .map_err(|_| failure(err("CACHE_LOCK")))?
            .get(&id)
            .cloned()
            .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?;
        current(&api, &cached.view, &cached.plan.context).map_err(failure)?;
        let cfg = config(&api, &cached.plan.context.chain_key).map_err(failure)?;
        let bound = bind_v2_exact_out_execution(
            &cached.plan,
            &cached.view.pools,
            &cfg.executor,
            std::slice::from_ref(&cfg.executor.target),
        )
        .map_err(failure)?;
        let capture = evm_rpc::prepare(&cfg, Bound::ExactOut(&bound), &cached.view.pools, request)
            .await
            .map_err(failure)?;
        (cached.view, cached.plan.context, capture, "EXACT_OUT")
    };
    current(&api, &view, &context).map_err(failure)?;
    let component = if kind == "EXACT_OUT" {
        &capture.result["component"]["preparation"]
    } else {
        &capture.result["component"]
    };
    let mut result = json!({"schemaVersion":1,"quoteId":id,"quoteKind":kind,"quoteViewId":view.view_id,
        "snapshotRef":view.reference,"ready":component["ready"],"blockers":component["blockers"],
        "unsignedTransaction":component["unsigned_transaction"],"transaction":component["rpc_transaction"],
        "requiredInput":component["required_input"],"maxNetworkFee":component["max_network_fee"],
        "requiredSigners":component["required_signers"],"component":capture.result["component"],"rpcPreparation":capture.result,
        "rpcObservations":capture.observations,"signed":false,"submitted":false,
        "independentExecutionVerified":false,"chainFeesComplete":false,"productionReady":false});
    result["preparationHash"] = hash(&result).map_err(failure)?.into();
    current(&api, &view, &context).map_err(failure)?;
    let stored = if result["ready"] == true {
        let input = xone_simulation::worker::WorkerPreparation {
            funding: capture.funding,
            fees: capture.fees,
            component: capture.result["component"].clone(),
        };
        Some(
            Prepared::retain(
                &api,
                result["preparationHash"].as_str().unwrap().into(),
                capture.result["rpc_preparation_hash"]
                    .as_str()
                    .unwrap()
                    .into(),
                input,
            )
            .map_err(failure)?,
        )
    } else {
        None
    };
    if kind == "EXACT_OUT" {
        let mut cache = api
            .exact_out_cache
            .lock()
            .map_err(|_| failure(err("CACHE_LOCK")))?;
        let cached = cache
            .get_mut(&id)
            .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?;
        current(&api, &cached.view, &cached.plan.context).map_err(failure)?;
        let old = std::mem::replace(&mut cached.evm_preparation, stored);
        cached.simulation_evidence = None;
        drop(cache);
        drop(old);
    } else {
        let mut cache = api.cache.lock().map_err(|_| failure(err("CACHE_LOCK")))?;
        let cached = cache
            .get_mut(&id)
            .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?;
        current(&api, &cached.view, &cached.plan.context).map_err(failure)?;
        let old = std::mem::replace(&mut cached.evm_preparation, stored);
        cached.simulation_evidence = None;
        drop(cache);
        drop(old);
    }
    Ok(Json(result))
}

/// Explicitly execute the cached RPC-prepared transaction in the configured local bank.
pub(super) async fn simulate(
    State(api): State<Api>,
    headers: HeaderMap,
    Path((id, preparation_hash)): Path<(String, String)>,
    request: axum::extract::Request,
) -> std::result::Result<Json<Value>, HttpError> {
    let started = Instant::now();
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let deadline =
        started + Duration::from_millis(api.config.config.simulation.timeout_ms.min(60000));
    let body = tokio::time::timeout_at(
        deadline.into(),
        axum::body::to_bytes(request.into_body(), api.config.config.api.max_body_bytes),
    )
    .await
    .map_err(|_| failure(err("SIMULATION_TIMEOUT")))?
    .map_err(|_| failure(err("INVALID_SIMULATION_REQUEST")))?;
    if !body.is_empty() {
        let value: Value = serde_json::from_slice(&body)
            .map_err(|_| failure(err("INVALID_SIMULATION_REQUEST")))?;
        if !value.is_object() {
            return Err(failure(err("INVALID_SIMULATION_REQUEST")));
        }
        serde_json::from_value::<super::SimulationRequest>(value)
            .map_err(|_| failure(err("INVALID_SIMULATION_REQUEST")))?;
    }
    let backend = api
        .local_simulation
        .clone()
        .ok_or_else(|| failure(err("INDEPENDENT_SIMULATION_UNAVAILABLE")))?;
    let cached = if api.exact_out_execution(&id).map_err(failure)? {
        local_simulation::Selection::ExactOut(
            api.exact_out_cache
                .lock()
                .map_err(|_| failure(err("CACHE_LOCK")))?
                .get(&id)
                .cloned()
                .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?,
        )
    } else {
        local_simulation::Selection::ExactIn(
            api.cache
                .lock()
                .map_err(|_| failure(err("CACHE_LOCK")))?
                .get(&id)
                .cloned()
                .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?,
        )
    };
    if cached.expires() <= now_ms() {
        return Err(failure(err("QUOTE_EXPIRED")));
    }
    current(&api, cached.view(), cached.context()).map_err(failure)?;
    let prepared = cached
        .prepared()
        .filter(|p| p.hash == preparation_hash)
        .cloned()
        .ok_or_else(|| failure(err("EVM_PREPARATION_UNKNOWN_OR_REPLACED")))?;
    api.metrics
        .simulation_started
        .fetch_add(1, Ordering::Relaxed);
    let mut response = match backend
        .execute_prepared(
            api.config.clone(),
            cached.clone(),
            deadline,
            prepared.input.clone(),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => {
            if error.code == "SIMULATION_BUSY" {
                api.metrics.simulation_busy.fetch_add(1, Ordering::Relaxed);
            } else {
                api.metrics
                    .simulation_failed
                    .fetch_add(1, Ordering::Relaxed);
            }
            return Err(simulation_failure(error));
        }
    };
    if Instant::now() >= deadline {
        return Err(failure(err("SIMULATION_TIMEOUT")));
    }
    current(&api, cached.view(), cached.context()).map_err(failure)?;
    response["apiPreparationHash"] = prepared.hash.into();
    response["rpcPreparationHash"] = prepared.rpc_hash.into();
    match cached {
        local_simulation::Selection::ExactIn(c) => publish_simulation(&api, &id, &c, response),
        local_simulation::Selection::ExactOut(c) => {
            publish_exact_out_simulation(&api, &id, &c, response)
        }
    }
}
