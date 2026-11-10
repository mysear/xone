use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use xone_domain::{Amount, Pool, Snapshot};
use xone_optimizer::{
    evaluate_allocation, optimize_with_limits, AllocatedRoute, OptimizationExecution,
    OptimizationLimits,
};
use xone_routing::{Budget, Graph, PathQuote};

fn amount(value: u64) -> Amount {
    Amount::checked(value.into()).unwrap()
}

fn pool(id: &str, a: &str, b: &str, x: u64, y: u64) -> Pool {
    let mut p: Pool =
        serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()
            .pools[0]
            .clone();
    p.pool_id = id.into();
    p.token0 = a.into();
    p.token1 = b.into();
    p.reserve0 = amount(x);
    p.reserve1 = amount(y);
    p
}
fn candidates(pools: &[Pool], input: u64, parts: usize) -> Vec<PathQuote> {
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
fn optimize(
    pools: &[Pool],
    paths: &[PathQuote],
    input: u64,
    parts: usize,
    splits: usize,
    cap: usize,
    cost: u64,
) -> xone_optimizer::OptimizationResult {
    optimize_with_limits(
        pools,
        paths,
        &amount(input),
        &amount(cost),
        OptimizationLimits {
            parts,
            max_splits: splits,
            max_evaluations: cap,
        },
        OptimizationExecution {
            budget: None,
            funds: None,
        },
    )
    .unwrap()
}

// Independent small-integer CPMM execution: no routing, protocol-math or
// optimizer replay helpers. Physical reserves change across all route legs.
fn original_formula(
    pools: &[Pool],
    paths: &[PathQuote],
    input: u64,
    parts: usize,
    proposal: &[(usize, usize)],
    cost: u64,
) -> (u128, u128) {
    let mut reserves = pools
        .iter()
        .map(|p| {
            (
                p.pool_id.clone(),
                (
                    p.reserve0.to_string().parse::<u128>().unwrap(),
                    p.reserve1.to_string().parse::<u128>().unwrap(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut used = 0;
    let mut output = 0;
    for &(index, units) in proposal {
        let before = input as u128 * used as u128 / parts as u128;
        used += units;
        let mut amount = input as u128 * used as u128 / parts as u128 - before;
        if amount == 0 {
            return (0, 0);
        }
        for edge in &paths[index].edges {
            let (r0, r1) = reserves.get_mut(&edge.pool_id).unwrap();
            let (rin, rout) = if edge.zero_for_one {
                (r0, r1)
            } else {
                (r1, r0)
            };
            let weighted = amount * 997;
            let out = weighted * *rout / (*rin * 1000 + weighted);
            if out == 0 {
                return (0, 0);
            }
            *rin += amount;
            *rout -= out;
            amount = out;
        }
        output += amount;
    }
    (
        output.saturating_sub(cost as u128 * proposal.len() as u128),
        output,
    )
}
fn oracle(
    pools: &[Pool],
    paths: &[PathQuote],
    input: u64,
    parts: usize,
    splits: usize,
    cost: u64,
) -> (u128, u128) {
    // An ordered grid is enumerated using a different loop organization:
    // choose the next path and its ending boundary, finishing at the full sum.
    let mut pending = vec![(Vec::<(usize, usize)>::new(), 0)];
    let mut best = (0, 0);
    while let Some((prefix, used)) = pending.pop() {
        if used == parts {
            best = best.max(original_formula(pools, paths, input, parts, &prefix, cost));
            continue;
        }
        if prefix.len() == splits {
            continue;
        }
        for index in 0..paths.len() {
            if prefix.iter().any(|&(p, _)| p == index) {
                continue;
            }
            for end in used + 1..=parts {
                let mut next = prefix.clone();
                next.push((index, end - used));
                pending.push((next, end));
            }
        }
    }
    best
}

#[test]
fn three_and_four_way_splits_match_independent_integer_oracle_with_remainders_and_costs() {
    for n in [3, 4] {
        let pools = (0..n)
            .map(|i| {
                pool(
                    &format!("ac-{i}"),
                    "A",
                    "C",
                    1_000 + i * 120,
                    1_000 + i * 95,
                )
            })
            .collect::<Vec<_>>();
        let paths = candidates(&pools, 1_003, 8);
        for input in [101, 1_003] {
            for cost in [0, 7, 200] {
                let actual = optimize(&pools, &paths, input, 8, n as usize, 100_000, cost);
                assert!(actual.grid_exhaustive);
                assert!(!actual.budget_exhausted);
                let expected = oracle(&pools, &paths, input, 8, n as usize, cost);
                assert_eq!(
                    actual.allocation.net_buy_amount.to_string(),
                    expected.0.to_string()
                );
                assert_eq!(
                    actual.allocation.gross_buy_amount.to_string(),
                    expected.1.to_string()
                );
                assert_eq!(actual.allocation.sell_amount, amount(input));
                assert_eq!(actual.allocation.refund, Amount::zero());
            }
        }
    }
}

#[test]
fn three_routes_improve_over_any_two_and_share_actual_pool_post_state() {
    let pools = vec![
        pool("ab-0", "A", "B", 1_000, 1_000),
        pool("ab-1", "A", "B", 1_000, 1_000),
        pool("ab-2", "A", "B", 1_000, 1_000),
        pool("bc", "B", "C", 9_000, 9_000),
    ];
    let paths = candidates(&pools, 903, 9);
    let two = optimize(&pools, &paths, 903, 9, 2, 100_000, 0);
    let three = optimize(&pools, &paths, 903, 9, 3, 100_000, 0);
    assert!(three.allocation.gross_buy_amount > two.allocation.gross_buy_amount);
    assert_eq!(three.allocation.routes.len(), 3);
    assert_eq!(
        three.allocation.gross_buy_amount.to_string(),
        oracle(&pools, &paths, 903, 9, 3, 0).1.to_string()
    );
    let replay = evaluate_allocation(&pools, &three.allocation.routes, &Amount::zero()).unwrap();
    assert_eq!(replay.gross_buy_amount, three.allocation.gross_buy_amount);
}

#[test]
fn eight_disjoint_routes_are_solved_on_the_complete_grid_with_finite_work() {
    let pools = (0..8)
        .map(|i| pool(&format!("ac-{i}"), "A", "C", 1_000, 1_000))
        .collect::<Vec<_>>();
    let paths = candidates(&pools, 8_003, 16);
    let report = optimize(&pools, &paths, 8_003, 16, 8, 20_000, 0);
    assert!(!report.budget_exhausted);
    assert!(report.grid_exhaustive);
    assert!(report.evaluations < 300);
    assert_eq!(report.allocation.routes.len(), 8);
    assert_eq!(report.allocation.gross_buy_amount.to_string(), "3992");
    assert_eq!(report.allocation.sell_amount, amount(8_003));
    assert_eq!(
        report.allocation.gross_buy_amount,
        evaluate_allocation(&pools, &report.allocation.routes, &Amount::zero())
            .unwrap()
            .gross_buy_amount
    );
}

#[test]
fn cancellation_and_invalid_path_metadata_fail_without_panics() {
    let pools = vec![pool("ac", "A", "C", 1_000, 1_000)];
    let paths = candidates(&pools, 100, 4);
    let cancel = AtomicBool::new(true);
    let result = optimize_with_limits(
        &pools,
        &paths,
        &amount(100),
        &Amount::zero(),
        OptimizationLimits {
            parts: 4,
            max_splits: 8,
            max_evaluations: 100,
        },
        OptimizationExecution {
            budget: Some((&cancel, Instant::now() + Duration::from_secs(1))),
            funds: None,
        },
    );
    assert_eq!(result.err().unwrap().0, "BUDGET_EXHAUSTED");
    for kind in 0..3 {
        let mut path = paths[0].clone();
        if kind == 0 {
            path.tokens.clear();
        }
        if kind == 1 {
            path.tokens[0] = "FORGED".into();
        }
        if kind == 2 {
            path.edges.clear();
        }
        let routes = vec![AllocatedRoute {
            path,
            amount_in: amount(100),
            amount_out: Amount::zero(),
        }];
        assert!(evaluate_allocation(&pools, &routes, &Amount::zero()).is_err());
    }
    let duplicate = vec![pools[0].clone(), pools[0].clone()];
    let routes = vec![AllocatedRoute {
        path: paths[0].clone(),
        amount_in: amount(100),
        amount_out: Amount::zero(),
    }];
    assert_eq!(
        evaluate_allocation(&duplicate, &routes, &Amount::zero())
            .err()
            .unwrap()
            .0,
        "DUPLICATE_PHYSICAL_POOL"
    );
}

#[test]
fn shared_eight_way_scope_seeds_larger_allocations_before_pairs_spend_the_budget() {
    let mut pools = (0..8)
        .map(|i| pool(&format!("ab-{i}"), "A", "B", 1_000, 1_000))
        .collect::<Vec<_>>();
    pools.push(pool("bc", "B", "C", 90_000, 90_000));
    let paths = candidates(&pools, 8_003, 16);
    let report = optimize(&pools, &paths, 8_003, 16, 8, 1_000, 0);
    assert!(report.budget_exhausted);
    assert!(!report.grid_exhaustive);
    assert_eq!(report.evaluations, 1_000);
    assert!(report.allocation.routes.len() > 2);
    assert_eq!(report.allocation.sell_amount, amount(8_003));
    let two = optimize(&pools, &paths, 8_003, 16, 2, 1_000, 0);
    assert!(report.allocation.gross_buy_amount > two.allocation.gross_buy_amount);
    assert_eq!(
        report.allocation.gross_buy_amount,
        evaluate_allocation(&pools, &report.allocation.routes, &Amount::zero())
            .unwrap()
            .gross_buy_amount
    );
}

#[test]
fn split_size_ranking_keeps_a_small_pool_that_full_order_ranking_discards() {
    let pools = vec![
        pool("deep", "A", "C", 1_000_000, 1_000_000),
        pool("small", "A", "C", 10_000, 15_000),
        pool("medium", "A", "C", 100_000, 100_000),
    ];
    let full = candidates(&pools, 100_000, 20);
    let limited = xone_routing::search(
        &Graph::build(&pools).unwrap(),
        &pools,
        "A",
        "C",
        &amount(100_000),
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 2,
            max_expansions: 100,
        },
    )
    .unwrap();
    assert!(limited
        .candidates
        .iter()
        .any(|p| p.edges[0].pool_id == "small"));
    assert!(limited
        .candidates
        .iter()
        .any(|p| p.edges[0].pool_id == "deep"));
    assert!(limited.budget_exhausted);
    let selected = optimize(&pools, &limited.candidates, 100_000, 20, 2, 10_000, 0);
    let reference = oracle(&pools, &full, 100_000, 20, 2, 0);
    assert_eq!(
        selected.allocation.gross_buy_amount.to_string(),
        reference.1.to_string()
    );
}

#[test]
fn unfinished_capacity_search_does_not_claim_that_no_allocation_exists() {
    let first: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    let mut second = first.clone();
    second.pool_id = "0x0606060606060606060606060606060606060606".into();
    let pools = vec![first, second];
    let input: Amount = "60000000".parse().unwrap();
    let paths = xone_routing::search(
        &Graph::build(&pools).unwrap(),
        &pools,
        &pools[0].token0,
        &pools[0].token1,
        &input,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 100,
        },
    )
    .unwrap()
    .candidates;
    let error = optimize_with_limits(
        &pools,
        &paths,
        &input,
        &Amount::zero(),
        OptimizationLimits {
            parts: 20,
            max_splits: 2,
            max_evaluations: 1,
        },
        OptimizationExecution {
            budget: None,
            funds: None,
        },
    )
    .err()
    .unwrap();
    assert_eq!(error.0, "BUDGET_EXHAUSTED");
    let complete = optimize_with_limits(
        &pools,
        &paths,
        &input,
        &Amount::zero(),
        OptimizationLimits {
            parts: 20,
            max_splits: 2,
            max_evaluations: 1_000,
        },
        OptimizationExecution {
            budget: None,
            funds: None,
        },
    )
    .unwrap();
    assert_eq!(complete.allocation.routes.len(), 2);
    assert!(complete.grid_exhaustive);
}
