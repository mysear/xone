use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{
    exact_out::{evaluate_allocation, optimize, ExactOutProposal},
    OptimizationLimits,
};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
const SENDER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
const RECIPIENT: &str = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/orca-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn edge(p: &Pool, zero: bool) -> Edge {
    Edge {
        from_token: None,
        protocol: p.protocol.clone(),
        pool_id: p.pool_id.clone(),
        zero_for_one: zero,
        to: if zero {
            p.token1.clone()
        } else {
            p.token0.clone()
        },
    }
}
#[test]
fn orca_two_pool_optimizer_matches_independent_native_allocation_grid() {
    let f = corpus();
    assert_eq!(f["pairs"].as_array().unwrap().len(), 3);
    for pair in f["pairs"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(pair["pools"].clone()).unwrap();
        let target: Amount = pair["target"].as_str().unwrap().parse().unwrap();
        let budget: Amount = pair["bestRequiredInput"].as_str().unwrap().parse().unwrap();
        let funds = Funds {
            sender: SENDER,
            recipient: RECIPIENT,
        };
        let mut paths = pools
            .iter()
            .map(|p| {
                let mut map = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
                evaluate_path(&mut map, &[edge(p, true)], &target, funds).unwrap()
            })
            .collect::<Vec<_>>();
        // Candidate hint amounts are not authority; optimizer must reprice each allocation.
        for p in &mut paths {
            p.amount_in = "1".parse().unwrap();
        }
        let q = optimize(
            &pools,
            &paths,
            &target,
            &budget,
            OptimizationLimits {
                parts: 4,
                max_splits: 2,
                max_evaluations: 1000,
            },
            funds,
            None,
        )
        .unwrap();
        assert_eq!(q.allocation.required_input, budget);
        assert_eq!(q.allocation.gross_output(), &target);
        assert!(q.grid_exhaustive && !q.budget_exhausted);
        for row in pair["grid"].as_array().unwrap() {
            let proposals = row["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| {
                    let p: Pool = serde_json::from_value(r["pool"].clone()).unwrap();
                    ExactOutProposal {
                        edges: vec![edge(&p, true)],
                        amount_out: r["target"].as_str().unwrap().parse().unwrap(),
                    }
                })
                .collect::<Vec<_>>();
            let expected: Amount = row["requiredInput"].as_str().unwrap().parse().unwrap();
            let full = evaluate_allocation(
                &pools,
                &proposals,
                &target,
                &"18446744073709551615".parse().unwrap(),
                funds,
            )
            .unwrap();
            assert_eq!(full.required_input, expected);
        }
        let too_small: Amount = (budget.0.clone() - 1u32).to_string().parse().unwrap();
        assert!(optimize(
            &pools,
            &paths,
            &target,
            &too_small,
            OptimizationLimits {
                parts: 4,
                max_splits: 2,
                max_evaluations: 1000
            },
            funds,
            None
        )
        .is_err());
    }
}
#[test]
fn orca_shared_pool_allocations_reprice_from_prior_native_exact_output_state() {
    let f = corpus();
    for seq in f["sequences"].as_array().unwrap() {
        let steps = seq["steps"].as_array().unwrap();
        let p: Pool = serde_json::from_value(steps[0]["pool"].clone()).unwrap();
        let selected = &steps[..2];
        let target = selected
            .iter()
            .map(|c| c["target"].as_str().unwrap().parse::<u64>().unwrap())
            .sum::<u64>();
        let input = selected
            .iter()
            .map(|c| c["requiredInput"].as_str().unwrap().parse::<u64>().unwrap())
            .sum::<u64>();
        let proposals = selected
            .iter()
            .map(|c| ExactOutProposal {
                edges: vec![edge(&p, true)],
                amount_out: c["target"].as_str().unwrap().parse().unwrap(),
            })
            .collect::<Vec<_>>();
        let q = evaluate_allocation(
            &[p],
            &proposals,
            &target.to_string().parse().unwrap(),
            &input.to_string().parse().unwrap(),
            Funds {
                sender: SENDER,
                recipient: RECIPIENT,
            },
        )
        .unwrap();
        assert_eq!(q.required_input.to_string(), input.to_string());
    }
}
