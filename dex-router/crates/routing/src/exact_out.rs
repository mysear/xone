//! Amount-specific exact-output paths for the fixed original Router02 template.
use crate::{Budget, Edge, Funds, Graph, SearchExecution};
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::atomic::Ordering,
    time::Instant,
};
use xone_domain::{err, Amount, Pool, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactOutPathQuote {
    pub edges: Vec<Edge>,
    pub tokens: Vec<String>,
    pub amounts: Vec<Amount>,
    pub amount_in: Amount,
    pub amount_out: Amount,
    /// Minimum target when physical Pair delivery includes explicit surplus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_output: Option<Amount>,
    pub factory: String,
}
impl ExactOutPathQuote {
    pub fn output_target(&self) -> &Amount {
        self.target_output.as_ref().unwrap_or(&self.amount_out)
    }
}
#[derive(Clone, Debug)]
pub struct ExactOutSearchResult {
    pub candidates: Vec<ExactOutPathQuote>,
    pub excluded_pools: BTreeMap<String, String>,
    pub budget_exhausted: bool,
    pub expansions: usize,
    pub evaluations: usize,
}

fn validate_funds(pools: &BTreeMap<String, Pool>, funds: Funds<'_>) -> Result<()> {
    if pools
        .values()
        .next()
        .is_some_and(|p| p.chain_key.starts_with("solana:"))
    {
        for address in [funds.sender, funds.recipient] {
            xone_domain::raydium::validate_pubkey(address)?;
            for pool in pools.values() {
                if pool.pool_id == address
                    || pool.token0 == address
                    || pool.token1 == address
                    || pool.orca_whirlpool.as_ref().is_some_and(|m| {
                        [
                            &m.program_id,
                            &m.programdata,
                            &m.vault0,
                            &m.vault1,
                            &m.oracle,
                            &m.whirlpools_config,
                        ]
                        .into_iter()
                        .any(|id| id == address)
                            || m.tick_arrays.iter().any(|a| a.address == address)
                            || m.reward_infos.iter().any(|r| {
                                r.mint != "11111111111111111111111111111111"
                                    && (r.mint == address || r.vault == address)
                            })
                    })
                    || pool.raydium_cpmm.as_ref().is_some_and(|m| {
                        [
                            &m.program_id,
                            &m.programdata,
                            &m.vault0,
                            &m.vault1,
                            &m.authority,
                            &m.amm_config,
                        ]
                        .into_iter()
                        .any(|id| id == address)
                    })
                    || pool.raydium_clmm.as_ref().is_some_and(|m| {
                        [
                            &m.program_id,
                            &m.programdata,
                            &m.vault0,
                            &m.vault1,
                            &m.observation,
                        ]
                        .into_iter()
                        .any(|id| id == address)
                    })
                {
                    return Err(err("EXACT_OUT_UNMODELED_SELF_FUNDING"));
                }
                if let Some(m) = &pool.meteora {
                    if m.internal_accounts(pool)?
                        .iter()
                        .any(|owner| owner == address)
                    {
                        return Err(err("EXACT_OUT_UNMODELED_SELF_FUNDING"));
                    }
                }
            }
        }
        return Ok(());
    }
    for address in [funds.sender, funds.recipient] {
        let address = address.to_ascii_lowercase();
        xone_domain::protocols::validate_address(&address)?;
        if address == xone_domain::singleton::ZERO_ADDRESS {
            return Err(err("EXACT_OUT_FUNDS_REQUIRED"));
        }
        for pool in pools.values() {
            let mut internal = vec![pool.pool_id.as_str()];
            if let Some(s) = &pool.v2 {
                internal.push(&s.factory);
            }
            if let Some(s) = &pool.multi_asset {
                internal.push(&s.vault);
                if let Some(m) = &s.managed_v2 {
                    internal.extend([m.manager.as_str(), m.protocol_collector.as_str()]);
                }
            }
            if let Some(s) = &pool.singleton_cl {
                internal.push(&s.manager);
            }
            if let Some(s) = &pool.singleton_advanced {
                internal.push(&s.snapshot.manager);
            }
            if let Some(s) = &pool.infinity_bin {
                internal.extend([s.manager.as_str(), s.vault.as_str()]);
            }
            if let Some(s) = &pool.lfj {
                internal.extend([s.factory.as_str(), s.implementation.as_str()]);
            }
            if let Some(s) = &pool.slipstream_context {
                internal.extend([
                    s.core.factory.as_str(),
                    s.core.implementation.as_str(),
                    s.factory_voter.as_str(),
                    s.factory_registry.as_str(),
                    s.gauge.as_str(),
                    s.nft.as_str(),
                    s.swap_fee_module.address.as_str(),
                    s.unstaked_fee_module.address.as_str(),
                ]);
            }
            if let Some(s) = &pool.pancake_lm {
                internal.extend([
                    s.lm.address.as_str(),
                    s.master_chef.address.as_str(),
                    s.core.factory.as_str(),
                ]);
            }
            if let Some(s) = &pool.pancake_bsc_lm {
                internal.extend([
                    s.ledger.lm.address.as_str(),
                    s.ledger.master_chef.address.as_str(),
                    s.ledger.core.factory.as_str(),
                ]);
                internal.extend(s.predecessors.iter().map(|p| p.address.as_str()));
            }
            if internal
                .iter()
                .any(|owner| owner.eq_ignore_ascii_case(&address))
            {
                return Err(err("EXACT_OUT_UNMODELED_SELF_FUNDING"));
            }
        }
    }
    Ok(())
}
fn edge_tokens<'a>(p: &'a Pool, e: &Edge) -> Result<(&'a str, &'a str)> {
    if !xone_protocol_math::exact_out::supports_protocol(&p.protocol)
        || e.protocol != p.protocol
        || e.pool_id != p.pool_id
        || e.from_token.is_some()
    {
        return Err(err("EXACT_OUT_PATH_PROTOCOL"));
    }
    let pair = if e.zero_for_one {
        (p.token0.as_str(), p.token1.as_str())
    } else {
        (p.token1.as_str(), p.token0.as_str())
    };
    if pair.1 != e.to {
        return Err(err("PATH_OUTPUT_TOKEN_MISMATCH"));
    }
    Ok(pair)
}

