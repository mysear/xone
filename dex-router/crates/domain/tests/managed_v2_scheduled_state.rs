use num_bigint::BigUint;
use serde_json::{json, Value};
use xone_domain::{
    managed_v2_schedule::{ManagedV2Schedule, ManagedV2ScheduledToken, ADAPTER_VERSION, PROTOCOL},
    Amount, Pool,
};
const ONE: u64 = 1_000_000_000_000_000_000;
fn golden() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-aum-wire.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn packed(start: u64, end: u64, a: u64, b: u64, flags: u64) -> Amount {
    Amount(
        BigUint::from(start)
            | (BigUint::from(end) << 32usize)
            | (BigUint::from(start) << 64usize)
            | (BigUint::from(end) << 96usize)
            | (BigUint::from(a) << 128usize)
            | (BigUint::from(b) << 190usize)
            | (BigUint::from(flags) << 252usize),
    )
}
// Synthetic domain fixture, seeded from the actual old golden Pool. AUM is
// explicitly zero; this test supplies no independent AUM execution authority.
fn scheduled_json() -> Value {
    let mut p = golden()["original_fields"].clone();
    p["protocol"] = json!(PROTOCOL);
    p["adapter_version"] = json!(ADAPTER_VERSION);
    let time = p["multi_asset"]["managed_v2"]["timestamp"]
        .as_u64()
        .unwrap();
    let tokens = p["multi_asset"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (start, end) = if i == 0 {
                (8 * ONE / 10, 2 * ONE / 10)
            } else {
                (2 * ONE / 10, 8 * ONE / 10)
            };
            ManagedV2ScheduledToken {
                token: a["token"].as_str().unwrap().into(),
                state_word: Amount(BigUint::from(start) | (BigUint::from(end) << 64usize)),
            }
        })
        .collect();
    let raw = ManagedV2Schedule {
        pool_state_word: packed(time - 1, time + 2_591_999, ONE / 100, 3 * ONE / 100, 9),
        tokens,
    };
    let v = raw.at(time).unwrap();
    p["fee_numerator"] = json!(v.swap_fee.to_string().parse::<u64>().unwrap());
    p["multi_asset"]["weights"] = serde_json::to_value(v.weights).unwrap();
    for (i, scale) in v.scaling.iter().enumerate() {
        p["multi_asset"]["assets"][i]["scaling"] = json!(scale.to_string());
    }
    let m = &mut p["multi_asset"]["managed_v2"];
    m["management_aum_fee"] = json!("0");
    m["pending_aum"] = json!("0");
    m["actual_supply"] = m["virtual_supply"].clone();
    m["gradual_weight_end"] = json!(v.weight_end);
    m["gradual_fee_end"] = json!(v.fee_end);
    m["schedule"] = serde_json::to_value(raw).unwrap();
    p
}
#[test]
fn scheduled_parent_accepts_active_schedule() {
    let p: Pool = serde_json::from_value(scheduled_json()).expect("field40 JSON schedule");
    p.validate().unwrap();
}
#[test]
fn scheduled_profile_is_registered() {
    let profile = xone_domain::protocols::protocol(PROTOCOL).expect("scheduled multi profile");
    assert_eq!(profile.family, "multi");
    assert_eq!(profile.adapter, ADAPTER_VERSION);
    assert_eq!(profile.fee_denominator, ONE);
    assert!(profile.require_identity);
}

use prost::Message;
use xone_domain::{
    managed_v2_state::{BalancerManagedV2State, ManagedV2ExecutionContext},
    multi_asset::MultiAssetState,
    scope_hash, wire, Snapshot,
};
fn pool() -> Pool {
    serde_json::from_value(scheduled_json()).unwrap()
}
fn base(p: &Pool) -> &BalancerManagedV2State {
    p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap()
}
fn base_mut(p: &mut Pool) -> &mut BalancerManagedV2State {
    p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap()
}
fn project(p: &mut Pool) {
    let s = p.multi_asset.as_mut().unwrap();
    let m = s.managed_v2.as_mut().unwrap();
    let time = m
        .execution_context
        .as_ref()
        .map_or(m.timestamp, |c| c.timestamp);
    let v = m.schedule.as_ref().unwrap().at(time).unwrap();
    s.weights = v.weights;
    for (asset, scale) in s.assets.iter_mut().zip(v.scaling) {
        asset.scaling = scale;
    }
    m.gradual_weight_end = v.weight_end;
    m.gradual_fee_end = v.fee_end;
    m.swaps_enabled = v.swaps_enabled;
    m.recovery_mode = v.recovery_mode;
    m.allowlist_enabled = v.must_allowlist_lps;
    p.fee_numerator = v.swap_fee.to_string().parse().unwrap();
}
fn snapshot(p: Pool) -> Snapshot {
    let mut s: Snapshot = serde_json::from_value(golden()["snapshot"].clone()).unwrap();
    s.pools = vec![p];
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    s.reference.adapter_set_hash = xone_domain::hash(&vec![&s.pools[0].adapter_version]).unwrap();
    s
}
fn reject(p: Pool) {
    assert!(p.validate().is_err());
    assert!(xone_domain::managed_v2_dependencies::validate([&p]).is_err());
}

