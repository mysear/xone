//! QuoteView/cache authorization boundary for explicit EVM RPC preparation.
use super::*;
use xone_execution_plan::{bind_v2_exact_out_execution, bind_v2_execution, ExecutorIdentity};
use xone_execution_preparation::evm_rpc::{self, Bound, Request, RpcConfig};
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
    Ok(Json(result))
}
