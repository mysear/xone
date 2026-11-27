use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use xone_domain::{err, v2_swap, Amount, Pool, Result};
use xone_routing::Edge;

pub enum Objective<'a> {
    ExactIn {
        input: &'a Amount,
        selected_output: &'a Amount,
    },
    ExactOut {
        output: &'a Amount,
        selected_input: &'a Amount,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bound {
    scope: &'static str,
    objective: &'static str,
    candidate_paths: usize,
    limit_atoms: String,
    gap_atoms: String,
}

/// Bound allocations across the retained standard Uniswap V2 paths, including
/// off-grid amounts. Each pool's fee-adjusted spot rate bounds execution from
/// above. A same-direction swap can only reduce that rate as reserves change.
/// Combining per-hop rates and taking the maximum path rate bounds any split.
pub fn evaluate<'a>(
    pools: &'a [Pool],
    paths: impl IntoIterator<Item = &'a [Edge]>,
    objective: Objective<'_>,
) -> Result<Option<Bound>> {
    let mut candidate_paths = 0;
    let Some((output_rate, input_rate)) =
        max_path_rate(pools, paths.into_iter().inspect(|_| candidate_paths += 1))
    else {
        return Ok(None);
    };
    let (kind, limit, gap) = match objective {
        Objective::ExactIn {
            input,
            selected_output,
        } => {
            let limit = (&input.0 * output_rate) / input_rate;
            if limit < selected_output.0 {
                return Err(err("CONSTANT_PRODUCT_BOUND_MISMATCH"));
            }
            let gap = &limit - &selected_output.0;
            ("GROSS_OUTPUT_UPPER_BOUND", limit, gap)
        }
        Objective::ExactOut {
            output,
            selected_input,
        } => {
            let limit = (&output.0 * input_rate + (&output_rate - BigUint::one())) / output_rate;
            if limit > selected_input.0 {
                return Err(err("CONSTANT_PRODUCT_BOUND_MISMATCH"));
            }
            let gap = &selected_input.0 - &limit;
            ("REQUIRED_INPUT_LOWER_BOUND", limit, gap)
        }
    };
    Ok(Some(Bound {
        scope: "RETAINED_STANDARD_V2_PATHS_AT_SNAPSHOT",
        objective: kind,
        candidate_paths,
        limit_atoms: limit.to_string(),
        gap_atoms: gap.to_string(),
    }))
}

fn max_path_rate<'a>(
    pools: &'a [Pool],
    paths: impl IntoIterator<Item = &'a [Edge]>,
) -> Option<(BigUint, BigUint)> {
    let by_id: BTreeMap<_, _> = pools.iter().map(|p| (p.pool_id.as_str(), p)).collect();
    if by_id.len() != pools.len() {
        return None;
    }
    let mut directions = BTreeMap::new();
    let mut best: Option<(BigUint, BigUint)> = None;
    for path in paths {
        if path.is_empty() {
            return None;
        }
        let mut used = BTreeSet::new();
        let (mut output_rate, mut input_rate) = (BigUint::one(), BigUint::one());
        for edge in path {
            let pool = by_id.get(edge.pool_id.as_str())?;
            let state = pool.v2.as_ref()?.swap.as_ref()?;
            if edge.protocol != v2_swap::PROFILE
                || pool.protocol != edge.protocol
                || pool.token_behavior != "standard"
                || pool.reserve0.0.is_zero()
                || pool.reserve1.0.is_zero()
                || state.raw_balance0 != pool.reserve0
                || state.raw_balance1 != pool.reserve1
                || !used.insert(edge.pool_id.as_str())
            {
                return None;
            }
            if directions
                .insert(edge.pool_id.as_str(), edge.zero_for_one)
                .is_some_and(|prior| prior != edge.zero_for_one)
            {
                return None;
            }
            let (reserve_in, reserve_out) = if edge.zero_for_one {
                (&pool.reserve0, &pool.reserve1)
            } else {
                (&pool.reserve1, &pool.reserve0)
            };
            output_rate *= &reserve_out.0 * 997u16;
            input_rate *= &reserve_in.0 * 1000u16;
        }
        if best
            .as_ref()
            .is_none_or(|(out, input)| &output_rate * input > out * &input_rate)
        {
            best = Some((output_rate, input_rate));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, to_value};

    fn pool() -> Pool {
        serde_json::from_value(json!({
            "chain_key": "evm:31337",
            "pool_id": "pool",
            "protocol": v2_swap::PROFILE,
            "adapter_version": v2_swap::ADAPTER,
            "token0": "sell",
            "token1": "buy",
            "reserve0": "2000",
            "reserve1": "3000",
            "fee_numerator": 3,
            "fee_denominator": 1000,
            "token_behavior": "standard",
            "v2": {
                "factory": "factory",
                "code_sha256": v2_swap::PAIR_CODE_SHA256,
                "swap": {
                    "raw_balance0": "2000",
                    "raw_balance1": "3000",
                    "block_timestamp_last": 99,
                    "price0_cumulative_last": "0",
                    "price1_cumulative_last": "0",
                    "unlocked": true,
                    "factory_code_sha256": v2_swap::FACTORY_CODE_SHA256,
                    "bank_block_number": 1,
                    "bank_block_hash": format!("0x{:064x}", 1),
                    "bank_unix_timestamp": 100
                }
            }
        }))
        .unwrap()
    }

    fn edge(pool: &Pool, zero_for_one: bool) -> Edge {
        Edge {
            from_token: None,
            protocol: pool.protocol.clone(),
            pool_id: pool.pool_id.clone(),
            zero_for_one,
            to: if zero_for_one {
                &pool.token1
            } else {
                &pool.token0
            }
            .clone(),
        }
    }

    #[test]
    fn bounds_round_outward_and_allow_same_direction_sharing() {
        let pools = [pool()];
        let forward = [edge(&pools[0], true)];
        let paths = [forward.as_slice(), forward.as_slice()];
        let quote_in = evaluate(
            &pools,
            paths,
            Objective::ExactIn {
                input: &"100".parse().unwrap(),
                selected_output: &"142".parse().unwrap(),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            to_value(quote_in).unwrap(),
            json!({
                "scope": "RETAINED_STANDARD_V2_PATHS_AT_SNAPSHOT",
                "objective": "GROSS_OUTPUT_UPPER_BOUND",
                "candidatePaths": 2,
                "limitAtoms": "149",
                "gapAtoms": "7",
            })
        );
        let quote_out = evaluate(
            &pools,
            paths,
            Objective::ExactOut {
                output: &"142".parse().unwrap(),
                selected_input: &"100".parse().unwrap(),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(quote_out.limit_atoms, "95");
        assert_eq!(quote_out.gap_atoms, "5");
    }

    #[test]
    fn reverse_shared_directions_and_unreconciled_reserves_disable_the_bound() {
        let mut pools = [pool()];
        let forward = [edge(&pools[0], true)];
        let reverse = [edge(&pools[0], false)];
        assert!(max_path_rate(&pools, [forward.as_slice(), reverse.as_slice()]).is_none());
        pools[0]
            .v2
            .as_mut()
            .unwrap()
            .swap
            .as_mut()
            .unwrap()
            .raw_balance0 = "2001".parse().unwrap();
        assert!(max_path_rate(&pools, [forward.as_slice()]).is_none());
    }
}
