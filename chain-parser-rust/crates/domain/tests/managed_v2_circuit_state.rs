use num_bigint::BigUint;
use prost::Message;
use serde_json::{json, Value};
use xone_domain::{
    managed_v2_circuit_state::{
        ManagedV2CircuitState, ManagedV2CircuitToken, ADAPTER_VERSION, PROTOCOL,
    },
    multi_asset::MultiAssetState,
    wire, Pool,
};
fn n(x: u64) -> BigUint {
    x.into()
}
fn row(
    token: &str,
    weight: &str,
    scale: &str,
    bounds: [u64; 2],
    price: u64,
    cache: [u64; 2],
) -> Value {
    let [lower, upper] = bounds;
    let [lo, hi] = cache;
    let one = n(1_000_000_000_000_000_000);
    let weight = weight.parse::<BigUint>().unwrap();
    let scale = scale.parse::<BigUint>().unwrap();
    let word: BigUint = n(lower)
        | (n(upper) << 16)
        | (&weight << 32)
        | (n(price) << 96)
        | (n(lo) << 192)
        | (n(hi) << 224);
    let bound = |side: u64, cache: u64, ceil: bool| {
        if side == 0 || price == 0 {
            return "0".to_string();
        };
        let ratio = n(cache) * n(10) * &one / n(u32::MAX.into());
        let mut x = n(price) * ratio;
        if ceil {
            x += &one - n(1)
        };
        (x / &one * &scale / &one).to_string()
    };
    json!({"token":token,"state_word":word.to_string(),"reference_bpt_price":(n(price)*&scale/&one).to_string(),"reference_weight":weight.to_string(),"lower_percentage":(n(lower)<<48usize).to_string(),"upper_percentage":(n(upper)<<48usize).to_string(),"lower_bpt_price_bound":bound(lower,lo,true),"upper_bpt_price_bound":bound(upper,hi,false)})
}
fn fixture_value() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-bpt-wire.json");
    let f: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut p = f["snapshot"]["pools"][0].clone();
    p["protocol"] = json!(PROTOCOL);
    p["adapter_version"] = json!(ADAPTER_VERSION);
    let a = p["multi_asset"]["assets"].as_array().unwrap();
    let weights = p["multi_asset"]["weights"].as_array().unwrap();
    let tokens: Vec<Value> = a
        .iter()
        .enumerate()
        .map(|(i, a)| {
            row(
                a["token"].as_str().unwrap(),
                weights[i].as_str().unwrap(),
                a["scaling"].as_str().unwrap(),
                [355, 7105],
                1_234_567_890_123_456_789,
                [1 << 31, u32::MAX.into()],
            )
        })
        .collect();
    p["multi_asset"]["managed_v2"]["circuit_breakers"] = json!({"tokens":tokens});
    for t in p["multi_asset"]["managed_v2"]["token_info"]
        .as_array_mut()
        .unwrap()
    {
        t["circuit_breaker_enabled"] = json!(true)
    }
    p
}
fn fixture() -> Pool {
    serde_json::from_value(fixture_value()).unwrap()
}
#[test]
fn closed_cache_full_word_wire_presence_clone() {
    let p = fixture();
    p.validate().unwrap();
    let m = p.multi_asset.as_ref().unwrap();
    let c = m
        .managed_v2
        .as_ref()
        .unwrap()
        .circuit_breakers
        .as_ref()
        .unwrap();
    assert_eq!(c.tokens[0].state_word.0.bits(), 256);
    let w = m.to_proto().unwrap();
    let bytes = w.encode_to_vec();
    assert_eq!(
        MultiAssetState::from_proto(wire::MultiAssetState::decode(bytes.as_slice()).unwrap())
            .unwrap(),
        *m
    );
    let mut q = p.clone();
    q.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .circuit_breakers
        .as_mut()
        .unwrap()
        .tokens[0]
        .state_word = xone_domain::Amount(n(0));
    assert_ne!(q, p);
    for i in 0..7 {
        let mut w = c.to_proto().unwrap();
        let t = &mut w.tokens[0];
        let fields = [
            &mut t.state_word,
            &mut t.reference_bpt_price,
            &mut t.reference_weight,
            &mut t.lower_percentage,
            &mut t.upper_percentage,
            &mut t.lower_bpt_price_bound,
            &mut t.upper_bpt_price_bound,
        ];
        *fields.into_iter().nth(i).unwrap() = None;
        assert!(ManagedV2CircuitState::from_proto(w).is_err());
    }
    let mut w = c.to_proto().unwrap();
    w.tokens[0].state_word.as_mut().unwrap().uint256_be = vec![0; 33];
    assert!(ManagedV2CircuitState::from_proto(w).is_err());
}
#[test]
fn strict_required_json_unknown_case_duplicate_null() {
    let v = fixture_value();
    let row = &v["multi_asset"]["managed_v2"]["circuit_breakers"]["tokens"][0];
    for key in row.as_object().unwrap().keys() {
        for null in [false, true] {
            let mut r = row.clone();
            if null {
                r[key] = Value::Null
            } else {
                r.as_object_mut().unwrap().remove(key);
            }
            assert!(
                serde_json::from_value::<ManagedV2CircuitToken>(r).is_err(),
                "{key}"
            );
        }
    }
    let s = serde_json::to_string(row).unwrap();
    for tail in [",\"Token\":\"x\"}", ",\"unknown\":0}", ",\"token\":\"x\"}"] {
        assert!(serde_json::from_str::<ManagedV2CircuitToken>(&format!(
            "{}{tail}",
            &s[..s.len() - 1]
        ))
        .is_err());
    }
    for text in [
        "{}",
        "{\"tokens\":null}",
        "{\"Tokens\":[]}",
        "{\"tokens\":[],\"tokens\":[]}",
        "{\"tokens\":[],\"extra\":0}",
    ] {
        assert!(serde_json::from_str::<ManagedV2CircuitState>(text).is_err());
    }
    let mut q = v;
    q["multi_asset"]["managed_v2"]["circuit_breakers"] = Value::Null;
    assert!(serde_json::from_value::<Pool>(q).is_err());
}
#[test]
fn profile_getters_tokens_and_ranges_are_enforced() {
    let v = fixture_value();
    for path in [
        "/multi_asset/managed_v2/circuit_breakers",
        "/multi_asset/managed_v2/bpt",
        "/multi_asset/managed_v2/schedule",
    ] {
        let mut q = v.clone();
        *q.pointer_mut(path).unwrap() = Value::Null;
        assert!(serde_json::from_value::<Pool>(q).is_err());
    }
    for field in [
        "reference_weight",
        "reference_bpt_price",
        "lower_percentage",
        "upper_percentage",
        "lower_bpt_price_bound",
        "upper_bpt_price_bound",
        "state_word",
    ] {
        let mut q = v.clone();
        q["multi_asset"]["managed_v2"]["circuit_breakers"]["tokens"][0][field] = json!("0");
        assert!(
            serde_json::from_value::<Pool>(q)
                .unwrap()
                .validate()
                .is_err(),
            "{field}"
        );
    }
    for mutation in 0..8 {
        let mut q = v.clone();
        let tokens = q["multi_asset"]["managed_v2"]["circuit_breakers"]["tokens"]
            .as_array_mut()
            .unwrap();
        match mutation {
            0 => tokens.swap(0, 1),
            1 => tokens[1] = tokens[0].clone(),
            2 => tokens.truncate(1),
            3 => tokens.clear(),
            4 => tokens[0]["token"] = json!("0x0000000000000000000000000000000000000000"),
            5 => tokens[0]["state_word"] = json!((n(1) << 256usize).to_string()),
            6 => {
                while tokens.len() < 51 {
                    tokens.push(tokens[0].clone())
                }
            }
            _ => {
                q["multi_asset"]["managed_v2"]["token_info"][0]["circuit_breaker_enabled"] =
                    json!(false)
            }
        };
        assert!(
            serde_json::from_value::<Pool>(q).map_or(true, |p| p.validate().is_err()),
            "mutation {mutation}"
        );
    }
    for (old, adapter) in [
        (
            "balancer-v2-managed-aum",
            "balancer-v2/20230411-managed-aum-3",
        ),
        (
            "balancer-v2-managed-scheduled",
            "balancer-v2/20230411-managed-scheduled-1",
        ),
        (
            "balancer-v2-managed-bpt",
            "balancer-v2/20230411-managed-bpt-1",
        ),
    ] {
        for zero in [false, true] {
            let mut q = v.clone();
            q["protocol"] = json!(old);
            q["adapter_version"] = json!(adapter);
            let m = q["multi_asset"]["managed_v2"].as_object_mut().unwrap();
            if old != "balancer-v2-managed-bpt" {
                m.remove("bpt");
            }
            if old == "balancer-v2-managed-aum" {
                m.remove("schedule");
            }
            if zero {
                for t in m["circuit_breakers"]["tokens"].as_array_mut().unwrap() {
                    for (k, x) in t.as_object_mut().unwrap() {
                        if k != "token" {
                            *x = json!("0")
                        }
                    }
                }
                for t in m["token_info"].as_array_mut().unwrap() {
                    t["circuit_breaker_enabled"] = json!(false)
                }
            }
            assert!(
                serde_json::from_value::<Pool>(q)
                    .unwrap()
                    .validate()
                    .is_err(),
                "old {old}/{zero}"
            );
        }
    }
}
#[test]
fn zero_disabled_and_dynamic_bounds_keep_explicit_state() {
    for (lower, upper, price) in [
        (0, 0, 0),
        (0, 0, 123),
        (0, 7105, 123),
        (355, 0, 123),
        (355, 7105, 0),
    ] {
        let mut q = fixture_value();
        let a = q["multi_asset"]["assets"].as_array().unwrap().clone();
        let w = q["multi_asset"]["weights"].as_array().unwrap().clone();
        for (i, a) in a.iter().enumerate() {
            let weight = if lower == 0 && upper == 0 && price == 0 {
                "0"
            } else {
                w[i].as_str().unwrap()
            };
            let r = row(
                a["token"].as_str().unwrap(),
                weight,
                a["scaling"].as_str().unwrap(),
                [lower, upper],
                price,
                [0, 0],
            );
            q["multi_asset"]["managed_v2"]["token_info"][i]["circuit_breaker_enabled"] =
                json!(r["state_word"] != "0");
            q["multi_asset"]["managed_v2"]["circuit_breakers"]["tokens"][i] = r;
        }
        serde_json::from_value::<Pool>(q)
            .unwrap()
            .validate()
            .unwrap();
    }
    let mut q = fixture_value();
    let a = q["multi_asset"]["assets"][0].clone();
    let mut r = row(
        a["token"].as_str().unwrap(),
        "10000000000000000",
        a["scaling"].as_str().unwrap(),
        [355, 7105],
        123,
        [1, 1],
    );
    r["lower_bpt_price_bound"] = json!("7");
    r["upper_bpt_price_bound"] = json!("9");
    q["multi_asset"]["managed_v2"]["circuit_breakers"]["tokens"][0] = r;
    let mut p: Pool = serde_json::from_value(q).unwrap();
    p.validate().unwrap();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.execution_context = Some(xone_domain::managed_v2_state::ManagedV2ExecutionContext {
        block_number: m.block_number,
        timestamp: m.timestamp,
    });
    p.validate().unwrap();
    assert!(p.multi_asset.as_ref().unwrap().to_proto().is_err());
}

