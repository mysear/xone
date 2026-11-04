use xone_domain::*;
use xone_kafka_transport::*;
#[test]
fn no_snapshot_before_complete_commit() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let frames = encode(
        &Unit {
            normalized_facts: None,
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: s.clone(),
        },
        1024,
    )
    .unwrap();
    let mut a = Assembler::new(8 << 20);
    for (i, f) in frames.iter().cloned().enumerate() {
        let u = a.push(f).unwrap();
        if i + 1 < frames.len() {
            assert!(u.is_none());
        } else {
            assert_eq!(u.unwrap().snapshot.reference, s.reference);
        }
    }
    let mut a = Assembler::new(8 << 20);
    a.push(frames[0].clone()).unwrap();
    assert!(a.push(frames.last().unwrap().clone()).is_err());
}

#[test]
fn cross_language_frames() {
    let frames: Vec<Frame> =
        serde_json::from_str(include_str!("../../../test/contracts/journal-frames.json")).unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut last = None;
    for f in frames {
        last = a.push(f).unwrap();
    }
    assert_eq!(
        last.unwrap().snapshot.reference.block.chain_key,
        "evm:31337"
    );
}

#[test]
fn kafka_payload_is_protobuf_not_json() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    for f in encode(
        &Unit {
            normalized_facts: None,
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: s,
        },
        1024,
    )
    .unwrap()
    {
        let bytes = encode_frame(&f).unwrap();
        assert_ne!(bytes[0], b'{');
        let decoded = decode_frame(&bytes).unwrap();
        assert_eq!(decoded.reference, f.reference);
        assert_eq!(decoded.data_hex, f.data_hex);
    }
}

#[test]
fn revocation_and_reorg_barriers_use_complete_binary_units() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    for kind in ["ONLINE_SCOPE_REVOKED", "REORG_BEGIN", "REORG_COMMIT"] {
        let frames = encode(
            &Unit {
                normalized_facts: None,
                kind: kind.into(),
                snapshot: s.clone(),
            },
            1024,
        )
        .unwrap();
        let mut a = Assembler::new(8 << 20);
        let mut result = None;
        for f in frames {
            let binary = encode_frame(&f).unwrap();
            result = a.push(decode_frame(&binary).unwrap()).unwrap();
        }
        let unit = result.unwrap();
        assert_eq!(unit.kind, kind);
        assert_eq!(unit.snapshot.reference, s.reference);
    }
}

#[test]
fn protoc_normalized_fact_golden_keeps_all_amount_bits() {
    let frames: Vec<Frame> = serde_json::from_str(include_str!(
        "../../../test/contracts/normalized-facts.json"
    ))
    .unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut result = None;
    for frame in frames {
        result = a.push(frame).unwrap();
    }
    let u = result.unwrap();
    let facts = u.normalized_facts.as_ref().unwrap();
    assert_eq!(
        facts.blocks[0].swaps[0].amount0_in.to_string(),
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
    );
    let binary = encode_unit(&u).unwrap();
    assert_eq!(
        decode_unit(&binary).unwrap().normalized_facts,
        u.normalized_facts
    );
}

#[test]
fn independent_protoc_v3_state_proof_and_signed_net() {
    let frames: Vec<Frame> = serde_json::from_str(include_str!(
        "../../../test/contracts/v3-journal-frames.json"
    ))
    .unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut result = None;
    for frame in frames {
        result = a.push(frame).unwrap();
    }
    let u = result.unwrap();
    let v = u.snapshot.pools[0].v3.as_ref().unwrap();
    assert_eq!(v.ticks[2].liquidity_net, "-2000000000");
    assert_eq!(v.tick_words.len(), 2);
    assert_eq!(
        decode_unit(&encode_unit(&u).unwrap()).unwrap().snapshot,
        u.snapshot
    );
}

#[test]
fn independent_protoc_preserves_six_dex_version_profiles() {
    let frames: Vec<Frame> = serde_json::from_str(include_str!(
        "../../../test/contracts/dex-version-frames.json"
    ))
    .unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut result = None;
    for f in frames {
        result = a.push(f).unwrap();
    }
    let u = result.unwrap();
    assert_eq!(u.snapshot.pools.len(), 6);
    for p in &u.snapshot.pools {
        let profile = xone_domain::protocols::protocol(&p.protocol).unwrap();
        assert_eq!(p.adapter_version, profile.adapter);
        if p.protocol == "pancake-v3" {
            assert_eq!(p.v3.as_ref().unwrap().fee_protocol, 209718400);
        }
        if profile.family == "v2" {
            assert!(p.v2.is_some());
        }
    }
    assert_eq!(
        decode_unit(&encode_unit(&u).unwrap()).unwrap().snapshot,
        u.snapshot
    );
}

