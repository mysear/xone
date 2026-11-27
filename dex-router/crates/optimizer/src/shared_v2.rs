use crate::Allocation;
use num_bigint::BigUint;
use num_traits::Zero;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use xone_domain::{v2_swap, Amount, Pool};
use xone_routing::{evaluate_path, Edge};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowStep {
    edges: Vec<Edge>,
    amount_in: Amount,
    amount_out: Amount,
    input_from_steps: Vec<usize>,
    pool_state_from_steps: Vec<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedSuffixQuote {
    sell_amount: Amount,
    buy_amount: Amount,
    steps: Vec<FlowStep>,
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
    let reference = &routes.first()?.path.edges;
    let suffix_len = (1..reference.len())
        .take_while(|&len| {
            routes.iter().all(|route| {
                let edges = &route.path.edges;
                edges.len() > len && edges.ends_with(&reference[reference.len() - len..])
            })
        })
        .last()?;
    let suffix = &reference[reference.len() - suffix_len..];
    let suffix_ids: BTreeSet<_> = suffix.iter().map(|e| e.pool_id.as_str()).collect();
    let mut state: BTreeMap<_, _> = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    if state.len() != pools.len() {
        return None;
    }
    for route in routes {
        let edges = &route.path.edges;
        let tokens = &route.path.tokens;
        if tokens.len() != edges.len() + 1
            || tokens.first() != routes[0].path.tokens.first()
            || tokens.last() != routes[0].path.tokens.last()
            || tokens.iter().collect::<BTreeSet<_>>().len() != tokens.len()
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
        for (edge, pair) in edges.iter().zip(tokens.windows(2)) {
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

    let mut used_input = BigUint::zero();
    let mut merged_input = BigUint::zero();
    let mut last_pool_step = BTreeMap::new();
    let mut steps = Vec::with_capacity(routes.len() + 1);
    for route in routes {
        let prefix = &route.path.edges[..route.path.edges.len() - suffix_len];
        let amount_out = evaluate_path(&mut state, prefix, &route.amount_in).ok()?;
        used_input += &route.amount_in.0;
        merged_input += &amount_out.0;
        let pool_state_from_steps = prefix
            .iter()
            .filter_map(|e| last_pool_step.insert(e.pool_id.as_str(), steps.len()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        steps.push(FlowStep {
            edges: prefix.to_vec(),
            amount_in: route.amount_in.clone(),
            amount_out,
            input_from_steps: Vec::new(),
            pool_state_from_steps,
        });
    }
    if used_input != allocation.sell_amount.0 {
        return None;
    }
    let merged_input = Amount::checked(merged_input).ok()?;
    let buy_amount = evaluate_path(&mut state, suffix, &merged_input).ok()?;
    steps.push(FlowStep {
        edges: suffix.to_vec(),
        amount_in: merged_input,
        amount_out: buy_amount.clone(),
        input_from_steps: (0..steps.len()).collect(),
        pool_state_from_steps: Vec::new(),
    });
    Some(SharedSuffixQuote {
        sell_amount: allocation.sell_amount.clone(),
        buy_amount,
        steps,
    })
}
