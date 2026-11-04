use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
#[test]
fn weighted_shared_pool_replays_both_directions_and_recalculates_candidates() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/weighted-quotes.json")).unwrap();
    for case in data["sequences"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let graph = Graph::build(std::slice::from_ref(&p)).unwrap();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let mut routes = Vec::new();
        for step in case["steps"].as_array().unwrap() {
            let zero = step["zero_for_one"].as_bool().unwrap();
            let (sell, buy) = if zero {
                (&p.token0, &p.token1)
            } else {
                (&p.token1, &p.token0)
            };
            let edge = graph.adjacency[sell][0].clone();
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let actual = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
            assert_eq!(
                actual.to_string(),
                step["expected"]["amount_out"].as_str().unwrap()
            );
            assert_eq!(
                state[&p.pool_id].reserve0.to_string(),
                step["expected"]["next_balances"][0].as_str().unwrap()
            );
            assert_eq!(
                state[&p.pool_id].reserve1.to_string(),
                step["expected"]["next_balances"][1].as_str().unwrap()
            );
            // The first two steps share sell/buy but use different input amounts. Later
            // steps reverse direction and are tested above as ordered physical state.
            if routes.len() < 2 {
                routes.push(AllocatedRoute {
                    path: PathQuote {
                        amount_in: Amount::zero(),
                        amount_out: Amount::zero(),
                        edges: vec![edge],
                        tokens: vec![sell.clone(), buy.clone()],
                    },
                    amount_in: input,
                    amount_out: Amount::zero(),
                });
            }
        }
        if case["steps"][0]["zero_for_one"] != case["steps"][1]["zero_for_one"] {
            continue;
        }
        let a = evaluate_allocation(std::slice::from_ref(&p), &routes, &Amount::zero()).unwrap();
        let steps = case["steps"].as_array().unwrap();
        let expected: Vec<Amount> = steps[..2]
            .iter()
            .map(|s| {
                s["expected"]["amount_out"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap()
            })
            .collect();
        for (actual, e) in a.routes.iter().zip(expected) {
            assert_eq!(actual.amount_out, e);
            assert_eq!(actual.path.amount_out, e);
            assert_eq!(actual.path.amount_in, actual.amount_in);
        }
    }
}
