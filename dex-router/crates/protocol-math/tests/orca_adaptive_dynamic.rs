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

#[test]
fn adaptive_dynamic_accepts_supplemental_arrays_without_changing_quote() {
    let (state, params) = kernel();
    let reference = quote_exact_in_dynamic(&state, params).unwrap();
    let mut expanded = state.clone();
    let span = 88 * i32::from(expanded.tick_spacing);
    let template = expanded.tick_arrays.last().unwrap().clone();
    for offset in 1..=4 {
        let mut array = template.clone();
        array.start_tick_index += span * offset;
        array.initialized_bitmap = 0;
        array.ticks.clear();
        expanded.tick_arrays.push(array);
    }
    let result = quote_exact_in_dynamic(&expanded, params).unwrap();
    assert_eq!(result.amount_in, reference.amount_in);
    assert_eq!(result.amount_out, reference.amount_out);
    assert_eq!(result.amount_remaining, reference.amount_remaining);
    assert_eq!(result.lp_fee, reference.lp_fee);
    assert_eq!(result.protocol_fee, reference.protocol_fee);
    assert_eq!(
        result.next_state.tick_arrays[..3],
        reference.next_state.tick_arrays[..3]
    );
    assert!(result.next_state.tick_arrays[3..]
        .iter()
        .all(|array| array.initialized_bitmap == 0 && array.ticks.is_empty()));
}

#[test]
fn adaptive_dynamic_uses_only_three_directional_arrays_for_exact_in_and_out() {
    use xone_domain::orca_math::sqrt_price_from_tick;

    let (mut state, mut params) = kernel();
    let span = 88 * i32::from(state.tick_spacing);
    let empty = |start_tick_index| AdaptiveSwapTickArray {
        start_tick_index,
        layout: "dynamic".into(),
        initialized_bitmap: 0,
        ticks: vec![],
        whirlpool: state.whirlpool,
    };
    state.tick_arrays.insert(0, empty(-2 * span));
    state.tick_arrays.insert(0, empty(-3 * span));
    state.tick_arrays.push(empty(2 * span));
    state.tick_arrays.push(empty(3 * span));
    params.amount = 1_000_000_000;
    for (left, direction) in [(true, -1), (false, 1)] {
        params.a_to_b = left;
        for quote in [
            quote_exact_in_dynamic as fn(&_, _) -> _,
            quote_exact_out_dynamic,
        ] {
            let limit = |tick| sqrt_price_from_tick(tick).unwrap().try_into().unwrap();
            params.sqrt_price_limit = limit((span + i32::from(state.tick_spacing)) * direction);
            let mut selected = state.clone();
            let indices = if left { 1..4 } else { 3..6 };
            selected.tick_arrays = state.tick_arrays[indices.clone()].to_vec();
            let expected = quote(&selected, params).unwrap();
            let mut actual = quote(&state, params).unwrap();
            let mut sparse = state.clone();
            let supplemental = if left { 1 } else { 5 };
            sparse.tick_arrays[supplemental].layout = "sparse".into();
            let mut sparse_result = quote(&sparse, params).unwrap();
            assert_eq!(
                sparse_result.next_state.tick_arrays[supplemental].layout,
                "sparse"
            );
            sparse_result.next_state.tick_arrays[supplemental].layout = "dynamic".into();
            assert_eq!(
                serde_json::to_value(&sparse_result).unwrap(),
                serde_json::to_value(&actual).unwrap()
            );
            assert_eq!(actual.next_state.sqrt_price_x64, params.sqrt_price_limit);
            actual.next_state.tick_arrays = actual.next_state.tick_arrays[indices].to_vec();
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );

            params.sqrt_price_limit = limit((3 * span + 1) * direction);
            assert_eq!(
                quote(&state, params).unwrap_err().0,
                "ORCA_ADAPTIVE_SWAP_PROOF_EXHAUSTED"
            );
        }
    }
}

use xone_domain::{Amount, Pool};

#[test]
fn adaptive_token2022_quotes_user_and_vault_amounts_in_both_directions() {
    use xone_domain::spl_token_2022::TransferFeeAccount;
    use xone_protocol_math::{capabilities, exact_out, quote_exact_in};

    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-token2022-wire.json"
    ))
    .unwrap();
    let pool: Pool = serde_json::from_value(fixture["snapshot"]["pools"][0].clone()).unwrap();
    let model = capabilities::describe(&pool).unwrap();
    assert_eq!(model.quote_modes, ["EXACT_IN", "EXACT_OUT"]);
    assert!(model.execution_preparation && model.independent_simulation);
    assert!(!model.wallet_submission);
    let mut classic = pool.clone();
    classic.protocol = xone_domain::orca_adaptive_state::DYNAMIC_PROFILE.into();
    classic.adapter_version = xone_domain::orca_adaptive_state::DYNAMIC_ADAPTER.into();
    classic.orca_whirlpool.as_mut().unwrap().token_2022 = None;

    let uint = |v: u64| Amount::checked(v.into()).unwrap();
    for (left, fee_in, fee_out, gross_target) in [(true, 20, 100u64, 3093), (false, 100, 20, 3020)]
    {
        let classic_quote = quote_exact_in(&classic, &uint(5000 - fee_in), left).unwrap();
        let quote = quote_exact_in(&pool, &uint(5000), left).unwrap();
        assert_eq!(quote.consumed_in, uint(5000));
        assert_eq!(quote.amount_out.0, classic_quote.amount_out.0 - fee_out);
        let classic_out = exact_out::quote_exact_out(&classic, &uint(gross_target), left).unwrap();
        let exact_out = exact_out::quote_exact_out(&pool, &uint(3000), left).unwrap();
        let gross_input_fee = if left {
            20
        } else {
            let net = u64::try_from(&classic_out.amount_in.0).unwrap();
            (net * 300).div_ceil(9700).min(100)
        };
        assert_eq!(
            exact_out.amount_in.0,
            classic_out.amount_in.0 + gross_input_fee
        );
        assert_eq!(exact_out.amount_out, uint(3000));
        let next = quote.next_pool.orca_whirlpool.unwrap();
        let expected = classic_quote.next_pool.orca_whirlpool.unwrap();
        assert_eq!(
            (next.raw_vault0, next.raw_vault1),
            (expected.raw_vault0, expected.raw_vault1)
        );
        assert_eq!(next.adaptive, expected.adaptive);
        let token = next.token_2022.unwrap();
        for (i, asset) in [token.token0, token.token1].into_iter().enumerate() {
            let vault = hex::decode(asset.unwrap().vault_data).unwrap();
            let fee = if (i == 0) == left { fee_in } else { 0 };
            assert_eq!(
                TransferFeeAccount::unpack(&vault).unwrap().withheld,
                17 + i as u64 + fee
            );
        }
    }

    let output = uint(3000);
    let before = exact_out::quote_exact_out(&pool, &output, true).unwrap();
    let mut later = pool.clone();
    later.orca_whirlpool.as_mut().unwrap().bank_epoch += 1;
    let after = exact_out::quote_exact_out(&later, &output, true).unwrap();
    assert!(after.amount_in < before.amount_in);
    later
        .orca_whirlpool
        .as_mut()
        .unwrap()
        .token_2022
        .as_mut()
        .unwrap()
        .token0 = None;
    assert!(
        exact_out::quote_exact_out(&later, &output, true)
            .unwrap()
            .amount_in
            < after.amount_in
    );
}

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
