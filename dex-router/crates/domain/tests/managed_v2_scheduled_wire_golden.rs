use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use xone_domain::{
    hash,
    managed_v2_state::ManagedV2ExecutionContext,
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Pool, Snapshot,
};

// Synthetic syntax/domain fixture; no chain deployment or native execution proof.
fn golden() -> (Snapshot, wire::OnlineSnapshot, Vec<u8>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-scheduled-wire.json");
    let f: Value = serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .unwrap();
    assert_eq!(
        f["authority"],
        "synthetic-scheduled-state-wire-contract-no-native-execution-proof"
    );
    let s: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    assert_eq!(s.source, "SYNTHETIC_MANAGED_SCHEDULED_WIRE");
    s.validate(&s.reference.owner_service, 1).unwrap();
    let raw = hex::decode(f["protobuf_hex"].as_str().unwrap()).unwrap();
    assert_eq!(
        hex::encode(Sha256::digest(&raw)),
        f["protobuf_sha256"].as_str().unwrap()
    );
    let w = wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
    (s, w, raw)
}

#[test]
fn scheduled_cpp_wire_roundtrip_and_clone() {
    let (s, w, raw) = golden();
    assert_eq!(
        snapshot_to_proto(&s).unwrap().encode_to_vec(),
        raw,
        "Rust differs from C++ protoc"
    );
    let round = snapshot_from_proto(w.clone()).unwrap();
    assert_eq!(round, s, "complete Snapshot/Pool/raw schedule");
    assert_eq!(hash(&round).unwrap(), hash(&s).unwrap());
    let mut p = s.pools[0].clone();
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .schedule
        .as_mut()
        .unwrap()
        .tokens[0]
        .state_word = "0".parse().unwrap();
    assert_ne!(p, s.pools[0], "clone aliases schedule");
    let multi = s.pools[0].multi_asset.as_ref().unwrap();
    let m = multi.managed_v2.as_ref().unwrap();
    let view = m.schedule.as_ref().unwrap().at(m.timestamp).unwrap();
    let sum: num_bigint::BigUint = view.weights.iter().map(|w| &w.0).sum();
    assert_eq!(
        sum.to_string(),
        "1000000000000000002",
        "getter was normalized"
    );
    for (time, expected) in [
        (view.weight_start, "1000000000000000001"),
        (view.weight_end, "1000000000000000000"),
    ] {
        let at = m.schedule.as_ref().unwrap().at(time).unwrap();
        let sum: num_bigint::BigUint = at.weights.iter().map(|w| &w.0).sum();
        assert_eq!(
            sum.to_string(),
            expected,
            "computed starts and caller targets have different sum rules"
        );
    }
    let m = w.pools[0]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert_eq!(m.gradual_fee_end, Some(0));
    assert_eq!(m.paused, Some(false));
    assert_eq!(
        m.schedule
            .as_ref()
            .unwrap()
            .pool_state_word
            .as_ref()
            .unwrap()
            .uint256_be
            .len(),
        32
    );
}

#[test]
fn scheduled_wire_rejects_mutations() {
    let (_, w, _) = golden();
    for case in 0..10 {
        let mut q = w.clone();
        let p = &mut q.pools[0];
        let multi = p.multi_asset.as_mut().unwrap();
        let m = multi.managed_v2.as_mut().unwrap();
        match case {
            0 => m.schedule = None,
            1 => m.schedule.as_mut().unwrap().pool_state_word = None,
            2 => {
                m.schedule
                    .as_mut()
                    .unwrap()
                    .pool_state_word
                    .as_mut()
                    .unwrap()
                    .uint256_be = vec![0; 32]
            }
            3 => m.schedule.as_mut().unwrap().tokens[0].state_word = None,
            4 => {
                m.schedule.as_mut().unwrap().tokens[0]
                    .state_word
                    .as_mut()
                    .unwrap()
                    .uint256_be = vec![1]
            }
            5 => {
                m.schedule.as_mut().unwrap().tokens[0]
                    .state_word
                    .as_mut()
                    .unwrap()
                    .uint256_be[0] = 1
            }
            6 => m.schedule.as_mut().unwrap().tokens[1].token = p.token0.clone(),
            7 => multi.weights[0].uint256_be[31] -= 2,
            8 => p.adapter_version = "balancer-v2/20230411-managed-aum-3".into(),
            _ => {
                m.schedule.as_mut().unwrap().tokens[0]
                    .state_word
                    .as_mut()
                    .unwrap()
                    .uint256_be[23] += 1
            }
        }
        assert!(
            snapshot_from_proto(q).is_err(),
            "invalid scheduled wire case {case}"
        );
    }
}

