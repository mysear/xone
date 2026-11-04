use xone_domain::{orca::OrcaWhirlpoolState, Pool};
fn model() -> serde_json::Value {
    let mut p: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap();
    p["protocol"] = "orca-whirlpools-adaptive".into();
    p["adapter_version"] = "orca-whirlpools/cc683ec9-spl-adaptive-1".into();
    let id = p["pool_id"].clone();
    let s = &mut p["orca_whirlpool"];
    s["fee_tier_index"] = 65.into();
    s["tick_arrays"].as_array_mut().unwrap().truncate(3);
    s["reward_infos"]=serde_json::json!((0..3).map(|i|serde_json::json!({"mint":"11111111111111111111111111111111","vault":"11111111111111111111111111111111","extension":vec![if i==0{3}else{0};32],"emissions_per_second_x64":"0","growth_global_x64":"0"})).collect::<Vec<_>>());
    for a in s["tick_arrays"].as_array_mut().unwrap() {
        a["layout"] = "fixed".into();
        for t in a["ticks"].as_array_mut().unwrap() {
            t["reward_growths_outside"] = serde_json::json!(["0", "0", "0"])
        }
    }
    let time = s["bank_unix_timestamp"].clone();
    s["adaptive"] = serde_json::json!({"whirlpool":id,"trade_enable_timestamp":0,"filter_period":10,"decay_period":100,"reduction_factor":5000,"adaptive_fee_control_factor":1000,"max_volatility_accumulator":100000,"tick_group_size":8,"major_swap_threshold_ticks":64,"constants_reserved":vec![3;16],"last_reference_update_timestamp":time,"last_major_swap_timestamp":time,"volatility_reference":20000,"tick_group_index_reference":0,"volatility_accumulator":40000,"variables_reserved":vec![5;16],"reserved":vec![7;128]});
    p
}
#[test]
fn dedicated_adaptive_state_preserves_complete_oracle_and_rejects_unmodeled_combinations() {
    let p: Pool = serde_json::from_value(model()).unwrap();
    p.validate().unwrap();
    let s = p.orca_whirlpool.as_ref().unwrap();
    let wire = s.to_proto().unwrap();
    assert_eq!(OrcaWhirlpoolState::from_proto(wire).unwrap(), *s);
    for bad in 0..18 {
        let mut v = model();
        let s = &mut v["orca_whirlpool"];
        match bad {
            0 => {
                s.as_object_mut().unwrap().remove("adaptive");
            }
            1 => s["adaptive"]["filter_period"] = 0.into(),
            2 => s["adaptive"]["decay_period"] = 10.into(),
            3 => s["adaptive"]["tick_group_size"] = 3.into(),
            4 => s["adaptive"]["constants_reserved"] = serde_json::json!([]),
            5 => s["adaptive"]["variables_reserved"] = serde_json::json!([]),
            6 => s["adaptive"]["reserved"] = serde_json::json!([]),
            7 => s["adaptive"]["volatility_reference"] = 100001.into(),
            8 => {
                s["adaptive"]["last_major_swap_timestamp"] =
                    (s["bank_unix_timestamp"].as_u64().unwrap() + 1).into()
            }
            9 => {
                s["adaptive"]["trade_enable_timestamp"] =
                    (s["bank_unix_timestamp"].as_u64().unwrap() + 1).into()
            }
            10 => s["adaptive"]["whirlpool"] = s["vault0"].clone(),
            11 => s["fee_tier_index"] = s["tick_spacing"].clone(),
            12 => s["reward_infos"][0]["mint"] = s["vault0"].clone(),
            13 => s["tick_arrays"][0]["layout"] = "dynamic".into(),
            14 => s["adaptive"]["major_swap_threshold_ticks"] = 65536.into(),
            15 => s["adaptive"]["tick_group_index_reference"] = i32::MIN.into(),
            16 => v["protocol"] = "orca-whirlpools-static".into(),
            _ => s["adaptive"]["volatility_accumulator"] = 100001.into(),
        };
        let parsed = serde_json::from_value::<Pool>(v);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "bad {bad}"
        )
    }
}

#[test]
fn independent_adaptive_proto_binds_oracle_to_snapshot_bank() {
    use prost::Message;
    use xone_domain::{
        wire,
        wire_conversion::{snapshot_from_proto, snapshot_to_proto},
        Snapshot,
    };
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-wire.json"
    ))
    .unwrap();
    let expected: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    let w = wire::OnlineSnapshot::decode(
        hex::decode(f["protobuf_hex"].as_str().unwrap())
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    let s = snapshot_from_proto(w.clone()).unwrap();
    assert_eq!(s, expected);
    assert_eq!(snapshot_to_proto(&s).unwrap(), w);
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    for bad in 0..5 {
        let mut q = s.clone();
        let a = q.pools[0].orca_whirlpool.as_mut().unwrap();
        match bad {
            0 => a.bank_slot += 1,
            1 => a.bank_unix_timestamp += 1,
            2 => a.adaptive.as_mut().unwrap().whirlpool = a.vault0.clone(),
            3 => a.adaptive.as_mut().unwrap().constants_reserved.clear(),
            _ => a.adaptive.as_mut().unwrap().reserved.clear(),
        };
        assert!(
            q.validate(
                &s.reference.owner_service,
                s.reference.owner_assignment_version
            )
            .is_err(),
            "bad {bad}"
        )
    }
}
