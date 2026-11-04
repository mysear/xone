use raydium_clmm_reference::{base_pool, quote, Pool};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const PROFILE: &str = "raydium-clmm-token-fee";
const ADAPTER: &str = "raydium-clmm/6d7a0f82-spl-token-fee-1";

fn token_pool(mode: u32) -> Pool {
    let mut p = serde_json::to_value(base_pool()).unwrap();
    p["protocol"] = json!(PROFILE);
    p["adapter_version"] = json!(ADAPTER);
    p["raydium_clmm"]["collect_fee_on"] = json!(mode);
    serde_json::from_value(p).expect("token-fee state must deserialize")
}

#[test]
fn old_json_defaults_to_zero_and_explicit_zero_stays_omitted() {
    let p = base_pool();
    assert_eq!(p.raydium_clmm.collect_fee_on, 0);
    assert_eq!(raydium_clmm_reference::TOKEN_FEE_PROFILE, PROFILE);
    assert_eq!(raydium_clmm_reference::TOKEN_FEE_ADAPTER, ADAPTER);
    let old = serde_json::to_value(&p).unwrap();
    assert!(old["raydium_clmm"].get("collect_fee_on").is_none());
    let parsed: Pool = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(parsed, p);
    let mut explicit = old.clone();
    explicit["raydium_clmm"]["collect_fee_on"] = json!(0);
    let parsed: Pool = serde_json::from_value(explicit).expect("explicit mode zero is valid");
    assert_eq!(serde_json::to_value(parsed).unwrap(), old);
}

#[test]
fn profile_adapter_and_mode_combinations_are_strict() {
    let base = base_pool();
    for (profile, adapter) in [
        ("unknown-profile", base.adapter_version.as_str()),
        (base.protocol.as_str(), "unknown-adapter"),
        (base.protocol.as_str(), ADAPTER),
        (PROFILE, base.adapter_version.as_str()),
    ] {
        let mut p = base.clone();
        p.protocol = profile.into();
        p.adapter_version = adapter.into();
        assert!(
            quote(&p, "1000000", true).is_err(),
            "accepted {profile}/{adapter}"
        );
    }
    for mode in [0, 1, 2, 3, 255, 256, 257, u32::MAX] {
        for token_profile in [false, true] {
            let mut p = serde_json::to_value(&base).unwrap();
            if token_profile {
                p["protocol"] = json!(PROFILE);
                p["adapter_version"] = json!(ADAPTER);
            }
            p["raydium_clmm"]["collect_fee_on"] = json!(mode);
            let p: Pool = serde_json::from_value(p).unwrap();
            for direction in [false, true] {
                assert_eq!(
                    quote(&p, "1000000", direction).is_ok(),
                    if token_profile {
                        mode == 1 || mode == 2
                    } else {
                        mode == 0
                    },
                    "mode={mode} token_profile={token_profile} direction={direction}"
                );
            }
        }
    }
}

#[test]
fn native_fees_follow_selected_token_in_both_directions() {
    for mode in [1, 2] {
        for direction in [false, true] {
            for input in [1_000_000u64, 2_000_000_000] {
                let p = token_pool(mode);
                let before = p.raydium_clmm.clone();
                let q = quote(&p, &input.to_string(), direction).unwrap();
                let after = &q.pool.raydium_clmm;
                assert_eq!(serde_json::to_value(after).unwrap()["collect_fee_on"], mode);
                if mode == 1 {
                    assert!(after.protocol_fees0 > before.protocol_fees0);
                    assert!(after.fund_fees0 > before.fund_fees0);
                    assert!(
                        after.fee_growth_global0.parse::<u128>().unwrap()
                            > before.fee_growth_global0.parse::<u128>().unwrap()
                    );
                    assert_eq!(after.protocol_fees1, before.protocol_fees1);
                    assert_eq!(after.fund_fees1, before.fund_fees1);
                    assert_eq!(after.fee_growth_global1, before.fee_growth_global1);
                } else {
                    assert!(after.protocol_fees1 > before.protocol_fees1);
                    assert!(after.fund_fees1 > before.fund_fees1);
                    assert!(
                        after.fee_growth_global1.parse::<u128>().unwrap()
                            > before.fee_growth_global1.parse::<u128>().unwrap()
                    );
                    assert_eq!(after.protocol_fees0, before.protocol_fees0);
                    assert_eq!(after.fund_fees0, before.fund_fees0);
                    assert_eq!(after.fee_growth_global0, before.fee_growth_global0);
                }
                let (raw_in_before, raw_out_before, raw_in_after, raw_out_after) = if direction {
                    (
                        before.raw_vault0,
                        before.raw_vault1,
                        after.raw_vault0,
                        after.raw_vault1,
                    )
                } else {
                    (
                        before.raw_vault1,
                        before.raw_vault0,
                        after.raw_vault1,
                        after.raw_vault0,
                    )
                };
                assert_eq!(raw_in_after, raw_in_before + input);
                assert_eq!(raw_out_after, raw_out_before - q.amount_out);
                if (mode == 1) == direction {
                    let old = quote(&base_pool(), &input.to_string(), direction).unwrap();
                    assert_eq!(q.amount_out, old.amount_out);
                    assert_eq!(after.sqrt_price_x64, old.pool.raydium_clmm.sqrt_price_x64);
                } else {
                    let mut no_fee = p.clone();
                    no_fee.fee_numerator = 0;
                    no_fee.raydium_clmm.trade_fee_rate = 0;
                    let gross = quote(&no_fee, &input.to_string(), direction).unwrap();
                    assert!(q.amount_out < gross.amount_out);
                    assert_eq!(after.sqrt_price_x64, gross.pool.raydium_clmm.sqrt_price_x64);
                }
            }
        }
    }
}

