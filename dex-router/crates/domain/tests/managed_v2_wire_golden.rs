use prost::Message;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use xone_domain::{
    hash as state_hash,
    multi_asset::MultiAssetState,
    scope_hash, wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Amount, Pool, Snapshot,
};

const BANK_SHA: &str = "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66";
const POOL_SHA: &str = "736d758cba41e28332db994c15b3d9fe13fe04fe08f57e9f80ed90425e35002c";
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

// Native authority covers the observed local replay Pool. The synthetic snapshot
// envelope and C++ wire agreement establish no live-chain trust or execution proof.
fn golden() -> (Snapshot, wire::OnlineSnapshot, Vec<u8>) {
    // Runtime loading keeps a missing fixture a test RED rather than a compiler error.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-aum-wire.json");
    let f: Value = serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .unwrap();
    assert_eq!(f["original_bank_sha256"], BANK_SHA);
    assert_eq!(f["original_bank_bytes"], 7905624u64);
    assert_eq!(f["authority"], "native-state + synthetic snapshot envelope");
    assert_eq!(
        f["source_refs"],
        json!({
            "bank_sha256": BANK_SHA,
            "snapshot": {"ref":"f2b87673a859fedade77f33b274c251f5e2e76f595705497d0dfc1c6dc455f1d"},
            "state_header": {"ref":"89b0bd3d9e255afa44d247e4c4b3fb4b10c393b8a4befcffb459533ad7340452"},
            "origin_snapshot": {"ref":"f2b87673a859fedade77f33b274c251f5e2e76f595705497d0dfc1c6dc455f1d"}
        })
    );
    assert_eq!(
        state_hash(&f["original_fields"]).unwrap(),
        POOL_SHA,
        "complete original Pool differs from frozen first execution.before projection"
    );
    let original: Pool = serde_json::from_value(f["original_fields"].clone()).unwrap();
    let s: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    assert_eq!(s.pools, vec![original]);
    assert_eq!(
        serde_json::to_value(&s.pools[0]).unwrap(),
        f["original_fields"]
    );
    let b = &s.reference.block;
    assert_eq!(b.chain_key, "evm:31337");
    assert_eq!(b.number_or_slot, 26);
    assert_eq!(b.timestamp_ms, 1767225600000);
    assert_eq!(
        b.hash,
        "0xc37cd406f5e09888d3d5a8517cb32e277a7d1ba2f438a87b248407e7b1dc5f4b"
    );
    assert_eq!(
        b.parent_hash,
        "0xf4cce27eb31871df28e471708277aa7c57dac7a919fc98e2ff086d5dcf7922c1"
    );
    assert_ne!(
        s.source, "CURRENT_CHAIN_STATE",
        "synthetic envelope claimed current chain authority"
    );
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    assert_eq!(s.reference.scope_hash, scope_hash(&s.pools).unwrap());
    assert_eq!(
        s.reference.adapter_set_hash,
        state_hash(&vec![&s.pools[0].adapter_version]).unwrap()
    );
    let raw = hex::decode(f["protobuf_hex"].as_str().unwrap()).unwrap();
    assert_eq!(
        hex::encode(Sha256::digest(&raw)),
        f["protobuf_sha256"].as_str().unwrap(),
        "independent C++ protobuf digest"
    );
    let w = wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
    (s, w, raw)
}

#[test]
fn managed_v2_native_wire_golden() {
    let (s, w, raw) = golden();
    assert_eq!(
        snapshot_to_proto(&s).unwrap().encode_to_vec(),
        raw,
        "Rust marshal differs from independent C++ protoc bytes"
    );
    let round = snapshot_from_proto(w.clone()).unwrap();
    assert_eq!(round, s, "complete Snapshot/Pool lost during wire decode");
    assert_eq!(state_hash(&round).unwrap(), state_hash(&s).unwrap());
    assert_eq!(snapshot_to_proto(&round).unwrap().encode_to_vec(), raw);
    let m = w.pools[0]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert_eq!(m.dependencies.len(), 5);
    assert_eq!(
        m.token_info.len(),
        s.pools[0].multi_asset.as_ref().unwrap().assets.len()
    );
    // Prost has no runtime reflection: enumerate every one of the 39 fields.
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
    // Flip an actual serialized bank hash byte while leaving protobuf decodable.
    let offset = raw
        .windows(m.block_hash.len())
        .position(|v| v == m.block_hash)
        .unwrap();
    let mut corrupt = raw.clone();
    corrupt[offset] ^= 1;
    assert_ne!(Sha256::digest(&corrupt), Sha256::digest(&raw));
    let bad = wire::OnlineSnapshot::decode(corrupt.as_slice()).unwrap();
    assert!(
        snapshot_from_proto(bad).is_err(),
        "flipped source-bank byte accepted"
    );
    // A structurally valid changed dependency identity must alter the full hash.
    let mut changed = w;
    changed.pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .dependencies[0]
        .code_sha256[0] ^= 1;
    let changed = snapshot_from_proto(changed).unwrap();
    assert_ne!(state_hash(&changed).unwrap(), state_hash(&s).unwrap());
}

