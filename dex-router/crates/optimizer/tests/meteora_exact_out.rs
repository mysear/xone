use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_optimizer::{
    exact_out::{evaluate_allocation, optimize, ExactOutProposal},
    OptimizationLimits,
};
use xone_routing::{Edge, Funds};
const SENDER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
const RECIPIENT: &str = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/meteora-exact-out.json"
    ))
    .unwrap()
}
fn proposal(p: &Pool, s: &Value) -> ExactOutProposal {
    let z = s["zero_for_one"].as_bool().unwrap();
    ExactOutProposal {
        amount_out: s["target"].as_str().unwrap().parse().unwrap(),
        edges: vec![Edge {
            from_token: None,
            protocol: p.protocol.clone(),
            pool_id: p.pool_id.clone(),
            zero_for_one: z,
            to: if z {
                p.token1.clone()
            } else {
                p.token0.clone()
            },
        }],
    }
}
#[test]
fn meteora_ordered_output_allocations_reuse_original_bins_fees_and_vault_dust() {
    let f = data();
    let mut count = 0;
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["steps"].as_array().unwrap().len() == 3)
    {
        let p: Pool = serde_json::from_value(c["steps"][0]["pool"].clone()).unwrap();
        let proposals = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| proposal(&p, s))
            .collect::<Vec<_>>();
        let required = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                s["required_input"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
            })
            .sum::<u64>();
        let budget: Amount = required.to_string().parse().unwrap();
        let q = evaluate_allocation(
            std::slice::from_ref(&p),
            &proposals,
            &"1030000".parse().unwrap(),
            &budget,
            Funds {
                sender: SENDER,
                recipient: RECIPIENT,
            },
        )
        .unwrap();
        assert_eq!(q.required_input, budget);
        assert_eq!(q.target_output.to_string(), "1030000");
        assert!(q.gross_buy_amount.is_none());
        assert_eq!(q.unused_input_budget, Amount::zero());
        assert!(evaluate_allocation(
            &[p],
            &proposals,
            &"1030000".parse().unwrap(),
            &(required - 1).to_string().parse().unwrap(),
            Funds {
                sender: SENDER,
                recipient: RECIPIENT
            }
        )
        .is_err());
        count += 1;
    }
    assert_eq!(count, 10);
}
#[test]
fn meteora_optimization_reprices_stale_candidates_and_enforces_u64_input_budget() {
    let f = data();
    let s = &f["cases"][8]["steps"][0];
    let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
    let target: Amount = s["target"].as_str().unwrap().parse().unwrap();
    let budget: Amount = s["required_input"].as_str().unwrap().parse().unwrap();
    let prop = proposal(&p, s);
    let funds = Funds {
        sender: SENDER,
        recipient: RECIPIENT,
    };
    let mut state = std::collections::BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let mut path =
        xone_routing::exact_out::evaluate_path(&mut state, &prop.edges, &target, funds).unwrap();
    path.amount_in = "1".parse().unwrap();
    let q = optimize(
        std::slice::from_ref(&p),
        std::slice::from_ref(&path),
        &target,
        &budget,
        OptimizationLimits {
            parts: 8,
            max_splits: 4,
            max_evaluations: 1000,
        },
        funds,
        None,
    )
    .unwrap();
    assert_eq!(q.allocation.required_input, budget);
    assert_eq!(q.allocation.gross_output(), &target);
    assert_eq!(q.allocation.unused_input_budget, Amount::zero());
    assert!(optimize(
        &[p],
        &[path],
        &target,
        &"18446744073709551616".parse().unwrap(),
        OptimizationLimits {
            parts: 8,
            max_splits: 1,
            max_evaluations: 100
        },
        funds,
        None
    )
    .is_err());
}