#[test]
fn closed_rounding_scaling_ranges_and_full_raw96() {
    use xone_domain::Amount;
    let p = fixture();
    let m = p.multi_asset.as_ref().unwrap();
    let c = m
        .managed_v2
        .as_ref()
        .unwrap()
        .circuit_breakers
        .as_ref()
        .unwrap();
    let a = &m.assets[0];
    let r = row(
        &a.token,
        &m.weights[0].to_string(),
        "10000000000000000000",
        [355, 7105],
        1,
        [1 << 31, 1 << 31],
    );
    assert_eq!(r["reference_bpt_price"], "10");
    assert_eq!(r["lower_bpt_price_bound"], "60");
    assert_eq!(r["upper_bpt_price_bound"], "50");
    assert_eq!(r["lower_percentage"], "99923616732282880");
    // The standalone wire preserves these values. Parent scaling checks run
    // against the current raw schedule, so test a valid non-18-decimal parent.
    let mut q = fixture_value();
    q["multi_asset"]["assets"][0]["scaling"] = json!("10000000000000000000");
    let old: BigUint = q["multi_asset"]["managed_v2"]["schedule"]["tokens"][0]["state_word"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    // Schedule token decimal difference is stored at bit 128 (five bits).
    let mask = (n(1) << 256usize) - n(1) - (n(31) << 128usize);
    q["multi_asset"]["managed_v2"]["schedule"]["tokens"][0]["state_word"] =
        json!(((old & mask) | (n(1) << 128usize)).to_string());
    q["multi_asset"]["managed_v2"]["circuit_breakers"]["tokens"][0] = r;
    let scaled: Pool = serde_json::from_value(q.clone()).unwrap();
    scaled.validate().unwrap();
    q["multi_asset"]["managed_v2"]["circuit_breakers"]["tokens"][0]["reference_bpt_price"] =
        json!("1");
    assert!(serde_json::from_value::<Pool>(q)
        .unwrap()
        .validate()
        .is_err());
    for (weight, lower, upper, ok) in [
        ("10000000000000000", 355, 3552, true),
        ("1000000000000000000", 3552, 35527, true),
        ("9999999999999999", 355, 7105, false),
        ("1000000000000000001", 355, 7105, false),
        ("0", 355, 7105, false),
        ("500000000000000000", 3553, 7105, false),
        ("500000000000000000", 355, 35528, false),
    ] {
        let mut q = c.clone();
        q.tokens[0] = serde_json::from_value(row(
            &a.token,
            weight,
            &a.scaling.to_string(),
            [lower, upper],
            1,
            [0, 0],
        ))
        .unwrap();
        assert_eq!(q.validate().is_ok(), ok, "range {weight}/{lower}/{upper}");
    }
    let mut q = fixture();
    let m = q.multi_asset.as_mut().unwrap();
    let t = &mut m
        .managed_v2
        .as_mut()
        .unwrap()
        .circuit_breakers
        .as_mut()
        .unwrap()
        .tokens[0];
    let max96 = (n(1) << 96usize) - n(1);
    t.state_word = Amount((&m.weights[0].0 << 32usize) | (&max96 << 96usize));
    t.reference_bpt_price = Amount(max96);
    t.lower_percentage = Amount(n(0));
    t.upper_percentage = Amount(n(0));
    t.lower_bpt_price_bound = Amount(n(0));
    t.upper_bpt_price_bound = Amount(n(0));
    q.validate().unwrap();
    for lower in [false, true] {
        let mut q = q.clone();
        let t = &mut q
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap()
            .circuit_breakers
            .as_mut()
            .unwrap()
            .tokens[0];
        if lower {
            t.lower_bpt_price_bound = Amount(n(1))
        } else {
            t.upper_bpt_price_bound = Amount(n(1))
        };
        assert!(q.validate().is_err());
    }
}
fn snapshot(p: Pool) -> xone_domain::Snapshot {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-bpt-wire.json");
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut s: xone_domain::Snapshot = serde_json::from_value(v["snapshot"].clone()).unwrap();
    s.pools = vec![p];
    s.reference.scope_hash = xone_domain::scope_hash(&s.pools).unwrap();
    s.reference.adapter_set_hash = xone_domain::hash(&vec![&s.pools[0].adapter_version]).unwrap();
    s
}
#[test]
fn source_h_and_derived_canonical_isolation() {
    let p = fixture();
    let s = snapshot(p.clone());
    let owner = &s.reference.owner_service;
    let assignment = s.reference.owner_assignment_version;
    s.validate(owner, assignment).unwrap();
    let w = xone_domain::wire_conversion::snapshot_to_proto(&s).unwrap();
    assert_eq!(
        xone_domain::wire_conversion::snapshot_from_proto(w).unwrap(),
        s
    );
    for mutation in 0..4 {
        let mut q = s.clone();
        let m = q.pools[0]
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        match mutation {
            0 => m.timestamp += 1,
            1 => m.block_number += 1,
            2 => m.block_hash = format!("0x{}", "b".repeat(64)),
            _ => m.token_info[0].last_change_block = m.block_number + 1,
        };
        assert!(q.validate(owner, assignment).is_err());
    }
    let mut q = p;
    let m = q.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.execution_context = Some(xone_domain::managed_v2_state::ManagedV2ExecutionContext {
        block_number: m.block_number,
        timestamp: m.timestamp,
    });
    q.validate().unwrap();
    let q = snapshot(q);
    assert!(q.validate(owner, assignment).is_err());
    assert!(xone_domain::wire_conversion::snapshot_to_proto(&q).is_err());
}
#[test]
fn known_zero_presence_old_absence_and_all_amount_widths() {
    let p = fixture();
    let c = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .circuit_breakers
        .as_ref()
        .unwrap();
    for i in 0..7 {
        let mut q = c.clone();
        let t = &mut q.tokens[0];
        let fields = [
            &mut t.state_word,
            &mut t.reference_bpt_price,
            &mut t.reference_weight,
            &mut t.lower_percentage,
            &mut t.upper_percentage,
            &mut t.lower_bpt_price_bound,
            &mut t.upper_bpt_price_bound,
        ];
        fields.into_iter().nth(i).unwrap().0 = n(1) << 256usize;
        assert!(q.validate().is_err());
    }
    let mut q = fixture_value();
    let m = &mut q["multi_asset"]["managed_v2"];
    for t in m["circuit_breakers"]["tokens"].as_array_mut().unwrap() {
        for (k, v) in t.as_object_mut().unwrap() {
            if k != "token" {
                *v = json!("0")
            }
        }
    }
    for t in m["token_info"].as_array_mut().unwrap() {
        t["circuit_breaker_enabled"] = json!(false)
    }
    let p: Pool = serde_json::from_value(q).unwrap();
    p.validate().unwrap();
    let w = p.multi_asset.as_ref().unwrap().to_proto().unwrap();
    let m = w.managed_v2.as_ref().unwrap();
    assert_eq!(m.token_info[0].circuit_breaker_enabled, Some(false));
    assert_eq!(
        m.circuit_breakers.as_ref().unwrap().tokens[0]
            .state_word
            .as_ref()
            .unwrap()
            .uint256_be,
        vec![0; 32]
    );
    assert_eq!(
        MultiAssetState::from_proto(w).unwrap(),
        *p.multi_asset.as_ref().unwrap()
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/managed-v2-bpt-wire.json");
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let p: Pool = serde_json::from_value(v["snapshot"]["pools"][0].clone()).unwrap();
    let w = p.multi_asset.as_ref().unwrap().to_proto().unwrap();
    assert!(w.managed_v2.as_ref().unwrap().circuit_breakers.is_none());
    let back = MultiAssetState::from_proto(w).unwrap();
    assert!(back.managed_v2.unwrap().circuit_breakers.is_none());
    assert!(
        serde_json::to_value(p).unwrap()["multi_asset"]["managed_v2"]
            .get("circuit_breakers")
            .is_none()
    );
}

#[test]
fn proto_widths_canonical_tokens_and_fifty_token_limit() {
    let p = fixture();
    let c = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .circuit_breakers
        .as_ref()
        .unwrap();
    for field in 0..7 {
        for width in [0, 31, 33] {
            let mut w = c.to_proto().unwrap();
            let t = &mut w.tokens[0];
            let fields = [
                &mut t.state_word,
                &mut t.reference_bpt_price,
                &mut t.reference_weight,
                &mut t.lower_percentage,
                &mut t.upper_percentage,
                &mut t.lower_bpt_price_bound,
                &mut t.upper_bpt_price_bound,
            ];
            fields
                .into_iter()
                .nth(field)
                .unwrap()
                .as_mut()
                .unwrap()
                .uint256_be = vec![0; width];
            assert!(ManagedV2CircuitState::from_proto(w).is_err());
        }
    }
    for token in [
        String::new(),
        c.tokens[0].token.to_uppercase(),
        "0x1".into(),
    ] {
        let mut q = c.clone();
        q.tokens[0].token = token;
        assert!(q.validate().is_err());
    }
    let mut fifty = ManagedV2CircuitState {
        tokens: (1..=50)
            .map(|i| {
                serde_json::from_value(row(
                    &format!("0x{i:040x}"),
                    "0",
                    "1000000000000000000",
                    [0, 0],
                    0,
                    [0, 0],
                ))
                .unwrap()
            })
            .collect(),
    };
    fifty.validate().unwrap();
    fifty.tokens.push(fifty.tokens[49].clone());
    assert!(fifty.validate().is_err());
}

#[test]
fn dependency_entry_requires_managed_state() {
    for protocol in [
        xone_domain::managed_v2_state::PROTOCOL,
        xone_domain::managed_v2_schedule::PROTOCOL,
        xone_domain::managed_v2_bpt_state::PROTOCOL,
        PROTOCOL,
    ] {
        for missing_multi in [false, true] {
            let mut p = fixture();
            p.protocol = protocol.into();
            if missing_multi {
                p.multi_asset = None;
            } else {
                p.multi_asset.as_mut().unwrap().managed_v2 = None;
            }
            assert!(
                xone_domain::managed_v2_dependencies::validate([&p]).is_err(),
                "{protocol} multi={missing_multi}"
            );
            assert!(
                xone_domain::managed_v2_dependencies::blocked_pools([&p]).is_err(),
                "{protocol} capability multi={missing_multi}"
            );
        }
    }
}

#[test]
fn upper_input_rules_accept_sub_one_and_compressed_disabled_bounds() {
    for (lower, upper) in [
        (0u64, 500_000_000_000_000_000u64),
        (100_000_000_000_000_000, 200_000_000_000_000_000),
        (100_000_000_000_000_000, 100_000_000_000_000_000),
        (0, 1),
        (100_000_000_000_000_000, 0),
        (0, 1 << 48),
    ] {
        let mut p = fixture();
        let m = p.multi_asset.as_mut().unwrap();
        for (i, a) in m.assets.iter().enumerate() {
            m.managed_v2
                .as_mut()
                .unwrap()
                .circuit_breakers
                .as_mut()
                .unwrap()
                .tokens[i] = serde_json::from_value(row(
                &a.token,
                &m.weights[i].to_string(),
                &a.scaling.to_string(),
                [lower >> 48, upper >> 48],
                1,
                [1 << 31, 1 << 31],
            ))
            .unwrap();
        }
        p.validate()
            .unwrap_or_else(|e| panic!("input lower={lower} upper={upper}: {e}"));
        let m = p.multi_asset.as_ref().unwrap();
        assert_eq!(
            MultiAssetState::from_proto(m.to_proto().unwrap()).unwrap(),
            *m
        );
    }
}
#[test]
fn upper_input_rules_reject_lower_above_nonzero_upper() {
    for (lower, upper) in [
        (200_000_000_000_000_000u64, 100_000_000_000_000_000u64),
        (100_000_000_000_000_000, 1 << 48),
    ] {
        let mut p = fixture();
        let m = p.multi_asset.as_mut().unwrap();
        for (i, a) in m.assets.iter().enumerate() {
            m.managed_v2
                .as_mut()
                .unwrap()
                .circuit_breakers
                .as_mut()
                .unwrap()
                .tokens[i] = serde_json::from_value(row(
                &a.token,
                &m.weights[i].to_string(),
                &a.scaling.to_string(),
                [lower >> 48, upper >> 48],
                1,
                [1 << 31, 1 << 31],
            ))
            .unwrap();
        }
        assert!(p.validate().is_err(), "input lower={lower} upper={upper}");
    }
}