#[test]
fn managed_v2_required_json_wire_golden() {
    let (s, _, _) = golden();
    let original = serde_json::to_value(&s.pools[0]).unwrap();
    let m = &original["multi_asset"]["managed_v2"];
    let mut actual_fields: Vec<_> = m.as_object().unwrap().keys().map(String::as_str).collect();
    let mut expected_fields = FIELDS.to_vec();
    actual_fields.sort();
    expected_fields.sort();
    assert_eq!(actual_fields, expected_fields);
    for part in ["state", "dependencies", "token_info"] {
        let objects = if part == "state" {
            vec![m]
        } else {
            m[part].as_array().unwrap().iter().collect()
        };
        for (index, object) in objects.iter().enumerate() {
            for field in object.as_object().unwrap().keys() {
                for null in [false, true] {
                    let mut p = original.clone();
                    let target = if part == "state" {
                        &mut p["multi_asset"]["managed_v2"]
                    } else {
                        &mut p["multi_asset"]["managed_v2"][part][index]
                    };
                    let target = target.as_object_mut().unwrap();
                    if null {
                        target.insert(field.clone(), Value::Null);
                    } else {
                        target.remove(field);
                    }
                    assert!(
                        serde_json::from_value::<Pool>(p).is_err(),
                        "missing/null {part}[{index}].{field} accepted"
                    );
                }
            }
        }
    }
}

#[test]
fn managed_v2_known_zero_presence_wire_golden() {
    let (s, _, _) = golden();
    // This separate synthetic variant exercises Known(0/false) presence, without
    // assigning original execution authority to the modified state.
    let mut state = s.pools[0].multi_asset.clone().unwrap();
    let m = state.managed_v2.as_mut().unwrap();
    m.timestamp = 0;
    m.block_number = 0;
    m.last_collection_timestamp = 0;
    m.bpt_index = 0;
    m.pause_window_end = 0;
    m.buffer_period_end = 0;
    m.gradual_weight_end = 0;
    m.gradual_fee_end = 0;
    m.swaps_enabled = false;
    m.paused = false;
    m.recovery_mode = false;
    m.allowlist_enabled = false;
    m.provider_fee_id = "0".parse().unwrap();
    m.management_aum_fee = "0".parse().unwrap();
    m.live_provider_aum_percentage = "0".parse().unwrap();
    m.cached_protocol_aum_percentage = "0".parse().unwrap();
    for token in &mut m.token_info {
        token.last_change_block = 0;
        token.circuit_breaker_enabled = false;
    }
    let raw = state.to_proto().unwrap().encode_to_vec();
    let w = wire::MultiAssetState::decode(raw.as_slice()).unwrap();
    let m = w.managed_v2.as_ref().unwrap();
    for value in [
        m.timestamp,
        m.block_number,
        m.last_collection_timestamp,
        m.bpt_index,
        m.pause_window_end,
        m.buffer_period_end,
        m.gradual_weight_end,
        m.gradual_fee_end,
    ] {
        assert_eq!(value, Some(0), "Known(0) presence lost after serialization");
    }
    for value in [
        m.swaps_enabled,
        m.paused,
        m.recovery_mode,
        m.allowlist_enabled,
    ] {
        assert_eq!(
            value,
            Some(false),
            "Known(false) presence lost after serialization"
        );
    }
    for token in &m.token_info {
        assert_eq!(token.last_change_block, Some(0));
        assert_eq!(token.circuit_breaker_enabled, Some(false));
    }
    assert_eq!(MultiAssetState::from_proto(w).unwrap(), state);
}

#[test]
fn canonical_registration_and_fee_id_reject_impossible_original_values() {
    let (source, _, _) = golden();
    for which in 0..7 {
        let mut changed = source.clone();
        let m = changed.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        match which {
            0 => m.bpt_index = 1,
            1 => {
                m.vault_bpt_cash.0 -= 1u8;
                m.vault_bpt_managed = "1".parse().unwrap();
            }
            2 => m.provider_fee_id = "256".parse().unwrap(),
            3 => {
                m.token_info[0].cash.0 -= 1u8;
                m.token_info[0].managed = "1".parse().unwrap();
            }
            4 => m.recovery_mode = true,
            _ => {
                m.recovery_mode = which == 5;
                m.management_aum_fee = Amount::zero();
                m.pending_aum = "1".parse().unwrap();
                m.actual_supply.0 = &m.virtual_supply.0 + 1u8;
            }
        }
        assert!(
            changed.pools[0].validate().is_err(),
            "impossible original Pool {which}"
        );
        assert!(
            changed
                .validate(
                    &source.reference.owner_service,
                    source.reference.owner_assignment_version
                )
                .is_err(),
            "impossible original Snapshot {which}"
        );
        assert!(
            changed.pools[0]
                .multi_asset
                .as_ref()
                .unwrap()
                .to_proto()
                .is_err(),
            "impossible original wire {which}"
        );
    }
}
