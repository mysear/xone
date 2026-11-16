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
            "/../../test/contracts/weighted-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn weighted_non_primary_pair_allocation_matches_original_given_out_grid() {
    let f = corpus();
    let funds = Funds {
        sender: "0x000000000000000000000000000000000000dead",
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for grid in f["grids"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(grid["pools"].clone()).unwrap();
        let target: Amount = grid["target"].as_str().unwrap().parse().unwrap();
        let budget: Amount = grid["bestRequiredInput"].as_str().unwrap().parse().unwrap();
        let edge = |p: &Pool| Edge {
            from_token: Some(p.multi_asset.as_ref().unwrap().assets[1].token.clone()),
            protocol: p.protocol.clone(),
            pool_id: p.pool_id.clone(),
            zero_for_one: false,
            to: p.multi_asset.as_ref().unwrap().assets[2].token.clone(),
        };
        let paths = pools
            .iter()
            .map(|p| {
                let mut m = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
                evaluate_path(&mut m, &[edge(p)], &target, funds).unwrap()
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
            )
            .unwrap();
            assert_eq!(a.required_input.to_string(), row["requiredInput"]);
        }
    }
}

#[test]
fn repeated_physical_weighted_pool_recomputes_each_ordered_target() {
    let f = corpus();
    let funds = Funds {
        sender: "0x000000000000000000000000000000000000dead",
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for c in f["ordered"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let s = p.multi_asset.as_ref().unwrap();
        let proposals = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| ExactOutProposal {
                edges: vec![Edge {
                    pool_id: p.pool_id.clone(),
                    protocol: p.protocol.clone(),
                    from_token: Some(s.assets[1].token.clone()),
                    zero_for_one: false,
                    to: s.assets[2].token.clone(),
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
            assert_eq!(r.amount_in.to_string(), step["expected"]["amount_in"]);
        }
        let low = Amount::checked(&budget.0 - 1u8).unwrap();
        assert!(evaluate_allocation(&[p], &proposals, &output, &low, funds).is_err());
    }
}
