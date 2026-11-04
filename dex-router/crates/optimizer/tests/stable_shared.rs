use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Edge, Graph, PathQuote};

#[test]
fn stable_allocations_reuse_physical_post_state_against_executed_contracts() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/stable-allocations.json"
    ))
    .unwrap();
    let cases = golden["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let zero = case["zero_for_one"].as_bool().unwrap();
        let (sell, buy) = if zero {
            (p.token0.clone(), p.token1.clone())
        } else {
            (p.token1.clone(), p.token0.clone())
        };
        let graph = Graph::build(std::slice::from_ref(&p)).unwrap();
        let edge: Edge = graph.adjacency[&sell][0].clone();
        assert_eq!(edge.protocol, p.protocol);
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let mut routes = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let expected: Amount = step["amount_out"].as_str().unwrap().parse().unwrap();
            let actual = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
            assert_eq!(actual, expected, "{}", case["name"]);
            assert_eq!(
                state[&p.pool_id].reserve0.to_string(),
                step["next_balances"][0].as_str().unwrap()
            );
            assert_eq!(
                state[&p.pool_id].reserve1.to_string(),
                step["next_balances"][1].as_str().unwrap()
            );
            // Deliberately stale candidate amounts must be recalculated by allocation replay.
            routes.push(AllocatedRoute {
                path: PathQuote {
                    amount_in: Amount::zero(),
                    edges: vec![edge.clone()],
                    tokens: vec![sell.clone(), buy.clone()],
                    amount_out: Amount::zero(),
                },
                amount_in: input,
                amount_out: Amount::zero(),
            });
        }
        let allocation = evaluate_allocation(&[p], &routes, &Amount::zero()).unwrap();
        assert_eq!(
            allocation.sell_amount.to_string(),
            "40000000000000000000000"
        );
        assert_eq!(allocation.refund, Amount::zero());
        assert_eq!(
            allocation.gross_buy_amount.to_string(),
            case["gross_buy_amount"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        for (actual, step) in allocation
            .routes
            .iter()
            .zip(case["steps"].as_array().unwrap())
        {
            assert_eq!(
                actual.amount_out.to_string(),
                step["amount_out"].as_str().unwrap()
            );
            assert_eq!(actual.path.amount_out, actual.amount_out);
            assert_eq!(actual.path.amount_in, actual.amount_in);
        }
    }
}
