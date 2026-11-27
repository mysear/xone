use super::{err, json, now_ms, Api, Result, Value};
use xone_domain::{hash, singleton::ZERO_ADDRESS};
use xone_execution_plan::v4_single;

const PROFILE: &str = "uniswap-v4-advanced";

pub(super) fn configured(api: &Api, pool: &xone_domain::Pool) -> bool {
    pool.protocol == PROFILE
        && api
            .config
            .config
            .api
            .v4_execution
            .get(&pool.chain_key)
            .is_some_and(|config| {
                [&pool.token0, &pool.token1].into_iter().all(|token| {
                    token == ZERO_ADDRESS || config.token_code_sha256.contains_key(token)
                })
            })
}

pub(super) async fn build(api: &Api, id: &str) -> Result<Option<Value>> {
    if api.config.config.api.v4_execution.is_empty() {
        return Ok(None);
    }
    let exact_in = api
        .cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned()
        .filter(|quote| {
            quote
                .plan
                .allocation
                .routes
                .iter()
                .any(|route| route.path.edges.iter().any(|edge| edge.protocol == PROFILE))
        });
    let exact_out = api
        .exact_out_cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned()
        .filter(|quote| {
            quote
                .plan
                .allocation
                .routes
                .iter()
                .any(|route| route.edges.iter().any(|edge| edge.protocol == PROFILE))
        });
    let get_config = |chain: &str| api.config.config.api.v4_execution.get(chain);
    let (view, context, bound, configuration, plan_hash, kind, expires, quote) =
        if let Some(quote) = &exact_in {
            let Some(config) = get_config(&quote.plan.context.chain_key) else {
                return Ok(None);
            };
            let bound = v4_single::bind_exact_in(
                &quote.plan,
                &quote.view.pools,
                &config.executor,
                &config.executor_code_sha256,
            )?;
            (
                &quote.view,
                &quote.plan.context,
                bound,
                config,
                &quote.plan.plan_hash,
                "EXACT_IN",
                quote.expires,
                &quote.response,
            )
        } else if let Some(quote) = &exact_out {
            let Some(config) = get_config(&quote.plan.context.chain_key) else {
                return Ok(None);
            };
            let bound = v4_single::bind_exact_out(
                &quote.plan,
                &quote.view.pools,
                &config.executor,
                &config.executor_code_sha256,
            )?;
            (
                &quote.view,
                &quote.plan.context,
                bound,
                config,
                &quote.plan.plan_hash,
                "EXACT_OUT",
                quote.expires,
                &quote.response,
            )
        } else {
            return Ok(None);
        };
    let check = || -> Result<()> {
        if expires <= now_ms() || context.deadline_ms / 1000 <= now_ms() / 1000 {
            return Err(err("QUOTE_EXPIRED"));
        }
        api.check_view(view)?;
        api.check_quote_prices(quote)?;
        if api.replica.view(&context.chain_key)?.view_id != view.view_id {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
        Ok(())
    };
    check()?;
    let chain = api
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == context.chain_key)
        .ok_or_else(|| err("V4_EXECUTION_CONFIG"))?;
    let [url] = chain.rpc_urls.as_slice() else {
        return Err(err("V4_EXECUTION_CONFIG"));
    };
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| err("RATE_LIMITED"))?;
    xone_execution_preparation::evm_rpc::verify_v4_executor(
        url,
        &chain.genesis_hash,
        &view.reference,
        context,
        &bound,
        &configuration.token_code_sha256,
    )
    .await?;
    check()?;
    let approval = (context.sell_token != ZERO_ADDRESS).then(|| {
        super::evm_approval::Requirement {
            token: context.sell_token.clone(),
            owner: context.caller.clone(),
            spender: bound.transaction.to.clone(),
            input: bound.input_limit.clone(),
        }
        .disclose(None, &bound.binding_hash, "QUOTE_PLAN")
    });
    let mut result = json!({
        "quoteId": id, "quoteViewId": view.view_id, "quoteKind": kind, "planHash": plan_hash,
        "snapshotRef": view.reference, "binding": bound, "transaction": bound.transaction,
        "callerApproval": approval, "independentExecutionVerified": false, "productionReady": false,
    });
    result["buildHash"] = hash(&result)?.into();
    Ok(Some(result))
}