#[test]
fn profile_requires_exact_version_and_schedule_presence() {
    let p = pool();
    p.validate().unwrap();
    let mut missing = p.clone();
    base_mut(&mut missing).schedule = None;
    reject(missing);
    let mut old = p.clone();
    old.protocol = xone_domain::managed_v2_state::PROTOCOL.into();
    old.adapter_version = xone_domain::managed_v2_state::ADAPTER_VERSION.into();
    reject(old);
    for version in [
        "balancer-v2/20230411-managed-aum-1",
        "balancer-v2/20230411-managed-aum-2",
        xone_domain::managed_v2_state::ADAPTER_VERSION,
        "balancer-v2/20230411-managed-scheduled-2",
    ] {
        let mut bad = p.clone();
        bad.adapter_version = version.into();
        reject(bad);
    }
    let mut old: Pool = serde_json::from_value(golden()["original_fields"].clone()).unwrap();
    old.validate().unwrap();
    base_mut(&mut old).gradual_weight_end += 1;
    reject(old);
    for remove_multi in [false, true] {
        let mut bad = p.clone();
        if remove_multi {
            bad.multi_asset = None;
        } else {
            bad.multi_asset.as_mut().unwrap().managed_v2 = None;
        }
        reject(bad);
    }
}

#[test]
fn every_visible_projection_must_match_raw() {
    let p = pool();
    for which in 0..12 {
        let mut bad = p.clone();
        match which {
            0 => bad.fee_numerator += 1,
            1 => bad.fee_denominator -= 1,
            2 => bad.multi_asset.as_mut().unwrap().weights[0].0 += 1u8,
            3 => bad.multi_asset.as_mut().unwrap().assets[0].scaling.0 *= 10u8,
            4 => base_mut(&mut bad).swaps_enabled ^= true,
            5 => base_mut(&mut bad).recovery_mode ^= true,
            6 => base_mut(&mut bad).allowlist_enabled ^= true,
            7 => base_mut(&mut bad).gradual_weight_end += 1,
            8 => base_mut(&mut bad).gradual_fee_end += 1,
            9 => base_mut(&mut bad)
                .schedule
                .as_mut()
                .unwrap()
                .tokens
                .swap(0, 1),
            10 => {
                base_mut(&mut bad).schedule.as_mut().unwrap().tokens[0].token =
                    "0x0000000000000000000000000000000000000001".into()
            }
            _ => {
                base_mut(&mut bad).schedule.as_mut().unwrap().tokens[0]
                    .state_word
                    .0 += BigUint::from(1u8) << 64usize
            }
        }
        reject(bad);
    }
    // The raw token metadata, rather than any valid decimal scaling, is authoritative.
    let mut bad = p.clone();
    base_mut(&mut bad).schedule.as_mut().unwrap().tokens[0]
        .state_word
        .0 |= BigUint::from(1u8) << 128usize;
    reject(bad);
    for which in 0..5 {
        let mut bad = p.clone();
        let m = base_mut(&mut bad);
        match which {
            0 => m.swaps_enabled ^= true,
            1 => m.recovery_mode ^= true,
            2 => m.allowlist_enabled ^= true,
            3 => m.gradual_weight_end += 1,
            _ => m.gradual_fee_end += 1,
        }
        assert!(base(&bad).validate().is_err());
    }
}

