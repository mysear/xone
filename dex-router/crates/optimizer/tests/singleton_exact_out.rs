use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{
    exact_out::{evaluate_allocation, optimize, ExactOutProposal},
    OptimizationLimits,
};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/singleton-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn singleton_capacity_allocation_matches_native_grid() {
    let f = corpus();
    let funds = Funds {
        sender: "0x000000000000000000000000000000000000dead",
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for grid in f["grids"].as_array().unwrap() {
        let zero = grid["zeroForOne"].as_bool().unwrap();
        let pools: Vec<Pool> = serde_json::from_value(grid["pools"].clone()).unwrap();
        let target: Amount = grid["target"].as_str().unwrap().parse().unwrap();
        let budget: Amount = grid["bestRequiredInput"].as_str().unwrap().parse().unwrap();
        let edge = |p: &Pool| Edge {
            from_token: None,
            protocol: p.protocol.clone(),
            pool_id: p.pool_id.clone(),
            zero_for_one: zero,
            to: if zero {
                p.token1.clone()
            } else {
                p.token0.clone()
            },
        };
        let paths = pools
            .iter()
            .map(|p| {
                let mut m = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
                evaluate_path(&mut m, &[edge(p)], &target, funds).unwrap_or_else(|_| {
                    let probe = Amount::checked(&target.0 / 4u8).unwrap();
                    let mut m = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
                    evaluate_path(&mut m, &[edge(p)], &probe, funds).unwrap()
                })
            })
            .collect::<Vec<_>>();
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
        for row in grid["rows"].as_array().unwrap() {
            let proposals = row["proposals"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
                    ExactOutProposal {
                        edges: vec![edge(&p)],
                        amount_out: c["target"].as_str().unwrap().parse().unwrap(),
                    }
                })
                .collect::<Vec<_>>();
            let a = evaluate_allocation(
                &pools,
                &proposals,
                &target,
                &"115792089237316195423570985008687907853269984665640564039457584007913129639935"
                    .parse()
                    .unwrap(),
                funds,
            );
            if row["passed"] == false {
                assert!(a.is_err());
                continue;
            }
            let a = a.unwrap();
            assert_eq!(a.required_input.to_string(), row["requiredInput"]);
        }
    }
}

#[test]
fn repeated_physical_singleton_pool_recomputes_each_ordered_target() {
    let f = corpus();
    let funds = Funds {
        sender: "0x000000000000000000000000000000000000dead",
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for c in f["ordered"].as_array().unwrap() {
        let zero = c["zeroForOne"].as_bool().unwrap();
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();

        let proposals = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| ExactOutProposal {
                edges: vec![Edge {
                    pool_id: p.pool_id.clone(),
                    protocol: p.protocol.clone(),
                    from_token: None,
                    zero_for_one: zero,
                    to: if zero {
                        p.token1.clone()
                    } else {
                        p.token0.clone()
                    },
                }],
                amount_out: step["target"].as_str().unwrap().parse().unwrap(),
            })
            .collect::<Vec<_>>();
        let output = c["target"].as_str().unwrap().parse().unwrap();
        let budget: Amount = c["requiredInput"].as_str().unwrap().parse().unwrap();
        let q = evaluate_allocation(
            std::slice::from_ref(&p),
            &proposals,
            &output,
            &budget,
            funds,
        )
        .unwrap();
        assert_eq!(q.required_input, budget);
        for (r, step) in q.routes.iter().zip(c["steps"].as_array().unwrap()) {
            assert_eq!(r.amount_in.to_string(), step["requiredInput"]);
        }
        let low = Amount::checked(&budget.0 - 1u8).unwrap();
        assert!(evaluate_allocation(&[p], &proposals, &output, &low, funds).is_err());
    }
}
