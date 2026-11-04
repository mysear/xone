use prost::Message;
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Pool, Snapshot,
};
#[test]
fn ng_state_strictly_matches_effective_balances_fee_oracle_and_wire() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-state.json"
    ))
    .unwrap();
    for c in d["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        p.validate().unwrap();
        let mut bad = p.clone();
        bad.multi_asset
            .as_mut()
            .unwrap()
            .curve_ng
            .as_mut()
            .unwrap()
            .admin_balances[0] = "0".parse().unwrap();
        assert!(bad.validate().is_err());
    }
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-wire.json"
    ))
    .unwrap();
    let expected: Snapshot = serde_json::from_value(d["snapshot"].clone()).unwrap();
    let bytes = hex::decode(d["protobuf_hex"].as_str().unwrap()).unwrap();
    let message = wire::OnlineSnapshot::decode(bytes.as_slice()).unwrap();
    let actual = snapshot_from_proto(message.clone()).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(snapshot_to_proto(&actual).unwrap(), message);
    let mut bad = actual.clone();
    bad.pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .curve_ng
        .as_mut()
        .unwrap()
        .block_number += 1;
    assert!(bad
        .validate(
            &actual.reference.owner_service,
            actual.reference.owner_assignment_version
        )
        .is_err());
    let mut bad = actual.clone();
    bad.pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .curve_ng
        .as_mut()
        .unwrap()
        .timestamp += 1;
    assert!(bad
        .validate(
            &actual.reference.owner_service,
            actual.reference.owner_assignment_version
        )
        .is_err());
}