#[test]
fn independent_protoc_stable_parameters_and_slipstream_dependencies() {
    let frames: Vec<Frame> =
        serde_json::from_str(include_str!("../../../test/contracts/stable-frames.json")).unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut result = None;
    for f in frames {
        result = a.push(f).unwrap();
    }
    let unit = result.unwrap();
    assert_eq!(unit.snapshot.pools.len(), 5);
    for p in &unit.snapshot.pools {
        if p.protocol == "aerodrome-slipstream" {
            assert!(!p.v3.as_ref().unwrap().factory_code_sha256.is_empty());
        } else {
            assert!(p.stable.is_some());
        }
    }
    assert_eq!(
        decode_unit(&encode_unit(&unit).unwrap()).unwrap().snapshot,
        unit.snapshot
    );
}

#[test]
fn independent_protoc_singleton_key_manager_and_core() {
    let frames: Vec<Frame> = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-frames.json"
    ))
    .unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut result = None;
    for f in frames {
        result = a.push(f).unwrap();
    }
    let u = result.unwrap();
    assert_eq!(u.snapshot.pools.len(), 2);
    for p in &u.snapshot.pools {
        p.validate().unwrap();
        assert!(p.v3.is_none());
        let s = p.singleton_cl.as_ref().unwrap();
        assert_eq!(s.protocol_fee1, 500);
        assert_eq!(s.core.ticks[2].liquidity_net, "-2000000000");
    }
    assert_eq!(
        decode_unit(&encode_unit(&u).unwrap()).unwrap().snapshot,
        u.snapshot
    );
}

#[test]
fn independent_protoc_preserves_raydium_creator_program_bank_and_fee_ledgers() {
    let frames: Vec<Frame> =
        serde_json::from_str(include_str!("../../../test/contracts/raydium-frames.json")).unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut last = None;
    for f in frames {
        last = a.push(f).unwrap();
    }
    let u = last.unwrap();
    let s = u.snapshot.pools[0].raydium_cpmm.as_ref().unwrap();
    assert_eq!(s.programdata, xone_domain::raydium::PROGRAM_DATA);
    assert_eq!(s.creator_fees0, 700);
    assert_eq!(s.creator_fees1, 1700);
    assert_eq!(s.bank_slot, u.snapshot.reference.block.number_or_slot);
    u.snapshot.validate("chain-parser-rust", 1).unwrap();
    assert_eq!(
        decode_unit(&encode_unit(&u).unwrap()).unwrap().snapshot,
        u.snapshot
    );
}

#[test]
fn independent_protoc_preserves_orca_q64_array_proof_and_fee_growth() {
    let frames: Vec<Frame> =
        serde_json::from_str(include_str!("../../../test/contracts/orca-frames.json")).unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut u = None;
    for f in frames {
        u = a.push(f).unwrap();
    }
    let u = u.unwrap();
    let s = u.snapshot.pools[0].orca_whirlpool.as_ref().unwrap();
    assert_eq!(s.sqrt_price_x64.to_string(), "18446744073709551616");
    assert_eq!(s.programdata, xone_domain::orca::PROGRAM_DATA);
    assert_eq!(s.tick_arrays[1].ticks[0].liquidity_net, "-1000000000");
    u.snapshot.validate("chain-parser-rust", 1).unwrap();
    assert_eq!(
        decode_unit(&encode_unit(&u).unwrap()).unwrap().snapshot,
        u.snapshot
    );
}

#[test]
fn independent_protoc_preserves_rayclmm_base_bitmap_observation_and_signed_ticks() {
    let frames: Vec<Frame> =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-frames.json")).unwrap();
    let mut a = Assembler::new(8 << 20);
    let mut u = None;
    for f in frames {
        u = a.push(f).unwrap();
    }
    let u = u.unwrap();
    let s = u.snapshot.pools[0].raydium_clmm.as_ref().unwrap();
    assert_eq!(s.bitmap.len(), 16);
    assert_eq!(s.bitmap[7], 1u64 << 63);
    assert_eq!(s.observations.len(), 100);
    assert_eq!(s.tick_arrays[0].ticks[0].liquidity_net, "1000000000");
    u.snapshot.validate("chain-parser-rust", 1).unwrap();
    assert_eq!(
        decode_unit(&encode_unit(&u).unwrap()).unwrap().snapshot,
        u.snapshot
    );
    for mode in ["bump", "array"] {
        let mut encoded = xone_domain::wire_conversion::snapshot_to_proto(&u.snapshot).unwrap();
        let state = encoded.pools[0].raydium_clmm.as_mut().unwrap();
        if mode == "bump" {
            state.pool_bump ^= 1;
        } else {
            state.tick_arrays[0].address = xone_domain::orca::PROGRAM_DATA.into();
        }
        assert!(
            xone_domain::wire_conversion::snapshot_from_proto(encoded).is_err(),
            "forged PDA admitted from protobuf: {mode}"
        );
    }
}

