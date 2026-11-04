use std::collections::BTreeMap;
use xone_domain::{curve_meta_state::BASE_POOL, Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Edge, PathQuote};
fn pool() -> Pool {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-meta-service.json"
    ))
    .unwrap();
    serde_json::from_value(d["cases"][24]["pool"].clone()).unwrap()
}
#[test]
fn shared_meta_swaps_recompute_entire_raw_admin_state_and_keep_base_readonly() {
    let p = pool();
    let edge = Edge {
        from_token: Some(p.token0.clone()),
        protocol: p.protocol.clone(),
        pool_id: p.pool_id.clone(),
        zero_for_one: true,
        to: p.token1.clone(),
    };
    let input: Amount = "1000000000000000000000".parse().unwrap();
    let path = PathQuote {
        amount_in: input.clone(),
        amount_out: Amount::zero(),
        edges: vec![edge.clone()],
        tokens: vec![p.token0.clone(), p.token1.clone()],
    };
    let routes = vec![
        AllocatedRoute {
            path: path.clone(),
            amount_in: input.clone(),
            amount_out: Amount::zero(),
        },
        AllocatedRoute {
            path,
            amount_in: input.clone(),
            amount_out: Amount::zero(),
        },
    ];
    let q = evaluate_allocation(std::slice::from_ref(&p), &routes, &Amount::zero()).unwrap();
    let mut bank = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let first = evaluate_path(&mut bank, std::slice::from_ref(&edge), &input).unwrap();
    let before = bank[&p.pool_id]
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_meta
        .clone();
    let second = evaluate_path(&mut bank, &[edge], &input).unwrap();
    assert_eq!(q.gross_buy_amount.0, &first.0 + &second.0);
    let after = bank[&p.pool_id]
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_meta
        .as_ref()
        .unwrap();
    assert_ne!(after.admin_balances, before.unwrap().admin_balances);
    assert_eq!(
        after.base_virtual_price,
        p.multi_asset
            .as_ref()
            .unwrap()
            .curve_meta
            .as_ref()
            .unwrap()
            .base_virtual_price
    );
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-state.json"
    ))
    .unwrap();
    let mut base: Pool = serde_json::from_value(d["cases"][0]["pool"].clone()).unwrap();
    base.pool_id = BASE_POOL.into();
    let err = evaluate_allocation(&[p, base], &routes, &Amount::zero()).unwrap_err();
    assert_eq!(err.0, "CURVE_META_MUTABLE_BASE_DEPENDENCY");
}
