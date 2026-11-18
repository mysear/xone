use num_bigint::BigUint;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use xone_domain::{err, Amount, Pool, Snapshot};
use xone_optimizer::{Allocation, OptimizationExecution, OptimizationLimits};
use xone_routing::{Budget, Graph, PathQuote};

fn amount(n: u128) -> Amount {
    Amount::checked(n.into()).unwrap()
}
fn limits(parts: usize, splits: usize, evaluations: usize) -> OptimizationLimits {
    OptimizationLimits {
        parts,
        max_splits: splits,
        max_evaluations: evaluations,
    }
}
fn pool(id: &str, from: &str, to: &str, x: u128, y: u128) -> Pool {
    let mut p =
        serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()
            .pools[0]
            .clone();
    p.pool_id = id.into();
    p.token0 = from.into();
    p.token1 = to.into();
    p.reserve0 = amount(x);
    p.reserve1 = amount(y);
    p
}
fn paths(pools: &[Pool], input: u128, parts: usize) -> Vec<PathQuote> {
    xone_routing::search(
        &Graph::build(pools).unwrap(),
        pools,
        "A",
        "C",
        &amount(input),
        Budget {
            split_parts: parts,
            max_hops: 3,
            max_candidates: 32,
            max_expansions: 10_000,
        },
    )
    .unwrap()
    .candidates
}
fn execution() -> OptimizationExecution<'static> {
    OptimizationExecution {
        budget: None,
        funds: None,
    }
}
fn number(a: &Amount) -> u128 {
    a.to_string().parse().unwrap()
}
fn allocation_fee(a: &Allocation) -> u128 {
    a.routes
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (i as u128 + 1) * (r.path.edges[0].pool_id.as_bytes()[0] as u128 % 7 + 1) * 19
                + number(&r.amount_in) % 11
        })
        .sum()
}

