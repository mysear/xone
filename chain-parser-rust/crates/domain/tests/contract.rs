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

#[test]
fn http_record_reads_legacy_and_native_bnb_swap_facts() {
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.reference.block.chain_key = "evm:56".into();
    snapshot.pools.truncate(1);
    let pool = &mut snapshot.pools[0];
    pool.chain_key = "evm:56".into();
    pool.protocol = "pancake-v2".into();
    pool.v2 = Some(protocols::V2State {
        factory: format!("0x{}", "12".repeat(20)),
        code_sha256: "ab".repeat(32),
        swap: None,
    });
    let block = &snapshot.reference.block;
    let mut raw = serde_json::json!({
        "reference": snapshot.reference,
        "kind": "BLOCK_COMMIT",
        "record_id": "evm:56/1/2",
        "payload_hash": hash(&snapshot.pools).unwrap(),
        "pools": snapshot.pools,
    });
    let legacy: Record = serde_json::from_value(raw.clone()).unwrap();
    assert!(legacy.normalized_facts.is_none());
    legacy.validate_payload().unwrap();
    assert_eq!(serde_json::to_value(legacy).unwrap(), raw);

    raw["normalized_facts"] = serde_json::json!({
        "schema_version": 1,
        "decoder_version": "evm-v2-swap/2",
        "blocks": [{"block": block, "swaps": [{
            "event_id": format!("evm:56/{}/1/2", block.hash),
            "tx_hash": format!("0x{}", "ad".repeat(32)),
            "tx_index": 1,
            "log_index": 2,
            "pool_id": snapshot.pools[0].pool_id,
            "token0": snapshot.pools[0].token0,
            "token1": snapshot.pools[0].token1,
            "amount0_in": "10",
            "amount1_in": "0",
            "amount0_out": "0",
            "amount1_out": "8"
        }]}],
        "reverted_blocks": []
    });
    let native: Record = serde_json::from_value(raw.clone()).unwrap();
    native.validate_payload().unwrap();
    let facts = native.normalized_facts.as_ref().unwrap();
    assert_eq!(
        facts::FactBatch::from_proto(facts.to_proto().unwrap()).unwrap(),
        *facts
    );
    raw["normalized_facts"]["decoder_version"] = "uniswap-v2/1".into();
    assert!(serde_json::from_value::<Record>(raw)
        .unwrap()
        .validate_payload()
        .is_err());
}