#[test]
fn scheduled_required_json_and_nullable_fields() {
    let (s, _, _) = golden();
    let original = serde_json::to_value(&s.pools[0]).unwrap();
    for part in ["state", "schedule", "tokens", "dependencies", "token_info"] {
        let m = &original["multi_asset"]["managed_v2"];
        let objects = match part {
            "state" => vec![m],
            "schedule" => vec![&m["schedule"]],
            "tokens" => m["schedule"]["tokens"].as_array().unwrap().iter().collect(),
            _ => m[part].as_array().unwrap().iter().collect(),
        };
        for (index, object) in objects.iter().enumerate() {
            for field in object.as_object().unwrap().keys() {
                for null in [false, true] {
                    let mut p = original.clone();
                    let m = &mut p["multi_asset"]["managed_v2"];
                    let target = match part {
                        "state" => m,
                        "schedule" => &mut m["schedule"],
                        "tokens" => &mut m["schedule"]["tokens"][index],
                        _ => &mut m[part][index],
                    }
                    .as_object_mut()
                    .unwrap();
                    if null {
                        target.insert(field.clone(), Value::Null);
                    } else {
                        target.remove(field);
                    }
                    let decoded = serde_json::from_value::<Pool>(p);
                    if part == "state" && field == "schedule" && !null {
                        assert!(
                            decoded.unwrap().validate().is_err(),
                            "new profile requires optional field40"
                        );
                    } else {
                        assert!(decoded.is_err(), "missing/null {part}[{index}].{field}");
                    }
                }
            }
        }
    }
}

#[test]
fn scheduled_derived_same_h_cannot_be_snapshot_or_wire() {
    let (mut s, _, _) = golden();
    let m = s.pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap();
    m.execution_context = Some(ManagedV2ExecutionContext {
        block_number: m.block_number,
        timestamp: m.timestamp,
    });
    s.pools[0].validate().unwrap();
    assert!(s.validate(&s.reference.owner_service, 1).is_err());
    assert!(snapshot_to_proto(&s).is_err());
    assert!(s.pools[0].multi_asset.as_ref().unwrap().to_proto().is_err());
}

const FIELDS: [&str; 39] = [
    "timestamp",
    "block_number",
    "block_hash",
    "factory",
    "factory_code_sha256",
    "pool_runtime_template_sha256",
    "fee_provider",
    "fee_provider_code_sha256",
    "protocol_collector",
    "protocol_collector_code_sha256",
    "dependencies",
    "total_supply",
    "virtual_supply",
    "actual_supply",
    "pending_aum",
    "vault_bpt_cash",
    "vault_bpt_managed",
    "bpt_index",
    "manager",
    "manager_bpt_balance",
    "protocol_collector_bpt_balance",
    "management_aum_fee",
    "last_collection_timestamp",
    "provider_fee_id",
    "live_provider_aum_percentage",
    "cached_protocol_aum_percentage",
    "swaps_enabled",
    "paused",
    "recovery_mode",
    "allowlist_enabled",
    "pause_window_end",
    "buffer_period_end",
    "gradual_weight_end",
    "gradual_fee_end",
    "token_info",
    "vault_bpt_actual_balance",
    "vault_paused",
    "vault_pause_window_end",
    "vault_buffer_period_end",
];

#[test]
fn scheduled_all_original_39_wire_fields_remain_required() {
    let (_, w, _) = golden();
    let m = w.pools[0]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    for (i, field) in FIELDS.iter().enumerate() {
        let mut q = w.clone();
        let m = q.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        match i {
            0 => m.timestamp = None,
            1 => m.block_number = None,
            2 => m.block_hash.clear(),
            3 => m.factory.clear(),
            4 => m.factory_code_sha256.clear(),
            5 => m.pool_runtime_template_sha256.clear(),
            6 => m.fee_provider.clear(),
            7 => m.fee_provider_code_sha256.clear(),
            8 => m.protocol_collector.clear(),
            9 => m.protocol_collector_code_sha256.clear(),
            10 => m.dependencies.clear(),
            11 => m.total_supply = None,
            12 => m.virtual_supply = None,
            13 => m.actual_supply = None,
            14 => m.pending_aum = None,
            15 => m.vault_bpt_cash = None,
            16 => m.vault_bpt_managed = None,
            17 => m.bpt_index = None,
            18 => m.manager.clear(),
            19 => m.manager_bpt_balance = None,
            20 => m.protocol_collector_bpt_balance = None,
            21 => m.management_aum_fee = None,
            22 => m.last_collection_timestamp = None,
            23 => m.provider_fee_id = None,
            24 => m.live_provider_aum_percentage = None,
            25 => m.cached_protocol_aum_percentage = None,
            26 => m.swaps_enabled = None,
            27 => m.paused = None,
            28 => m.recovery_mode = None,
            29 => m.allowlist_enabled = None,
            30 => m.pause_window_end = None,
            31 => m.buffer_period_end = None,
            32 => m.gradual_weight_end = None,
            33 => m.gradual_fee_end = None,
            34 => m.token_info.clear(),
            35 => m.vault_bpt_actual_balance = None,
            36 => m.vault_paused = None,
            37 => m.vault_pause_window_end = None,
            38 => m.vault_buffer_period_end = None,
            _ => unreachable!(),
        }
        assert!(
            snapshot_from_proto(q).is_err(),
            "missing required {field} accepted"
        );
    }
    for index in 0..m.dependencies.len() {
        for field in 0..4 {
            let mut q = w.clone();
            let d = &mut q.pools[0]
                .multi_asset
                .as_mut()
                .unwrap()
                .managed_v2
                .as_mut()
                .unwrap()
                .dependencies[index];
            match field {
                0 => d.role.clear(),
                1 => d.address.clear(),
                2 => d.code_sha256.clear(),
                _ => d.template_sha256.clear(),
            }
            assert!(
                snapshot_from_proto(q).is_err(),
                "dependency {index} field {field}"
            );
        }
    }
    for index in 0..m.token_info.len() {
        for field in 0..6 {
            let mut q = w.clone();
            let t = &mut q.pools[0]
                .multi_asset
                .as_mut()
                .unwrap()
                .managed_v2
                .as_mut()
                .unwrap()
                .token_info[index];
            match field {
                0 => t.cash = None,
                1 => t.managed = None,
                2 => t.asset_manager.clear(),
                3 => t.last_change_block = None,
                4 => t.circuit_breaker_enabled = None,
                _ => t.vault_actual_balance = None,
            }
            assert!(
                snapshot_from_proto(q).is_err(),
                "token_info {index} field {field}"
            );
        }
    }
}

