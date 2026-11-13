//! HTTP/cache boundary. Dynamic account resolution lives in execution-preparation.
use super::*;
use num_traits::ToPrimitive;
use xone_domain::{meteora as m, Pool};
use xone_execution_preparation::{SolanaQuote, SolanaRpcConfig};
pub(super) type Request = xone_execution_preparation::SolanaRequest;
#[derive(Clone)]
struct Selection {
    view: Arc<QuoteView>,
    context: Context,
    pool: Pool,
    plan_hash: String,
    zero: bool,
    exact: bool,
    amount: u64,
    limit: u64,
    expected_input: u64,
    expected_output: u64,
}
fn bad() -> xone_domain::Error {
    err("PREPARATION_ACCOUNT_INVALID")
}
fn select(api: &Api, id: &str) -> Result<Selection> {
    if let Some(c) = api
        .cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned()
    {
        xone_execution_plan::verify_plan_hash(&c.plan)?;
        if c.expires <= now_ms() {
            return Err(err("QUOTE_EXPIRED"));
        }
        if c.plan.allocation.routes.len() != 1 {
            return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
        }
        let route = &c.plan.allocation.routes[0];
        if route.path.edges.len() != 1 {
            return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
        }
        let mut selection = selected(
            c.view,
            c.plan.context,
            c.plan.plan_hash,
            &route.path.edges[0],
            false,
            &route.amount_in,
            &c.plan.min_buy_amount,
        )?;
        selection.expected_input = route.amount_in.0.to_u64().ok_or_else(bad)?;
        selection.expected_output = route.amount_out.0.to_u64().ok_or_else(bad)?;
        return Ok(selection);
    }
    let c = api
        .exact_out_cache
        .lock()
        .map_err(|_| err("CACHE_LOCK"))?
        .get(id)
        .cloned()
        .ok_or_else(|| err("UNKNOWN_QUOTE"))?;
    xone_execution_plan::exact_out::verify(&c.plan)?;
    if c.expires <= now_ms() {
        return Err(err("QUOTE_EXPIRED"));
    }
    if c.plan.allocation.routes.len() != 1 {
        return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
    }
    let route = &c.plan.allocation.routes[0];
    if route.edges.len() != 1 {
        return Err(err("PREPARATION_SINGLE_POOL_ONLY"));
    }
    let mut selection = selected(
        c.view,
        c.plan.context,
        c.plan.plan_hash,
        &route.edges[0],
        true,
        route.output_target(),
        &c.plan.max_sell_amount,
    )?;
    selection.expected_input = route.amount_in.0.to_u64().ok_or_else(bad)?;
    selection.expected_output = route.amount_out.0.to_u64().ok_or_else(bad)?;
    Ok(selection)
}
fn selected(
    view: Arc<QuoteView>,
    context: Context,
    plan_hash: String,
    edge: &xone_routing::Edge,
    exact: bool,
    amount: &Amount,
    limit: &Amount,
) -> Result<Selection> {
    let pool = view
        .pools
        .iter()
        .find(|p| p.pool_id == edge.pool_id)
        .cloned()
        .ok_or_else(bad)?;
    if !(m::supports_profile(&pool.protocol) || pool.protocol == xone_domain::raydium::PROFILE)
        || edge.from_token.is_some()
    {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    pool.validate()?;
    Ok(Selection {
        view,
        context,
        pool,
        plan_hash,
        zero: edge.zero_for_one,
        exact,
        amount: amount.0.to_u64().ok_or_else(bad)?,
        limit: limit.0.to_u64().ok_or_else(bad)?,
        expected_input: 0,
        expected_output: 0,
    })
}
fn current(api: &Api, s: &Selection) -> Result<()> {
    api.check_view(&s.view)?;
    if now_ms() >= s.context.deadline_ms {
        return Err(err("QUOTE_EXPIRED"));
    }
    if api.replica.view(&s.context.chain_key)?.view_id != s.view.view_id {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    Ok(())
}
pub(super) async fn prepare(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<Request>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure(err("RATE_LIMITED")))?;
    let s = select(&api, &id).map_err(failure)?;
    current(&api, &s).map_err(failure)?;
    let deadline = Duration::from_millis(api.config.config.api.quote_timeout_ms.clamp(1000, 15000));
    let result = tokio::time::timeout(deadline, prepare_inner(&api, &s, request))
        .await
        .map_err(|_| failure(err("PREPARATION_RPC_TIMEOUT")))?
        .map_err(failure)?;
    current(&api, &s).map_err(failure)?;
    let mut result = result;
    result["quoteId"] = id.into();
    result["quoteKind"] = if s.exact { "EXACT_OUT" } else { "EXACT_IN" }.into();
    result
        .as_object_mut()
        .ok_or_else(|| failure(bad()))?
        .remove("preparationHash");
    result["preparationHash"] = hash(&result).map_err(failure)?.into();
    solana_simulation::store(
        &api,
        s.view.clone(),
        s.context.clone(),
        s.pool.clone(),
        &result,
    )
    .map_err(failure)?;
    Ok(Json(result))
}
async fn prepare_inner(api: &Api, s: &Selection, request: Request) -> Result<Value> {
    let chain = api
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == s.context.chain_key)
        .ok_or_else(bad)?;
    let config = SolanaRpcConfig {
        url: chain
            .rpc_urls
            .first()
            .ok_or_else(|| err("PREPARATION_RPC_NOT_CONFIGURED"))?
            .clone(),
        genesis_hash: chain.genesis_hash.clone(),
    };
    let quote = SolanaQuote {
        context: s.context.clone(),
        pool: s.pool.clone(),
        plan_hash: s.plan_hash.clone(),
        quote_view_id: s.view.view_id.clone(),
        snapshot: s.view.reference.clone(),
        zero: s.zero,
        exact: s.exact,
        amount: s.amount,
        limit: s.limit,
        expected_input: s.expected_input,
        expected_output: s.expected_output,
    };
    xone_execution_preparation::prepare_solana(&config, &quote, request).await
}