#[test]
fn base39_constraints_are_shared_by_scheduled_profile() {
    let p = pool();
    let value = serde_json::to_value(&p).unwrap();
    for (path, bad) in [
        (
            "/multi_asset/managed_v2/factory_code_sha256",
            json!("A".repeat(64)),
        ),
        (
            "/multi_asset/managed_v2/dependencies/0/role",
            json!("unknown"),
        ),
        ("/multi_asset/managed_v2/actual_supply", json!("1")),
        ("/multi_asset/managed_v2/pending_aum", json!("1")),
        ("/multi_asset/managed_v2/provider_fee_id", json!("256")),
        (
            "/multi_asset/managed_v2/vault_bpt_actual_balance",
            json!("0"),
        ),
        ("/multi_asset/managed_v2/token_info/0/cash", json!("0")),
        (
            "/multi_asset/managed_v2/token_info/0/circuit_breaker_enabled",
            json!(true),
        ),
        (
            "/multi_asset/managed_v2/manager_bpt_balance",
            json!("9".repeat(70)),
        ),
        (
            "/multi_asset/managed_v2/token_info/0/vault_actual_balance",
            json!("0"),
        ),
    ] {
        let mut v = value.clone();
        *v.pointer_mut(path).unwrap() = bad;
        reject(serde_json::from_value(v).unwrap());
    }
    for field in ["curve_crypto", "curve_ng", "curve_meta", "managed_fees"] {
        let mut v = value.clone();
        v["multi_asset"][field] = json!({});
        assert!(serde_json::from_value::<Pool>(v).is_err());
    }
}

#[test]
fn raw_current_drift_two_units_is_valid_but_never_arbitrary() {
    for positive in [true, false] {
        let mut p = pool();
        let time = base(&p).timestamp;
        let s = p.multi_asset.as_mut().unwrap();
        let mut asset = s.assets[1].clone();
        asset.token = "0xf000000000000000000000000000000000000003".into();
        s.assets.push(asset);
        let m = s.managed_v2.as_mut().unwrap();
        m.token_info.push(m.token_info[1].clone());
        let (starts, ends, expected) = if positive {
            (
                [
                    500000000000000001u64,
                    300000000000000000,
                    200000000000000000,
                ],
                [
                    100000000000000000u64,
                    800000000000000000,
                    100000000000000000,
                ],
                ONE + 2,
            )
        } else {
            (
                [99999999999999999u64, 800000000000000000, 100000000000000000],
                [
                    500000000000000000u64,
                    300000000000000000,
                    200000000000000000,
                ],
                ONE - 2,
            )
        };
        m.schedule = Some(ManagedV2Schedule {
            pool_state_word: packed(time - 1, time + 1000, ONE / 100, 3 * ONE / 100, 9),
            tokens: s
                .assets
                .iter()
                .enumerate()
                .map(|(i, a)| ManagedV2ScheduledToken {
                    token: a.token.clone(),
                    state_word: Amount(
                        BigUint::from(starts[i]) | (BigUint::from(ends[i]) << 64usize),
                    ),
                })
                .collect(),
        });
        project(&mut p);
        p.validate().unwrap();
        assert_eq!(
            p.multi_asset
                .as_ref()
                .unwrap()
                .weights
                .iter()
                .map(|v| v.0.clone())
                .sum::<BigUint>(),
            BigUint::from(expected)
        );
        let multi = p.multi_asset.as_ref().unwrap();
        assert_eq!(
            MultiAssetState::from_proto(multi.to_proto().unwrap()).unwrap(),
            *multi
        );
        let mut bad = p.clone();
        bad.multi_asset.as_mut().unwrap().weights[0].0 += 1u8;
        reject(bad);
        let mut bad = p;
        base_mut(&mut bad).schedule.as_mut().unwrap().tokens[0]
            .state_word
            .0 += BigUint::from(1u8) << 64usize;
        reject(bad);
    }
}

#[test]
fn all_flags_including_join_exit_stay_in_raw_schedule() {
    for flags in 0..16 {
        let mut p = pool();
        let time = base(&p).timestamp;
        base_mut(&mut p).schedule.as_mut().unwrap().pool_state_word =
            packed(time - 1, time + 1000, ONE / 100, 3 * ONE / 100, flags);
        project(&mut p);
        p.validate().unwrap();
        let raw = base(&p).schedule.as_ref().unwrap();
        assert_eq!(raw.at(time).unwrap().join_exit_enabled, flags & 8 != 0);
        let wire = p.multi_asset.as_ref().unwrap().to_proto().unwrap();
        assert_eq!(
            MultiAssetState::from_proto(wire).unwrap(),
            *p.multi_asset.as_ref().unwrap()
        );
    }
}

