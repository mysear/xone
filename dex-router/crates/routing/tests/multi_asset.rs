use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{evaluate_path, search, Budget, Graph};
#[test]
fn every_asset_pair_uses_one_mutable_physical_pool() {
    let doc: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(doc["pools"][0].clone()).unwrap();
    let s = p.multi_asset.as_ref().unwrap();
    let graph = Graph::build(std::slice::from_ref(&p)).unwrap();
    assert_eq!(graph.adjacency.len(), 3);
    assert_eq!(graph.adjacency.values().map(Vec::len).sum::<usize>(), 6);
    let input: Amount = "1000000000000000000".parse().unwrap();
    let result = search(
        &graph,
        std::slice::from_ref(&p),
        &s.assets[2].token,
        &s.assets[0].token,
        &input,
        Budget {
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 100,
            split_parts: 1,
        },
    )
    .unwrap();
    assert_eq!(result.candidates.len(), 1);
    let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let q = evaluate_path(&mut state, &result.candidates[0].edges, &input).unwrap();
    assert!(q > Amount::zero());
    let next = state[&p.pool_id].multi_asset.as_ref().unwrap();
    assert_eq!(next.assets[2].balance.0, &s.assets[2].balance.0 + &input.0);
    assert_eq!(next.assets[0].balance.0, &s.assets[0].balance.0 - &q.0);
    assert_eq!(next.assets[1], s.assets[1]);
    assert_eq!(state[&p.pool_id].reserve0, next.assets[0].balance);
    let mut bad = result.candidates[0].edges.clone();
    bad[0].from_token = None;
    let original = state.clone();
    assert!(evaluate_path(&mut state, &bad, &input).is_err());
    assert_eq!(state, original);
}

#[test]
fn weighted_and_managed_capacity_failures_still_probe_smaller_allocations() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let base: Pool = serde_json::from_value(d["pools"][0].clone()).unwrap();
    for managed in [false, true] {
        let mut p = base.clone();
        if managed {
            p.protocol = "balancer-v2-managed-snapshot".into();
            p.adapter_version = "balancer-v2-managed-snapshot/no-bpt-fees-1".into();
        }
        let mut second = p.clone();
        second.pool_id = "0x0000000000000000000000000000000000000006".into();
        second.multi_asset.as_mut().unwrap().pool_id =
            format!("{}0001{}1", second.pool_id, "0".repeat(19));
        let pools = vec![p, second];
        let g = Graph::build(&pools).unwrap();
        let amount: Amount = "500000000000000000000000".parse().unwrap();
        let result = search(
            &g,
            &pools,
            &pools[0].token0,
            &pools[0].token1,
            &amount,
            Budget {
                max_hops: 1,
                max_candidates: 8,
                max_expansions: 100,
                split_parts: 2,
            },
        )
        .unwrap();
        assert_eq!(
            result.candidates.len(),
            2,
            "both partial-capacity pools must remain candidates"
        );
        assert!(result
            .candidates
            .iter()
            .all(|c| c.amount_in.0 == &amount.0 / 2u8));
    }
}

#[test]
fn ng_capacity_overflow_keeps_smaller_allocations_discoverable() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-state.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(d["cases"][0]["pool"].clone()).unwrap();
    let assets = &p.multi_asset.as_ref().unwrap().assets;
    let amount: Amount = "100000000000000000000000000000000".parse().unwrap();
    let full = xone_protocol_math::multi_asset::quote_exact_in_tokens(
        &p,
        &amount,
        &assets[0].token,
        &assets[1].token,
    );
    assert_eq!(full.unwrap_err().0, "CURVE_UINT256_OVERFLOW");
    let half = Amount::checked(&amount.0 / 2u8).unwrap();
    xone_protocol_math::multi_asset::quote_exact_in_tokens(
        &p,
        &half,
        &assets[0].token,
        &assets[1].token,
    )
    .unwrap();
    let graph = Graph::build(std::slice::from_ref(&p)).unwrap();
    let result = search(
        &graph,
        std::slice::from_ref(&p),
        &assets[0].token,
        &assets[1].token,
        &amount,
        Budget {
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 100,
            split_parts: 2,
        },
    )
    .unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].amount_in, half);
}