/// Reverse targets are recomputed against current physical state; the entire
/// forward replay is committed only when token flow and every bound are valid.
pub fn evaluate_path(
    pools: &mut BTreeMap<String, Pool>,
    edges: &[Edge],
    output: &Amount,
    funds: Funds<'_>,
) -> Result<ExactOutPathQuote> {
    xone_domain::validate_pool_dependencies(pools.values())?;
    validate_funds(pools, funds)?;
    if edges.is_empty() || output.0.is_zero() {
        return Err(err("EXACT_OUT_INVALID_PATH"));
    }
    let mut chain = None;
    for (key, pool) in pools.iter() {
        if key != &pool.pool_id {
            return Err(err("PHYSICAL_POOL_MAP_KEY"));
        }
        if chain.is_some_and(|c| c != &pool.chain_key) {
            return Err(err("CROSS_CHAIN_ROUTE"));
        }
        chain = Some(&pool.chain_key);
    }
    let excluded = xone_domain::unavailable_pool_reasons(pools.values())?;
    let (mut seen, mut token_seen, mut tokens, mut factory) =
        (BTreeSet::new(), BTreeSet::new(), Vec::new(), None);
    for edge in edges {
        if !seen.insert(&edge.pool_id) {
            return Err(err("EXACT_OUT_REPEATED_PHYSICAL_POOL"));
        }
        if let Some(reason) = excluded.get(&edge.pool_id) {
            return Err(err(reason));
        }
        let pool = pools
            .get(&edge.pool_id)
            .ok_or_else(|| err("MISSING_POOL"))?;
        pool.validate()?;
        // The callback-funded CL templates have only been admitted for one
        // physical pool. It cannot silently become a V2 or mixed multi-hop route.
        if (xone_protocol_math::orca::supports_profile(&pool.protocol)
            || matches!(
                pool.protocol.as_str(),
                xone_domain::pancake_bsc_lm::PROFILE
                    | xone_domain::pancake_lm::PROFILE
                    | xone_domain::slipstream_context::PROFILE
                    | xone_domain::lfj::PROFILE
                    | xone_domain::meteora::PROFILE
                    | xone_domain::meteora_token2022::PROFILE
                    | xone_domain::meteora::REWARDS_PROFILE
                    | xone_domain::meteora::BITMAP_PROFILE
                    | xone_domain::meteora::ORDERS_PROFILE
                    | xone_domain::meteora::ONLY_Y_PROFILE
                    | xone_domain::pump::PROFILE
                    | xone_domain::pump::MIGRATION_PROFILE
                    | xone_domain::pump_amm::PROFILE
                    | xone_domain::pump_amm::BOOST_PROFILE
                    | xone_domain::raydium::PROFILE
            ))
            && edges.len() != 1
        {
            return Err(err(
                if xone_protocol_math::orca::supports_profile(&pool.protocol) {
                    "EXACT_OUT_ORCA_SINGLE_POOL_ONLY"
                } else if pool.protocol == xone_domain::pancake_bsc_lm::PROFILE {
                    "EXACT_OUT_BSC4_SINGLE_POOL_ONLY"
                } else if pool.protocol == xone_domain::slipstream_context::PROFILE {
                    "EXACT_OUT_SLIPSTREAM_SINGLE_POOL_ONLY"
                } else if xone_domain::pump::supports_curve_profile(&pool.protocol) {
                    "EXACT_OUT_PUMP_SINGLE_POOL_ONLY"
                } else if pool.protocol == xone_domain::raydium::PROFILE {
                    "EXACT_OUT_RAYDIUM_SINGLE_POOL_ONLY"
                } else if xone_domain::meteora::supports_profile(&pool.protocol) {
                    "EXACT_OUT_METEORA_SINGLE_POOL_ONLY"
                } else if pool.protocol == xone_domain::lfj::PROFILE {
                    "EXACT_OUT_LFJ_SINGLE_POOL_ONLY"
                } else {
                    "EXACT_OUT_LM_SINGLE_POOL_ONLY"
                },
            ));
        }
        if let Some(pump) = &pool.pump {
            if funds.sender != pump.user || funds.recipient != pump.user {
                return Err(err("PUMP_DECLARED_USER_REQUIRED"));
            }
        }
        let (from, to) = edge_tokens(pool, edge)?;
        if tokens.is_empty() {
            tokens.push(from.to_owned());
            token_seen.insert(from);
        }
        if tokens.last().map(String::as_str) != Some(from) {
            return Err(err("PATH_TOKEN_CONTINUITY"));
        }
        if !token_seen.insert(to) {
            return Err(err("EXACT_OUT_PATH_CYCLE_UNSUPPORTED"));
        }
        tokens.push(to.to_owned());
        let current: &str = if let Some(state) = &pool.v2 {
            &state.factory
        } else if let Some(state) = &pool.pancake_bsc_lm {
            &state.ledger.core.factory
        } else if let Some(state) = &pool.pancake_lm {
            &state.core.factory
        } else if let Some(state) = &pool.slipstream_context {
            &state.core.factory
        } else if let Some(state) = &pool.lfj {
            &state.factory
        } else if xone_domain::pump_amm::supports_profile(&pool.protocol) {
            xone_domain::pump_amm::PROGRAM
        } else if pool.pump.is_some() {
            xone_domain::pump::PROGRAM
        } else if pool.orca_whirlpool.is_some() {
            xone_domain::orca::PROGRAM_ID
        } else if pool.raydium_cpmm.is_some() {
            xone_domain::raydium::PROGRAM_ID
        } else if pool.meteora.is_some() {
            xone_domain::meteora::PROGRAM
        } else {
            return Err(err("EXACT_OUT_PATH_PROTOCOL"));
        };
        if factory.is_some_and(|f| f != current) {
            return Err(err("EXACT_OUT_CROSS_FACTORY_UNSUPPORTED"));
        }
        factory = Some(current);
    }
    let mut amounts = vec![Amount::zero(); edges.len() + 1];
    amounts[edges.len()] = output.clone();
    for i in (0..edges.len()).rev() {
        let p = pools
            .get(&edges[i].pool_id)
            .ok_or_else(|| err("MISSING_POOL"))?;
        amounts[i] = xone_protocol_math::exact_out::quote_exact_out_for_origin(
            p,
            &amounts[i + 1],
            edges[i].zero_for_one,
            funds.sender,
        )?
        .amount_in;
    }
    let mut candidate = pools.clone();
    for (i, edge) in edges.iter().enumerate() {
        let p = candidate
            .get(&edge.pool_id)
            .ok_or_else(|| err("MISSING_POOL"))?;
        let q = xone_protocol_math::exact_out::quote_exact_out_for_origin(
            p,
            &amounts[i + 1],
            edge.zero_for_one,
            funds.sender,
        )?;
        let surplus_template = (p.protocol == xone_domain::lfj::PROFILE
            || ((xone_domain::pump::supports_curve_profile(&p.protocol)
                || xone_domain::pump_amm::supports_profile(&p.protocol))
                && edge.zero_for_one))
            && edges.len() == 1;
        if q.amount_in != amounts[i]
            || (if surplus_template {
                q.amount_out.0 < amounts[i + 1].0
            } else {
                q.amount_out != amounts[i + 1]
            })
        {
            return Err(err("EXACT_OUT_REPLAY_AMOUNTS_CHANGED"));
        }
        if surplus_template {
            amounts[i + 1] = q.amount_out;
        }
        candidate.insert(edge.pool_id.clone(), q.next_pool);
    }
    let result = ExactOutPathQuote {
        edges: edges.to_vec(),
        tokens,
        amount_in: amounts[0].clone(),
        amount_out: amounts[edges.len()].clone(),
        target_output: (amounts[edges.len()] != *output).then(|| output.clone()),
        amounts,
        factory: factory
            .ok_or_else(|| err("EXACT_OUT_FACTORY_REQUIRED"))?
            .to_owned(),
    };
    *pools = candidate;
    Ok(result)
}

