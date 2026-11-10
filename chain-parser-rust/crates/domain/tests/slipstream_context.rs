use prost::Message;
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Pool, Snapshot,
};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-context-state.json"
    ))
    .unwrap()
}
#[test]
fn independent_contextual_cpp_wire_preserves_every_origin_ledger_and_explicit_zero() {
    let file: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-context-wire.json"
    ))
    .unwrap();
    assert_eq!(file["vectors"].as_array().unwrap().len(), 4);
    for vector in file["vectors"].as_array().unwrap() {
        let snapshot: Snapshot = serde_json::from_value(vector["snapshot"].clone()).unwrap();
        snapshot.validate("chain-parser-go", 1).unwrap();
        let raw = hex::decode(vector["protobuf_hex"].as_str().unwrap()).unwrap();
        let encoded = wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
        assert_eq!(snapshot_to_proto(&snapshot).unwrap(), encoded);
        assert_eq!(snapshot_from_proto(encoded).unwrap(), snapshot);
    }
}
#[test]
fn required_contextual_wire_fields_and_canonical_source_flags_fail_closed() {
    let file: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-context-wire.json"
    ))
    .unwrap();
    let s: Snapshot = serde_json::from_value(file["vectors"][2]["snapshot"].clone()).unwrap();
    let encoded = snapshot_to_proto(&s).unwrap();
    for bad in 0..17 {
        let mut value = encoded.clone();
        let p = &mut value.pools[0];
        let state = p.slipstream_context.as_mut().unwrap();
        match bad {
            0 => state.timestamp = None,
            1 => state.block_number = None,
            2 => state.derived = None,
            3 => state.derived = Some(true),
            4 => state.unlocked = None,
            5 => state.unlocked = Some(false),
            6 => state.observations[0].initialized = None,
            7 => state.origins[0].discount = None,
            8 => state.origins[0].view_swap_fee = None,
            9 => state.core = None,
            10 => state.ticks[0].liquidity_gross = None,
            11 => state.ticks[0].staked_liquidity_net.truncate(15),
            12 => state.ticks[0].tick_cumulative_outside.truncate(6),
            13 => {
                state
                    .swap_fee_module
                    .as_mut()
                    .unwrap()
                    .dynamic
                    .as_mut()
                    .unwrap()
                    .initial_fee_enabled = None
            }
            14 => state.factory_registered = None,
            15 => state.voter_code_sha256.clear(),
            16 => state.observations.pop().map(|_| ()).unwrap(),
            _ => unreachable!(),
        };
        let result = snapshot_from_proto(value).and_then(|s| s.validate("chain-parser-go", 1));
        assert!(result.is_err(), "contextual wire {bad}");
    }
}
#[test]
fn contradictory_readonly_module_factory_and_origin_facts_reject_shared_banks() {
    let file = data();
    let value = file["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["mode"] == "dynamic")
        .unwrap();
    let p: Pool = serde_json::from_value(value["pool"].clone()).unwrap();
    let mut q = p.clone();
    q.pool_id = "0x000000000000000000000000000000000000aacc".into();
    assert!(
        xone_domain::validate_pool_dependencies([&p, &q]).is_err(),
        "one registered factory key cannot describe two physical pools"
    );
    q.slipstream_context.as_mut().unwrap().core.tick_spacing = 100;
    let min = (-887272 / 100) * 100;
    let max = (887272 / 100) * 100;
    let count = (max - min) / 100 + 1;
    q.slipstream_context
        .as_mut()
        .unwrap()
        .max_liquidity_per_tick = xone_domain::Amount::checked(
        ((num_bigint::BigUint::from(1u8) << 128usize) - 1u8)
            / num_bigint::BigUint::from(count as u32),
    )
    .unwrap();
    for word in &mut q.slipstream_context.as_mut().unwrap().core.tick_words {
        word.bitmap = xone_domain::Amount::zero();
    }
    let full = q.slipstream_context.as_ref().unwrap().ticks.clone();
    let core = &mut q.slipstream_context.as_mut().unwrap().core;
    for tick in full {
        let compressed = tick.tick.div_euclid(100);
        let index = compressed.div_euclid(256);
        let bit = compressed.rem_euclid(256);
        core.tick_words
            .iter_mut()
            .find(|w| w.word_pos == index)
            .unwrap()
            .bitmap
            .0
            .set_bit(bit as u64, true);
    }
    p.validate().unwrap();
    q.validate().unwrap();
    xone_domain::validate_pool_dependencies([&p, &q]).unwrap();
    for bad in 0..5 {
        let mut other = q.clone();
        let state = other.slipstream_context.as_mut().unwrap();
        match bad {
            0 => state.factory_default_unstaked_fee += 1,
            1 => state.origins[0].discount += 1,
            2 => {
                state
                    .swap_fee_module
                    .dynamic
                    .as_mut()
                    .unwrap()
                    .default_scaling_factor += 1
            }
            3 => state.voter_gauge_alive = !state.voter_gauge_alive,
            4 => state.core.factory_code_sha256 = "a".repeat(64),
            _ => unreachable!(),
        };
        assert!(
            xone_domain::validate_pool_dependencies([&p, &other]).is_err(),
            "readonly conflict {bad}"
        );
    }
}
#[test]
fn null_missing_duplicate_and_unknown_context_members_never_become_observed_zero() {
    let file = data();
    let value = file["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["mode"] == "dynamic")
        .unwrap();
    let original = value["pool"].clone();
    for key in [
        "timestamp",
        "block_number",
        "derived",
        "factory_registered",
        "view_swap_fee",
        "origins",
        "observations",
    ] {
        for null in [false, true] {
            let mut changed = original.clone();
            if null {
                changed["slipstream_context"][key] = serde_json::Value::Null;
            } else {
                changed["slipstream_context"]
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
            }
            assert!(
                serde_json::from_value::<Pool>(changed).is_err(),
                "missing/null {key}"
            );
        }
    }
    let text = serde_json::to_string(&original["slipstream_context"]).unwrap();
    for (key, value) in [("timestamp", "0"), ("unknown", "0")] {
        let duplicate = format!("{},\"{}\":{}}}", &text[..text.len() - 1], key, value);
        assert!(
            serde_json::from_str::<xone_domain::slipstream_context::SlipstreamState>(&duplicate)
                .is_err()
        );
    }
}
