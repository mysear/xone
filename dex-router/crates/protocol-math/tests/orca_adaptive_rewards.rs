use xone_domain::{Amount, Pool};
use xone_protocol_math::orca_adaptive_swap::*;
fn kernel() -> (AdaptiveSwapState, AdaptiveExactInParams) {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-swap.json"
    ))
    .unwrap();
    let c = &f["cases"][0];
    (
        serde_json::from_value(c["request"]["state"].clone()).unwrap(),
        serde_json::from_value(c["request"]["params"].clone()).unwrap(),
    )
}
#[test]
fn adaptive_rewards_kernel_checked_product_wrap_cross_and_preserve_uninitialized() {
    let (mut s, mut p) = kernel();
    p.amount = 15000;
    p.timestamp = s.reward_last_updated_timestamp + 11;
    s.reward_infos[0].mint = [1; 32];
    s.reward_infos[0].vault = [2; 32];
    s.reward_infos[0].emissions_per_second_x64 = 10 << 64;
    s.reward_infos[0].growth_global_x64 = u128::MAX - 17;
    s.reward_infos[1].mint = [3; 32];
    s.reward_infos[1].vault = [4; 32];
    s.reward_infos[1].emissions_per_second_x64 = u128::MAX;
    s.reward_infos[1].growth_global_x64 = 20;
    // The original skips every field of an uninitialized slot, even historical outside data.
    s.reward_infos[2].growth_global_x64 = 123;
    s.reward_infos[2].emissions_per_second_x64 = 456;
    for a in &mut s.tick_arrays {
        for t in &mut a.ticks {
            t.reward_growths_outside = [101, 102, 103];
        }
    }
    assert!(quote_exact_in(&s, p).is_err());
    let q = quote_exact_in_with_rewards(&s, p).unwrap();
    let expected = (u128::MAX - 17).wrapping_add((11u128 * (10u128 << 64)) / s.liquidity);
    assert_eq!(q.next_state.reward_infos[0].growth_global_x64, expected);
    assert_eq!(q.next_state.reward_infos[1].growth_global_x64, 20);
    assert_eq!(q.next_state.reward_infos[2], s.reward_infos[2]);
    assert_eq!(q.next_state.reward_last_updated_timestamp, p.timestamp);
    let mut crosses = 0;
    for (a, b) in s.tick_arrays.iter().zip(&q.next_state.tick_arrays) {
        for (t, u) in a.ticks.iter().zip(&b.ticks) {
            assert_eq!(u.reward_growths_outside[2], 103);
            if t.fee_growth_outside_a != u.fee_growth_outside_a {
                crosses += 1;
                assert_eq!(u.reward_growths_outside[0], expected.wrapping_sub(101));
                assert_eq!(u.reward_growths_outside[1], 20u128.wrapping_sub(102));
            }
        }
    }
    assert!(crosses > 0);
    let mut same = q.next_state.clone();
    p.timestamp = same.reward_last_updated_timestamp;
    p.amount = 1;
    let again = quote_exact_in_with_rewards(&same, p).unwrap();
    assert_eq!(again.next_state.reward_infos, same.reward_infos);
    same.tick_arrays.pop();
    assert!(quote_exact_in_with_rewards(&same, p).is_err());
}
#[test]
fn adaptive_rewards_original_normal_dispatch_all_twenty_banks_and_full_sequences() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-pools.json"
    ))
    .unwrap();
    let mut success = 0;
    let mut failure = 0;
    let mut previous: Option<Pool> = None;
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        p.validate().unwrap();
        if i % 5 != 0 {
            let mut carried = previous.take().unwrap();
            let a = carried.orca_whirlpool.as_mut().unwrap();
            let b = p.orca_whirlpool.as_ref().unwrap();
            a.bank_slot = b.bank_slot;
            a.bank_unix_timestamp = b.bank_unix_timestamp;
            a.bank_epoch = b.bank_epoch;
            assert_eq!(carried, p, "sequence {}", c["name"]);
        }
        let amount: Amount = c["params"]["amount"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::quote_exact_in(
            &p,
            &amount,
            c["params"]["a_to_b"].as_bool().unwrap(),
        );
        if c["expected"]["status"] == "native_rejected" {
            assert!(q.is_err(), "{}", c["name"]);
            failure += 1;
            previous = None;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        let expected: Pool = serde_json::from_value(c["expected"]["post_pool"].clone()).unwrap();
        assert_eq!(q.next_pool, expected, "full poststate {}", c["name"]);
        assert_eq!(
            q.amount_out.to_string(),
            c["expected"]["amount_out"].as_str().unwrap()
        );
        assert_eq!(q.consumed_in, amount);
        assert_eq!(q.remaining_in, Amount::zero());
        previous = Some(q.next_pool);
        success += 1;
    }
    assert_eq!((success, failure), (16, 4));
}