// Independent small-integer constant-product swaps. All physical reserves are
// replayed in proposal order, without routing/math/optimizer evaluation helpers.
fn original(
    pools: &[Pool],
    paths: &[PathQuote],
    input: u128,
    parts: usize,
    proposal: &[(usize, usize)],
) -> (i128, u128) {
    let mut state = pools
        .iter()
        .map(|p| {
            (
                p.pool_id.clone(),
                (number(&p.reserve0), number(&p.reserve1)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let (mut used, mut total, mut fee) = (0, 0u128, 0u128);
    for (position, &(index, units)) in proposal.iter().enumerate() {
        let before = input * used as u128 / parts as u128;
        used += units;
        let mut n = input * used as u128 / parts as u128 - before;
        fee += (position as u128 + 1)
            * (paths[index].edges[0].pool_id.as_bytes()[0] as u128 % 7 + 1)
            * 19
            + n % 11;
        for edge in &paths[index].edges {
            let (a, b) = state.get_mut(&edge.pool_id).unwrap();
            let (rin, rout) = if edge.zero_for_one { (a, b) } else { (b, a) };
            let weighted = n * 997;
            let output = weighted * *rout / (*rin * 1000 + weighted);
            *rin += n;
            *rout -= output;
            n = output;
        }
        total += n;
    }
    (total as i128 - fee as i128, total)
}
fn grid(count: usize, parts: usize, splits: usize) -> Vec<Vec<(usize, usize)>> {
    fn visit(
        count: usize,
        parts: usize,
        splits: usize,
        used: usize,
        prefix: &mut Vec<(usize, usize)>,
        out: &mut Vec<Vec<(usize, usize)>>,
    ) {
        if used == parts {
            out.push(prefix.clone());
            return;
        }
        if prefix.len() == splits {
            return;
        }
        for next in 0..count {
            if prefix.iter().any(|(i, _)| *i == next) {
                continue;
            }
            for end in used + 1..=parts {
                prefix.push((next, end - used));
                visit(count, parts, splits, end, prefix, out);
                prefix.pop();
            }
        }
    }
    let mut out = vec![];
    visit(count, parts, splits, 0, &mut vec![], &mut out);
    out
}

#[test]
fn candidate_costs_match_independent_ordered_integer_grid_including_shared_state() {
    let scenarios = [
        vec![
            pool("a", "A", "C", 1000, 3000),
            pool("b", "A", "C", 1200, 2700),
            pool("c", "A", "C", 800, 1900),
        ],
        vec![
            pool("a", "A", "B", 3000, 6000),
            pool("b", "B", "C", 4000, 5000),
            pool("c", "A", "C", 1100, 2100),
            pool("d", "A", "B", 2800, 5700),
        ],
    ];
    for pools in scenarios {
        let candidates = paths(&pools, 103, 4);
        let proposals = grid(candidates.len(), 4, 3);
        let expected = proposals
            .iter()
            .map(|p| original(&pools, &candidates, 103, 4, p))
            .max()
            .unwrap();
        let mut calls = 0;
        let result = xone_optimizer::optimize_with_costs(
            &pools,
            &candidates,
            &amount(103),
            limits(4, 3, 100_000),
            execution(),
            &mut |a| {
                calls += 1;
                Ok(Some(amount(allocation_fee(a))))
            },
        )
        .unwrap();
        assert!(result.optimization.grid_exhaustive);
        assert!(!result.optimization.budget_exhausted);
        assert_eq!(calls, proposals.len());
        assert_eq!(result.ranking.evaluated_candidates, calls);
        assert_eq!(result.ranking.priced_candidates, calls);
        assert_eq!(result.ranking.unavailable_candidates, 0);
        assert!(result.ranking.cost_adjustment_applied);
        assert_eq!(
            result.ranking.signed_objective_token_units,
            Some(expected.0.to_string())
        );
        assert_eq!(
            number(&result.optimization.allocation.gross_buy_amount),
            expected.1
        );
        assert_eq!(number(&result.optimization.allocation.sell_amount), 103);
        assert_eq!(result.optimization.allocation.refund, Amount::zero());
    }
}

#[test]
fn negative_candidate_scores_are_not_clamped_or_ranked_by_gross_output() {
    let pools = vec![
        pool("a", "A", "C", 1000, 3000),
        pool("b", "A", "C", 1000, 2000),
    ];
    let candidates = paths(&pools, 100, 1);
    let result = xone_optimizer::optimize_with_costs(
        &pools,
        &candidates,
        &amount(100),
        limits(1, 1, 100),
        execution(),
        &mut |a| {
            let penalty = if a.routes[0].path.edges[0].pool_id == "b" {
                1
            } else {
                2
            };
            Ok(Some(amount(number(&a.gross_buy_amount) + penalty)))
        },
    )
    .unwrap();
    assert_eq!(
        result.optimization.allocation.routes[0].path.edges[0].pool_id,
        "b"
    );
    assert_eq!(
        result.ranking.signed_objective_token_units.as_deref(),
        Some("-1")
    );
    assert_eq!(
        result.ranking.objective,
        "MAXIMUM_OUTPUT_MINUS_CANDIDATE_COST"
    );
}

#[test]
fn one_missing_candidate_cost_restores_raw_best_for_the_same_evaluated_set() {
    let pools = vec![
        pool("a", "A", "C", 1000, 3000),
        pool("b", "A", "C", 1000, 2000),
    ];
    let mut candidates = paths(&pools, 100, 1);
    for unknown in ["a", "b"] {
        for _ in 0..2 {
            candidates.reverse();
            let result = xone_optimizer::optimize_with_costs(
                &pools,
                &candidates,
                &amount(100),
                limits(1, 1, 100),
                execution(),
                &mut |a| {
                    Ok(if a.routes[0].path.edges[0].pool_id == unknown {
                        None
                    } else {
                        Some(amount(1000))
                    })
                },
            )
            .unwrap();
            assert_eq!(
                result.optimization.allocation.routes[0].path.edges[0].pool_id,
                "a"
            );
            assert!(!result.ranking.cost_adjustment_applied);
            assert_eq!(result.ranking.unavailable_candidates, 1);
            assert_eq!(result.ranking.objective, "MAXIMUM_GROSS_BUY_AMOUNT");
            assert!(result.ranking.signed_objective_token_units.is_none());
            assert!(result.ranking.selected_cost_token_units.is_none());
        }
    }
}

#[test]
fn candidate_cost_errors_width_budget_and_cancellation_are_not_silenced() {
    let pools = vec![
        pool("a", "A", "C", 1000, 3000),
        pool("b", "A", "C", 1000, 2000),
    ];
    let candidates = paths(&pools, 100, 4);
    let error = xone_optimizer::optimize_with_costs(
        &pools,
        &candidates,
        &amount(100),
        limits(4, 2, 100),
        execution(),
        &mut |_| Err(err("COST_SOURCE_FAILED")),
    )
    .err()
    .unwrap();
    assert_eq!(error.0, "COST_SOURCE_FAILED");
    let error = xone_optimizer::optimize_with_costs(
        &pools,
        &candidates,
        &amount(100),
        limits(4, 2, 100),
        execution(),
        &mut |_| Ok(Some(Amount(BigUint::from(1u8) << 256usize))),
    )
    .err()
    .unwrap();
    assert_eq!(error.0, "AMOUNT_OVERFLOW");
    let result = xone_optimizer::optimize_with_costs(
        &pools,
        &candidates,
        &amount(100),
        limits(4, 2, 1),
        execution(),
        &mut |_| Ok(Some(amount(3))),
    )
    .unwrap();
    assert!(result.optimization.budget_exhausted);
    assert!(!result.optimization.grid_exhaustive);
    assert_eq!(result.ranking.evaluated_candidates, 1);
    let cancel = AtomicBool::new(false);
    let error = xone_optimizer::optimize_with_costs(
        &pools,
        &candidates,
        &amount(100),
        limits(4, 2, 100),
        OptimizationExecution {
            budget: Some((&cancel, Instant::now() + Duration::from_secs(3))),
            funds: None,
        },
        &mut |_| {
            cancel.store(true, Ordering::Relaxed);
            Ok(None)
        },
    )
    .err()
    .unwrap();
    assert_eq!(error.0, "BUDGET_EXHAUSTED");
    let mut calls = 0;
    let error = xone_optimizer::optimize_with_costs(
        &pools,
        &candidates,
        &amount(100),
        limits(4, 2, 100),
        OptimizationExecution {
            budget: Some((
                &AtomicBool::new(false),
                Instant::now() - Duration::from_secs(1),
            )),
            funds: None,
        },
        &mut |_| {
            calls += 1;
            Ok(Some(amount(0)))
        },
    )
    .err()
    .unwrap();
    assert_eq!(error.0, "BUDGET_EXHAUSTED");
    assert_eq!(calls, 0);
}

fn exact_data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/v2-exact-out.json")).unwrap()
}
fn exact_fee(a: &xone_optimizer::exact_out::ExactOutAllocation, data: &Value) -> u128 {
    a.routes
        .iter()
        .enumerate()
        .map(|(position, r)| {
            let index = data["grid"]["paths"]
                .as_array()
                .unwrap()
                .iter()
                .position(|p| serde_json::to_value(&r.tokens).unwrap() == *p)
                .unwrap();
            (position as u128 + 1) * (index as u128 + 1) * 100_000_000_000_000_000_000_000
                + number(&r.amount_out) % 17
        })
        .sum()
}

#[test]
fn exact_out_costs_match_all_original_router_grid_results_without_inverting_exact_in() {
    let data = exact_data();
    let g = &data["grid"];
    let pools: Vec<Pool> = serde_json::from_value(g["pools"].clone()).unwrap();
    let target: Amount = g["target_output"].as_str().unwrap().parse().unwrap();
    let max: Amount = g["max_input"].as_str().unwrap().parse().unwrap();
    let funds = xone_routing::Funds {
        sender: g["context"]["sender"].as_str().unwrap(),
        recipient: g["context"]["recipient"].as_str().unwrap(),
    };
    let paths = xone_routing::exact_out::search(
        &Graph::build(&pools).unwrap(),
        &pools,
        data["identities"]["token0"]["address"].as_str().unwrap(),
        data["identities"]["token2"]["address"].as_str().unwrap(),
        &target,
        Budget {
            split_parts: 4,
            max_hops: 3,
            max_candidates: 32,
            max_expansions: 10_000,
        },
        xone_routing::SearchExecution {
            control: None,
            funds: Some(funds),
        },
    )
    .unwrap()
    .candidates;
    let originals = g["plans"].as_array().unwrap();
    assert_eq!(originals.len(), 39);
    let expected = originals
        .iter()
        .filter(|p| p["feasible"] == true)
        .map(|p| {
            let fee: u128 = p["proposal"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(position, r)| {
                    (position as u128 + 1)
                        * (r["path_index"].as_u64().unwrap() as u128 + 1)
                        * 100_000_000_000_000_000_000_000
                        + r["amount_out"].as_str().unwrap().parse::<u128>().unwrap() % 17
                })
                .sum();
            let input = p["required_input"]
                .as_str()
                .unwrap()
                .parse::<u128>()
                .unwrap();
            (input + fee, input)
        })
        .min()
        .unwrap();
    let result = xone_optimizer::exact_out::optimize_with_costs(
        &pools,
        &paths,
        &target,
        &max,
        limits(4, 3, 100_000),
        OptimizationExecution {
            budget: None,
            funds: Some(funds),
        },
        &mut |a| Ok(Some(amount(exact_fee(a, &data)))),
    )
    .unwrap();
    assert!(result.optimization.grid_exhaustive);
    assert_eq!(result.ranking.evaluated_candidates, 38);
    assert!(result.ranking.cost_adjustment_applied);
    assert_eq!(
        result.ranking.signed_objective_token_units,
        Some(expected.0.to_string())
    );
    assert_eq!(
        number(&result.optimization.allocation.required_input),
        expected.1
    );
    assert_ne!(
        result.optimization.allocation.required_input.to_string(),
        g["best_required_input"].as_str().unwrap()
    );
    assert_eq!(result.optimization.allocation.target_output, target);
    assert_eq!(
        &result.optimization.allocation.required_input.0
            + &result.optimization.allocation.unused_input_budget.0,
        max.0
    );
    let mut calls = 0;
    let fallback = xone_optimizer::exact_out::optimize_with_costs(
        &pools,
        &paths,
        &target,
        &max,
        limits(4, 3, 100_000),
        OptimizationExecution {
            budget: None,
            funds: Some(funds),
        },
        &mut |a| {
            calls += 1;
            Ok(if calls == 38 {
                None
            } else {
                Some(amount(exact_fee(a, &data)))
            })
        },
    )
    .unwrap();
    assert_eq!(
        fallback.optimization.allocation.required_input.to_string(),
        g["best_required_input"].as_str().unwrap()
    );
    assert!(!fallback.ranking.cost_adjustment_applied);
    assert_eq!(fallback.ranking.objective, "MINIMUM_REQUIRED_SELL_AMOUNT");
    let maximum = Amount((BigUint::from(1u8) << 256usize) - BigUint::from(1u8));
    let wide = xone_optimizer::exact_out::optimize_with_costs(
        &pools,
        &paths,
        &target,
        &max,
        limits(4, 3, 100_000),
        OptimizationExecution {
            budget: None,
            funds: Some(funds),
        },
        &mut |_| Ok(Some(maximum.clone())),
    )
    .unwrap();
    assert_eq!(
        wide.ranking.signed_objective_token_units,
        Some((&maximum.0 + &wide.optimization.allocation.required_input.0).to_string())
    );
    let failure = xone_optimizer::exact_out::optimize_with_costs(
        &pools,
        &paths,
        &target,
        &max,
        limits(4, 3, 100_000),
        OptimizationExecution {
            budget: None,
            funds: Some(funds),
        },
        &mut |_| Err(err("COST_SOURCE_FAILED")),
    )
    .err()
    .unwrap();
    assert_eq!(failure.0, "COST_SOURCE_FAILED");
    let cancel = AtomicBool::new(false);
    let cancelled = xone_optimizer::exact_out::optimize_with_costs(
        &pools,
        &paths,
        &target,
        &max,
        limits(4, 3, 100_000),
        OptimizationExecution {
            budget: Some((&cancel, Instant::now() + Duration::from_secs(3))),
            funds: Some(funds),
        },
        &mut |_| {
            cancel.store(true, Ordering::Relaxed);
            Ok(Some(amount(0)))
        },
    )
    .err()
    .unwrap();
    assert_eq!(cancelled.0, "BUDGET_EXHAUSTED");
}
