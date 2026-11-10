use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Snapshot,
};
#[test]
fn independent_breaker_cpp_field42_retains_full_word_and_every_required_getter() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-breaker-wire.json");
    let f: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        f["authority"],
        "synthetic-breaker-state-wire-contract-no-native-execution-proof"
    );
    let s: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    s.validate(&s.reference.owner_service, 1).unwrap();
    let raw = hex::decode(f["protobuf_hex"].as_str().unwrap()).unwrap();
    assert_eq!(
        hex::encode(Sha256::digest(&raw)),
        f["protobuf_sha256"].as_str().unwrap()
    );
    let w = wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
    assert_eq!(snapshot_to_proto(&s).unwrap().encode_to_vec(), raw);
    assert_eq!(snapshot_from_proto(w.clone()).unwrap(), s);
    let c = w.pools[0]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .circuit_breakers
        .as_ref()
        .unwrap();
    assert_eq!(c.tokens.len(), 3);
    assert!(c.tokens[0].state_word.as_ref().unwrap().uint256_be[0] >= 128);
    for field in 0..8 {
        let mut q = w.clone();
        let m = q.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        if field == 0 {
            m.circuit_breakers = None;
        } else {
            let t = &mut m.circuit_breakers.as_mut().unwrap().tokens[0];
            match field {
                1 => t.state_word = None,
                2 => t.reference_bpt_price = None,
                3 => t.reference_weight = None,
                4 => t.lower_percentage = None,
                5 => t.upper_percentage = None,
                6 => t.lower_bpt_price_bound = None,
                _ => t.upper_bpt_price_bound = None,
            }
        }
        assert!(
            snapshot_from_proto(q).is_err(),
            "missing circuit field {field}"
        );
    }
}
#[test]
fn original_breaker_projected_sources_keep_complete_wire_and_raw_identity() {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/contracts/managed-v2-breaker.json"),
    )
    .unwrap();
    let v: Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/contracts/managed-v2-breaker-pools.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        v["authority"],
        "authenticated-original-managed-v2-breaker-observations"
    );
    assert_eq!(
        v["original_raw"]["sha256"],
        hex::encode(Sha256::digest(&raw))
    );
    assert_eq!(v["original_raw"]["bytes"], raw.len());
    let mut count = 0;
    for bank in v["banks"].as_array().unwrap() {
        for entry in bank["pools"].as_array().unwrap() {
            let p: xone_domain::Pool = serde_json::from_value(entry["pool"].clone()).unwrap();
            p.validate().unwrap();
            let state = p.multi_asset.as_ref().unwrap();
            let w = state.to_proto().unwrap();
            assert_eq!(
                xone_domain::multi_asset::MultiAssetState::from_proto(
                    wire::MultiAssetState::decode(w.encode_to_vec().as_slice()).unwrap()
                )
                .unwrap(),
                *state
            );
            count += 1;
        }
    }
    assert_eq!(count, 600);
}