#[test]
fn output_fee_ceil_charges_one_unit_and_tiny_zero_net_output_reverts() {
    for (mode, direction) in [(1, false), (2, true)] {
        let p = token_pool(mode);
        let mut no_fee = p.clone();
        no_fee.fee_numerator = 0;
        no_fee.raydium_clmm.trade_fee_rate = 0;
        let gross = quote(&no_fee, "3", direction).unwrap();
        let q = quote(&p, "3", direction).unwrap();
        assert_eq!(q.amount_out, gross.amount_out - 1);
        assert_eq!(
            q.pool.raydium_clmm.sqrt_price_x64,
            gross.pool.raydium_clmm.sqrt_price_x64
        );
        assert_eq!(
            q.pool.raydium_clmm.protocol_fees0,
            p.raydium_clmm.protocol_fees0
        );
        assert_eq!(
            q.pool.raydium_clmm.protocol_fees1,
            p.raydium_clmm.protocol_fees1
        );
        assert_eq!(q.pool.raydium_clmm.fund_fees0, p.raydium_clmm.fund_fees0);
        assert_eq!(q.pool.raydium_clmm.fund_fees1, p.raydium_clmm.fund_fees1);
        assert!(quote(&p, "1", direction).is_err());
    }
}

#[test]
fn native_counter_boundaries_follow_fee_token_and_failures_leave_input_unchanged() {
    for mode in [1, 2] {
        for direction in [false, true] {
            let mut p = token_pool(mode);
            if mode == 1 {
                p.raydium_clmm.protocol_fees1 = u64::MAX;
                p.raydium_clmm.fund_fees1 = u64::MAX;
            } else {
                p.raydium_clmm.protocol_fees0 = u64::MAX;
                p.raydium_clmm.fund_fees0 = u64::MAX;
            }
            let q = quote(&p, "1000000", direction).unwrap();
            if mode == 1 {
                assert_eq!(q.pool.raydium_clmm.protocol_fees1, u64::MAX);
                assert_eq!(q.pool.raydium_clmm.fund_fees1, u64::MAX);
            } else {
                assert_eq!(q.pool.raydium_clmm.protocol_fees0, u64::MAX);
                assert_eq!(q.pool.raydium_clmm.fund_fees0, u64::MAX);
            }
            for counter in ["protocol", "fund"] {
                let mut overflow = p.clone();
                let s = &mut overflow.raydium_clmm;
                match (mode, counter) {
                    (1, "protocol") => s.protocol_fees0 = u64::MAX,
                    (1, _) => s.fund_fees0 = u64::MAX,
                    (2, "protocol") => s.protocol_fees1 = u64::MAX,
                    (2, _) => s.fund_fees1 = u64::MAX,
                    _ => unreachable!(),
                }
                let before = overflow.clone();
                assert!(quote(&overflow, "1000000", direction).is_err());
                assert_eq!(overflow, before);
            }
        }
    }
}

#[test]
fn native_growth_wrap_and_tick_outside_updates_follow_fee_token() {
    for mode in [1, 2] {
        for direction in [false, true] {
            let mut p = token_pool(mode);
            p.raydium_clmm.fee_growth_global0 = (u128::MAX - 10).to_string();
            p.raydium_clmm.fee_growth_global1 = (u128::MAX - 20).to_string();
            let q = quote(&p, "2000000000", direction).unwrap();
            let before = &p.raydium_clmm;
            let after = &q.pool.raydium_clmm;
            if mode == 1 {
                assert!(
                    after.fee_growth_global0.parse::<u128>().unwrap()
                        < before.fee_growth_global0.parse::<u128>().unwrap()
                );
                assert_eq!(after.fee_growth_global1, before.fee_growth_global1);
            } else {
                assert!(
                    after.fee_growth_global1.parse::<u128>().unwrap()
                        < before.fee_growth_global1.parse::<u128>().unwrap()
                );
                assert_eq!(after.fee_growth_global0, before.fee_growth_global0);
            }
            assert!(after
                .tick_arrays
                .iter()
                .zip(&before.tick_arrays)
                .any(|(a, b)| a.recent_epoch != b.recent_epoch));
            assert!(after
                .tick_arrays
                .iter()
                .zip(&before.tick_arrays)
                .flat_map(|(a, b)| a.ticks.iter().zip(&b.ticks))
                .any(|(a, b)| a.fee_growth_outside0 != b.fee_growth_outside0
                    && a.fee_growth_outside1 != b.fee_growth_outside1));
        }
    }
}

