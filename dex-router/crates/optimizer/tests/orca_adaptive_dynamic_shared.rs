use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
#[test]
fn carried_dynamic_mixed_arrays_rewards_and_oracle_are_reused_by_shared_allocations() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    for prefix in [
        "pool-0-a-to-b-continuous",
        "pool-0-b-to-a-continuous",
        "pool-1-a-to-b-continuous",
        "pool-1-b-to-a-continuous",
    ] {
        let first = cases
            .iter()
            .find(|c| c["name"] == format!("{prefix}/0"))
            .unwrap();
        let second = cases
            .iter()
            .find(|c| c["name"] == format!("{prefix}/1"))
            .unwrap();
        let p: Pool = serde_json::from_value(first["pool"].clone()).unwrap();
        let left = first["params"]["a_to_b"].as_bool().unwrap();
        let (sell, buy) = if left {
            (&p.token0, &p.token1)
        } else {
            (&p.token1, &p.token0)
        };
        let g = Graph::build(std::slice::from_ref(&p)).unwrap();
        let edge = g.adjacency[sell]
            .iter()
            .find(|e| &e.to == buy)
            .unwrap()
            .clone();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let mut routes = Vec::new();
        for c in [first, second] {
            let input: Amount = c["params"]["amount"].as_str().unwrap().parse().unwrap();
            let out = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
            assert_eq!(out.to_string(), c["expected"]["amount_out"]);
            routes.push(AllocatedRoute {
                path: PathQuote {
                    amount_in: Amount::zero(),
                    amount_out: Amount::zero(),
                    edges: vec![edge.clone()],
                    tokens: vec![sell.clone(), buy.clone()],
                },
                amount_in: input,
                amount_out: out,
            });
        }
        let mut expected: Pool =
            serde_json::from_value(second["expected"]["post_pool"].clone()).unwrap();
        expected.orca_whirlpool.as_mut().unwrap().bank_slot =
            p.orca_whirlpool.as_ref().unwrap().bank_slot;
        assert_eq!(state[&p.pool_id], expected);
        let allocation =
            evaluate_allocation(std::slice::from_ref(&p), &routes, &Amount::zero()).unwrap();
        assert_eq!(allocation.routes[0].amount_out, routes[0].amount_out);
        assert_eq!(allocation.routes[1].amount_out, routes[1].amount_out);
        assert_eq!(
            allocation.gross_buy_amount.0,
            &routes[0].amount_out.0 + &routes[1].amount_out.0
        );
        assert_eq!(allocation.refund, Amount::zero());
        assert_eq!(
            p,
            serde_json::from_value::<Pool>(first["pool"].clone()).unwrap()
        );
    }
}