#[test]
fn field40_json_presence_and_wire_required_words() {
    let p = pool();
    let value = serde_json::to_value(&p).unwrap();
    let mut null = value.clone();
    null["multi_asset"]["managed_v2"]["schedule"] = Value::Null;
    assert!(serde_json::from_value::<Pool>(null).is_err());
    let old: Pool = serde_json::from_value(golden()["original_fields"].clone()).unwrap();
    let old_json = serde_json::to_value(&old).unwrap();
    assert!(old_json["multi_asset"]["managed_v2"]
        .get("schedule")
        .is_none());
    assert_eq!(
        old_json["multi_asset"]["managed_v2"]
            .as_object()
            .unwrap()
            .len(),
        39
    );
    let multi = p.multi_asset.as_ref().unwrap();
    let w = multi.to_proto().unwrap();
    assert!(w.managed_v2.as_ref().unwrap().schedule.is_some());
    let bytes = w.encode_to_vec();
    assert_eq!(
        MultiAssetState::from_proto(wire::MultiAssetState::decode(bytes.as_slice()).unwrap())
            .unwrap(),
        *multi
    );
    for which in 0..9 {
        let mut bad = w.clone();
        let raw = bad.managed_v2.as_mut().unwrap().schedule.as_mut().unwrap();
        match which {
            0 => raw.pool_state_word = None,
            1 => raw.tokens[0].state_word = None,
            2 => raw.tokens.clear(),
            3 => raw
                .pool_state_word
                .as_mut()
                .unwrap()
                .uint256_be
                .pop()
                .map(|_| ())
                .unwrap(),
            4 => raw.tokens[0]
                .state_word
                .as_mut()
                .unwrap()
                .uint256_be
                .push(0),
            5 => raw.tokens[0].token = "0x000000000000000000000000000000000000000A".into(),
            6 => raw.tokens[0].token.clear(),
            7 => raw.tokens[0].state_word.as_mut().unwrap().uint256_be[0] = 1,
            _ => raw.tokens.pop().map(|_| ()).unwrap(),
        }
        assert!(MultiAssetState::from_proto(bad).is_err(), "case {which}");
    }
    let mut missing = w;
    missing.managed_v2.as_mut().unwrap().schedule = None;
    assert!(
        MultiAssetState::from_proto(missing).is_err(),
        "active times cannot silently lose field40"
    );
    // At an ended schedule only the parent protocol can distinguish missing 40.
    let mut ended = p.clone();
    base_mut(&mut ended).timestamp += 3_000_000;
    project(&mut ended);
    let mut v = serde_json::to_value(ended).unwrap();
    v["multi_asset"]["managed_v2"]
        .as_object_mut()
        .unwrap()
        .remove("schedule");
    reject(serde_json::from_value(v).unwrap());
}

#[test]
fn derived_context_roundtrip_deep_clone_and_canonical_guards() {
    let mut p = pool();
    let source = serde_json::to_value(&p).unwrap();
    let m = base_mut(&mut p);
    m.execution_context = Some(ManagedV2ExecutionContext {
        block_number: m.block_number + 1,
        timestamp: m.timestamp + 1,
    });
    project(&mut p);
    p.validate().unwrap();
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(serde_json::from_str::<Pool>(&json).unwrap(), p);
    let mut clone = p.clone();
    base_mut(&mut clone).schedule.as_mut().unwrap().tokens[0]
        .state_word
        .0 += 1u8;
    assert_ne!(base(&clone).schedule, base(&p).schedule);
    assert!(p.multi_asset.as_ref().unwrap().to_proto().is_err());
    let s = snapshot(p);
    assert!(s
        .validate(
            &s.reference.owner_service,
            s.reference.owner_assignment_version
        )
        .is_err());
    assert!(xone_domain::wire_conversion::snapshot_to_proto(&s).is_err());
    let original: Pool = serde_json::from_value(source).unwrap();
    let s = snapshot(original);
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    let w = xone_domain::wire_conversion::snapshot_to_proto(&s).unwrap();
    assert_eq!(
        xone_domain::wire_conversion::snapshot_from_proto(w).unwrap(),
        s
    );
}

#[test]
fn snapshot_source_header_closure_remains_exact() {
    let p = pool();
    let s = snapshot(p);
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    for which in 0..4 {
        let mut bad = s.clone();
        let m = base_mut(&mut bad.pools[0]);
        match which {
            0 => m.timestamp += 1,
            1 => m.block_number += 1,
            2 => m.block_hash = format!("0x{}", "b".repeat(64)),
            _ => m.token_info[0].last_change_block = m.block_number + 1,
        }
        project(&mut bad.pools[0]);
        assert!(bad
            .validate(
                &s.reference.owner_service,
                s.reference.owner_assignment_version
            )
            .is_err());
    }
}

