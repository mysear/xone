use prost::Message;
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Amount, Snapshot,
};
#[test]
fn lfj_complete_cxx_wire_and_source_bank_bindings_are_preserved() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/lfj-v21-wire.json")).unwrap();
    assert_eq!(data["vectors"].as_array().unwrap().len(), 4);
    for vector in data["vectors"].as_array().unwrap() {
        let snapshot: Snapshot = serde_json::from_value(vector["snapshot"].clone()).unwrap();
        snapshot.validate("chain-parser-go", 1).unwrap();
        let bytes = hex::decode(vector["protobuf_hex"].as_str().unwrap()).unwrap();
        let encoded = wire::OnlineSnapshot::decode(bytes.as_slice()).unwrap();
        assert_eq!(snapshot_to_proto(&snapshot).unwrap(), encoded);
        assert_eq!(snapshot_from_proto(encoded).unwrap(), snapshot);
        for bad in 0..14 {
            let mut changed = snapshot.clone();
            let pool = &mut changed.pools[0];
            let state = pool.lfj.as_mut().unwrap();
            match bad {
                0 => state.raw_reserve0 = Amount::zero(),
                1 => state.bins.pop().map(|_| ()).unwrap(),
                2 => state.bins[0].total_shares = Amount::zero(),
                3 => state.bins[0].id = state.bins[1].id,
                4 => state.factory = pool.token0.clone(),
                5 => state.timestamp += 1,
                6 => state.block_hash = format!("0x{}", "1".repeat(64)),
                7 => state.block_number += 1,
                8 => state.max_bins = 0,
                9 => state.max_steps = 0,
                10 => pool.protocol = "uniswap-v2".into(),
                11 => pool.adapter_version = "lfj-lb/v2.2.0".into(),
                12 => state.parameters.0 <<= 256usize,
                13 => {
                    xone_domain::lfj::set_field(&mut state.parameters, 176, 40, state.timestamp + 1)
                }
                _ => unreachable!(),
            }
            assert!(changed.validate("chain-parser-go", 1).is_err(), "bad {bad}");
        }
    }
}
#[test]
fn lfj_wire_requires_each_digest_amount_and_bounded_oracle_collection() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/lfj-v21-wire.json")).unwrap();
    let snapshot: Snapshot =
        serde_json::from_value(data["vectors"][1]["snapshot"].clone()).unwrap();
    let encoded = snapshot_to_proto(&snapshot).unwrap();
    for bad in 0..11 {
        let mut changed = encoded.clone();
        let state = changed.pools[0].lfj.as_mut().unwrap();
        match bad {
            0 => state.parameters = None,
            1 => state.raw_reserve0 = None,
            2 => state.bins[0].reserve0 = None,
            3 => state.bins[0].total_shares = None,
            4 => state.code_sha256.clear(),
            5 => state.block_hash.clear(),
            6 => state.oracle_samples[0].data = None,
            7 => state.oracle_samples.push(state.oracle_samples[0].clone()),
            8 => state.parameters.as_mut().unwrap().uint256_be.truncate(31),
            9 => state.timestamp = None,
            10 => state.block_number = None,
            _ => unreachable!(),
        }
        assert!(snapshot_from_proto(changed).is_err(), "bad {bad}");
    }
}
