mod shared_vault;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use xone_domain::{err, Amount, Pool, Result};
use xone_protocol_math::quote_exact_in;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_token: Option<String>,
    #[serde(default)]
    pub protocol: String,
    pub pool_id: String,
    pub zero_for_one: bool,
    pub to: String,
}
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub adjacency: BTreeMap<String, Vec<Edge>>,
    pub chain_key: String,
    /// Canonical pools retained in the source view but unavailable to this engine.
    pub excluded_pools: BTreeMap<String, String>,
}
impl Graph {
    pub fn build(pools: &[Pool]) -> Result<Self> {
        xone_domain::validate_pool_dependencies(pools.iter())?;
        let mut g = Self {
            excluded_pools: xone_domain::unavailable_pool_reasons(pools.iter())?,
            ..Self::default()
        };
        let mut seen = BTreeSet::new();
        for p in pools {
            p.validate()?;
            if !seen.insert(&p.pool_id) {
                return Err(err("DUPLICATE_PHYSICAL_POOL"));
            }
            if g.chain_key.is_empty() {
                g.chain_key = p.chain_key.clone();
            }
            if g.chain_key != p.chain_key {
                return Err(err("CROSS_CHAIN_GRAPH"));
            }
            if g.excluded_pools.contains_key(&p.pool_id) {
                continue;
            }
            if !(xone_domain::protocols::protocol(&p.protocol).is_some()
                || p.protocol == "fixture-cpmm")
                || p.token_behavior != "standard"
            {
                continue;
            }
            if let Some(multi) = &p.multi_asset {
                for (i, asset) in multi.assets.iter().enumerate() {
                    for (j, target) in multi.assets.iter().enumerate() {
                        if i == j
                            || (p.protocol == "balancer-v2-composable-regular"
                                && (asset.token == p.pool_id || target.token == p.pool_id))
                        {
                            continue;
                        }
                        g.adjacency
                            .entry(asset.token.clone())
                            .or_default()
                            .push(Edge {
                                from_token: Some(asset.token.clone()),
                                pool_id: p.pool_id.clone(),
                                protocol: p.protocol.clone(),
                                zero_for_one: i == 0 && j == 1,
                                to: target.token.clone(),
                            });
                    }
                }
                continue;
            }
            g.adjacency.entry(p.token0.clone()).or_default().push(Edge {
                from_token: None,
                pool_id: p.pool_id.clone(),
                protocol: p.protocol.clone(),
                zero_for_one: true,
                to: p.token1.clone(),
            });
            g.adjacency.entry(p.token1.clone()).or_default().push(Edge {
                from_token: None,
                pool_id: p.pool_id.clone(),
                protocol: p.protocol.clone(),
                zero_for_one: false,
                to: p.token0.clone(),
            });
        }
        Ok(g)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathQuote {
    pub amount_in: Amount,
    pub edges: Vec<Edge>,
    pub tokens: Vec<String>,
    pub amount_out: Amount,
}
#[derive(Clone, Copy)]
pub struct Budget {
    pub split_parts: usize,
    pub max_hops: usize,
    pub max_candidates: usize,
    pub max_expansions: usize,
}
pub struct SearchResult {
    pub candidates: Vec<PathQuote>,
    pub budget_exhausted: bool,
    pub expansions: usize,
}
pub fn evaluate_path(
    pools: &mut BTreeMap<String, Pool>,
    path: &[Edge],
    input: &Amount,
) -> Result<Amount> {
    xone_domain::validate_pool_dependencies(pools.values())?;
    let blocked = xone_domain::unavailable_pool_reasons(pools.values())?;
    let mut chain = None;
    for (key, pool) in pools.iter() {
        if key != &pool.pool_id {
            return Err(err("PHYSICAL_POOL_MAP_KEY"));
        }
        if chain.is_some_and(|value| value != &pool.chain_key) {
            return Err(err("CROSS_CHAIN_ROUTE"));
        }
        chain = Some(&pool.chain_key);
    }
    for edge in path {
        if let Some(reason) = blocked.get(&edge.pool_id) {
            return Err(err(reason));
        }
    }
    let mut candidate = pools.clone();
    let out = evaluate_path_in_place(&mut candidate, path, input)?;
    *pools = candidate;
    Ok(out)
}
fn evaluate_path_in_place(
    pools: &mut BTreeMap<String, Pool>,
    path: &[Edge],
    input: &Amount,
) -> Result<Amount> {
    let mut amount = input.clone();
    let mut previous_output: Option<String> = None;
    for e in path {
        let pool = pools.get(&e.pool_id).ok_or_else(|| err("MISSING_POOL"))?;
        if e.protocol != pool.protocol {
            return Err(err("PATH_PROTOCOL_MISMATCH"));
        }
        let token_in = if pool.multi_asset.is_some() {
            e.from_token
                .as_deref()
                .ok_or_else(|| err("MULTI_ASSET_PATH_TOKEN_REQUIRED"))?
        } else {
            if e.from_token.is_some() {
                return Err(err("LEGACY_PATH_TOKEN_UNEXPECTED"));
            }
            if e.zero_for_one {
                &pool.token0
            } else {
                &pool.token1
            }
        };
        if previous_output.as_deref().is_some_and(|t| t != token_in) {
            return Err(err("PATH_TOKEN_CONTINUITY"));
        }
        let q = if pool.multi_asset.is_some() {
            xone_protocol_math::multi_asset::quote_exact_in_tokens(pool, &amount, token_in, &e.to)?
        } else {
            if &e.to
                != if e.zero_for_one {
                    &pool.token1
                } else {
                    &pool.token0
                }
            {
                return Err(err("PATH_OUTPUT_TOKEN_MISMATCH"));
            }
            quote_exact_in(pool, &amount, e.zero_for_one)?
        };
        previous_output = Some(e.to.clone());
        if q.consumed_in != amount || q.remaining_in != Amount::zero() {
            return Err(err("PARTIAL_FILL_UNSUPPORTED"));
        }
        if q.amount_out == Amount::zero() {
            return Err(err("INSUFFICIENT_OUTPUT"));
        }
        shared_vault::propagate(pools, &q.next_pool)?;
        amount = q.amount_out;
        if let Some(updated) = &q.next_pool.infinity_bin {
            for other in pools.values_mut() {
                let Some(target) = &mut other.infinity_bin else {
                    continue;
                };
                if target.manager != updated.manager {
                    continue;
                }
                for (token, reserve, fees) in [
                    (
                        &q.next_pool.token0,
                        &updated.app_reserve0,
                        &updated.protocol_fees0,
                    ),
                    (
                        &q.next_pool.token1,
                        &updated.app_reserve1,
                        &updated.protocol_fees1,
                    ),
                ] {
                    if other.token0 == *token {
                        target.app_reserve0 = reserve.clone();
                        target.protocol_fees0 = fees.clone();
                    }
                    if other.token1 == *token {
                        target.app_reserve1 = reserve.clone();
                        target.protocol_fees1 = fees.clone();
                    }
                }
            }
        }
        pools.insert(e.pool_id.clone(), q.next_pool);
    }
    Ok(amount)
}
pub fn search(
    graph: &Graph,
    pools: &[Pool],
    sell: &str,
    buy: &str,
    input: &Amount,
    budget: Budget,
) -> Result<SearchResult> {
    search_bounded(graph, pools, sell, buy, input, budget, None)
}
pub struct SearchControl<'a> {
    pub cancel: &'a AtomicBool,
    pub deadline: Instant,
}
pub fn search_bounded(
    graph: &Graph,
    pools: &[Pool],
    sell: &str,
    buy: &str,
    input: &Amount,
    budget: Budget,
    control: Option<SearchControl<'_>>,
) -> Result<SearchResult> {
    if sell == buy
        || budget.max_hops == 0
        || budget.max_candidates == 0
        || budget.max_expansions == 0
        || budget.split_parts == 0
        || budget.split_parts > 100
    {
        return Err(err("INVALID_SEARCH"));
    }
    let base = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut queue = VecDeque::from([(vec![sell.to_owned()], Vec::<Edge>::new())]);
    let mut found = Vec::new();
    let mut expansions = 0;
    let mut exhausted = false;
    let mut evaluations = 0;
    let mut probes = vec![input.clone()];
    for part in 1..budget.split_parts {
        let amount = Amount::checked(&input.0 * part / budget.split_parts)?;
        if amount != Amount::zero() && !probes.contains(&amount) {
            probes.push(amount);
        }
    }
    while let Some((tokens, edges)) = queue.pop_front() {
        if control
            .as_ref()
            .is_some_and(|c| c.cancel.load(Ordering::Relaxed) || Instant::now() >= c.deadline)
        {
            return Err(err("BUDGET_EXHAUSTED"));
        }
        if edges.len() >= budget.max_hops {
            continue;
        }
        for e in graph
            .adjacency
            .get(tokens.last().ok_or_else(|| err("INVALID_PATH"))?)
            .into_iter()
            .flatten()
        {
            if expansions >= budget.max_expansions {
                exhausted = true;
                break;
            }
            expansions += 1;
            if tokens.contains(&e.to) || edges.iter().any(|p| p.pool_id == e.pool_id) {
                continue;
            }
            let mut next = edges.clone();
            next.push(e.clone());
            let mut next_tokens = tokens.clone();
            next_tokens.push(e.to.clone());
            if e.to == buy {
                for amount in &probes {
                    if control.as_ref().is_some_and(|c| {
                        c.cancel.load(Ordering::Relaxed) || Instant::now() >= c.deadline
                    }) {
                        return Err(err("BUDGET_EXHAUSTED"));
                    }
                    if evaluations >= budget.max_expansions {
                        exhausted = true;
                        break;
                    }
                    evaluations += 1;
                    let mut state = base.clone();
                    match evaluate_path(&mut state, &next, amount) {
                        Ok(out) => {
                            found.push(PathQuote {
                                amount_in: amount.clone(),
                                edges: next.clone(),
                                tokens: next_tokens.clone(),
                                amount_out: out,
                            });
                            break;
                        }
                        Err(e) => {
                            if ![
                                "V3_TICK_RANGE_INCOMPLETE",
                                "V3_MAX_STEPS",
                                "V3_PRICE_LIMIT_REACHED",
                                "V3_INPUT_INT256_RANGE",
                                "V2_RESERVE_OVERFLOW",
                                "BALANCER_MULTI_MAX_IN_RATIO",
                                "MANAGED_V2_MAX_IN_RATIO",
                                "WEIGHTED_MAX_IN_RATIO",
                                "V2_INTERMEDIATE_OVERFLOW",
                                // Checked Curve arithmetic/packing can exhaust capacity
                                // for this allocation; every retry revalidates the pool.
                                "CURVE_UINT256_OVERFLOW",
                                "CURVE_NG_PACK128_OVERFLOW",
                                "INSUFFICIENT_OUTPUT",
                            ]
                            .contains(&e.0.as_str())
                            {
                                break;
                            }
                        }
                    }
                }
            } else {
                queue.push_back((next_tokens, next));
            }
        }
        if exhausted {
            break;
        }
    }
    found.sort_by(|a, b| {
        (&b.amount_out.0 * &a.amount_in.0)
            .cmp(&(&a.amount_out.0 * &b.amount_in.0))
            .then_with(|| a.tokens.cmp(&b.tokens))
    });
    if found.len() > budget.max_candidates {
        // Small-capacity probes may have a better average rate while being
        // unable to fill the order together. Never evict the best already
        // proven full-order fallback solely because of those probe rates.
        let fallback = found
            .iter()
            .filter(|p| p.amount_in == *input)
            .max_by(|a, b| a.amount_out.cmp(&b.amount_out))
            .cloned();
        found.truncate(budget.max_candidates);
        if !found.iter().any(|p| p.amount_in == *input) {
            if let Some(path) = fallback {
                found[budget.max_candidates - 1] = path;
            }
        }
        exhausted = true;
    }
    Ok(SearchResult {
        candidates: found,
        budget_exhausted: exhausted,
        expansions,
    })
}
