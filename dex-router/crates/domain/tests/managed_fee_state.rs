use serde_json::{json, Value};
use xone_domain::{multi_asset::MultiAssetState, Pool};
fn sample() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let mut p = fixture["pools"][0].clone();
    p["protocol"] = json!("balancer-v2-managed-fees");
    p["adapter_version"] = json!("balancer-v2-managed-fees/e281ff40-bpt-1");
    p["multi_asset"]["managed_fees"] = json!({"total_supply":"1000000000000000000000000","protocol_swap_fee":"300000000000000000","management_swap_fee":"100000000000000000","owner":"0x0000000000000000000000000000000000000090","protocol_collector":"0x0000000000000000000000000000000000000091","owner_balance":"700000000000000000000000","protocol_collector_balance":"200000000000000000000000","timestamp":1767225600u64,"block_number":100,"block_hash":format!("0x{}","a".repeat(64))});
    p
}
#[test]
fn managed_fee_state_preserves_complete_ledger_and_coalesces_aliases() {
    let mut p: Pool = serde_json::from_value(sample()).unwrap();
    p.validate().unwrap();
    let state = p.multi_asset.as_ref().unwrap();
    assert_eq!(
        MultiAssetState::from_proto(state.to_proto().unwrap()).unwrap(),
        *state
    );
    let mut copy = p.clone();
    copy.multi_asset
        .as_mut()
        .unwrap()
        .managed_fees
        .as_mut()
        .unwrap()
        .owner_balance = "1".parse().unwrap();
    assert_ne!(copy.multi_asset, p.multi_asset);
    p.fee_numerator = 800_000_000_000_000_000;
    p.validate().unwrap();
    p.fee_numerator += 1;
    assert!(p.validate().is_err());
    let mut raw = sample();
    let fees = &mut raw["multi_asset"]["managed_fees"];
    fees["protocol_collector"] = fees["owner"].clone();
    fees["protocol_collector_balance"] = fees["owner_balance"].clone();
    serde_json::from_value::<Pool>(raw)
        .unwrap()
        .validate()
        .unwrap();
}
#[test]
fn malformed_managed_fee_state_and_legacy_payload_are_rejected() {
    for (field, value) in [
        ("total_supply", json!("0")),
        ("protocol_swap_fee", json!("500000000000000001")),
        ("management_swap_fee", json!("1000000000000000001")),
        (
            "protocol_collector_balance",
            json!("400000000000000000000000"),
        ),
        (
            "protocol_collector",
            json!("0x0000000000000000000000000000000000000090"),
        ),
        ("block_hash", json!(format!("0x{}", "0".repeat(64)))),
        ("timestamp", json!(0)),
    ] {
        let mut p = sample();
        p["multi_asset"]["managed_fees"][field] = value;
        assert!(
            serde_json::from_value::<Pool>(p)
                .unwrap()
                .validate()
                .is_err(),
            "{}",
            field
        );
    }
    let mut p = sample();
    p["multi_asset"]
        .as_object_mut()
        .unwrap()
        .remove("managed_fees");
    assert!(serde_json::from_value::<Pool>(p)
        .unwrap()
        .validate()
        .is_err());
    let mut p = sample();
    p["protocol"] = json!("balancer-v2-managed-snapshot");
    p["adapter_version"] = json!("balancer-v2-managed-snapshot/no-bpt-fees-1");
    assert!(serde_json::from_value::<Pool>(p)
        .unwrap()
        .validate()
        .is_err());
    let p: Pool = serde_json::from_value(sample()).unwrap();
    let mut wire = p.multi_asset.unwrap().to_proto().unwrap();
    wire.managed_fees.as_mut().unwrap().total_supply = None;
    assert!(MultiAssetState::from_proto(wire).is_err());
}
#[test]
fn independent_managed_bpt_wire_preserves_source_and_rejects_foreign_bank() {
    use prost::Message;
    use xone_domain::{
        wire,
        wire_conversion::{snapshot_from_proto, snapshot_to_proto},
        Snapshot,
    };
    let d: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-fees-wire.json"
    ))
    .unwrap();
    let expected: Snapshot = serde_json::from_value(d["snapshot"].clone()).unwrap();
    let bytes = hex::decode(d["protobuf_hex"].as_str().unwrap()).unwrap();
    let w = wire::OnlineSnapshot::decode(bytes.as_slice()).unwrap();
    let actual = snapshot_from_proto(w.clone()).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(snapshot_to_proto(&actual).unwrap(), w);
    for bad in [0, 1, 2] {
        let mut s = actual.clone();
        let m = s.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_fees
            .as_mut()
            .unwrap();
        match bad {
            0 => m.block_number += 1,
            1 => m.timestamp += 1,
            _ => m.block_hash = format!("0x{}", "b".repeat(64)),
        };
        assert!(s
            .validate(
                &s.reference.owner_service,
                s.reference.owner_assignment_version
            )
            .is_err());
    }
}
