use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_optimizer::{
    exact_out::{evaluate_allocation, optimize, ExactOutProposal},
    OptimizationLimits,
};
use xone_routing::{Edge, Funds};
#[test]
fn orders_exact_out_optimizer_reprices_candidates_and_shared_pool_allocations() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-orders-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let funds = Funds {
        sender: "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso",
        recipient: "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE",
    };
    let mut sequences = 0;
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["steps"].as_array().unwrap().len() == 3)
    {
        let steps = c["steps"].as_array().unwrap();
        let p: Pool = serde_json::from_value(steps[0]["pool"].clone()).unwrap();
        let zero = c["zero_for_one"].as_bool().unwrap();
        let edge = Edge {
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
        let proposals = steps
            .iter()
            .map(|s| ExactOutProposal {
                edges: vec![edge.clone()],
                amount_out: s["amount"].as_str().unwrap().parse().unwrap(),
            })
            .collect::<Vec<_>>();
        let input = steps
            .iter()
            .map(|s| s["actual_input"].as_str().unwrap().parse::<u64>().unwrap())
            .sum::<u64>();
        let budget: Amount = input.to_string().parse().unwrap();
        let target: Amount = "2100".parse().unwrap();
        let q = evaluate_allocation(
            std::slice::from_ref(&p),
            &proposals,
            &target,
            &budget,
            funds,
        )
        .unwrap();
        assert_eq!(q.required_input, budget);
        assert_eq!(q.gross_output(), &target);
        assert!(q.gross_buy_amount.is_none());
        assert_eq!(q.unused_input_budget, Amount::zero());
        assert!(evaluate_allocation(
            std::slice::from_ref(&p),
            &proposals,
            &target,
            &(input - 1).to_string().parse().unwrap(),
            funds
        )
        .is_err());
        let first_target = &proposals[0].amount_out;
        let first_input: Amount = steps[0]["actual_input"].as_str().unwrap().parse().unwrap();
        let mut state = std::collections::BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let mut candidate =
            xone_routing::exact_out::evaluate_path(&mut state, &[edge], first_target, funds)
                .unwrap();
        candidate.amount_in = "1".parse().unwrap();
        let optimized = optimize(
            &[p],
            &[candidate],
            first_target,
            &first_input,
            OptimizationLimits {
                parts: 4,
                max_splits: 1,
                max_evaluations: 100,
            },
            funds,
            None,
        )
        .unwrap();
        assert_eq!(optimized.allocation.required_input, first_input);
        assert_eq!(optimized.allocation.gross_output(), first_target);
        sequences += 1;
    }
    assert_eq!(sequences, 16);
}
