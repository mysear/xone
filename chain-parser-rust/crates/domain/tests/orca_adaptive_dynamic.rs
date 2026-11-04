use xone_domain::{orca::OrcaWhirlpoolState, Pool};
const PROFILE: &str = "orca-whirlpools-adaptive-dynamic";
const ADAPTER: &str = "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1";
fn pool() -> Pool {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
    ))
    .unwrap();
    let mut p: Pool = serde_json::from_value(f["cases"][0]["pool"].clone()).unwrap();
    p.protocol = PROFILE.into();
    p.adapter_version = ADAPTER.into();
    p
}
#[test]
fn adaptive_dynamic_profile_mixed_wire_and_old_profile_isolation() {
    for mask in 0..8 {
        let mut p = pool();
        for (i, a) in p
            .orca_whirlpool
            .as_mut()
            .unwrap()
            .tick_arrays
            .iter_mut()
            .enumerate()
        {
            a.layout = if mask & (1 << i) != 0 {
                "dynamic"
            } else {
                "fixed"
            }
            .into();
        }
        p.validate().unwrap();
        let s = p.orca_whirlpool.as_ref().unwrap();
        assert_eq!(
            OrcaWhirlpoolState::from_proto(s.to_proto().unwrap()).unwrap(),
            *s
        );
        for (profile, adapter) in [
            (
                "orca-whirlpools-adaptive",
                "orca-whirlpools/cc683ec9-spl-adaptive-1",
            ),
            (
                "orca-whirlpools-adaptive-rewards",
                "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1",
            ),
        ] {
            let mut q = p.clone();
            q.protocol = profile.into();
            q.adapter_version = adapter.into();
            // Isolate layout rejection from reward restrictions.
            for r in &mut q.orca_whirlpool.as_mut().unwrap().reward_infos {
                r.mint = "11111111111111111111111111111111".into();
                r.vault = r.mint.clone();
                r.emissions_per_second_x64 = xone_domain::Amount::zero();
                r.growth_global_x64 = xone_domain::Amount::zero();
            }
            for t in q
                .orca_whirlpool
                .as_mut()
                .unwrap()
                .tick_arrays
                .iter_mut()
                .flat_map(|a| &mut a.ticks)
            {
                t.reward_growths_outside = vec![xone_domain::Amount::zero(); 3];
            }
            assert_eq!(q.validate().is_ok(), mask == 0, "{profile}/{mask}");
        }
    }
}
#[test]
fn adaptive_dynamic_profile_rejects_incomplete_bitmap_identity_and_oracle() {
    for bad in 0..10 {
        let mut p = pool();
        let s = p.orca_whirlpool.as_mut().unwrap();
        s.tick_arrays[0].layout = "dynamic".into();
        match bad {
            0 => {
                s.tick_arrays.pop();
            }
            1 => s.tick_arrays[2].start_tick_index += 88,
            2 => s.tick_arrays[0].layout = "sparse".into(),
            3 => s.tick_arrays[0].initialized_bitmap = xone_domain::Amount::zero(),
            4 => s.adaptive = None,
            5 => s.adaptive.as_mut().unwrap().whirlpool = s.vault0.clone(),
            6 => s.reward_infos.pop().map(|_| ()).unwrap(),
            7 => s
                .adaptive
                .as_mut()
                .unwrap()
                .reserved
                .pop()
                .map(|_| ())
                .unwrap(),
            8 => s.reward_infos[1].extension[0] = 1,
            _ => p.adapter_version = "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1".into(),
        }
        assert!(p.validate().is_err(), "bad {bad}");
    }
}

use prost::Message;
use xone_domain::{wire, Snapshot};
#[test]
fn adaptive_dynamic_independent_wire_preserves_full_snapshot() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-wire.json"
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
