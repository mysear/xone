use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_optimizer::exact_out::{evaluate_allocation, optimize, ExactOutProposal};
use xone_routing::{Budget, Edge, Funds, Graph, SearchExecution};
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/lfj-exact-out.json")).unwrap()
}
fn select(f: &Value, offset: i64) -> Pool {
    serde_json::from_value(
        f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["original"]["active_offset"] == offset
                    && c["original"]["dynamic"] == false
                    && c["original"]["oracle"] == false
                    && c["original"]["elapsed"] == 0
            })
            .unwrap()["pool"]
            .clone(),
    )
    .unwrap()
}
#[test]
fn lfj_minimum_input_scores_actual_surplus_and_keeps_target_conservation() {
    let f = data();
    let high = select(&f, 500);
    let mut flat = select(&f, 0);
    flat.pool_id = "0x000000000000000000000000000000000000abac".into();
    let pools = vec![high.clone(), flat];
    let funds = Funds {
        sender: f["deployment"]["sender"].as_str().unwrap(),
        recipient: f["deployment"]["recipient"].as_str().unwrap(),
    };
    let max: Amount = "1000000".parse().unwrap();
    for (target, expected_in, expected_out) in [("7", "4", "8"), ("1", "2", "2")] {
        let target: Amount = target.parse().unwrap();
        let mut paths = xone_routing::exact_out::search(
            &Graph::build(&pools).unwrap(),
            &pools,
            &high.token0,
            &high.token1,
            &target,
            Budget {
                max_hops: 1,
                max_candidates: 8,
                max_expansions: 100,
                split_parts: 4,
            },
            SearchExecution {
                control: None,
                funds: Some(funds),
            },
        )
        .unwrap()
        .candidates;
        paths.reverse();
        let q = optimize(
            &pools,
            &paths,
            &target,
            &max,
            xone_optimizer::OptimizationLimits {
                parts: 4,
                max_splits: 2,
                max_evaluations: 1000,
            },
            funds,
            None,
        )
        .unwrap();
        assert!(q.grid_exhaustive);
        assert_eq!(q.allocation.required_input.to_string(), expected_in);
        assert_eq!(q.allocation.gross_output().to_string(), expected_out);
        assert_eq!(q.allocation.target_output, target);
        assert_eq!(q.allocation.routes.len(), 1);
        assert_eq!(q.allocation.routes[0].edges[0].pool_id, high.pool_id);
    }
}
#[test]
fn lfj_ordered_targets_recompute_shared_bins_and_preserve_every_actual_output() {
    let f = data();
    let funds = Funds {
        sender: f["deployment"]["sender"].as_str().unwrap(),
        recipient: f["deployment"]["recipient"].as_str().unwrap(),
    };
    let max: Amount = "1000000000".parse().unwrap();
    for seq in f["sequences"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        let steps = seq["steps"].as_array().unwrap();
        let zero = steps[0]["zero_for_one"].as_bool().unwrap();
        let e = Edge {
            pool_id: p.pool_id.clone(),
            protocol: p.protocol.clone(),
            zero_for_one: zero,
            to: if zero {
                p.token1.clone()
            } else {
                p.token0.clone()
            },
            from_token: None,
        };
        let proposals = steps
            .iter()
            .map(|s| ExactOutProposal {
                edges: vec![e.clone()],
                amount_out: s["target_output"].as_str().unwrap().parse().unwrap(),
            })
            .collect::<Vec<_>>();
        let target = Amount(proposals.iter().map(|p| p.amount_out.0.clone()).sum());
        let q = evaluate_allocation(&[p], &proposals, &target, &max, funds).unwrap();
        let input = Amount(
            steps
                .iter()
                .map(|s| {
                    s["required_input"]
                        .as_str()
                        .unwrap()
                        .parse::<Amount>()
                        .unwrap()
                        .0
                })
                .sum(),
        );
        let actual = Amount(
            steps
                .iter()
                .map(|s| {
                    s["actual_output"]
                        .as_str()
                        .unwrap()
                        .parse::<Amount>()
                        .unwrap()
                        .0
                })
                .sum(),
        );
        assert_eq!(q.required_input, input);
        assert_eq!(q.gross_output(), &actual);
        assert_eq!(q.target_output, target);
    }
}
