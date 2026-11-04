use xone_domain::{Amount, Pool};
#[test]
fn a_multicoin_pool_requires_one_typed_physical_state_and_all_asset_balances() {
    let raw: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let raw = raw["pools"][0].clone();
    let p: Pool = serde_json::from_value(raw).unwrap();
    p.validate().unwrap();
    assert_eq!(p.multi_asset.as_ref().unwrap().assets.len(), 3);
    let mut bad = p.clone();
    bad.reserve0 = Amount::zero();
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.multi_asset.as_mut().unwrap().weights[0] = "500000000000000000".parse().unwrap();
    assert!(bad.validate().is_err());
}
