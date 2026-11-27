use crate::{exact_out::ExactOutAllocation, Allocation};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use xone_domain::{v2_swap, Amount, Pool};
use xone_routing::{evaluate_path, exact_out, Edge, Funds};

pub const CANDIDATE_VERSION: &str = "xone-shared-v2-suffix/1";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStep {
    pub edges: Vec<Edge>,
    pub amount_in: Amount,
    pub amount_out: Amount,
    input_from_steps: Vec<usize>,
    pool_state_from_steps: Vec<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSuffixQuote {
    pub sell_amount: Amount,
    pub buy_amount: Amount,
    pub steps: Vec<FlowStep>,
}

/// Replay fan-in through a shared physical suffix on a private snapshot.
/// Caller funds are partitioned among the prefixes, their outputs are spent
/// once by the suffix, and any prefix pools used more than once advance in
/// route order. The suffix only moves across operations on different pools.
pub fn quote_suffix(pools: &[Pool], allocation: &Allocation) -> Option<SharedSuffixQuote> {
    let routes = &allocation.routes;
    if routes.len() < 2 || !allocation.refund.0.is_zero() {
        return None;
    }
    let paths = routes
        .iter()
        .map(|r| (r.path.edges.as_slice(), r.path.tokens.as_slice()))
        .collect::<Vec<_>>();
    let (suffix, mut state) = shared_state(pools, &paths)?;
    let mut used_input = BigUint::zero();
    let mut merged_input = BigUint::zero();
    let mut steps = Vec::with_capacity(routes.len() + 1);
    for (route, (edges, _)) in routes.iter().zip(&paths) {
        let prefix = &edges[..edges.len() - suffix.len()];
        let amount_out = evaluate_path(&mut state, prefix, &route.amount_in).ok()?;
        used_input += &route.amount_in.0;
        merged_input += &amount_out.0;
        steps.push(prefix_step(prefix, route.amount_in.clone(), amount_out));
    }
    if used_input != allocation.sell_amount.0 {
        return None;
    }
    let merged_input = Amount::checked(merged_input).ok()?;
    let buy_amount = evaluate_path(&mut state, suffix, &merged_input).ok()?;
    Some(join_steps(
        steps,
        suffix,
        merged_input,
        allocation.sell_amount.clone(),
        buy_amount,
    ))
}

/// The suffix's required input is independent of prefix state. Allocate its
/// output targets by the original legs' weights, preserving the integer total.
pub fn quote_suffix_exact_out(
    pools: &[Pool],
    allocation: &ExactOutAllocation,
    funds: Funds<'_>,
) -> Option<SharedSuffixQuote> {
    if allocation.routes.len() < 2
        || allocation.target_output.0.is_zero()
        || allocation.gross_buy_amount.is_some()
        || allocation
            .routes
            .iter()
            .any(|route| route.target_output.is_some())
    {
        return None;
    }
    let paths = allocation
        .routes
        .iter()
        .map(|r| (r.edges.as_slice(), r.tokens.as_slice()))
        .collect::<Vec<_>>();
    let (suffix, mut state) = shared_state(pools, &paths)?;
    let tail =
        exact_out::evaluate_path(&mut state, suffix, &allocation.target_output, funds).ok()?;
    if tail.amount_out != allocation.target_output || tail.target_output.is_some() {
        return None;
    }
    let mut total_weight = BigUint::zero();
    let mut distributed = BigUint::zero();
    let mut used = BigUint::zero();
    let mut steps = Vec::with_capacity(paths.len() + 1);
    for (route, (edges, _)) in allocation.routes.iter().zip(&paths) {
        total_weight += &route.amount_out.0;
        if total_weight > allocation.target_output.0 {
            return None;
        }
        let next = &tail.amount_in.0 * &total_weight / &allocation.target_output.0;
        let target = Amount::checked(&next - &distributed).ok()?;
        distributed = next;
        let prefix = &edges[..edges.len() - suffix.len()];
        let leg = exact_out::evaluate_path(&mut state, prefix, &target, funds).ok()?;
        if leg.amount_out != target || leg.target_output.is_some() || leg.factory != tail.factory {
            return None;
        }
        used += &leg.amount_in.0;
        if used > allocation.input_budget.0 {
            return None;
        }
        steps.push(prefix_step(prefix, leg.amount_in, target));
    }
    if total_weight != allocation.target_output.0 || distributed != tail.amount_in.0 {
        return None;
    }
    Some(join_steps(
        steps,
        suffix,
        tail.amount_in,
        Amount::checked(used).ok()?,
        tail.amount_out,
    ))
}

fn shared_state<'a>(
    pools: &[Pool],
    paths: &[(&'a [Edge], &[String])],
) -> Option<(&'a [Edge], BTreeMap<String, Pool>)> {
    let (reference, tokens) = *paths.first()?;
    let suffix_len = (1..reference.len())
        .take_while(|&len| {
            paths.iter().all(|(edges, _)| {
                edges.len() > len && edges.ends_with(&reference[reference.len() - len..])
            })
        })
        .last()?;
    let suffix = &reference[reference.len() - suffix_len..];
    let suffix_ids: BTreeSet<_> = suffix.iter().map(|e| e.pool_id.as_str()).collect();
    let state: BTreeMap<_, _> = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    if state.len() != pools.len() {
        return None;
    }
    for (edges, path_tokens) in paths {
        if path_tokens.len() != edges.len() + 1
            || path_tokens.first() != tokens.first()
            || path_tokens.last() != tokens.last()
            || path_tokens.iter().collect::<BTreeSet<_>>().len() != path_tokens.len()
            || edges
                .iter()
                .map(|e| &e.pool_id)
                .collect::<BTreeSet<_>>()
                .len()
                != edges.len()
            || edges[..edges.len() - suffix_len]
                .iter()
                .any(|e| suffix_ids.contains(e.pool_id.as_str()))
        {
            return None;
        }
        for (edge, pair) in edges.iter().zip(path_tokens.windows(2)) {
            let pool = state.get(&edge.pool_id)?;
            let (from, to) = if edge.zero_for_one {
                (&pool.token0, &pool.token1)
            } else {
                (&pool.token1, &pool.token0)
            };
            if pool.protocol != v2_swap::PROFILE
                || edge.protocol != pool.protocol
                || pool.token_behavior != "standard"
                || edge.from_token.is_some()
                || from != &pair[0]
                || to != &pair[1]
                || to != &edge.to
            {
                return None;
            }
        }
    }
    Some((suffix, state))
}

fn prefix_step(edges: &[Edge], amount_in: Amount, amount_out: Amount) -> FlowStep {
    FlowStep {
        edges: edges.to_vec(),
        amount_in,
        amount_out,
        input_from_steps: Vec::new(),
        pool_state_from_steps: Vec::new(),
    }
}

fn join_steps(
    mut steps: Vec<FlowStep>,
    suffix: &[Edge],
    merged_input: Amount,
    sell_amount: Amount,
    buy_amount: Amount,
) -> SharedSuffixQuote {
    let mut last_pool_step = BTreeMap::new();
    for (index, step) in steps.iter_mut().enumerate() {
        step.pool_state_from_steps = step
            .edges
            .iter()
            .filter_map(|e| last_pool_step.insert(e.pool_id.as_str(), index))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }
    steps.push(FlowStep {
        edges: suffix.to_vec(),
        amount_in: merged_input,
        amount_out: buy_amount.clone(),
        input_from_steps: (0..steps.len()).collect(),
        pool_state_from_steps: Vec::new(),
    });
    SharedSuffixQuote {
        sell_amount,
        buy_amount,
        steps,
    }
}
