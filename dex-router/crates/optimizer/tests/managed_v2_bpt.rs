//! Funds survive candidate search and allocation replay of one physical pool.
use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_optimizer::{
    evaluate_allocation, evaluate_allocation_with_funds, optimize_bounded_with_funds,
    AllocatedRoute, OptimizationExecution,
};
use xone_routing::{evaluate_path_with_funds, Funds, Graph, PathQuote};
#[test]
fn bpt_allocation_recomputes_real_shared_pool_with_explicit_sender_and_preserves_units() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-bpt-pools.json");
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let c = &v["cases"][0];
    let bank = v["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == c["before"])
        .unwrap();
    let pools: Vec<Pool> = bank["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| serde_json::from_value(e["pool"].clone()).unwrap())
        .collect();
    let p = &pools[0];
    let g = Graph::build(&pools).unwrap();
    let token = &p.multi_asset.as_ref().unwrap().assets[0].token;
    let edge = g.adjacency[token]
        .iter()
        .find(|e| e.to == p.pool_id && e.pool_id == p.pool_id)
        .unwrap()
        .clone();
    let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
    let half = Amount(&input.0 / 2u8);
    let proposal = PathQuote {
        amount_in: "1".parse().unwrap(),
        edges: vec![edge],
        tokens: vec![token.clone(), p.pool_id.clone()],
        amount_out: "1".parse().unwrap(),
    };
    let routes = vec![
        AllocatedRoute {
            path: proposal.clone(),
            amount_in: half.clone(),
            amount_out: "1".parse().unwrap(),
        },
        AllocatedRoute {
            path: proposal.clone(),
            amount_in: Amount(&input.0 - &half.0),
            amount_out: "1".parse().unwrap(),
        },
    ];
    let funds = Funds {
        sender: c["actor"].as_str().unwrap(),
        recipient: c["actor"].as_str().unwrap(),
    };
    assert!(evaluate_allocation(&pools, &routes, &Amount::zero()).is_err());
    let allocation =
        evaluate_allocation_with_funds(&pools, &routes, &Amount::zero(), funds).unwrap();
    assert_eq!(allocation.sell_amount, input);
    assert_eq!(allocation.refund, Amount::zero());
    let mut state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    let mut sum = num_bigint::BigUint::from(0u8);
    for (route, actual) in routes.iter().zip(&allocation.routes) {
        let out = evaluate_path_with_funds(&mut state, &route.path.edges, &route.amount_in, funds)
            .unwrap();
        assert_eq!(actual.amount_out, out);
        sum += out.0;
    }
    assert_eq!(allocation.gross_buy_amount, Amount(sum));
    let best = optimize_bounded_with_funds(
        &pools,
        &[proposal],
        &input,
        4,
        2,
        &Amount::zero(),
        OptimizationExecution {
            budget: None,
            funds: Some(funds),
        },
    )
    .unwrap();
    assert_eq!(best.sell_amount, input);
    assert!(best.gross_buy_amount > Amount::zero());
}
