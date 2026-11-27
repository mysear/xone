use super::{err, json, now_ms, Api, Result, Value};
use xone_domain::{hash, singleton::ZERO_ADDRESS};
use xone_execution_plan::v4_single::{self, Executor, INFINITY_BIN, INFINITY_CL};

const PROFILE: &str = "uniswap-v4-advanced";

pub(super) fn configuration<'a>(
    api: &'a xone_config::Api,
    pool: &xone_domain::Pool,
) -> Option<(Executor<'a>, &'a std::collections::BTreeMap<String, String>)> {
    let (config, vault) = match pool.protocol.as_str() {
        PROFILE => (api.v4_execution.get(&pool.chain_key)?, None),
        INFINITY_BIN | INFINITY_CL => {
            let manager = pool.infinity_bin.as_ref().map(|s| &s.manager).or_else(|| {
                pool.singleton_advanced
                    .as_ref()
                    .map(|s| &s.snapshot.manager)
            })?;
            let config = api.infinity_execution.get(&pool.chain_key)?.get(manager)?;
            if pool.infinity_bin.as_ref().is_some_and(|s| {
                s.vault != config.vault || s.vault_code_sha256 != config.vault_code_sha256
            }) {
                return None;
            }
            (
                &config.transaction,
                Some((config.vault.as_str(), config.vault_code_sha256.as_str())),
            )
        }
        _ => return None,
    };
    [&pool.token0, &pool.token1]
        .into_iter()
        .all(|token| {
            if token == ZERO_ADDRESS {
                return true;
            }
            let Some(digest) = config.token_code_sha256.get(token) else {
                return false;
            };
            pool.infinity_bin.as_ref().is_none_or(|state| {
                digest
                    == if token == &pool.token0 {
                        &state.token0_code_sha256
                    } else {
                        &state.token1_code_sha256
                    }
            })
        })
        .then_some((
            Executor {
                address: &config.executor,
                code_sha256: &config.executor_code_sha256,
                infinity_vault: vault,
            },
            &config.token_code_sha256,
        ))
}

pub(super) fn configured(api: &Api, pool: &xone_domain::Pool) -> bool {
    configuration(&api.config.config.api, pool).is_some()
}

pub(super) async fn build(api: &Api, id: &str) -> Result<Option<Value>> {
    if api.config.config.api.v4_execution.is_empty()
        && api.config.config.api.infinity_execution.is_empty()
    {
        return Ok(None);
    }
    let exact_in = api
        .cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned();
    let exact_out = api
        .exact_out_cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned();
    let (view, context, bound, token_digests, plan_hash, kind, expires, quote) =
        if let Some(quote) = &exact_in {
            let [route] = quote.plan.allocation.routes.as_slice() else {
                return Ok(None);
            };
            let [edge] = route.path.edges.as_slice() else {
                return Ok(None);
            };
            let Some((executor, token_digests)) = quote
                .view
                .pools
                .iter()
                .find(|pool| pool.pool_id == edge.pool_id)
                .and_then(|pool| configuration(&api.config.config.api, pool))
            else {
                return Ok(None);
            };
            let bound = v4_single::bind_exact_in(&quote.plan, &quote.view.pools, executor)?;
            (
                &quote.view,
                &quote.plan.context,
                bound,
                token_digests,
                &quote.plan.plan_hash,
                "EXACT_IN",
                quote.expires,
                &quote.response,
            )
        } else if let Some(quote) = &exact_out {
            let [route] = quote.plan.allocation.routes.as_slice() else {
                return Ok(None);
            };
            let [edge] = route.edges.as_slice() else {
                return Ok(None);
            };
            let Some((executor, token_digests)) = quote
                .view
                .pools
                .iter()
                .find(|pool| pool.pool_id == edge.pool_id)
                .and_then(|pool| configuration(&api.config.config.api, pool))
            else {
                return Ok(None);
            };
            let bound = v4_single::bind_exact_out(&quote.plan, &quote.view.pools, executor)?;
            (
                &quote.view,
                &quote.plan.context,
                bound,
                token_digests,
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
    xone_execution_preparation::evm_rpc::verify_singleton_executor(
        url,
        &chain.genesis_hash,
        &view.reference,
        context,
        &bound,
        token_digests,
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