fn check_expected(pool: &Pool, step: &Value) -> Pool {
    match quote(
        pool,
        step["input"].as_str().unwrap(),
        step["zero_for_one"].as_bool().unwrap(),
    ) {
        Ok(q) => {
            assert_eq!(step["expected"]["amount_out"], q.amount_out.to_string());
            assert_eq!(
                step["expected"]["next_pool"],
                serde_json::to_value(&q.pool).unwrap()
            );
            q.pool
        }
        Err(_) => {
            assert_eq!(step["expected"], json!({"reverted":true}));
            pool.clone()
        }
    }
}

#[test]
fn token_fee_command_emits_balanced_independent_native_corpus_and_complete_sequences() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_raydium-clmm-reference"))
        .arg("generate-token-fee")
        .output()
        .unwrap();
    assert!(output.status.success(), "generate-token-fee command failed");
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["reference"]["revision"],
        raydium_clmm_reference::REVISION
    );
    assert_eq!(corpus["reference"]["business_math_imported"], false);
    assert_eq!(corpus["reference"]["sbf_executed"], false);
    assert!(corpus["reference"]["execution_limits"]["max_steps"]
        .as_str()
        .unwrap()
        .contains("conservative"));
    assert_eq!(
        corpus["base_pool"],
        serde_json::to_value(token_pool(1)).unwrap()
    );
    assert_eq!(
        corpus["base_pool_mode2"],
        serde_json::to_value(token_pool(2)).unwrap()
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() >= 300);
    let mut counts = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut successful = BTreeMap::new();
    for case in cases {
        assert!(names.insert(case["name"].as_str().unwrap()));
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        assert_eq!(pool.protocol, PROFILE);
        assert_eq!(pool.adapter_version, ADAPTER);
        let mode = case["pool"]["raydium_clmm"]["collect_fee_on"]
            .as_u64()
            .unwrap();
        assert!(mode == 1 || mode == 2);
        let direction = case["zero_for_one"].as_bool().unwrap();
        *counts.entry((mode, direction)).or_insert(0usize) += 1;
        if case["expected"].get("next_pool").is_some() {
            *successful.entry((mode, direction)).or_insert(0usize) += 1;
        }
        check_expected(&pool, case);
    }
    assert_eq!(counts.len(), 4);
    assert_eq!(counts.values().copied().collect::<BTreeSet<_>>().len(), 1);
    assert_eq!(successful.len(), 4);
    assert!(successful.values().all(|&count| count >= 30));
    let independent = corpus["independent_vectors"].as_array().unwrap();
    assert_eq!(independent.len(), 8);
    for mode in [1, 2] {
        for direction in [false, true] {
            for input in [1_000_000u64, 2_000_000_000] {
                let name = format!("mode{mode}-independent-{input}-{direction}");
                assert!(independent.contains(&json!(name)));
                let case = cases.iter().find(|case| case["name"] == name).unwrap();
                assert_eq!(
                    case["pool"],
                    serde_json::to_value(token_pool(mode)).unwrap()
                );
                assert_eq!(case["input"], input.to_string());
                assert_eq!(case["zero_for_one"], direction);
                assert!(case["expected"]["next_pool"].is_object());
            }
        }
    }
    let sequences = corpus["sequences"].as_array().unwrap();
    assert!(sequences.len() >= 8);
    let mut sequence_shapes = BTreeSet::new();
    for sequence in sequences {
        let mut pool: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        let mode = sequence["pool"]["raydium_clmm"]["collect_fee_on"]
            .as_u64()
            .unwrap();
        let steps = sequence["steps"].as_array().unwrap();
        assert!(steps.len() >= 2);
        let directions: BTreeSet<_> = steps
            .iter()
            .map(|step| step["zero_for_one"].as_bool().unwrap())
            .collect();
        sequence_shapes.insert((
            mode,
            steps[0]["zero_for_one"].as_bool().unwrap(),
            directions.len(),
        ));
        for step in steps {
            assert!(step["expected"]["next_pool"].is_object());
            pool = check_expected(&pool, step);
        }
        assert_eq!(
            sequence["expected_final_pool"],
            serde_json::to_value(pool).unwrap()
        );
    }
    assert_eq!(sequence_shapes.len(), 8); // both modes: same and mixed, starting in either direction.
}
