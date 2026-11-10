use std::{
    collections::BTreeMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use xone_domain::{Amount, Pool};
use xone_optimizer::{
    exact_out::{evaluate_allocation, optimize, ExactOutProposal},
    OptimizationLimits,
};
use xone_routing::{Budget, Edge, Funds, Graph, SearchExecution};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../test/contracts/v2-exact-out.json")).unwrap()
}
fn path_edges(pools: &[Pool], tokens: &serde_json::Value) -> Vec<Edge> {
    tokens
        .as_array()
        .unwrap()
        .windows(2)
        .map(|pair| {
            let from = pair[0].as_str().unwrap();
            let to = pair[1].as_str().unwrap();
            let p = pools
                .iter()
                .find(|p| {
                    (p.token0 == from && p.token1 == to) || (p.token1 == from && p.token0 == to)
                })
                .unwrap();
            Edge {
                pool_id: p.pool_id.clone(),
                protocol: p.protocol.clone(),
                zero_for_one: p.token0 == from,
                to: to.into(),
                from_token: None,
            }
        })
        .collect()
}
#[test]
fn ordered_grid_allocations_match_original_router_transactions_and_all_pair_ledgers() {
    let f = data();
    let g = &f["grid"];
    let pools: Vec<Pool> = serde_json::from_value(g["pools"].clone()).unwrap();
    let target: Amount = g["target_output"].as_str().unwrap().parse().unwrap();
    let max: Amount = g["max_input"].as_str().unwrap().parse().unwrap();
    let funds = Funds {
        sender: g["context"]["sender"].as_str().unwrap(),
        recipient: g["context"]["recipient"].as_str().unwrap(),
    };
    let (mut successful, mut failed) = (0, 0);
    for p in g["plans"].as_array().unwrap() {
        let proposals = p["proposal"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| ExactOutProposal {
                edges: path_edges(
                    &pools,
                    &g["paths"][step["path_index"].as_u64().unwrap() as usize],
                ),
                amount_out: step["amount_out"].as_str().unwrap().parse().unwrap(),
            })
            .collect::<Vec<_>>();
        let allocation = evaluate_allocation(&pools, &proposals, &target, &max, funds);
        if p["feasible"] == false {
            assert!(allocation.is_err(), "plan {}", p["id"]);
            failed += 1;
            continue;
        }
        let a = allocation.unwrap_or_else(|e| panic!("plan {}: {e}", p["id"]));
        assert_eq!(
            a.required_input.to_string(),
            p["required_input"].as_str().unwrap()
        );
        assert_eq!(a.target_output, target);
        let mut state = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        for route in &a.routes {
            let q = xone_routing::exact_out::evaluate_path(
                &mut state,
                &route.edges,
                &route.amount_out,
                funds,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(q).unwrap(),
                serde_json::to_value(route).unwrap()
            );
        }
        let expected: Vec<Pool> = serde_json::from_value(p["expected_pools"].clone()).unwrap();
        assert_eq!(
            state,
            expected
                .into_iter()
                .map(|p| (p.pool_id.clone(), p))
                .collect(),
            "plan {}",
            p["id"]
        );
        successful += 1;
    }
    assert_eq!(successful + failed, 39);
    assert!(successful > 0 && failed > 0);
}
#[test]
fn exact_out_solver_matches_exhaustive_original_grid_and_preserves_a_bounded_fallback() {
    let f = data();
    let g = &f["grid"];
    let pools: Vec<Pool> = serde_json::from_value(g["pools"].clone()).unwrap();
    let target: Amount = g["target_output"].as_str().unwrap().parse().unwrap();
    let max: Amount = g["max_input"].as_str().unwrap().parse().unwrap();
    let funds = Funds {
        sender: g["context"]["sender"].as_str().unwrap(),
        recipient: g["context"]["recipient"].as_str().unwrap(),
    };
    let graph = Graph::build(&pools).unwrap();
    let sell = f["identities"]["token0"]["address"].as_str().unwrap();
    let buy = f["identities"]["token2"]["address"].as_str().unwrap();
    let search = xone_routing::exact_out::search(
        &graph,
        &pools,
        sell,
        buy,
        &target,
        Budget {
            split_parts: 4,
            max_hops: 3,
            max_candidates: 16,
            max_expansions: 1000,
        },
        SearchExecution {
            control: None,
            funds: Some(funds),
        },
    )
    .unwrap();
    assert_eq!(search.candidates.len(), 3);
    let limits = OptimizationLimits {
        parts: 4,
        max_splits: 3,
        max_evaluations: 10000,
    };
    let result = optimize(
        &pools,
        &search.candidates,
        &target,
        &max,
        limits,
        funds,
        None,
    )
    .unwrap();
    assert_eq!(
        result.allocation.required_input.to_string(),
        g["best_required_input"].as_str().unwrap()
    );
    assert_eq!(result.allocation.target_output, target);
    assert!(result.grid_exhaustive);
    assert!(!result.budget_exhausted);
    assert_eq!(result.evaluations, 39);
    assert!(result.allocation.routes.len() > 1);
    let bounded = optimize(
        &pools,
        &search.candidates,
        &target,
        &max,
        OptimizationLimits {
            max_evaluations: 1,
            ..limits
        },
        funds,
        None,
    )
    .unwrap();
    assert_eq!(bounded.allocation.routes.len(), 1);
    assert_eq!(bounded.allocation.target_output, target);
    assert!(bounded.budget_exhausted);
    assert!(!bounded.grid_exhaustive);
    let too_small = Amount::checked(&result.allocation.required_input.0 - 1u8).unwrap();
    assert_eq!(
        optimize(
            &pools,
            &search.candidates,
            &target,
            &too_small,
            limits,
            funds,
            None
        )
        .unwrap_err()
        .0,
        "EXACT_OUT_MAX_INPUT"
    );
    let cancel = AtomicBool::new(true);
    assert_eq!(
        optimize(
            &pools,
            &search.candidates,
            &target,
            &max,
            limits,
            funds,
            Some((&cancel, Instant::now() + Duration::from_secs(1)))
        )
        .unwrap_err()
        .0,
        "BUDGET_EXHAUSTED"
    );
}
