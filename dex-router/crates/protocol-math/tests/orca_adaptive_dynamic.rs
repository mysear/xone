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
fn adaptive_dynamic_kernel_mixed_layouts_preserve_exact_math_and_poststate() {
    let (mut s, p) = kernel();
    s.reward_infos[0].mint = [1; 32];
    s.reward_infos[0].vault = [2; 32];
    s.reward_infos[0].emissions_per_second_x64 = 10 << 64;
    let reference = quote_exact_in_with_rewards(&s, p).unwrap();
    for mask in 0..8 {
        let mut dynamic = s.clone();
        for (i, a) in dynamic.tick_arrays.iter_mut().enumerate() {
            a.layout = if mask & (1 << i) != 0 {
                "dynamic"
            } else {
                "fixed"
            }
            .into();
        }
        let mut expected = reference.clone();
        for (a, b) in expected
            .next_state
            .tick_arrays
            .iter_mut()
            .zip(&dynamic.tick_arrays)
        {
            a.layout = b.layout.clone();
        }
        assert_eq!(
            serde_json::to_value(quote_exact_in_dynamic(&dynamic, p).unwrap()).unwrap(),
            serde_json::to_value(expected).unwrap(),
            "mask {mask}"
        );
        if mask != 0 {
            assert!(quote_exact_in(&dynamic, p).is_err());
            assert!(quote_exact_in_with_rewards(&dynamic, p).is_err());
        }
    }
}
#[test]
fn adaptive_dynamic_kernel_requires_three_complete_arrays_and_known_layouts() {
    let (s, p) = kernel();
    for bad in 0..5 {
        let mut q = s.clone();
        match bad {
            0 => {
                q.tick_arrays.pop();
            }
            1 => q.tick_arrays[0].layout = "sparse".into(),
            2 => q.tick_arrays[0].initialized_bitmap ^= 1,
            3 => q.tick_arrays[1].start_tick_index += 88,
            _ => q.oracle.reserved.pop().map(|_| ()).unwrap(),
        };
        assert!(quote_exact_in_dynamic(&q, p).is_err(), "bad {bad}");
    }
}

use xone_domain::{Amount, Pool};
#[test]
fn adaptive_dynamic_original_normal_dispatch_all_twenty_banks_and_full_sequences() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
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
fn adaptive_dynamic_kernel_original_twenty_cases_full_poststate() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-service.json"
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
            let q = quote_exact_in_dynamic(&s, p);
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
fn adaptive_dynamic_replays_fixed_corpus_all_three_array_cases_and_errors() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-swap.json"
    ))
    .unwrap();
    let (mut success, mut failure, mut excluded) = (0, 0, 0);
    for (cases, succeeds) in [("cases", true), ("failures", false)] {
        for c in f[cases].as_array().unwrap() {
            let state: AdaptiveSwapState =
                serde_json::from_value(c["request"]["state"].clone()).unwrap();
            let params: AdaptiveExactInParams =
                serde_json::from_value(c["request"]["params"].clone()).unwrap();
            let reference = quote_exact_in(&state, params);
            assert_eq!(reference.is_ok(), succeeds, "{}", c["name"]);
            if state.tick_arrays.len() != 3 {
                assert_eq!(c["name"], "shifted-min-start");
                assert!(quote_exact_in_dynamic(&state, params).is_err());
                excluded += 1;
                continue;
            }
            for mask in 1..8 {
                let mut dynamic = state.clone();
                for (i, a) in dynamic.tick_arrays.iter_mut().enumerate() {
                    a.layout = if mask & (1 << i) != 0 {
                        "dynamic"
                    } else {
                        "fixed"
                    }
                    .into();
                }
                assert!(quote_exact_in(&dynamic, params).is_err());
                assert!(quote_exact_in_with_rewards(&dynamic, params).is_err());
                match &reference {
                    Ok(expected) => {
                        let mut expected = serde_json::to_value(expected).unwrap();
                        for (a, b) in expected["next_state"]["tick_arrays"]
                            .as_array_mut()
                            .unwrap()
                            .iter_mut()
                            .zip(&dynamic.tick_arrays)
                        {
                            a["layout"] = b.layout.clone().into();
                        }
                        assert_eq!(
                            serde_json::to_value(quote_exact_in_dynamic(&dynamic, params).unwrap())
                                .unwrap(),
                            expected,
                            "{}/{mask}",
                            c["name"]
                        );
                    }
                    Err(expected) => assert_eq!(
                        quote_exact_in_dynamic(&dynamic, params).unwrap_err().0,
                        expected.0,
                        "{}/{mask}",
                        c["name"]
                    ),
                }
            }
            if succeeds {
                success += 1;
            } else {
                failure += 1;
            }
        }
    }
    assert_eq!((success, failure, excluded), (100, 2, 1));
}
