use xone_domain::Snapshot;
#[test]
fn managed_reference_state_is_not_a_live_quote_capability() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-fees-wire.json"
    ))
    .unwrap();
    let s: Snapshot = serde_json::from_value(d["snapshot"].clone()).unwrap();
    let p = &s.pools[0];
    let err = xone_protocol_math::multi_asset::quote_exact_in_tokens(
        p,
        &"1000000000000000000".parse().unwrap(),
        &p.token0,
        &p.token1,
    )
    .unwrap_err();
    assert_eq!(err.0, "MULTI_ASSET_MATH_NOT_IMPLEMENTED");
}
