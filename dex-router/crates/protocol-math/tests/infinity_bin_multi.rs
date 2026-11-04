use xone_protocol_math::infinity_bin;
#[allow(dead_code)]
#[path = "../src/infinity_bin_multi.rs"]
mod infinity_bin_multi;
use infinity_bin_multi::*;
use std::collections::BTreeMap;
use xone_domain::Amount;
fn a(n: u128) -> Amount {
    n.to_string().parse().unwrap()
}
fn state() -> MultiBinState {
    MultiBinState {
        active_id: 8388608,
        bin_step: 1,
        lp_fee: 0,
        protocol_fee0: 0,
        protocol_fee1: 0,
        max_steps: 8,
        tree_min_id: 0,
        tree_max_id: 0xffffff,
        bins: BTreeMap::from([
            (
                8388608,
                BinReserves {
                    reserve0: a(100),
                    reserve1: a(10),
                },
            ),
            (
                8388606,
                BinReserves {
                    reserve0: a(100),
                    reserve1: a(100),
                },
            ),
        ]),
    }
}
#[test]
fn spills_to_strict_sparse_predecessor_and_keeps_depleted_tree_entry() {
    let s = state();
    let q = quote_exact_in(&s, &a(20), true).unwrap();
    assert_eq!(q.amount_out, a(19));
    assert_eq!(q.next_state.active_id, 8388606);
    assert_eq!(q.next_state.bins[&8388608].reserve1, a(0));
    assert_eq!(q.next_state.bins.len(), 2);
    assert_eq!(q.consumed_in, a(20));
    assert_eq!(q.remaining_in, a(0));
    assert_eq!(q.bin_fills.len(), 2);
    assert_eq!(s, state());
}
#[test]
fn missing_liquidity_and_budget_fail_atomically() {
    let mut s = state();
    assert_eq!(
        quote_exact_in(&s, &a(1000), true).unwrap_err().0,
        "INFINITY_BIN_OUT_OF_LIQUIDITY"
    );
    assert_eq!(s, state());
    s.max_steps = 1;
    assert_eq!(
        quote_exact_in(&s, &a(20), true).unwrap_err().0,
        "INFINITY_BIN_STEP_BUDGET"
    );
    assert_eq!(s.bins, state().bins);
}
#[test]
fn original_native_swap_differential() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-multi.json"
    ))
    .unwrap();
    assert_eq!(fixture["schema"], "infinity-bin-multi/1");
    assert!(fixture["reference"]["runtime_bytes"].as_u64().unwrap() <= 24576);
    assert_eq!(fixture["reference"]["unlimited_contract_size"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() >= 300);
    let (mut success, mut native_reverts, mut budget) = (0, 0, 0);
    let (mut protocol0, mut protocol1) = (
        num_bigint::BigUint::from(0u32),
        num_bigint::BigUint::from(0u32),
    );
    for c in cases {
        if c["name"].as_str().unwrap().ends_with("/0") {
            protocol0 = 0u32.into();
            protocol1 = 0u32.into();
        }
        let s: MultiBinState = serde_json::from_value(c["state"].clone()).unwrap();
        let before = s.clone();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let zero = c["zero_for_one"].as_bool().unwrap();
        let actual = quote_exact_in(&s, &input, zero);
        let e = &c["expected"];
        if e["native_reverted"] != true {
            let p: num_bigint::BigUint = e["protocol_amount"].as_str().unwrap().parse().unwrap();
            if zero {
                protocol0 += &p;
            } else {
                protocol1 += &p;
            }
        }
        assert_eq!(
            e["post"]["protocol0"],
            protocol0.to_string(),
            "{} protocol0 sequence",
            c["name"]
        );
        assert_eq!(
            e["post"]["protocol1"],
            protocol1.to_string(),
            "{} protocol1 sequence",
            c["name"]
        );
        assert_eq!(s, before, "{} mutated input", c["name"]);
        if let Some(reason) = e["support_rejection"].as_str() {
            assert_eq!(actual.unwrap_err().0, reason, "{}", c["name"]);
            budget += 1;
            continue;
        }
        if e["native_reverted"] == true {
            let error = actual.expect_err("native reverted but Rust filled").0;
            let selector = &e["revert_data"].as_str().unwrap()[..10];
            let allowed: &[&str] = match selector {
                "0x19f7ff4a" => &["INFINITY_BIN_ZERO_OUTPUT"],
                "0x96aa65ad" => &["INFINITY_BIN_OUT_OF_LIQUIDITY"],
                "0x3b74b31a" => &["INFINITY_BIN_PRICE_RANGE"],
                "0x2d54cbbe" => &["INFINITY_BIN_MAX_LIQUIDITY", "INFINITY_BIN_INPUT"],
                "0x16692de9" => &["INFINITY_BIN_RESERVE_OVERFLOW", "INFINITY_BIN_AMOUNT_WIDTH"],
                "0x93dafdf1" => &[
                    "INFINITY_BIN_INPUT",
                    "INFINITY_BIN_OUTPUT_WIDTH",
                    "INFINITY_BIN_AMOUNT_WIDTH",
                ],
                "0x4e487b71" => &["INFINITY_BIN_AMOUNT_WIDTH"],
                "0x63f1e01f" => &["INFINITY_BIN_LIQUIDITY_OVERFLOW"],
                _ => panic!("unclassified native revert {}: {}", c["name"], selector),
            };
            assert!(
                allowed.contains(&error.as_str()),
                "{}: native {} / Rust {}",
                c["name"],
                selector,
                error
            );
            assert_eq!(e["post"]["active_id"].as_u64().unwrap() as u32, s.active_id);
            assert_eq!(e["post"]["bins"], serde_json::to_value(&s.bins).unwrap());
            assert_ne!(e["revert_data"], "0x");
            native_reverts += 1;
            continue;
        }
        let q = actual.unwrap_or_else(|error| panic!("{}: {}", c["name"], error.0));
        assert_eq!(
            q.amount_out.to_string(),
            e["amount_out"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, a(0));
        assert_eq!(
            q.protocol_amount.to_string(),
            e["protocol_amount"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(q.swap_fee, e["swap_fee"].as_u64().unwrap() as u32);
        assert_eq!(q.steps, e["steps"].as_u64().unwrap() as u32);
        let mut expected_state = s.clone();
        expected_state.active_id = e["post"]["active_id"].as_u64().unwrap() as u32;
        expected_state.bins = serde_json::from_value(e["post"]["bins"].clone()).unwrap();
        assert_eq!(q.next_state, expected_state, "{}", c["name"]);
        let expected_fills = e["bin_fills"].as_array().unwrap();
        assert_eq!(q.bin_fills.len(), expected_fills.len());
        for (fill, expected) in q.bin_fills.iter().zip(expected_fills) {
            assert_eq!(fill.id, expected["id"].as_u64().unwrap() as u32);
            for (name, value) in [
                ("amount_in", &fill.amount_in),
                ("amount_out", &fill.amount_out),
                ("total_fee", &fill.total_fee),
                ("protocol_amount", &fill.protocol_amount),
                ("price_x128", &fill.price_x128),
            ] {
                assert_eq!(
                    value.to_string(),
                    expected[name].as_str().unwrap(),
                    "{} bin {} {}",
                    c["name"],
                    fill.id,
                    name
                );
            }
        }
        let delta_input = if zero { "delta0" } else { "delta1" };
        let delta_output = if zero { "delta1" } else { "delta0" };
        assert_eq!(e[delta_input], format!("-{}", q.consumed_in));
        assert_eq!(e[delta_output], q.amount_out.to_string());
        success += 1;
    }
    assert!(success >= 200 && native_reverts >= 80 && budget >= 2);
    println!("original BinPool.swap differential: {success} exact successes, {native_reverts} native reverts, {budget} bounded-support rejections");
}
#[test]
fn exact_depletion_stops_on_current_id_and_dust_can_finish_a_prior_fill() {
    let s = state();
    let q = quote_exact_in(&s, &a(10), true).unwrap();
    assert_eq!(q.steps, 1);
    assert_eq!(q.next_state.active_id, s.active_id);
    let mut with_fee = s;
    with_fee.lp_fee = 3000;
    with_fee.protocol_fee0 = 4000;
    let q = quote_exact_in(&with_fee, &a(12), true).unwrap();
    assert_eq!(q.amount_out, a(10));
    assert_eq!(q.bin_fills.len(), 2);
    assert_eq!(q.bin_fills[1].amount_out, a(0));
    assert_eq!(q.bin_fills[1].total_fee, a(1));
    assert_eq!(
        quote_exact_in(&with_fee, &a(1), true).unwrap_err().0,
        "INFINITY_BIN_ZERO_OUTPUT"
    );
}
#[test]
fn completeness_and_native_widths_are_explicit() {
    let mut s = state();
    s.tree_min_id = s.active_id - 100;
    assert_eq!(
        quote_exact_in(&s, &a(1), true).unwrap_err().0,
        "INFINITY_BIN_COMPLETE_TREE_REQUIRED"
    );
    s = state();
    s.tree_max_id = s.active_id + 100;
    assert!(quote_exact_in(&s, &a(1), false).is_err());
    s = state();
    s.max_steps = 0;
    assert_eq!(
        quote_exact_in(&s, &a(1), true).unwrap_err().0,
        "INFINITY_BIN_STEP_BUDGET"
    );
    s = state();
    for invalid in [a(0), a(1u128 << 127), a(u128::MAX)] {
        assert_eq!(
            quote_exact_in(&s, &invalid, true).unwrap_err().0,
            "INFINITY_BIN_INPUT"
        );
    }
    s.bins.get_mut(&s.active_id).unwrap().reserve0 =
        Amount(num_bigint::BigUint::from(1u32) << 128usize);
    assert_eq!(
        quote_exact_in(&s, &a(1), true).unwrap_err().0,
        "INFINITY_BIN_STATE"
    );
    for invalid in [0, 0xffffff, 0x1000000] {
        let mut s = state();
        s.active_id = invalid;
        assert_eq!(
            quote_exact_in(&s, &a(1), true).unwrap_err().0,
            "INFINITY_BIN_STATE"
        );
        let mut s = state();
        s.bins.insert(
            invalid,
            BinReserves {
                reserve0: a(0),
                reserve1: a(0),
            },
        );
        assert!(quote_exact_in(&s, &a(1), true).is_err());
    }
}
#[test]
fn exhausted_and_absent_active_bins_count_toward_budget() {
    let mut s = state();
    s.bins.get_mut(&s.active_id).unwrap().reserve1 = a(0);
    let q = quote_exact_in(&s, &a(10), true).unwrap();
    assert_eq!(q.steps, 2);
    assert_eq!(q.bin_fills.len(), 1);
    s.max_steps = 1;
    assert_eq!(
        quote_exact_in(&s, &a(10), true).unwrap_err().0,
        "INFINITY_BIN_STEP_BUDGET"
    );
    s.bins.remove(&s.active_id);
    s.max_steps = 2;
    assert_eq!(quote_exact_in(&s, &a(10), true).unwrap().steps, 2);
}

#[test]
fn typed_state_closes_invalid_fees_step_and_hook_fields() {
    for (lp, p0, p1, step) in [
        (100001, 0, 0, 1),
        (0, 4001, 0, 1),
        (0, 0, 4001, 1),
        (0, 0, 0, 0),
    ] {
        let mut s = state();
        s.lp_fee = lp;
        s.protocol_fee0 = p0;
        s.protocol_fee1 = p1;
        s.bin_step = step;
        assert_eq!(
            quote_exact_in(&s, &a(1), true).unwrap_err().0,
            "INFINITY_BIN_STATE"
        );
    }
    let mut value = serde_json::to_value(state()).unwrap();
    value["bin_step"] = serde_json::json!(65536);
    assert!(serde_json::from_value::<MultiBinState>(value).is_err());
    for unsupported in [
        "hook_return_delta",
        "lp_fee_override",
        "native_settlement",
        "complete",
    ] {
        let mut value = serde_json::to_value(state()).unwrap();
        value[unsupported] = serde_json::json!(true);
        assert!(serde_json::from_value::<MultiBinState>(value).is_err());
    }
}
