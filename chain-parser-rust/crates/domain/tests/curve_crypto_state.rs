use prost::Message;
use xone_domain::{
    wire,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    Snapshot,
};
#[test]
fn crypto_complete_wire_ledger_and_scope_are_preserved() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-wire.json"
    ))
    .unwrap();
    let s: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    let raw = hex::decode(f["protobuf_hex"].as_str().unwrap()).unwrap();
    let w = wire::OnlineSnapshot::decode(raw.as_slice()).unwrap();
    assert_eq!(snapshot_from_proto(w.clone()).unwrap(), s);
    assert_eq!(snapshot_to_proto(&s).unwrap(), w);
    for bad in 0..16 {
        let mut q = s.clone();
        let p = &mut q.pools[0];
        let c = p
            .multi_asset
            .as_mut()
            .unwrap()
            .curve_crypto
            .as_mut()
            .unwrap();
        match bad {
            0 => c.gamma = "1".parse().unwrap(),
            1 => c.out_fee = "10000000001".parse().unwrap(),
            2 => c.ma_half_time = 0,
            3 => c.price_scale = "0".parse().unwrap(),
            4 => c.total_supply = "0".parse().unwrap(),
            5 => c.admin_receiver_balance = "99999999999999999999999999".parse().unwrap(),
            6 => c.raw_balances.truncate(1),
            7 => c.raw_balances[0] = "0".parse().unwrap(),
            8 => c.last_prices_timestamp += 1,
            9 => c.timestamp = 0,
            10 => c.future_a_gamma_time = 100,
            11 => c.block_number += 1,
            12 => c.block_hash = format!("0x{}", "b".repeat(64)),
            13 => c.timestamp = u64::MAX / 1000 + 1,
            14 => c.lp_token = p.token0.clone(),
            _ => {
                p.protocol = "curve-plain3-basic".into();
                p.adapter_version = "curve-plain3-basic/factory-1".into();
            }
        }
        assert!(
            q.validate(
                &s.reference.owner_service,
                s.reference.owner_assignment_version
            )
            .is_err(),
            "bad={bad}"
        );
    }
    let mut missing = w;
    missing.pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .curve_crypto
        .as_mut()
        .unwrap()
        .gamma = None;
    assert!(snapshot_from_proto(missing).is_err());
}

#[test]
fn crypto_lp_mint_dependency_is_rejected_by_snapshot_installation() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-wire.json"
    ))
    .unwrap();
    let mut s: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    let mut receiver = s.pools[0].clone();
    receiver.pool_id = "0x0000000000000000000000000000000000000020".into();
    let source = s.pools[0]
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap();
    let lp = source.lp_token.clone();
    let code = source.lp_token_code_sha256.clone();
    receiver
        .multi_asset
        .as_mut()
        .unwrap()
        .curve_crypto
        .as_mut()
        .unwrap()
        .lp_token = "0x0000000000000000000000000000000000000097".into();
    receiver.multi_asset.as_mut().unwrap().assets[0].token = lp;
    receiver.multi_asset.as_mut().unwrap().assets[0].code_sha256 = code;
    receiver.token0 = receiver.multi_asset.as_ref().unwrap().assets[0]
        .token
        .clone();
    let id = receiver.pool_id.clone();
    let balance = receiver
        .multi_asset
        .as_ref()
        .unwrap()
        .curve_crypto
        .as_ref()
        .unwrap()
        .raw_balances[0]
        .clone();
    s.pools.push(receiver);
    s.reference.scope_hash = xone_domain::scope_hash(&s.pools).unwrap();
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    let c = s.pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .curve_crypto
        .as_mut()
        .unwrap();
    c.admin_receiver = id;
    c.admin_receiver_balance = balance;
    let e = s
        .validate(
            &s.reference.owner_service,
            s.reference.owner_assignment_version,
        )
        .unwrap_err();
    assert!(e.0.contains("CURVE_CRYPTO_MUTABLE_LP_DEPENDENCY"));
    s.pools[0].multi_asset.as_mut().unwrap().admin_fee = 0;
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
}

fn ramp_pool_json() -> serde_json::Value {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-wire.json"
    ))
    .unwrap();
    let mut p = f["snapshot"]["pools"][0].clone();
    p["protocol"] = "curve-crypto2-ramp".into();
    p["adapter_version"] = "curve-crypto2-ramp/d7d04cd9-ramp-1".into();
    p["multi_asset"]["curve_crypto"]["ramp"] = serde_json::json!({
        "initial_amp": p["multi_asset"]["amplification"],
        "initial_gamma": p["multi_asset"]["curve_crypto"]["gamma"],
        "initial_time": 0
    });
    p
}

