use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_optimizer::exact_out::{evaluate_allocation, optimize, ExactOutProposal};
use xone_optimizer::OptimizationLimits;
use xone_routing::{Edge, Funds};
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-exact-out.json"
    ))
    .unwrap()
}
#[test]
fn slipstream_ordered_exact_output_allocations_recompute_reused_physical_pool() {
    let f = data();
    let funds = Funds {
        sender: f["deployment"]["sender"].as_str().unwrap(),
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for seq in f["sequences"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        let e = Edge {
            pool_id: p.pool_id.clone(),
            protocol: p.protocol.clone(),
            zero_for_one: seq["steps"][0]["zero_for_one"].as_bool().unwrap(),
            to: if seq["steps"][0]["zero_for_one"] == true {
                p.token1.clone()
            } else {
                p.token0.clone()
            },
            from_token: None,
        };
        let steps = seq["steps"].as_array().unwrap();
        let target = Amount(
            steps[..2]
                .iter()
                .map(|s| {
                    s["target_output"]
                        .as_str()
                        .unwrap()
                        .parse::<Amount>()
                        .unwrap()
                        .0
                })
                .sum(),
        );
        let max: Amount = "1000000000000000000000000".parse().unwrap();
        let proposals = steps[..2]
            .iter()
            .map(|step| ExactOutProposal {
                edges: vec![e.clone()],
                amount_out: step["target_output"].as_str().unwrap().parse().unwrap(),
            })
            .collect::<Vec<_>>();
        let a = evaluate_allocation(std::slice::from_ref(&p), &proposals, &target, &max, funds)
            .unwrap();
        let expected = steps[..2]
            .iter()
            .map(|step| {
                step["consumed_in"]
                    .as_str()
                    .unwrap()
                    .parse::<Amount>()
                    .unwrap()
                    .0
            })
            .sum::<num_bigint::BigUint>();
        assert_eq!(a.required_input.0, expected);
        assert_eq!(a.target_output, target);
        assert_eq!(a.routes.len(), 2);
        let first = &a.routes[0];
        let isolated = xone_protocol_math_indirect(&p, &e, &first.amount_out, funds);
        assert_eq!(isolated, first.amount_in);
        let state = std::collections::BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let mut trial = state.clone();
        let mut last = None;
        for proposal in &proposals {
            last = Some(
                xone_routing::exact_out::evaluate_path(
                    &mut trial,
                    &proposal.edges,
                    &proposal.amount_out,
                    funds,
                )
                .unwrap(),
            );
        }
        assert_eq!(last.unwrap().amount_in, a.routes[1].amount_in);
        assert_eq!(
            serde_json::to_value(trial.values().next().unwrap()).unwrap(),
            steps[1]["expected_pool"]
        );
        assert!(evaluate_allocation(
            std::slice::from_ref(&p),
            &proposals,
            &target,
            &Amount(&a.required_input.0 - 1u8),
            funds
        )
        .is_err());
        let candidates = vec![a.routes[0].clone()];
        let q = optimize(
            std::slice::from_ref(&p),
            &candidates,
            &a.routes[0].amount_out,
            &max,
            OptimizationLimits {
                parts: 4,
                max_splits: 1,
                max_evaluations: 100,
            },
            funds,
            None,
        )
        .unwrap();
        assert_eq!(q.allocation.required_input, a.routes[0].amount_in);
    }
}
fn xone_protocol_math_indirect(p: &Pool, e: &Edge, target: &Amount, funds: Funds<'_>) -> Amount {
    let mut state = std::collections::BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    xone_routing::exact_out::evaluate_path(&mut state, std::slice::from_ref(e), target, funds)
        .unwrap()
        .amount_in
}