fn native_kernel(v: &serde_json::Value) -> AdaptiveSwapState {
    let mut v = v.clone();
    let template = serde_json::to_value(kernel().0).unwrap();
    v.as_object_mut()
        .unwrap()
        .retain(|k, _| template.get(k).is_some());
    v["token_program"] = CLASSIC_SPL_TOKEN_PROGRAM.into();
    fn key(v: &mut serde_json::Value) {
        *v = serde_json::json!(bs58::decode(v.as_str().unwrap()).into_vec().unwrap());
    }
    key(&mut v["whirlpool"]);
    for k in ["protocol_fee_owed_a", "protocol_fee_owed_b"] {
        v[k] = v[k].as_str().unwrap().parse::<u64>().unwrap().into();
    }
    v["oracle"].as_object_mut().unwrap().remove("address");
    key(&mut v["oracle"]["whirlpool"]);
    for r in v["reward_infos"].as_array_mut().unwrap() {
        r.as_object_mut().unwrap().remove("initialized");
        key(&mut r["mint"]);
        key(&mut r["vault"]);
    }
    for a in v["tick_arrays"].as_array_mut().unwrap() {
        a.as_object_mut().unwrap().remove("address");
        key(&mut a["whirlpool"]);
        a["ticks"]
            .as_array_mut()
            .unwrap()
            .retain(|t| t["initialized"] == true);
        for t in a["ticks"].as_array_mut().unwrap() {
            t.as_object_mut().unwrap().remove("initialized");
        }
    }
    serde_json::from_value(v).unwrap()
}
#[test]
fn adaptive_rewards_kernel_original_twenty_cases_full_poststate() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-service.json"
    ))
    .unwrap();
    let mut success = 0;
    let mut failure = 0;
    for scenario in f["scenarios"].as_array().unwrap() {
        for bank in scenario["banks"].as_array().unwrap() {
            let s = native_kernel(&bank["native_state"]);
            let mut p = bank["next_trade"]["params"].clone();
            p.as_object_mut()
                .unwrap()
                .remove("amount_specified_is_input");
            p["amount"] = p["amount"].as_str().unwrap().parse::<u64>().unwrap().into();
            let p: AdaptiveExactInParams = serde_json::from_value(p).unwrap();
            let q = quote_exact_in_with_rewards(&s, p);
            let e = &bank["next_trade"]["expected"];
            assert_eq!(
                quote_exact_in(&s, p).unwrap_err().0,
                "ORCA_ADAPTIVE_SWAP_REWARDS_UNSUPPORTED"
            );
            if e["status"] == "native_rejected" {
                assert!(q.is_err());
                failure += 1;
                continue;
            }
            let q = q.unwrap();
            assert_eq!(
                q.next_state,
                native_kernel(&e["next_state"]),
                "{}",
                scenario["name"]
            );
            for (actual, key) in [
                (q.amount_in, "amount_in"),
                (q.amount_out, "amount_out"),
                (q.amount_remaining, "amount_remaining"),
                (q.lp_fee, "lp_fee"),
                (q.protocol_fee, "protocol_fee"),
            ] {
                assert_eq!(actual.to_string(), e[key].as_str().unwrap(), "{key}");
            }
            success += 1;
        }
    }
    assert_eq!((success, failure), (16, 4));
}
#[test]
fn adaptive_rewards_kernel_control_layout_token_timestamp_and_no_accrual_edges() {
    let (s, p) = kernel();
    for slot in [1, 2] {
        let mut bad = s.clone();
        bad.reward_infos[slot].extension[0] = 1;
        assert!(quote_exact_in_with_rewards(&bad, p).is_err());
    }
    let mut bad = s.clone();
    bad.tick_arrays[0].layout = "dynamic".into();
    assert!(quote_exact_in_with_rewards(&bad, p).is_err());
    bad = s.clone();
    bad.token_program = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb".into();
    assert!(quote_exact_in_with_rewards(&bad, p).is_err());
    bad = s.clone();
    bad.reward_last_updated_timestamp = p.timestamp + 1;
    assert!(quote_exact_in_with_rewards(&bad, p).is_err());
    let mut s = s;
    s.reward_infos[0].mint = [1; 32];
    s.reward_infos[0].vault = [2; 32];
    s.reward_infos[0].emissions_per_second_x64 = u128::MAX;
    s.reward_infos[0].growth_global_x64 = 20;
    let mut p = p;
    p.timestamp = s.reward_last_updated_timestamp + 1;
    p.amount = 1;
    let q = quote_exact_in_with_rewards(&s, p).unwrap();
    assert_eq!(
        q.next_state.reward_infos[0].growth_global_x64,
        20u128.wrapping_add(u128::MAX / s.liquidity)
    );
    p.timestamp = s.reward_last_updated_timestamp;
    assert_eq!(
        quote_exact_in_with_rewards(&s, p)
            .unwrap()
            .next_state
            .reward_infos,
        s.reward_infos
    );
    s.liquidity = 0;
    for a in &mut s.tick_arrays {
        for t in &mut a.ticks {
            t.liquidity_net = 0;
        }
    }
    p.timestamp += 1;
    p.sqrt_price_limit =
        u128::try_from(xone_domain::orca_math::sqrt_price_from_tick(-1).unwrap()).unwrap();
    let q = quote_exact_in_with_rewards(&s, p).unwrap();
    assert_eq!(q.next_state.reward_infos, s.reward_infos);
    assert_eq!(q.next_state.reward_last_updated_timestamp, p.timestamp);
}
