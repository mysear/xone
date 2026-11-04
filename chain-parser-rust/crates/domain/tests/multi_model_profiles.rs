use xone_domain::Pool;
#[test]
fn managed_observed_weights_and_composable_bpt_inventory_are_distinct_models() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let p: Pool = serde_json::from_value(d["pools"][0].clone()).unwrap();
    let mut managed = p.clone();
    managed.protocol = "balancer-v2-managed-snapshot".into();
    managed.adapter_version = "balancer-v2-managed-snapshot/no-bpt-fees-1".into();
    managed.multi_asset.as_mut().unwrap().weights[0] = "399999999999999999".parse().unwrap();
    managed.validate().unwrap();
    let mut old = managed.clone();
    old.protocol = p.protocol.clone();
    old.adapter_version = p.adapter_version.clone();
    assert!(old.validate().is_err());
    let mut composable = p.clone();
    composable.protocol = "balancer-v2-composable-regular".into();
    composable.adapter_version = "balancer-v2-composable-regular/no-rates-1".into();
    let s = composable.multi_asset.as_mut().unwrap();
    s.pool_id = format!("{}0000{}1", p.pool_id, "0".repeat(19));
    s.weights.clear();
    s.amplification = "100000".parse().unwrap();
    s.assets.insert(
        2,
        xone_domain::multi_asset::MultiAsset {
            token: p.pool_id.clone(),
            balance: "1000000000000000000000000".parse().unwrap(),
            scaling: "1000000000000000000".parse().unwrap(),
            code_sha256: s.code_sha256.clone(),
        },
    );
    composable.validate().unwrap();
    let w = snapshot_roundtrip(&composable);
    assert_eq!(w.multi_asset, composable.multi_asset);
}
fn snapshot_roundtrip(p: &Pool) -> Pool {
    let s = p.multi_asset.as_ref().unwrap();
    let back =
        xone_domain::multi_asset::MultiAssetState::from_proto(s.to_proto().unwrap()).unwrap();
    let mut p = p.clone();
    p.multi_asset = Some(back);
    p
}