#[test]
fn scheduled_and_static_share_managed_vault_facts() {
    let mut a = pool();
    let mut b: Pool = serde_json::from_value(golden()["original_fields"].clone()).unwrap();
    b.pool_id = "0x0000000000000000000000000000000000000099".into();
    b.multi_asset.as_mut().unwrap().pool_id = format!("{}0001{}", b.pool_id, "0".repeat(20));
    for p in [&mut a, &mut b] {
        for info in &mut base_mut(p).token_info {
            info.vault_actual_balance.0 *= 2u8;
        }
        p.validate().unwrap();
    }
    xone_domain::managed_v2_dependencies::validate([&a, &b]).unwrap();
    assert!(
        xone_domain::managed_v2_dependencies::blocked_pools([&a, &b])
            .unwrap()
            .is_empty()
    );
    for which in 0..4 {
        let mut bad = b.clone();
        let m = base_mut(&mut bad);
        match which {
            0 => m.block_hash = format!("0x{}", "b".repeat(64)),
            1 => m.token_info[0].vault_actual_balance.0 += 1u8,
            2 => m.live_provider_aum_percentage.0 += 1u8,
            _ => {
                m.execution_context = Some(ManagedV2ExecutionContext {
                    block_number: m.block_number + 1,
                    timestamp: m.timestamp + 1,
                })
            }
        }
        assert!(xone_domain::managed_v2_dependencies::validate([&a, &bad]).is_err());
    }
}

#[test]
fn known_zero_clocks_and_direct_jump_roundtrip() {
    let mut p = pool();
    let m = base_mut(&mut p);
    m.timestamp = 0;
    m.block_number = 0;
    m.last_collection_timestamp = 0;
    for info in &mut m.token_info {
        info.last_change_block = 0;
    }
    m.schedule.as_mut().unwrap().pool_state_word = packed(0, 0, ONE / 100, 3 * ONE / 100, 0);
    project(&mut p);
    p.validate().unwrap();
    let m = base(&p);
    assert_eq!(m.gradual_weight_end, 0);
    assert_eq!(m.gradual_fee_end, 0);
    assert!(!m.swaps_enabled);
    assert_eq!(
        p.multi_asset.as_ref().unwrap().weights[0],
        "200000000000000000".parse().unwrap()
    );
    let multi = p.multi_asset.as_ref().unwrap();
    let w = multi.to_proto().unwrap();
    let m = w.managed_v2.as_ref().unwrap();
    assert_eq!(m.timestamp, Some(0));
    assert_eq!(m.swaps_enabled, Some(false));
    assert!(m.schedule.is_some());
    assert_eq!(MultiAssetState::from_proto(w).unwrap(), *multi);
}

#[test]
fn field40_remains_required_after_schedule_ends() {
    let mut p = pool();
    let time = base(&p).timestamp;
    base_mut(&mut p).schedule.as_mut().unwrap().pool_state_word =
        packed(time, time, ONE / 100, 3 * ONE / 100, 9);
    project(&mut p);
    p.validate().unwrap();
    let s = snapshot(p);
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    let mut w = xone_domain::wire_conversion::snapshot_to_proto(&s).unwrap();
    w.pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .schedule = None;
    assert_eq!(
        xone_domain::wire_conversion::snapshot_from_proto(w)
            .unwrap_err()
            .0,
        "MANAGED_V2_PROFILE_MISMATCH"
    );
}

#[test]
fn join_exit_bit_never_opens_own_bpt_as_underlying() {
    let mut p = pool();
    let own = p.pool_id.clone();
    p.token0 = own.clone();
    let s = p.multi_asset.as_mut().unwrap();
    s.assets[0].token = own.clone();
    s.managed_v2
        .as_mut()
        .unwrap()
        .schedule
        .as_mut()
        .unwrap()
        .tokens[0]
        .token = own;
    assert!(
        base(&p)
            .schedule
            .as_ref()
            .unwrap()
            .at(base(&p).timestamp)
            .unwrap()
            .join_exit_enabled
    );
    assert_eq!(p.validate().unwrap_err().0, "MANAGED_V2_ASSET");
}