#[test]
fn independent_protoc_preserves_token_fee_modes_and_rejects_profile_confusion() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-token-fee-frames.json"
    ))
    .unwrap();
    assert_eq!(vectors.as_array().unwrap().len(), 2);
    for vector in vectors.as_array().unwrap() {
        let frames: Vec<Frame> = serde_json::from_value(vector["frames"].clone()).unwrap();
        let mut assembler = Assembler::new(8 << 20);
        let mut unit = None;
        for f in frames {
            unit = assembler.push(f).unwrap();
        }
        let unit = unit.unwrap();
        unit.snapshot.validate("chain-parser-rust", 1).unwrap();
        assert_eq!(
            u64::from(
                unit.snapshot.pools[0]
                    .raydium_clmm
                    .as_ref()
                    .unwrap()
                    .collect_fee_on
            ),
            vector["collect_fee_on"].as_u64().unwrap()
        );
        for mode in [0, 3, 255] {
            let mut wire = xone_domain::wire_conversion::snapshot_to_proto(&unit.snapshot).unwrap();
            wire.pools[0].raydium_clmm.as_mut().unwrap().collect_fee_on = mode;
            assert!(xone_domain::wire_conversion::snapshot_from_proto(wire).is_err());
        }
        let mut wire = xone_domain::wire_conversion::snapshot_to_proto(&unit.snapshot).unwrap();
        wire.pools[0].protocol = "raydium-clmm-static".into();
        wire.pools[0].adapter_version = "raydium-clmm/6d7a0f82-spl-static-1".into();
        assert!(xone_domain::wire_conversion::snapshot_from_proto(wire).is_err());
    }
}

#[test]
fn independent_protoc_preserves_extension_high_words_and_profile_proof() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-bitmap-frames.json"
    ))
    .unwrap();
    assert_eq!(vectors.as_array().unwrap().len(), 2);
    for vector in vectors.as_array().unwrap() {
        let frames: Vec<Frame> = serde_json::from_value(vector["frames"].clone()).unwrap();
        let mut a = Assembler::new(8 << 20);
        let mut u = None;
        for f in frames {
            u = a.push(f).unwrap();
        }
        let u = u.unwrap();
        u.snapshot.validate("chain-parser-rust", 1).unwrap();
        let e = u.snapshot.pools[0]
            .raydium_clmm
            .as_ref()
            .unwrap()
            .bitmap_extension
            .as_ref()
            .unwrap();
        assert_eq!(e.positive[111], 1u64 << 63);
        assert_eq!(e.negative[7], (1u64 << 63) | 1);
        for mode in ["missing", "width", "pda", "profile"] {
            let mut w = xone_domain::wire_conversion::snapshot_to_proto(&u.snapshot).unwrap();
            let p = &mut w.pools[0];
            let state = p.raydium_clmm.as_mut().unwrap();
            match mode {
                "missing" => state.bitmap_extension = None,
                "width" => {
                    state.bitmap_extension.as_mut().unwrap().negative.pop();
                }
                "pda" => {
                    state.bitmap_extension.as_mut().unwrap().address =
                        xone_domain::orca::PROGRAM_DATA.into()
                }
                _ => {
                    p.protocol = "raydium-clmm-token-fee".into();
                    p.adapter_version = "raydium-clmm/6d7a0f82-spl-token-fee-1".into();
                }
            }
            assert!(
                xone_domain::wire_conversion::snapshot_from_proto(w).is_err(),
                "{mode}"
            );
        }
    }
}

