use xone_domain::*;
#[test]
fn uint256_json_and_wire_boundaries() {
    let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    let a: Amount = serde_json::from_str(&format!("\"{max}\"")).unwrap();
    assert_eq!(a.to_be_bytes().unwrap(), [255; 32]);
    for raw in [
        "123",
        "\"-1\"",
        "\"1.0\"",
        "\"01\"",
        "\"115792089237316195423570985008687907853269984665640564039457584007913129639936\"",
    ] {
        assert!(serde_json::from_str::<Amount>(raw).is_err());
    }
}
#[test]
fn deterministic_hash() {
    assert_eq!(
        hash(&serde_json::json!({"z":"1","a":"2"})).unwrap(),
        hash(&serde_json::json!({"a":"2","z":"1"})).unwrap()
    );
}
#[test]
fn cross_language_golden_protobuf_and_snapshot() {
    use prost::Message;
    let gold: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/snapshot-golden.json")).unwrap();
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    assert_eq!(hash(&snapshot).unwrap(), gold["snapshot_sha256"]);
    snapshot.validate("chain-parser-go", 1).unwrap();
    let msg = wire::Amount {
        uint256_be: vec![255; 32],
    };
    let bytes = msg.encode_to_vec();
    assert_eq!(hex::encode(&bytes), gold["protobuf_amount_hex"]);
    let decoded = wire::Amount::decode(bytes.as_slice()).unwrap();
    assert_eq!(
        Amount::from_be_bytes(&decoded.uint256_be)
            .unwrap()
            .to_be_bytes()
            .unwrap(),
        [255; 32]
    );
}
#[test]
fn unchecked_internal_amount_never_panics_wire_encoding() {
    let amount = Amount(num_bigint::BigUint::from(1u64) << 256usize);
    assert!(amount.to_be_bytes().is_err());
    assert!(serde_json::to_string(&amount).is_err());
}
