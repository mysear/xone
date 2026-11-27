use xone_domain::Snapshot;
#[test]
fn managed_reference_state_has_no_execution_or_production_capability() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-fees-wire.json"
    ))
    .unwrap();
    let s: Snapshot = serde_json::from_value(d["snapshot"].clone()).unwrap();
    let p = &s.pools[0];
    let capability = xone_protocol_math::capabilities::describe(p).unwrap();
    assert!(!capability.execution_preparation);
    assert!(!capability.independent_production_verification);
    assert!(!capability.production_ready);
}
