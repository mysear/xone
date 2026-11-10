mod bpt_wallets;
pub mod exact_out;
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
                if p.protocol == xone_domain::managed_v2_bpt_state::PROTOCOL
                    || p.protocol == xone_domain::managed_v2_circuit_state::PROTOCOL
                {
                    for asset in &multi.assets {
                        for (from, to) in [(&asset.token, &p.pool_id), (&p.pool_id, &asset.token)] {
                            g.adjacency.entry(from.clone()).or_default().push(Edge {
                                from_token: Some(from.clone()),
                                protocol: p.protocol.clone(),
                                pool_id: p.pool_id.clone(),
                                zero_for_one: false,
                                to: to.clone(),
                            });
                        }
                    }
                }
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
    pub evaluations: usize,
}
/// Funds of an indicative sequential path. Intermediate credits return to
/// sender; only the final hop credits recipient. No execution approval implied.
#[derive(Clone, Copy, Debug)]
pub struct Funds<'a> {
    pub sender: &'a str,
    pub recipient: &'a str,
}
pub fn evaluate_path(
    pools: &mut BTreeMap<String, Pool>,
    path: &[Edge],
    input: &Amount,
) -> Result<Amount> {
    evaluate_path_context(pools, path, input, None)
}
pub fn evaluate_path_with_funds(
    pools: &mut BTreeMap<String, Pool>,
    path: &[Edge],
    input: &Amount,
    funds: Funds<'_>,
) -> Result<Amount> {
    evaluate_path_context(pools, path, input, Some(funds))
}
fn evaluate_path_context(
    pools: &mut BTreeMap<String, Pool>,
    path: &[Edge],
    input: &Amount,
    funds: Option<Funds<'_>>,
) -> Result<Amount> {
    xone_domain::validate_pool_dependencies(pools.values())?;
    if pools.values().any(|p| {
        p.multi_asset
            .as_ref()
            .is_some_and(|s| s.managed_v2.is_some())
    }) {
        if let Some(funds) = funds {
            for address in [funds.sender, funds.recipient] {
                xone_domain::protocols::validate_address(address)?;
                if address == "0x0000000000000000000000000000000000000000" {
                    return Err(err("MANAGED_V2_BPT_ZERO_FUNDS_ADDRESS"));
                }
            }
        }
    }
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
    let contextual = path.iter().any(|edge| {
        pools
            .get(&edge.pool_id)
            .is_some_and(|p| p.slipstream_context.is_some())
    });
    let pancake_contextual = path.iter().any(|edge| {
        pools
            .get(&edge.pool_id)
            .is_some_and(|p| p.pancake_lm.is_some() || p.pancake_bsc_lm.is_some())
    });
    let external_funds_only = pancake_contextual
        || contextual
        || path
            .iter()
            .any(|edge| pools.get(&edge.pool_id).is_some_and(|p| p.lfj.is_some()));
    if external_funds_only {
        if let Some(funds) = funds {
            for address in [funds.sender, funds.recipient] {
                let lower = address.to_ascii_lowercase();
                xone_domain::protocols::validate_address(&lower)?;
                let internal = pools.values().any(|p| {
                    let mut accounts = vec![p.pool_id.as_str()];
                    if let Some(s) = &p.pancake_bsc_lm {
                        accounts.extend([
                            s.ledger.core.factory.as_str(),
                            s.ledger.lm.address.as_str(),
                            s.ledger.master_chef.address.as_str(),
                        ]);
                        accounts.extend(s.predecessors.iter().map(|p| p.address.as_str()));
                    }
                    if let Some(s) = &p.pancake_lm {
                        accounts.push(&s.lm.address);
                        accounts.push(&s.master_chef.address);
                        accounts.push(&s.core.factory);
                    }
                    if let Some(s) = &p.multi_asset {
                        accounts.push(&s.vault);
                    }
                    if let Some(s) = &p.weighted {
                        accounts.push(&s.vault);
                    }
                    if let Some(s) = &p.singleton_cl {
                        accounts.push(&s.manager);
                    }
                    if let Some(s) = &p.singleton_advanced {
                        accounts.push(&s.snapshot.manager);
                    }
                    if let Some(s) = &p.infinity_bin {
                        accounts.push(&s.manager);
                        accounts.push(&s.vault);
                    }
                    accounts
                        .into_iter()
                        .any(|owner| owner.eq_ignore_ascii_case(address))
                });
                if lower == xone_domain::singleton::ZERO_ADDRESS || internal {
                    return Err(err(if pancake_contextual {
                        "PANCAKE_LM_UNMODELED_SELF_FUNDING"
                    } else if contextual {
                        "SLIPSTREAM_UNMODELED_SELF_FUNDING"
                    } else {
                        "LFJ_UNMODELED_SELF_FUNDING"
                    }));
                }
            }
        }
    }
    let mut candidate = pools.clone();
    let out = evaluate_path_in_place(&mut candidate, path, input, funds)?;
    *pools = candidate;
    Ok(out)
}
fn evaluate_path_in_place(
    pools: &mut BTreeMap<String, Pool>,
    path: &[Edge],
    input: &Amount,
    funds: Option<Funds<'_>>,
) -> Result<Amount> {
    let mut amount = input.clone();
    let mut previous_output: Option<String> = None;
    for (hop, e) in path.iter().enumerate() {
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
        // Every pair kernel, including context-specific fee/LM branches,
        // produces the pool's opposite token for the selected direction.
        if pool.multi_asset.is_none()
            && &e.to
                != if e.zero_for_one {
                    &pool.token1
                } else {
                    &pool.token0
                }
        {
            return Err(err("PATH_OUTPUT_TOKEN_MISMATCH"));
        }
        let bpt_swap = (pool.protocol == xone_domain::managed_v2_bpt_state::PROTOCOL
            || pool.protocol == xone_domain::managed_v2_circuit_state::PROTOCOL)
            && (token_in == pool.pool_id || e.to == pool.pool_id);
        if !bpt_swap {
            if let (Some(funds), Some(multi)) = (funds, pool.multi_asset.as_ref()) {
                let recipient = if hop + 1 == path.len() {
                    funds.recipient
                } else {
                    funds.sender
                };
                if multi.managed_v2.is_some()
                    && (funds.sender == multi.vault || recipient == multi.vault)
                {
                    // Ordinary swaps currently model external ERC20 movement;
                    // self-funding leaves the actual Vault wallet unchanged.
                    return Err(err("MANAGED_V2_REGULAR_FUNDS_ALIAS_UNSUPPORTED"));
                }
            }
        }
        let q = if pool.protocol == xone_domain::pancake_bsc_lm::PROFILE {
            let _funds = funds.ok_or_else(|| err("BSC4_FUNDS_REQUIRED"))?;
            xone_protocol_math::pancake_bsc_lm::quote_exact_in(pool, &amount, e.zero_for_one)?
        } else if pool.protocol == xone_domain::pancake_lm::PROFILE {
            let _funds = funds.ok_or_else(|| err("PANCAKE_LM_FUNDS_REQUIRED"))?;
            xone_protocol_math::pancake_lm::quote_exact_in(pool, &amount, e.zero_for_one)?
        } else if pool.protocol == xone_domain::slipstream_context::PROFILE {
            let funds = funds.ok_or_else(|| err("SLIPSTREAM_FUNDS_REQUIRED"))?;
            xone_protocol_math::slipstream_context::quote_exact_in(
                pool,
                &amount,
                e.zero_for_one,
                funds.sender,
            )?
        } else if bpt_swap {
            let funds = funds.ok_or_else(|| err("MANAGED_V2_BPT_FUNDS_REQUIRED"))?;
            let m = pool
                .multi_asset
                .as_ref()
                .and_then(|s| s.managed_v2.as_ref())
                .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
            let (block_number, timestamp) = m
                .execution_context
                .as_ref()
                .map_or((m.block_number, m.timestamp), |c| {
                    (c.block_number, c.timestamp)
                });
            xone_protocol_math::managed_v2_pool::quote_bpt_exact_in_at(
                pool,
                &amount,
                token_in,
                &e.to,
                xone_protocol_math::managed_v2_pool::BptExecution {
                    sender: funds.sender,
                    recipient: if hop + 1 == path.len() {
                        funds.recipient
                    } else {
                        funds.sender
                    },
                    block_number,
                    timestamp,
                },
            )?
        } else if pool.multi_asset.is_some() {
            xone_protocol_math::multi_asset::quote_exact_in_tokens(pool, &amount, token_in, &e.to)?
        } else {
            quote_exact_in(pool, &amount, e.zero_for_one)?
        };
        let input_token = token_in.to_owned();
        let input_amount = amount.clone();
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
        pools.insert(e.pool_id.clone(), q.next_pool.clone());
        let hop_funds = funds.map(|f| Funds {
            sender: f.sender,
            recipient: if hop + 1 == path.len() {
                f.recipient
            } else {
                f.sender
            },
        });
        bpt_wallets::propagate(
            pools,
            &q.next_pool,
            &input_token,
            &e.to,
            &input_amount,
            &amount,
            hop_funds,
        )?;
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
pub struct SearchExecution<'a> {
    pub control: Option<SearchControl<'a>>,
    pub funds: Option<Funds<'a>>,
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
    search_bounded_with_funds(
        graph,
        pools,
        sell,
        buy,
        input,
        budget,
        SearchExecution {
            control,
            funds: None,
        },
    )
}
pub fn search_bounded_with_funds(
    graph: &Graph,
    pools: &[Pool],
    sell: &str,
    buy: &str,
    input: &Amount,
    budget: Budget,
    execution: SearchExecution<'_>,
) -> Result<SearchResult> {
    let SearchExecution { control, funds } = execution;
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
                let mut candidate = None;
                let mut rate: Option<(Amount, Amount)> = None;
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
                    match evaluate_path_context(&mut state, &next, amount, funds) {
                        Ok(out) => {
                            if rate.as_ref().is_none_or(|(output, input)| {
                                &out.0 * &input.0 > &output.0 * &amount.0
                            }) {
                                rate = Some((out.clone(), amount.clone()));
                            }
                            if candidate.is_none() {
                                candidate = Some(PathQuote {
                                    amount_in: amount.clone(),
                                    edges: next.clone(),
                                    tokens: next_tokens.clone(),
                                    amount_out: out,
                                });
                            }
                            // Keep a successful full-order quote, but rank it
                            // using a split-size probe as well. Full-order
                            // average prices alone discard useful small pools.
                            if *amount != *input {
                                break;
                            }
                        }
                        Err(e) => {
                            if ![
                                "PANCAKE_LM_RAW_BALANCE",
                                "PANCAKE_LM_FEE_GROWTH_WIDTH",
                                "PANCAKE_LM_PROTOCOL_MULTIPLICATION_OVERFLOW",
                                "PANCAKE_LM_REWARD_FULLMATH_WIDTH",
                                "PANCAKE_LM_REWARD_GROWTH_WIDTH",
                                "SLIPSTREAM_RAW_BALANCE",
                                "SLIPSTREAM_FEE_GROWTH_OVERFLOW",
                                "SLIPSTREAM_REWARD_GROWTH_OVERFLOW",
                                "LFJ_OUT_OF_LIQUIDITY",
                                "LFJ_MAX_STEPS",
                                "LFJ_UINT128_OVERFLOW",
                                "LFJ_INPUT_UINT128",
                                "LFJ_PRICE_RANGE",
                                "LFJ_ORACLE_UINT64_OVERFLOW",
                                "V3_TICK_RANGE_INCOMPLETE",
                                "V3_MAX_STEPS",
                                "V3_PRICE_LIMIT_REACHED",
                                "V3_INPUT_INT256_RANGE",
                                "V2_RESERVE_OVERFLOW",
                                "BALANCER_MULTI_MAX_IN_RATIO",
                                "MANAGED_V2_MAX_IN_RATIO",
                                "MANAGED_V2_CIRCUIT_BREAKER_TRIPPED",
                                "MANAGED_V2_BPT_MIN_INVARIANT_RATIO",
                                "MANAGED_V2_BPT_INSUFFICIENT_REGISTERED_CASH",
                                "MANAGED_V2_BPT_INSUFFICIENT_BALANCE",
                                "MANAGED_V2_BPT_VAULT_UINT112",
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
                if let (Some(path), Some((output, input))) = (candidate, rate) {
                    found.push((path, output, input));
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
        (&b.1 .0 * &a.2 .0)
            .cmp(&(&a.1 .0 * &b.2 .0))
            .then_with(|| a.0.tokens.cmp(&b.0.tokens))
    });
    if found.len() > budget.max_candidates {
        // Small-capacity probes may have a better average rate while being
        // unable to fill the order together. Never evict the best already
        // proven full-order fallback solely because of those probe rates.
        let fallback = found
            .iter()
            .filter(|p| p.0.amount_in == *input)
            .max_by(|a, b| a.0.amount_out.cmp(&b.0.amount_out))
            .cloned();
        found.truncate(budget.max_candidates);
        if let Some(path) = fallback {
            let retained = found.iter().any(|p| {
                p.0.tokens == path.0.tokens
                    && p.0.edges.iter().map(|e| (&e.pool_id, &e.to)).eq(path
                        .0
                        .edges
                        .iter()
                        .map(|e| (&e.pool_id, &e.to)))
            });
            if !retained {
                found[budget.max_candidates - 1] = path;
            }
        }
        exhausted = true;
    }
    Ok(SearchResult {
        candidates: found.into_iter().map(|p| p.0).collect(),
        budget_exhausted: exhausted,
        expansions,
        evaluations,
    })
}
