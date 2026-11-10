use prost::Message;
use xone_domain::{Pool, Snapshot};
fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-bsc4-wire.json"
    ))
    .unwrap()
}
#[test]
fn bsc4_independent_cpp_field34_preserves_local_and_predecessor_states() {
    let f = vectors();
    assert_eq!(f["vectors"].as_array().unwrap().len(), 4);
    for v in f["vectors"].as_array().unwrap() {
        let snapshot: Snapshot = serde_json::from_value(v["snapshot"].clone()).unwrap();
        snapshot.validate("chain-parser-go", 1).unwrap();
        let raw = hex::decode(v["protobuf_hex"].as_str().unwrap()).unwrap();
        let w = xone_domain::wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
        let actual = xone_domain::wire_conversion::snapshot_from_proto(w.clone()).unwrap();
        assert_eq!(actual, snapshot);
        assert_eq!(
            xone_domain::wire_conversion::snapshot_to_proto(&snapshot).unwrap(),
            w
        );
    }
}
#[test]
fn bsc4_missing_migration_flags_nulls_and_legacy_substitution_reject() {
    let f = vectors();
    let pool = f["vectors"][1]["snapshot"]["pools"][0].clone();
    for key in ["initialization", "flags", "predecessors", "ledger"] {
        let mut b = pool.clone();
        b["pancake_bsc_lm"].as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<Pool>(b).is_err());
        let mut b = pool.clone();
        b["pancake_bsc_lm"][key] = serde_json::Value::Null;
        assert!(serde_json::from_value::<Pool>(b).is_err());
    }
    let p: Pool = serde_json::from_value(pool.clone()).unwrap();
    let state = p.pancake_bsc_lm.as_ref().unwrap();
    let mut w = state.to_proto().unwrap();
    w.initialization = None;
    assert!(xone_domain::pancake_bsc_lm::PancakeBscLmState::from_proto(w).is_err());
    let mut w = state.to_proto().unwrap();
    w.flags[0].migrated = None;
    assert!(xone_domain::pancake_bsc_lm::PancakeBscLmState::from_proto(w).is_err());
    let mut bad = p.clone();
    bad.protocol = "pancake-v3-lm-context".into();
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.pancake_lm = Some(state.ledger.clone());
    bad.pancake_bsc_lm = None;
    bad.protocol = "pancake-v3-lm-context".into();
    bad.adapter_version = "pancake-v3/98684794-lm-1".into();
    assert!(bad.validate().is_err());
    for key in [0, 1, 2] {
        let mut b = p.clone();
        b.pancake_bsc_lm.as_mut().unwrap().predecessors.remove(key);
        assert!(b.validate().is_err());
    }
}

#[test]
fn bsc4_predecessor_scope_and_token_alias_are_errors_not_panics() {
    let f = vectors();
    let pool: Pool =
        serde_json::from_value(f["vectors"][1]["snapshot"]["pools"][0].clone()).unwrap();
    for mode in 0..6 {
        let mut bad = pool.clone();
        let state = bad.pancake_bsc_lm.as_mut().unwrap();
        match mode {
            0 => state.predecessors.push(state.predecessors[0].clone()),
            1 => state.flags.pop().map(|_| ()).unwrap(),
            2 => state.predecessors[2].flags.clear(),
            3 => state.predecessors[0].flags.push(state.flags[0].clone()),
            4 => state.predecessors[0].address = bad.token0.clone(),
            _ => state.predecessors[2].ticks[0].tick += 10,
        };
        assert!(bad.validate().is_err(), "mode {mode}");
    }
}

#[test]
fn bsc4_shared_predecessors_cannot_present_conflicting_readonly_banks() {
    let f = vectors();
    let pool: Pool =
        serde_json::from_value(f["vectors"][1]["snapshot"]["pools"][0].clone()).unwrap();
    let mut other = pool.clone();
    other.pancake_bsc_lm.as_mut().unwrap().predecessors[0].ticks[0].reward_growth_outside =
        "999".parse().unwrap();
    assert!(xone_domain::validate_pool_dependencies([&pool, &other]).is_err());
}

#[test]
fn bsc4_raw_source_and_generated_wire_decode_without_partial_or_panic() {
    let f = vectors();
    let source: Pool =
        serde_json::from_value(f["vectors"][1]["snapshot"]["pools"][0].clone()).unwrap();
    let state = source.pancake_bsc_lm.as_ref().unwrap();
    for mode in 0..5 {
        let mut wire = state.to_proto().unwrap();
        match mode {
            0 => wire.predecessors[0].ticks.clear(),
            1 => wire.predecessors[2].flags.clear(),
            2 => wire.flags.clear(),
            3 => wire.predecessors.push(wire.predecessors[0].clone()),
            _ => wire.ledger.as_mut().unwrap().factory_template_sha256 = vec![0; 32],
        };
        assert!(
            xone_domain::pancake_bsc_lm::PancakeBscLmState::from_proto(wire).is_err(),
            "mode {mode}"
        );
    }
}
