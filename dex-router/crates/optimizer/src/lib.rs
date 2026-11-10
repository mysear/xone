use num_bigint::BigUint;
use num_traits::Zero;
pub mod exact_out;
mod search;
pub use search::{optimize_with_limits, OptimizationLimits, OptimizationResult, MAX_SPLITS};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::atomic::AtomicBool, time::Instant};
use xone_domain::{err, Amount, Pool, Result};
use xone_routing::{evaluate_path, evaluate_path_with_funds, Funds, PathQuote};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AllocatedRoute {
    pub path: PathQuote,
    pub amount_in: Amount,
    pub amount_out: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Allocation {
    pub sell_amount: Amount,
    pub gross_buy_amount: Amount,
    pub net_buy_amount: Amount,
    pub refund: Amount,
    pub routes: Vec<AllocatedRoute>,
}
pub fn evaluate_allocation(
    pools: &[Pool],
    routes: &[AllocatedRoute],
    activation_cost: &Amount,
) -> Result<Allocation> {
    evaluate_allocation_context(pools, routes, activation_cost, None)
}
pub fn evaluate_allocation_with_funds(
    pools: &[Pool],
    routes: &[AllocatedRoute],
    activation_cost: &Amount,
    funds: Funds<'_>,
) -> Result<Allocation> {
    evaluate_allocation_context(pools, routes, activation_cost, Some(funds))
}
fn evaluate_allocation_context(
    pools: &[Pool],
    routes: &[AllocatedRoute],
    activation_cost: &Amount,
    funds: Option<Funds<'_>>,
) -> Result<Allocation> {
    xone_domain::validate_pool_dependencies(pools.iter())?;
    if routes.is_empty() {
        return Err(err("NO_ROUTE"));
    }
    let mut state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut out = BigUint::zero();
    let mut total = BigUint::zero();
    let mut recomputed = Vec::new();
    if state.len() != pools.len() {
        return Err(err("DUPLICATE_PHYSICAL_POOL"));
    }
    let sell = routes[0]
        .path
        .tokens
        .first()
        .ok_or_else(|| err("INVALID_PATH"))?;
    let buy = routes[0]
        .path
        .tokens
        .last()
        .ok_or_else(|| err("INVALID_PATH"))?;
    for route in routes {
        if route.amount_in.0.is_zero()
            || route.path.edges.is_empty()
            || route.path.tokens.len() != route.path.edges.len() + 1
        {
            return Err(err("INVALID_ALLOCATION_PATH"));
        }
        for (i, edge) in route.path.edges.iter().enumerate() {
            let pool = state
                .get(&edge.pool_id)
                .ok_or_else(|| err("MISSING_POOL"))?;
            let from = edge.from_token.as_deref().unwrap_or(if edge.zero_for_one {
                &pool.token0
            } else {
                &pool.token1
            });
            if route.path.tokens[i] != from || route.path.tokens[i + 1] != edge.to {
                return Err(err("ALLOCATION_PATH_TOKEN_MISMATCH"));
            }
        }
        if route.path.tokens.first() != Some(sell) || route.path.tokens.last() != Some(buy) {
            return Err(err("ALLOCATION_TOKEN_MISMATCH"));
        }
        let result = if let Some(funds) = funds {
            evaluate_path_with_funds(&mut state, &route.path.edges, &route.amount_in, funds)?
        } else {
            evaluate_path(&mut state, &route.path.edges, &route.amount_in)?
        };
        out += &result.0;
        total += &route.amount_in.0;
        let mut path = route.path.clone();
        path.amount_out = result.clone();
        path.amount_in = route.amount_in.clone();
        recomputed.push(AllocatedRoute {
            path,
            amount_in: route.amount_in.clone(),
            amount_out: result,
        });
    }
    let cost = &activation_cost.0 * BigUint::from(routes.len());
    let net = if out > cost {
        &out - cost
    } else {
        BigUint::zero()
    };
    Ok(Allocation {
        sell_amount: Amount::checked(total)?,
        gross_buy_amount: Amount::checked(out)?,
        net_buy_amount: Amount::checked(net)?,
        refund: Amount::zero(),
        routes: recomputed,
    })
}
/// Replays each proposal against shared physical state in execution order.
/// Exact grid enumeration is used when it fits the work budget; larger scopes
/// retain a bounded indicative result and expose exhaustion through the report.
pub fn optimize(
    pools: &[Pool],
    paths: &[PathQuote],
    input: &Amount,
    parts: usize,
    max_splits: usize,
    cost: &Amount,
) -> Result<Allocation> {
    optimize_bounded(pools, paths, input, parts, max_splits, cost, None)
}
pub struct OptimizationExecution<'a> {
    pub budget: Option<(&'a AtomicBool, Instant)>,
    pub funds: Option<Funds<'a>>,
}
pub fn optimize_bounded(
    pools: &[Pool],
    paths: &[PathQuote],
    input: &Amount,
    parts: usize,
    max_splits: usize,
    cost: &Amount,
    budget: Option<(&AtomicBool, Instant)>,
) -> Result<Allocation> {
    optimize_bounded_with_funds(
        pools,
        paths,
        input,
        parts,
        max_splits,
        cost,
        OptimizationExecution {
            budget,
            funds: None,
        },
    )
}
pub fn optimize_bounded_with_funds(
    pools: &[Pool],
    paths: &[PathQuote],
    input: &Amount,
    parts: usize,
    max_splits: usize,
    cost: &Amount,
    execution: OptimizationExecution<'_>,
) -> Result<Allocation> {
    Ok(optimize_with_limits(
        pools,
        paths,
        input,
        cost,
        OptimizationLimits {
            parts,
            max_splits,
            max_evaluations: 100_000,
        },
        execution,
    )?
    .allocation)
}
