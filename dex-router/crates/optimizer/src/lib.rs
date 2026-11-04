use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use xone_domain::{err, Amount, Pool, Result};
use xone_routing::{evaluate_path, PathQuote};
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
    let sell = &routes[0].path.tokens[0];
    let buy = routes[0]
        .path
        .tokens
        .last()
        .ok_or_else(|| err("INVALID_PATH"))?;
    for route in routes {
        if route.path.tokens.first() != Some(sell) || route.path.tokens.last() != Some(buy) {
            return Err(err("ALLOCATION_TOKEN_MISMATCH"));
        }
        let result = evaluate_path(&mut state, &route.path.edges, &route.amount_in)?;
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
/// Enumerates bounded two-route proposals, then replays physical pool state in execution order.
/// This is a heuristic in the declared candidate scope, without a concavity/global-optimum claim.
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
pub fn optimize_bounded(
    pools: &[Pool],
    paths: &[PathQuote],
    input: &Amount,
    parts: usize,
    max_splits: usize,
    cost: &Amount,
    budget: Option<(&AtomicBool, Instant)>,
) -> Result<Allocation> {
    if paths.is_empty() {
        return Err(err("NO_ROUTE"));
    }
    if parts == 0 || parts > 100 || max_splits == 0 || max_splits > 2 {
        return Err(err("INVALID_OPTIMIZER_BUDGET"));
    }
    let mut best: Option<Allocation> = None;
    let mut consider = |routes: Vec<AllocatedRoute>| {
        if budget.is_some_and(|(cancel, deadline)| {
            cancel.load(Ordering::Relaxed) || Instant::now() >= deadline
        }) {
            return;
        }
        if let Ok(q) = evaluate_allocation(pools, &routes, cost) {
            if best
                .as_ref()
                .is_none_or(|b| q.net_buy_amount > b.net_buy_amount)
            {
                best = Some(q);
            }
        }
    };
    let make = |p: &PathQuote, a: Amount| AllocatedRoute {
        path: p.clone(),
        amount_in: a,
        amount_out: Amount::zero(),
    };
    for p in paths {
        consider(vec![make(p, input.clone())]);
    }
    if max_splits == 2 {
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                for part in 1..parts {
                    if budget.is_some_and(|(cancel, deadline)| {
                        cancel.load(Ordering::Relaxed) || Instant::now() >= deadline
                    }) {
                        return Err(err("BUDGET_EXHAUSTED"));
                    }
                    let a = &input.0 * BigUint::from(part) / BigUint::from(parts);
                    let b = &input.0 - &a;
                    if a.is_zero() || b.is_zero() {
                        continue;
                    }
                    let first = make(&paths[i], Amount(a));
                    let second = make(&paths[j], Amount(b));
                    consider(vec![first.clone(), second.clone()]);
                    consider(vec![second, first]);
                }
            }
        }
    }
    if budget.is_some_and(|(cancel, deadline)| {
        cancel.load(Ordering::Relaxed) || Instant::now() >= deadline
    }) {
        return Err(err("BUDGET_EXHAUSTED"));
    }
    let result = best.ok_or_else(|| err("NO_EXECUTABLE_ALLOCATION"))?;
    if result.sell_amount != *input {
        return Err(err("INPUT_CONSERVATION"));
    }
    Ok(result)
}
