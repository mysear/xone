use prost::Message;
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Snapshot,
};
#[test]
fn independently_encoded_multi_singleton_and_rewards_survive_full_wire() {
    for text in [
        include_str!("../../../test/contracts/five-family-evm-wire.json"),
        include_str!("../../../test/contracts/five-family-solana-wire.json"),
        include_str!("../../../test/contracts/five-family-rayclmm-wire.json"),
    ] {
        let data: serde_json::Value = serde_json::from_str(text).unwrap();
        let expected: Snapshot = serde_json::from_value(data["snapshot"].clone()).unwrap();
        let bytes = hex::decode(data["protobuf_hex"].as_str().unwrap()).unwrap();
        let message = wire::OnlineSnapshot::decode(bytes.as_slice()).unwrap();
        let got = snapshot_from_proto(message.clone()).unwrap();
        assert_eq!(got, expected);
        for p in &got.pools {
            p.validate().unwrap();
        }
        if got.pools.iter().any(|p| p.singleton_advanced.is_some()) {
            let mut bad = got.clone();
            bad.pools
                .iter_mut()
                .find_map(|p| p.singleton_advanced.as_mut())
                .unwrap()
                .snapshot_block_hash = format!("0x{}", "cd".repeat(32));
            assert!(bad
                .validate(
                    &bad.reference.owner_service,
                    bad.reference.owner_assignment_version
                )
                .is_err());
        }
        assert_eq!(snapshot_to_proto(&got).unwrap(), message);
    }
}