#[test]
fn independent_protoc_preserves_custom_seed_identity_and_u16_width() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-seed-frames.json"
    ))
    .unwrap();
    assert_eq!(vectors.as_array().unwrap().len(), 3);
    for vector in vectors.as_array().unwrap() {
        let frames: Vec<Frame> = serde_json::from_value(vector["frames"].clone()).unwrap();
        let mut a = Assembler::new(8 << 20);
        let mut u = None;
        for f in frames {
            u = a.push(f).unwrap();
        }
        let u = u.unwrap();
        u.snapshot.validate("chain-parser-rust", 1).unwrap();
        assert_eq!(
            u64::from(
                u.snapshot.pools[0]
                    .raydium_clmm
                    .as_ref()
                    .unwrap()
                    .seed_index
            ),
            vector["seed_index"].as_u64().unwrap()
        );
        for mode in [
            "zero",
            "width",
            "different-seed",
            "legacy",
            "missing-extension",
        ] {
            let mut w = xone_domain::wire_conversion::snapshot_to_proto(&u.snapshot).unwrap();
            let p = &mut w.pools[0];
            let state = p.raydium_clmm.as_mut().unwrap();
            match mode {
                "zero" => state.seed_index = 0,
                "width" => state.seed_index = 65536,
                "different-seed" => state.seed_index ^= 1,
                "legacy" => {
                    p.protocol = "raydium-clmm-bitmap".into();
                    p.adapter_version = "raydium-clmm/6d7a0f82-spl-bitmap-1".into();
                }
                _ => state.bitmap_extension = None,
            }
            assert!(
                xone_domain::wire_conversion::snapshot_from_proto(w).is_err(),
                "{mode}"
            );
        }
    }
}

#[test]
fn independent_protoc_preserves_dynamic_signed_reference_and_rejects_bad_profile() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-dynamic-frames.json"
    ))
    .unwrap();
    assert_eq!(vectors.as_array().unwrap().len(), 3);
    for vector in vectors.as_array().unwrap() {
        let frames: Vec<Frame> = serde_json::from_value(vector["frames"].clone()).unwrap();
        let mut a = Assembler::new(8 << 20);
        let mut u = None;
        for f in frames {
            u = a.push(f).unwrap();
        }
        let u = u.unwrap();
        u.snapshot.validate("chain-parser-rust", 1).unwrap();
        let s = u.snapshot.pools[0].raydium_clmm.as_ref().unwrap();
        let d = s.dynamic_fee.as_ref().unwrap();
        assert_eq!(d.tick_spacing_index_reference, -7);
        assert_eq!(d.volatility_accumulator, 30000);
        assert_eq!(d.last_update_timestamp, s.bank_unix_timestamp);
        for mode in ["missing", "filterwidth", "capwidth", "oldprofile"] {
            let mut w = xone_domain::wire_conversion::snapshot_to_proto(&u.snapshot).unwrap();
            let p = &mut w.pools[0];
            let s = p.raydium_clmm.as_mut().unwrap();
            match mode {
                "missing" => s.dynamic_fee = None,
                "filterwidth" => s.dynamic_fee.as_mut().unwrap().filter_period = 65536,
                "capwidth" => s.dynamic_fee.as_mut().unwrap().max_volatility_accumulator = u32::MAX,
                _ => {
                    p.protocol = "raydium-clmm-custom-seed".into();
                    p.adapter_version = "raydium-clmm/6d7a0f82-spl-custom-seed-1".into();
                }
            }
            assert!(
                xone_domain::wire_conversion::snapshot_from_proto(w).is_err(),
                "{mode}"
            );
        }
    }
}

#[test]
fn independent_protoc_weighted_field27_and_full_roundtrip() {
    let frames: Vec<Frame> =
        serde_json::from_str(include_str!("../../../test/contracts/weighted-frames.json")).unwrap();
    let mut assembler = Assembler::new(8 << 20);
    let mut unit = None;
    for frame in frames {
        unit = assembler.push(frame).unwrap();
    }
    let unit = unit.unwrap();
    assert_eq!(unit.snapshot.pools.len(), 3);
    for pool in &unit.snapshot.pools {
        pool.validate().unwrap();
        assert!(pool.weighted.is_some());
        assert!(pool.stable.is_none());
    }
    assert_eq!(
        decode_unit(&encode_unit(&unit).unwrap()).unwrap().snapshot,
        unit.snapshot
    );
}

#[test]
fn independent_protoc_weighted_static_field27_and_full_roundtrip() {
    let frames: Vec<Frame> = serde_json::from_str(include_str!(
        "../../../test/contracts/weighted-static-frames.json"
    ))
    .unwrap();
    let mut assembler = Assembler::new(8 << 20);
    let mut unit = None;
    for frame in frames {
        unit = assembler.push(frame).unwrap();
    }
    let unit = unit.unwrap();
    assert_eq!(unit.snapshot.pools.len(), 6);
    let adapters = unit
        .snapshot
        .pools
        .iter()
        .map(|p| p.adapter_version.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    assert_eq!(
        unit.snapshot.reference.adapter_set_hash,
        hash(&adapters).unwrap()
    );
    for pool in &unit.snapshot.pools {
        pool.validate().unwrap();
        assert!(pool.weighted.is_some());
        assert!(pool.stable.is_none());
    }
    assert_eq!(
        decode_unit(&encode_unit(&unit).unwrap()).unwrap().snapshot,
        unit.snapshot
    );
}
