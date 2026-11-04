use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
#[test]
fn manager_currency_accounting_propagates_to_other_physical_pools_and_sequences() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-pools.json"
    ))
    .unwrap();
    let first = &d["banks"][0];
    let initial = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| serde_json::from_value::<Pool>(i["pool"].clone()).unwrap())
        .collect::<Vec<_>>();
    let mut state = initial
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let graph = Graph::build(&initial).unwrap();
    let mut routes = Vec::new();
    for trade in d["trades"].as_array().unwrap() {
        let raw = trade["pool_id"].as_str().unwrap();
        let p = state
            .values()
            .find(|p| p.infinity_bin.as_ref().unwrap().pool_id == raw)
            .unwrap()
            .clone();
        let left = trade["zero_for_one"].as_bool().unwrap();
        let (sell, buy) = if left {
            (&p.token0, &p.token1)
        } else {
            (&p.token1, &p.token0)
        };
        let edge = graph.adjacency[sell]
            .iter()
            .find(|e| e.pool_id == p.pool_id && &e.to == buy)
            .unwrap()
            .clone();
        let input: Amount = trade["amount_in"].as_str().unwrap().parse().unwrap();
        let q = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
        assert_eq!(q.to_string(), trade["expected"]["amount_out"]);
        routes.push(AllocatedRoute {
            path: PathQuote {
                amount_in: input.clone(),
                amount_out: q.clone(),
                edges: vec![edge],
                tokens: vec![sell.clone(), buy.clone()],
            },
            amount_in: input,
            amount_out: q,
        });
        let bank = d["banks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == trade["post_bank"])
            .unwrap();
        for item in bank["items"].as_array().unwrap() {
            let mut expected: Pool = serde_json::from_value(item["pool"].clone()).unwrap();
            let id = expected.pool_id.clone();
            let initial = initial
                .iter()
                .find(|p| p.pool_id == id)
                .unwrap()
                .infinity_bin
                .as_ref()
                .unwrap();
            let s = expected.infinity_bin.as_mut().unwrap();
            s.block_number = initial.block_number;
            s.timestamp = initial.timestamp;
            s.block_hash = initial.block_hash.clone();
            assert_eq!(state[&id], expected, "post bank {} pool {id}", trade["id"]);
        }
    }
    // Different directions cannot form one same-sell/buy allocation; two A->B pools do.
    let a = &routes[0];
    let b = &routes[2];
    let allocation =
        evaluate_allocation(&initial, &[a.clone(), b.clone()], &Amount::zero()).unwrap();
    assert!(allocation.gross_buy_amount > Amount::zero());
    let mut conflict = initial.clone();
    conflict[1].infinity_bin.as_mut().unwrap().app_reserve0 = "0".parse().unwrap();
    assert!(Graph::build(&conflict).is_err());
    assert!(evaluate_allocation(&conflict, std::slice::from_ref(a), &Amount::zero()).is_err());
}
