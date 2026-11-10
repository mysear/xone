use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Snapshot,
};
#[test]
fn independent_bpt_cpp_snapshot_preserves_full_wire_and_known_zero_false() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-bpt-wire.json");
    let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        value["authority"],
        "synthetic-bpt-state-wire-contract-no-native-execution-proof"
    );
    let snapshot: Snapshot = serde_json::from_value(value["snapshot"].clone()).unwrap();
    snapshot
        .validate(&snapshot.reference.owner_service, 1)
        .unwrap();
    let raw = hex::decode(value["protobuf_hex"].as_str().unwrap()).unwrap();
    assert_eq!(
        hex::encode(Sha256::digest(&raw)),
        value["protobuf_sha256"].as_str().unwrap()
    );
    let wire = wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
    assert_eq!(snapshot_to_proto(&snapshot).unwrap().encode_to_vec(), raw);
    assert_eq!(snapshot_from_proto(wire.clone()).unwrap(), snapshot);
    let b = wire.pools[0]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .bpt
        .as_ref()
        .unwrap();
    assert_eq!(b.bpt_last_change_block, Some(0));
    assert_eq!(b.allowlist_members[0].allowed, Some(false));
    for field in 0..5 {
        let mut w = wire.clone();
        let m = w.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        if field == 0 {
            m.bpt = None;
        } else {
            let b = m.bpt.as_mut().unwrap();
            match field {
                1 => b.join_exit_enabled = None,
                2 => b.minimum_bpt_locked = None,
                3 => b.bpt_last_change_block = None,
                _ => b.allowlist_members[0].allowed = None,
            }
        }
        assert!(
            snapshot_from_proto(w).is_err(),
            "absent BPT field {field} accepted"
        );
    }
}
#[test]
fn original_bpt_projection_preserves_every_supported_canonical_pool_and_wire() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-bpt-pools.json");
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        v["authority"],
        "authenticated-original-managed-v2-bpt-observations"
    );
    let source = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/contracts/managed-v2-bpt.json"),
    )
    .unwrap();
    assert_eq!(
        v["original_raw"]["sha256"],
        hex::encode(Sha256::digest(&source))
    );
    assert_eq!(v["original_raw"]["bytes"], source.len());
    let mut pools = 0;
    for row in v["banks"].as_array().unwrap() {
        for entry in row["pools"].as_array().unwrap() {
            let p: xone_domain::Pool = serde_json::from_value(entry["pool"].clone()).unwrap();
            p.validate().unwrap();
            let state = p.multi_asset.as_ref().unwrap();
            let encoded = state.to_proto().unwrap().encode_to_vec();
            assert_eq!(
                xone_domain::multi_asset::MultiAssetState::from_proto(
                    wire::MultiAssetState::decode(encoded.as_slice()).unwrap()
                )
                .unwrap(),
                *state
            );
            pools += 1;
        }
    }
    assert_eq!(pools, 286);
}
