use std::collections::BTreeMap;
use xone_domain::{curve_meta_state::BASE_POOL, Amount, Pool};
use xone_routing::{evaluate_path, Edge, Graph};
fn pools() -> Vec<Pool> {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-meta-service.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(d["cases"][0]["pool"].clone()).unwrap();
    let ng: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-state.json"
    ))
    .unwrap();
    let mut base: Pool = serde_json::from_value(ng["cases"][0]["pool"].clone()).unwrap();
    base.pool_id = BASE_POOL.into();
    vec![p, base]
}
#[test]
fn graph_and_recomputation_deny_mutable_virtual_price_dependency() {
    let p = pools();
    assert!(Graph::build(&p).is_err());
    let edge = Edge {
        from_token: Some(p[0].token0.clone()),
        protocol: p[0].protocol.clone(),
        pool_id: p[0].pool_id.clone(),
        zero_for_one: true,
        to: p[0].token1.clone(),
    };
    let mut bank = p
        .iter()
        .cloned()
        .map(|p| (p.pool_id.clone(), p))
        .collect::<BTreeMap<_, _>>();
    let before = bank.clone();
    let e = evaluate_path(&mut bank, &[edge], &"1".parse::<Amount>().unwrap()).unwrap_err();
    assert_eq!(e.0, "CURVE_META_MUTABLE_BASE_DEPENDENCY");
    assert_eq!(bank, before);
}
