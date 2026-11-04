use prost::Message;
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Pool, Snapshot,
};
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-wire.json"
    ))
    .unwrap()
}
#[test]
fn independent_bin_proto_has_complete_tree_balances_shares_and_source_bank() {
    let f = fixture();
    let expected: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    expected
        .validate(
            &expected.reference.owner_service,
            expected.reference.owner_assignment_version,
        )
        .unwrap();
    let bytes = hex::decode(f["protobuf_hex"].as_str().unwrap()).unwrap();
    let w = wire::OnlineSnapshot::decode(bytes.as_slice()).unwrap();
    assert_eq!(snapshot_from_proto(w.clone()).unwrap(), expected);
    assert_eq!(snapshot_to_proto(&expected).unwrap(), w);
}
#[test]
fn missing_tree_nodes_extra_bins_widths_or_identity_are_rejected() {
    for bad in 0..21 {
        let mut f = fixture()["snapshot"]["pools"][0].clone();
        let token0 = f["token0"].clone();
        let s = &mut f["infinity_bin"];
        match bad {
            0 => {
                s.as_object_mut().unwrap().remove("tree_level0");
            }
            1 => s["tree_level1"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            2 => s["tree_level2"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            3 => s["bins"].as_array_mut().unwrap().pop().map(|_| ()).unwrap(),
            4 => s["bins"][0]["id"] = 0.into(),
            5 => {
                s["bins"][0]["reserve0"] = (num_bigint::BigUint::from(1u8) << 128usize)
                    .to_string()
                    .into()
            }
            6 => s["bins"][0]["total_shares"] = "0".into(),
            7 => s["active_id"] = 0.into(),
            8 => s["lp_fee"] = 100001.into(),
            9 => s["hooks"] = token0.clone(),
            10 => s["parameters"] = format!("0x{}", "0".repeat(64)).into(),
            11 => s["max_bins"] = 1.into(),
            12 => s["max_tree_words"] = 1.into(),
            13 => s["max_steps"] = 0.into(),
            14 => s["protocol_fee0"] = 4001.into(),
            15 => s["timestamp"] = 0.into(),
            16 => s["block_hash"] = format!("0x{}", "0".repeat(64)).into(),
            17 => f["protocol"] = "pancake-infinity-cl".into(),
            18 => s["pool_id"] = format!("0x{}", "b".repeat(64)).into(),
            19 => s["vault"] = token0.clone(),
            _ => s["tree_level2"][0]["value"] = "0".into(),
        };
        let parsed = serde_json::from_value::<Pool>(f);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "bad={bad}"
        )
    }
}

#[test]
fn manager_currency_dependencies_and_source_time_are_bound_across_physical_pools() {
    let f = fixture();
    let mut snapshot: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    for bad in 0..3 {
        let mut s = snapshot.clone();
        let b = s.pools[0].infinity_bin.as_mut().unwrap();
        match bad {
            0 => b.block_number += 1,
            1 => b.timestamp += 1,
            _ => b.block_hash = format!("0x{}", "b".repeat(64)),
        };
        assert!(s
            .validate(
                &snapshot.reference.owner_service,
                snapshot.reference.owner_assignment_version
            )
            .is_err())
    }
    let mut second = snapshot.pools[0].clone();
    let b = second.infinity_bin.as_mut().unwrap();
    b.bin_step = 2;
    b.parameters = format!("0x{:064x}", 2 << 16);
    b.pool_id = b.key_id(&second.token0, &second.token1).unwrap();
    second.pool_id = format!("{}/{}", b.manager, b.pool_id);
    second.validate().unwrap();
    snapshot.pools.push(second);
    snapshot.reference.scope_hash = xone_domain::scope_hash(&snapshot.pools).unwrap();
    snapshot
        .validate(
            &snapshot.reference.owner_service,
            snapshot.reference.owner_assignment_version,
        )
        .unwrap();
    snapshot.pools[1]
        .infinity_bin
        .as_mut()
        .unwrap()
        .protocol_fees0 = "123".parse().unwrap();
    assert!(snapshot
        .validate(
            &snapshot.reference.owner_service,
            snapshot.reference.owner_assignment_version
        )
        .is_err());
}

#[test]
fn original_manager_projected_banks_validate_and_roundtrip_every_shared_currency() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-pools.json"
    ))
    .unwrap();
    let mut count = 0;
    for bank in d["banks"].as_array().unwrap() {
        let pools = bank["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| serde_json::from_value::<Pool>(i["pool"].clone()).unwrap())
            .collect::<Vec<_>>();
        xone_domain::validate_pool_dependencies(pools.iter()).unwrap();
        for p in pools {
            p.validate().unwrap();
            let state = p.infinity_bin.unwrap();
            assert_eq!(
                xone_domain::infinity_bin::InfinityBinState::from_proto(state.to_proto().unwrap())
                    .unwrap(),
                state
            );
            count += 1
        }
    }
    assert_eq!(count, 10)
}

#[test]
fn locked_active_outside_tree_preserves_dust_without_inventing_membership() {
    let f = fixture();
    let mut p: Pool = serde_json::from_value(f["snapshot"]["pools"][0].clone()).unwrap();
    let s = p.infinity_bin.as_mut().unwrap();
    s.bins.clear();
    s.tree_level0 = "0".parse().unwrap();
    s.tree_level1.clear();
    s.tree_level2.clear();
    s.detached_active = Some(xone_domain::infinity_bin::InfinityBinEntry {
        id: s.active_id,
        reserve0: "1000".parse().unwrap(),
        reserve1: "1000".parse().unwrap(),
        total_shares: "1000".parse().unwrap(),
    });
    p.validate().unwrap();
    let s = p.infinity_bin.as_ref().unwrap();
    assert_eq!(
        xone_domain::infinity_bin::InfinityBinState::from_proto(s.to_proto().unwrap()).unwrap(),
        *s
    );
    for bad in 0..3 {
        let mut q = p.clone();
        let d = q
            .infinity_bin
            .as_mut()
            .unwrap()
            .detached_active
            .as_mut()
            .unwrap();
        match bad {
            0 => d.id += 1,
            1 => d.total_shares = "1001".parse().unwrap(),
            _ => {
                d.reserve0 =
                    xone_domain::Amount::checked(num_bigint::BigUint::from(1u8) << 128usize)
                        .unwrap()
            }
        };
        assert!(q.validate().is_err())
    }
}
