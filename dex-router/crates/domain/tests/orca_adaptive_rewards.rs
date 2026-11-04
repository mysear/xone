use prost::Message;
use xone_domain::{orca::OrcaWhirlpoolState, wire, Pool, Snapshot};
fn corpus() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-pools.json"
    ))
    .unwrap()
}
#[test]
fn adaptive_rewards_profile_wire_and_old_profile_isolation() {
    let f = corpus();
    assert_eq!(f["cases"].as_array().unwrap().len(), 20);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        p.validate().unwrap();
        let s = p.orca_whirlpool.as_ref().unwrap();
        assert_eq!(
            OrcaWhirlpoolState::from_proto(s.to_proto().unwrap()).unwrap(),
            *s
        );
        for (protocol, adapter) in [
            (
                "orca-whirlpools-adaptive",
                "orca-whirlpools/cc683ec9-spl-adaptive-1",
            ),
            (
                "orca-whirlpools-static",
                "orca-whirlpools/cc683ec9-spl-static-1",
            ),
            (
                "orca-whirlpools-rewards",
                "orca-whirlpools/cc683ec9-spl-rewards-1",
            ),
        ] {
            let mut old = p.clone();
            old.protocol = protocol.into();
            old.adapter_version = adapter.into();
            assert!(old.validate().is_err());
        }
        for bad in 0..8 {
            let mut q = p.clone();
            let s = q.orca_whirlpool.as_mut().unwrap();
            match bad {
                0 => s.tick_arrays[0].layout = "dynamic".into(),
                1 => {
                    s.tick_arrays.pop();
                }
                2 => s.reward_infos[1].extension[0] = 1,
                3 => s.reward_infos.pop().map(|_| ()).unwrap(),
                4 => s
                    .adaptive
                    .as_mut()
                    .unwrap()
                    .reserved
                    .pop()
                    .map(|_| ())
                    .unwrap(),
                5 => s.reward_infos[0].vault = "11111111111111111111111111111111".into(),
                6 => s.adaptive = None,
                _ => q.adapter_version = "orca-whirlpools/cc683ec9-spl-adaptive-1".into(),
            };
            assert!(q.validate().is_err(), "bad {bad}");
        }
    }
}
#[test]
fn adaptive_rewards_independent_wire_preserves_full_snapshot() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-wire.json"
    ))
    .unwrap();
    let expected: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    let w = wire::OnlineSnapshot::decode(
        hex::decode(f["protobuf_hex"].as_str().unwrap())
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let s = xone_domain::wire_conversion::snapshot_from_proto(w.clone()).unwrap();
    assert_eq!(s, expected);
    assert_eq!(
        xone_domain::wire_conversion::snapshot_to_proto(&s).unwrap(),
        w
    );
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
}
