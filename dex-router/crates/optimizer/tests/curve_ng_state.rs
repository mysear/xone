use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
#[test]
fn ng_shared_allocations_recompute_raw_admin_and_oracles_on_each_complete_poststate() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-state.json"
    ))
    .unwrap();
    let c = &d["cases"][0];
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let input: Amount = c["original"]["input"].as_str().unwrap().parse().unwrap();
    let g = Graph::build(std::slice::from_ref(&p)).unwrap();
    let edge = g.adjacency[&p.token0]
        .iter()
        .find(|e| e.to == p.token1)
        .unwrap()
        .clone();
    let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let mut routes = Vec::new();
    let mut outputs = Vec::new();
    for _ in 0..2 {
        let q = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
        outputs.push(q);
        routes.push(AllocatedRoute {
            path: PathQuote {
                amount_in: Amount::zero(),
                amount_out: Amount::zero(),
                edges: vec![edge.clone()],
                tokens: vec![p.token0.clone(), p.token1.clone()],
            },
            amount_in: input.clone(),
            amount_out: Amount::zero(),
        });
    }
    let allocation =
        evaluate_allocation(std::slice::from_ref(&p), &routes, &Amount::zero()).unwrap();
    assert_eq!(allocation.routes[0].amount_out, outputs[0]);
    assert_eq!(allocation.routes[1].amount_out, outputs[1]);
    assert_eq!(allocation.gross_buy_amount.0, &outputs[0].0 + &outputs[1].0);
    assert_eq!(allocation.refund, Amount::zero());
    let next = state[&p.pool_id].multi_asset.as_ref().unwrap();
    let old = p.multi_asset.as_ref().unwrap();
    assert!(
        next.curve_ng.as_ref().unwrap().admin_balances[1]
            > old.curve_ng.as_ref().unwrap().admin_balances[1]
    );
    assert_eq!(
        next.curve_ng.as_ref().unwrap().block_hash,
        old.curve_ng.as_ref().unwrap().block_hash
    );
    assert_eq!(
        p.multi_asset
            .as_ref()
            .unwrap()
            .curve_ng
            .as_ref()
            .unwrap()
            .stored_balances,
        c["pool"]["multi_asset"]["curve_ng"]["stored_balances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().parse::<Amount>().unwrap())
            .collect::<Vec<_>>()
    );
}
