use xone_domain::Pool;
fn pool() -> Pool {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-lm-state.json"
    ))
    .unwrap();
    serde_json::from_value(f["cases"][2]["pool"].clone()).unwrap()
}
#[test]
fn required_lm_wire_scalar_presence_and_complete_readonly_views() {
    let p = pool();
    p.validate().unwrap();
    let s = p.pancake_lm.as_ref().unwrap();
    let wire = s.to_proto().unwrap();
    assert_eq!(wire.derived, Some(false));
    assert_eq!(wire.tick, Some(0));
    assert_eq!(
        xone_domain::pancake_lm::PancakeLmState::from_proto(wire.clone()).unwrap(),
        *s
    );
    for field in [
        "tick",
        "fee_protocol",
        "unlocked",
        "derived",
        "observation_index",
    ] {
        let mut v = serde_json::to_value(s).unwrap();
        v.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<xone_domain::pancake_lm::PancakeLmState>(v).is_err(),
            "{}",
            field
        );
    }
    let mut missing = wire.clone();
    missing.tick = None;
    assert!(xone_domain::pancake_lm::PancakeLmState::from_proto(missing).is_err());
    let mut missing = wire;
    missing.derived = None;
    assert!(xone_domain::pancake_lm::PancakeLmState::from_proto(missing).is_err());
}
#[test]
fn unknown_lm_code_reward_view_and_profile_smuggling_reject() {
    let original = pool();
    for mode in ["lm-code", "chef-code", "view", "binding", "tick", "profile"] {
        let mut p = original.clone();
        let s = p.pancake_lm.as_mut().unwrap();
        match mode {
            "lm-code" => s.lm.template_sha256 = "a".repeat(64),
            "chef-code" => s.master_chef.template_sha256 = "a".repeat(64),
            "view" => s.master_chef.view_reward_per_second = xone_domain::Amount(1u64.into()),
            "binding" => s.lm.pool = p.token0.clone(),
            "tick" => s.tick += 1,
            _ => {
                p.protocol = "pancake-v3".into();
                p.adapter_version = "pancake-v3/core-1".into();
            }
        }
        assert!(p.validate().is_err(), "{}", mode);
    }
}
#[test]
fn independent_cpp_field33_snapshot_preserves_complete_zero_and_false_presence() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/pancake-lm-wire.json")).unwrap();
    assert_eq!(f["vectors"].as_array().unwrap().len(), 4);
    use prost::Message;
    for v in f["vectors"].as_array().unwrap() {
        let raw = hex::decode(v["protobuf_hex"].as_str().unwrap()).unwrap();
        let snapshot: xone_domain::Snapshot =
            serde_json::from_value(v["snapshot"].clone()).unwrap();
        snapshot.validate("chain-parser-go", 1).unwrap();
        let decoded = xone_domain::wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
        assert_eq!(
            xone_domain::wire_conversion::snapshot_from_proto(decoded).unwrap(),
            snapshot
        );
        assert_eq!(
            xone_domain::wire_conversion::snapshot_to_proto(&snapshot)
                .unwrap()
                .encode_to_vec(),
            raw
        );
    }
}
#[test]
fn master_chef_and_lm_shared_readonly_facts_cannot_conflict_across_pools() {
    let p = pool();
    let mut q = p.clone();
    q.pool_id = "0x0000000000000000000000000000000000001122".into();
    q.token1 = "0xffffffffffffffffffffffffffffffffffffffff".into();
    let s = q.pancake_lm.as_mut().unwrap();
    s.lm.address = "0x0000000000000000000000000000000000003344".into();
    s.lm.pool = q.pool_id.clone();
    s.master_chef.pool_address = q.pool_id.clone();
    s.master_chef.pool_token1 = q.token1.clone();
    s.master_chef.pool_pid = xone_domain::Amount(2u64.into());
    p.validate().unwrap();
    q.validate().unwrap();
    xone_domain::validate_pool_dependencies([&p, &q]).unwrap();
    for mode in 0..5 {
        let mut bad = q.clone();
        let s = bad.pancake_lm.as_mut().unwrap();
        match mode {
            0 => {
                s.master_chef.total_alloc_point = xone_domain::Amount(301u64.into());
                s.master_chef.view_reward_per_second = xone_domain::Amount(
                    (&s.master_chef.latest_period_cake_per_second.0
                        * &s.master_chef.pool_alloc_point.0)
                        / &s.master_chef.total_alloc_point.0,
                );
            }
            1 => s.master_chef.pool_pid = xone_domain::Amount(1u64.into()),
            2 => s.lm.address = p.pancake_lm.as_ref().unwrap().lm.address.clone(),
            3 => s.core.factory_code_sha256 = "a".repeat(64),
            _ => s.token0_code_sha256 = "a".repeat(64),
        }
        bad.validate().unwrap();
        assert!(
            xone_domain::validate_pool_dependencies([&p, &bad]).is_err(),
            "{}",
            mode
        );
    }
}
