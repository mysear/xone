use prost::Message;
use xone_domain::{
    curve_meta_state::{validate_meta_dependencies, BASE_POOL},
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Pool, Snapshot,
};
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../test/contracts/curve-meta-wire.json")).unwrap()
}
#[test]
fn meta_complete_accounting_source_and_independent_wire_roundtrip() {
    let d = fixture();
    let expected: Snapshot = serde_json::from_value(d["snapshot"].clone()).unwrap();
    let raw = hex::decode(d["protobuf_hex"].as_str().unwrap()).unwrap();
    let w = wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
    let s = snapshot_from_proto(w.clone()).unwrap();
    assert_eq!(s, expected);
    assert_eq!(snapshot_to_proto(&s).unwrap(), w);
    for bad in 0..7 {
        let mut q = s.clone();
        let p = &mut q.pools[0];
        let m = p.multi_asset.as_mut().unwrap().curve_meta.as_mut().unwrap();
        match bad {
            0 => m.block_number += 1,
            1 => m.timestamp += 1,
            2 => m.base_virtual_price = "0".parse().unwrap(),
            3 => m.admin_balances[0] = "1".parse().unwrap(),
            4 => m.implementation_code_sha256 = "a".repeat(64),
            5 => m.stored_balances.truncate(1),
            _ => m.block_hash = format!("0x{}", "b".repeat(64)),
        };
        assert!(
            q.validate(
                &s.reference.owner_service,
                s.reference.owner_assignment_version
            )
            .is_err(),
            "{bad}"
        );
    }
    let mut q = s.pools[0].clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .curve_meta
        .as_mut()
        .unwrap()
        .stored_balances[0] = "1".parse().unwrap();
    assert_ne!(q, s.pools[0]);
}
#[test]
fn mutable_or_inconsistent_base_dependencies_are_not_quoteable() {
    let d = fixture();
    let p: Pool = serde_json::from_value(d["snapshot"]["pools"][0].clone()).unwrap();
    let mut q = p.clone();
    q.pool_id = "0x0000000000000000000000000000000000000020".into();
    validate_meta_dependencies([&p, &q]).unwrap();
    for bad in 0..5 {
        let mut q = q.clone();
        let s = q.multi_asset.as_mut().unwrap();
        let m = s.curve_meta.as_mut().unwrap();
        match bad {
            0 => q.pool_id = BASE_POOL.into(),
            1 => m.base_virtual_price = "1000000000000000000".parse().unwrap(),
            2 => m.base_pool_code_sha256 = "d".repeat(64),
            3 => s.assets[1].code_sha256 = "d".repeat(64),
            _ => m.timestamp += 1,
        };
        assert!(validate_meta_dependencies([&p, &q]).is_err(), "{bad}");
    }
}