#[test]
fn scheduled_legacy_rejects_ended_field40() {
    let (mut s, _, _) = golden();
    let p = &mut s.pools[0];
    let multi = p.multi_asset.as_mut().unwrap();
    let m = multi.managed_v2.as_mut().unwrap();
    m.timestamp = 1767226663;
    s.reference.block.timestamp_ms = m.timestamp * 1000;
    multi.weights = m
        .schedule
        .as_ref()
        .unwrap()
        .at(m.timestamp)
        .unwrap()
        .weights;
    s.validate(&s.reference.owner_service, 1).unwrap();
    let mut w = snapshot_to_proto(&s).unwrap();
    let p = &mut s.pools[0];
    p.protocol = "balancer-v2-managed-aum".into();
    p.adapter_version = "balancer-v2/20230411-managed-aum-3".into();
    s.reference.adapter_set_hash = hash(&vec![&p.adapter_version]).unwrap();
    w.pools[0].protocol = p.protocol.clone();
    w.pools[0].adapter_version = p.adapter_version.clone();
    w.reference.as_mut().unwrap().adapter_set_hash =
        hex::decode(&s.reference.adapter_set_hash).unwrap();
    assert!(s.validate(&s.reference.owner_service, 1).is_err());
    assert!(
        snapshot_from_proto(w).is_err(),
        "legacy accepted ended field40"
    );
}

#[test]
fn scheduled_token_vector_and_amounts() {
    let (_, w, _) = golden();
    for count in [0, 1, 51] {
        let mut q = w.clone();
        let schedule = q.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap()
            .schedule
            .as_mut()
            .unwrap();
        let original = schedule.tokens.clone();
        schedule.tokens = (0..count).map(|i| original[i % 3].clone()).collect();
        assert!(
            snapshot_from_proto(q).is_err(),
            "token vector length {count}"
        );
    }
    for count in [0, 31, 33] {
        for pool_word in [false, true] {
            let mut q = w.clone();
            let schedule = q.pools[0]
                .multi_asset
                .as_mut()
                .unwrap()
                .managed_v2
                .as_mut()
                .unwrap()
                .schedule
                .as_mut()
                .unwrap();
            let amount = if pool_word {
                schedule.pool_state_word.as_mut().unwrap()
            } else {
                schedule.tokens[0].state_word.as_mut().unwrap()
            };
            amount.uint256_be = vec![0; count];
            assert!(
                snapshot_from_proto(q).is_err(),
                "word must be exactly32 bytes"
            );
        }
    }
    for index in 0..3 {
        for field in 0..3 {
            let mut q = w.clone();
            let token = &mut q.pools[0]
                .multi_asset
                .as_mut()
                .unwrap()
                .managed_v2
                .as_mut()
                .unwrap()
                .schedule
                .as_mut()
                .unwrap()
                .tokens[index];
            match field {
                0 => token.token.clear(),
                1 => token.state_word = None,
                _ => token.state_word.as_mut().unwrap().uint256_be = vec![0; 32],
            }
            assert!(
                snapshot_from_proto(q).is_err(),
                "token {index} field {field}"
            );
        }
    }
}