#[test]
fn crypto_ramp_admits_origin_at_zero_active_completed_and_stopped_schedules() {
    let base = ramp_pool_json();
    let now = base["multi_asset"]["curve_crypto"]["timestamp"]
        .as_u64()
        .unwrap();
    for (initial, future) in [
        (0, 0),
        (0, 1),
        (now - 10, now + 10),
        (now - 10, now),
        (now, now),
    ] {
        let mut raw = base.clone();
        raw["multi_asset"]["curve_crypto"]["ramp"]["initial_time"] = initial.into();
        raw["multi_asset"]["curve_crypto"]["future_a_gamma_time"] = future.into();
        let p: xone_domain::Pool = serde_json::from_value(raw).unwrap();
        p.validate().unwrap();
        let c = p
            .multi_asset
            .as_ref()
            .unwrap()
            .curve_crypto
            .as_ref()
            .unwrap();
        assert_eq!(
            *c,
            xone_domain::curve_crypto_state::CurveCryptoState::from_proto(c.to_proto().unwrap())
                .unwrap()
        );
    }
}

#[test]
fn crypto_ramp_rejects_missing_invalid_origin_and_standard_profile_attachment() {
    let base = ramp_pool_json();
    let now = base["multi_asset"]["curve_crypto"]["timestamp"]
        .as_u64()
        .unwrap();
    for bad in 0..13 {
        let mut raw = base.clone();
        let c = &mut raw["multi_asset"]["curve_crypto"];
        match bad {
            0 => {
                c.as_object_mut().unwrap().remove("ramp");
            }
            1 => c["ramp"]["initial_time"] = (now + 1).into(),
            2 => {
                c["ramp"]["initial_time"] = now.into();
                c["future_a_gamma_time"] = (now - 1).into();
            }
            3 => c["future_a_gamma_time"] = (u64::MAX / 1000 + 1).into(),
            4 => c["ramp"]["initial_amp"] = "3999".into(),
            5 => c["ramp"]["initial_amp"] = "4000000001".into(),
            6 => c["ramp"]["initial_gamma"] = "9999999999".into(),
            7 => c["ramp"]["initial_gamma"] = "20000000000000001".into(),
            8 => c["ramp"]["initial_amp"] = "340282366920938463463374607431768211456".into(),
            9 => c["ramp"]["initial_gamma"] = "340282366920938463463374607431768211456".into(),
            10 => {
                raw["protocol"] = "curve-crypto2-full".into();
                raw["adapter_version"] = "curve-crypto2-full/d7d04cd9-standard-1".into();
            }
            11 => raw["adapter_version"] = "curve-crypto2-full/d7d04cd9-standard-1".into(),
            _ => {
                c["ramp"]["initial_time"] = (u64::MAX / 1000 + 1).into();
            }
        }
        match serde_json::from_value::<xone_domain::Pool>(raw) {
            Ok(p) => assert!(p.validate().is_err(), "invalid ramp {bad}"),
            Err(_) => assert!(
                bad == 0,
                "invalid ramp must decode before validation: {bad}"
            ),
        }
    }
}

#[test]
fn crypto_ramp_public_service_and_independent_wire_preserve_origin() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-ramp-service.json"
    ))
    .unwrap();
    assert_eq!(f["cases"].as_array().unwrap().len(), 28);
    for row in f["cases"].as_array().unwrap() {
        let p: xone_domain::Pool = serde_json::from_value(row["pool"].clone()).unwrap();
        p.validate().unwrap();
        let mut cloned = p.clone();
        cloned
            .multi_asset
            .as_mut()
            .unwrap()
            .curve_crypto
            .as_mut()
            .unwrap()
            .ramp
            .as_mut()
            .unwrap()
            .initial_amp = "4000".parse().unwrap();
        assert_eq!(serde_json::to_value(&p).unwrap(), row["pool"]);
    }
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-ramp-wire.json"
    ))
    .unwrap();
    let s: Snapshot = serde_json::from_value(f["snapshot"].clone()).unwrap();
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    let bytes = hex::decode(f["protobuf_hex"].as_str().unwrap()).unwrap();
    let w = wire::OnlineSnapshot::decode(bytes.as_slice()).unwrap();
    assert_eq!(snapshot_from_proto(w.clone()).unwrap(), s);
    assert_eq!(snapshot_to_proto(&s).unwrap(), w);
    assert_eq!(snapshot_to_proto(&s).unwrap().encode_to_vec(), bytes);
    for bad in 0..3 {
        let mut w = w.clone();
        let r = w.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .curve_crypto
            .as_mut()
            .unwrap()
            .ramp
            .as_mut()
            .unwrap();
        match bad {
            0 => r.initial_amp = None,
            1 => r.initial_gamma = None,
            _ => r.initial_amp.as_mut().unwrap().uint256_be = vec![0; 33],
        }
        assert!(
            snapshot_from_proto(w).is_err(),
            "malformed origin wire {bad}"
        );
    }
    let legacy: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-wire.json"
    ))
    .unwrap();
    let s: Snapshot = serde_json::from_value(legacy["snapshot"].clone()).unwrap();
    let json = serde_json::to_value(&s).unwrap();
    assert!(json["pools"][0]["multi_asset"]["curve_crypto"]
        .get("ramp")
        .is_none());
}