pub fn search(
    graph: &Graph,
    pools: &[Pool],
    sell: &str,
    buy: &str,
    output: &Amount,
    budget: Budget,
    execution: SearchExecution<'_>,
) -> Result<ExactOutSearchResult> {
    if sell == buy
        || output.0.is_zero()
        || budget.max_hops == 0
        || budget.max_candidates == 0
        || budget.max_expansions == 0
        || budget.split_parts == 0
    {
        return Err(err("EXACT_OUT_SEARCH_BUDGET"));
    }
    let funds = execution
        .funds
        .ok_or_else(|| err("EXACT_OUT_FUNDS_REQUIRED"))?;
    let state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    if state.len() != pools.len() {
        return Err(err("DUPLICATE_PHYSICAL_POOL"));
    }
    xone_domain::validate_pool_dependencies(pools.iter())?;
    validate_funds(&state, funds)?;
    if pools.iter().any(|p| p.chain_key != graph.chain_key) {
        return Err(err("CROSS_CHAIN_GRAPH"));
    }
    let mut excluded = xone_domain::unavailable_pool_reasons(pools.iter())?;
    for p in pools {
        if !xone_protocol_math::exact_out::supports_protocol(&p.protocol) {
            excluded
                .entry(p.pool_id.clone())
                .or_insert_with(|| "EXACT_OUT_PROTOCOL_UNSUPPORTED".into());
        }
    }
    let mut incoming: BTreeMap<String, Vec<(String, Edge)>> = BTreeMap::new();
    for (from, edges) in &graph.adjacency {
        for edge in edges {
            if excluded.contains_key(&edge.pool_id) {
                continue;
            }
            let p = state
                .get(&edge.pool_id)
                .ok_or_else(|| err("MISSING_POOL"))?;
            let (source, target) = edge_tokens(p, edge)?;
            if source != from {
                return Err(err("EXACT_OUT_GRAPH_TOKEN_BINDING"));
            }
            incoming
                .entry(target.to_owned())
                .or_default()
                .push((source.to_owned(), edge.clone()));
        }
    }
    let probe = Amount((&output.0 / budget.split_parts).max(num_bigint::BigUint::one()));
    let mut queue = VecDeque::from([(buy.to_owned(), Vec::<Edge>::new(), vec![buy.to_owned()])]);
    let (mut candidates, mut expansions, mut evaluations, mut exhausted, mut fallback) =
        (Vec::new(), 0, 0, false, None::<ExactOutPathQuote>);
    while let Some((token, edges, tokens)) = queue.pop_front() {
        if execution
            .control
            .as_ref()
            .is_some_and(|c| c.cancel.load(Ordering::Relaxed) || Instant::now() >= c.deadline)
        {
            return Err(err("BUDGET_EXHAUSTED"));
        }
        if edges.len() >= budget.max_hops {
            continue;
        }
        for (from, edge) in incoming.get(&token).into_iter().flatten() {
            if execution
                .control
                .as_ref()
                .is_some_and(|c| c.cancel.load(Ordering::Relaxed) || Instant::now() >= c.deadline)
            {
                return Err(err("BUDGET_EXHAUSTED"));
            }
            if expansions >= budget.max_expansions {
                exhausted = true;
                break;
            }
            expansions += 1;
            if tokens.contains(from) || edges.iter().any(|e| e.pool_id == edge.pool_id) {
                continue;
            }
            let mut next_edges = vec![edge.clone()];
            next_edges.extend(edges.iter().cloned());
            let mut trial = state.clone();
            evaluations += 1;
            let proposed = evaluate_path(&mut trial, &next_edges, &probe, funds).ok();
            // Pair settlement can reject the split-sized probe even when the
            // whole target is feasible. This also matters for a partial suffix:
            // dropping it here would hide valid multi-hop full-order paths.
            let full = if from == sell || proposed.is_none() {
                let mut full_state = state.clone();
                evaluations += 1;
                evaluate_path(&mut full_state, &next_edges, output, funds).ok()
            } else {
                None
            };
            if from == sell {
                if let Some(q) = &full {
                    if fallback
                        .as_ref()
                        .is_none_or(|best| q.amount_in.0 < best.amount_in.0)
                    {
                        fallback = Some(q.clone());
                    }
                }
                if let Some(candidate) = proposed.or(full) {
                    candidates.push(candidate);
                }
            } else if proposed.is_some() || full.is_some() {
                let mut next_tokens = vec![from.clone()];
                next_tokens.extend(tokens.iter().cloned());
                queue.push_back((from.clone(), next_edges, next_tokens));
            }
        }
        if exhausted {
            break;
        }
    }
    if execution
        .control
        .as_ref()
        .is_some_and(|c| c.cancel.load(Ordering::Relaxed) || Instant::now() >= c.deadline)
    {
        return Err(err("BUDGET_EXHAUSTED"));
    }
    candidates.sort_by(|a, b| {
        a.amount_in
            .0
            .cmp(&b.amount_in.0)
            .then_with(|| a.edges.len().cmp(&b.edges.len()))
    });
    if candidates.len() > budget.max_candidates {
        exhausted = true;
    }
    candidates.truncate(budget.max_candidates);
    if let Some(full) = fallback {
        if let Some(existing) = candidates.iter_mut().find(|p| {
            p.edges
                .iter()
                .map(|e| (&e.pool_id, e.zero_for_one))
                .eq(full.edges.iter().map(|e| (&e.pool_id, e.zero_for_one)))
        }) {
            *existing = full;
        } else if !candidates.iter().any(|p| {
            p.edges
                .iter()
                .map(|e| (&e.pool_id, e.zero_for_one))
                .eq(full.edges.iter().map(|e| (&e.pool_id, e.zero_for_one)))
        }) {
            if candidates.len() == budget.max_candidates {
                candidates.pop();
            }
            candidates.push(full);
        }
    }
    Ok(ExactOutSearchResult {
        candidates,
        excluded_pools: excluded,
        budget_exhausted: exhausted,
        expansions,
        evaluations,
    })
}
