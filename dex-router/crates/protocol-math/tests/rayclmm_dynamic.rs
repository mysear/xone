//! Focused dynamic-profile regressions. Fixtures are local to this service.
use serde_json::{json, Value};
use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;

fn pool_value() -> Value {
    static CORPUS: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../test/contracts/rayclmm-seed-quotes.json"
        ))
        .unwrap()
    });
    let corpus = &*CORPUS;
    let mut p = corpus["cases"][0]["pool"].clone();
    p["protocol"] = json!("raydium-clmm-dynamic");
    p["adapter_version"] = json!("raydium-clmm/6d7a0f82-spl-dynamic-1");
    let now = p["raydium_clmm"]["bank_unix_timestamp"].as_u64().unwrap();
    p["raydium_clmm"]["dynamic_fee"] = json!({
        "filter_period": 10, "decay_period": 100, "reduction_factor": 5000,
        "dynamic_fee_control": 99999, "max_volatility_accumulator": 1000000,
        "tick_spacing_index_reference": 30720, "volatility_reference": 10000,
        "volatility_accumulator": 40000, "last_update_timestamp": now
    });
    p
}
fn decode(p: Value) -> Pool {
    serde_json::from_value(p).unwrap()
}
fn dynamic(p: &Pool) -> Value {
    serde_json::to_value(p).unwrap()["raydium_clmm"]["dynamic_fee"].clone()
}
fn quote(p: &Pool, input: &str, left: bool) -> xone_protocol_math::Swap {
    let before = p.clone();
    let q = quote_exact_in(p, &input.parse().unwrap(), left).unwrap();
    assert_eq!(p, &before, "caller mutated");
    assert_eq!(q.consumed_in, input.parse().unwrap());
    assert_eq!(q.remaining_in, Amount::zero());
    q.next_pool.validate().unwrap();
    q
}

#[test]
fn dynamic_reference_time_windows_include_equalities_and_future_saturation() {
    for (elapsed, future, expected_ref, changed) in [
        (0, false, 10000, false),
        (9, false, 10000, false),
        (10, false, 20000, true),
        (99, false, 20000, true),
        (100, false, 0, true),
        (101, false, 0, true),
        (1, true, 10000, false),
    ] {
        let mut v = pool_value();
        let now = v["raydium_clmm"]["bank_unix_timestamp"].as_u64().unwrap();
        let last = if future { now + elapsed } else { now - elapsed };
        v["raydium_clmm"]["dynamic_fee"]["last_update_timestamp"] = json!(last);
        let p = decode(v);
        for left in [false, true] {
            let q = quote(&p, "1000000", left);
            let d = dynamic(&q.next_pool);
            assert_eq!(d["volatility_reference"], expected_ref, "elapsed {elapsed}");
            assert_eq!(d["last_update_timestamp"], if changed { now } else { last });
            assert_eq!(d["tick_spacing_index_reference"], 30720);
            assert_eq!(
                d["volatility_accumulator"],
                expected_ref + if left { 10000 } else { 0 }
            );
        }
    }
}

#[test]
fn dynamic_budget_counts_spacing_iterations_including_no_price_movement() {
    let mut v = pool_value();
    v["raydium_clmm"]["max_steps"] = json!(1);
    let p = decode(v);
    let before = p.clone();
    // At an exact spacing boundary left first performs a zero-amount iteration.
    let e = quote_exact_in(&p, &"1000000".parse().unwrap(), true).unwrap_err();
    assert_eq!(e.0, "RAYCLMM_STEP_BUDGET");
    assert_eq!(p, before);
    quote(&p, "1000000", false);
    let mut v = serde_json::to_value(p).unwrap();
    v["raydium_clmm"]["max_steps"] = json!(2);
    quote(&decode(v), "1000000", true);
}

#[test]
fn dynamic_modes_use_selected_token_for_raw_and_growth() {
    for mode in 0..=2 {
        for left in [false, true] {
            let mut v = pool_value();
            v["raydium_clmm"]["collect_fee_on"] = json!(mode);
            let p = decode(v);
            let q = quote(&p, "100000000", left);
            let a = p.raydium_clmm.as_ref().unwrap();
            let b = q.next_pool.raydium_clmm.as_ref().unwrap();
            let out = q.amount_out.to_string().parse::<u64>().unwrap();
            assert_eq!(
                (b.raw_vault0, b.raw_vault1),
                if left {
                    (a.raw_vault0 + 100000000, a.raw_vault1 - out)
                } else {
                    (a.raw_vault0 - out, a.raw_vault1 + 100000000)
                }
            );
            let on0 = mode == 1 || (mode == 0 && left);
            assert!(if on0 {
                b.fee_growth_global0 > a.fee_growth_global0
            } else {
                b.fee_growth_global1 > a.fee_growth_global1
            });
            assert_eq!(
                if on0 {
                    &b.fee_growth_global1
                } else {
                    &b.fee_growth_global0
                },
                if on0 {
                    &a.fee_growth_global1
                } else {
                    &a.fee_growth_global0
                }
            );
            assert_eq!(b.recent_epoch, a.bank_epoch);
        }
    }
}

#[test]
fn dynamic_errors_return_without_panics_or_caller_mutation() {
    for input in ["0", "18446744073709551616", "18446744073709551615"] {
        for left in [false, true] {
            let p = decode(pool_value());
            let before = p.clone();
            let q = std::panic::catch_unwind(|| quote_exact_in(&p, &input.parse().unwrap(), left));
            assert!(q.is_ok(), "panicked for {input}");
            assert!(q.unwrap().is_err(), "accepted {input}");
            assert_eq!(p, before);
        }
    }
}

#[test]
fn dynamic_crosses_bitmap_arrays_and_updates_crossed_growth_and_epochs() {
    for (left, input) in [(true, "1500000000"), (false, "30000000000")] {
        let p = decode(pool_value());
        let q = quote(&p, input, left);
        let a = p.raydium_clmm.as_ref().unwrap();
        let b = q.next_pool.raydium_clmm.as_ref().unwrap();
        assert_ne!(b.tick_current.div_euclid(60), a.tick_current.div_euclid(60));
        let crossed: Vec<_> = b
            .tick_arrays
            .iter()
            .filter(|array| array.recent_epoch == a.bank_epoch)
            .collect();
        assert!(crossed.len() >= 2, "must cross separate arrays");
        for array in crossed {
            let old = a
                .tick_arrays
                .iter()
                .find(|v| v.address == array.address)
                .unwrap();
            assert!(array.ticks.iter().zip(&old.ticks).any(|(after, before)| {
                after.fee_growth_outside0 != before.fee_growth_outside0
                    || after.fee_growth_outside1 != before.fee_growth_outside1
            }));
        }
        assert!(b.observation_initialized);
        assert_eq!(
            b.observations[b.observation_index as usize].block_timestamp,
            a.bank_unix_timestamp as u32
        );
    }
}

#[test]
fn dynamic_observation_filter_equal_boundary_and_ring_wrap() {
    for elapsed in [14, 15, 16] {
        let mut v = pool_value();
        let now = v["raydium_clmm"]["bank_unix_timestamp"].as_u64().unwrap();
        v["raydium_clmm"]["observation_initialized"] = json!(true);
        v["raydium_clmm"]["observation_index"] = json!(99);
        v["raydium_clmm"]["observations"][99] =
            json!({"block_timestamp":now-elapsed,"tick_cumulative":-100});
        let p = decode(v);
        let q = quote(&p, "100000000", true);
        let s = q.next_pool.raydium_clmm.as_ref().unwrap();
        assert_eq!(s.observation_index, if elapsed < 15 { 99 } else { 0 });
        if elapsed >= 15 {
            assert_eq!(
                s.observations[0].tick_cumulative,
                -100 + 30720 * elapsed as i64
            );
            assert_eq!(s.observations[0].block_timestamp, now as u32);
        } else {
            assert_eq!(
                s.observations,
                p.raydium_clmm.as_ref().unwrap().observations
            );
        }
        assert_eq!(
            s.observation_recent_epoch,
            p.raydium_clmm.as_ref().unwrap().observation_recent_epoch
        );
    }
}

#[test]
fn dynamic_zero_liquidity_gap_charges_no_fee_before_entry_crossing() {
    for left in [true, false] {
        let mut v = pool_value();
        v["raydium_clmm"]["liquidity"] = json!("0");
        let entry = if left { 30700 } else { 30740 };
        for array in v["raydium_clmm"]["tick_arrays"].as_array_mut().unwrap() {
            for tick in array["ticks"].as_array_mut().unwrap() {
                if tick["tick"] == entry {
                    tick["liquidity_net"] = json!(if left {
                        "-1000000000000"
                    } else {
                        "1000000000000"
                    });
                }
            }
        }
        let p = decode(v);
        let q = quote(&p, "100000000", left);
        let before = p.raydium_clmm.as_ref().unwrap();
        let after = q.next_pool.raydium_clmm.as_ref().unwrap();
        let crossed = after
            .tick_arrays
            .iter()
            .flat_map(|a| &a.ticks)
            .find(|t| t.tick == entry)
            .unwrap();
        assert_eq!(crossed.fee_growth_outside0, before.fee_growth_global0);
        assert_eq!(crossed.fee_growth_outside1, before.fee_growth_global1);
        assert_eq!(after.liquidity.to_string(), "1000000000000");
        assert!(if left {
            after.tick_current < entry
        } else {
            after.tick_current >= entry
        });
    }
}

#[test]
fn dynamic_invalid_parameters_and_counter_overflows_return_error() {
    for (field, value) in [
        ("filter_period", json!(0)),
        ("filter_period", json!(65536)),
        ("decay_period", json!(10)),
        ("decay_period", json!(65536)),
        ("reduction_factor", json!(0)),
        ("reduction_factor", json!(10000)),
        ("dynamic_fee_control", json!(0)),
        ("dynamic_fee_control", json!(100000)),
        ("volatility_reference", json!(1000001)),
        ("volatility_accumulator", json!(1000001)),
        ("tick_spacing_index_reference", json!(-443637)),
        ("tick_spacing_index_reference", json!(443637)),
        ("last_update_timestamp", json!(4294967296u64)),
    ] {
        let mut v = pool_value();
        v["raydium_clmm"]["dynamic_fee"][field] = value;
        let p = decode(v);
        let before = p.clone();
        let result =
            std::panic::catch_unwind(|| quote_exact_in(&p, &"100000000".parse().unwrap(), true));
        assert!(result.is_ok(), "panic for {field}");
        assert!(result.unwrap().is_err(), "accepted {field}");
        assert_eq!(p, before);
    }
    for left in [true, false] {
        for field in [
            if left {
                "protocol_fees0"
            } else {
                "protocol_fees1"
            },
            if left { "fund_fees0" } else { "fund_fees1" },
        ] {
            let mut v = pool_value();
            v["raydium_clmm"][field] = json!(u64::MAX);
            let p = decode(v);
            let before = p.clone();
            let result = std::panic::catch_unwind(|| {
                quote_exact_in(&p, &"100000000".parse().unwrap(), left)
            });
            assert!(result.is_ok());
            assert!(result.unwrap().is_err());
            assert_eq!(p, before);
        }
    }
}

#[test]
fn dynamic_each_spacing_step_uses_its_own_fee_rounding_and_growth() {
    use xone_domain::rayclmm_math::{sqrt_price_from_tick, tick_from_sqrt_price};
    use xone_protocol_math::raydium_clmm::step_exact_in;
    for mode in 0..=2 {
        for (left, input) in [(true, 100_000_000u64), (false, 3_000_000_000u64)] {
            let mut v = pool_value();
            v["raydium_clmm"]["collect_fee_on"] = json!(mode);
            let p = decode(v);
            let s = p.raydium_clmm.as_ref().unwrap();
            let on0 = mode == 1 || (mode == 0 && left);
            let fee_on_input = on0 == left;
            let mut remaining = input;
            let mut current = u128::try_from(s.sqrt_price_x64.0.clone()).unwrap();
            let mut target_tick = if left { 30719 } else { 30721 };
            let mut total_out = 0;
            let mut protocol = if on0 {
                s.protocol_fees0
            } else {
                s.protocol_fees1
            };
            let mut fund = if on0 { s.fund_fees0 } else { s.fund_fees1 };
            let mut growth = 0;
            let mut count = 0;
            let mut accumulator = 0;
            while remaining > 0 {
                let index = if left { target_tick } else { target_tick - 1 };
                accumulator = 10000 + (30720i32 - index).unsigned_abs() * 10000;
                let fee = s.trade_fee_rate
                    + (u128::from(accumulator).pow(2) * 99999).div_ceil(10_000_000_000_000) as u32;
                let target = sqrt_price_from_tick(target_tick)
                    .unwrap()
                    .try_into()
                    .unwrap();
                let step = step_exact_in(
                    remaining,
                    if fee_on_input { fee } else { 0 },
                    1_000_000_000_000,
                    current,
                    target,
                    left,
                )
                .unwrap();
                let fee_amount = if fee_on_input {
                    step.fee_amount
                } else {
                    (u128::from(step.amount_out) * u128::from(fee)).div_ceil(1_000_000) as u64
                };
                let consumed = if fee_on_input {
                    step.amount_in + fee_amount
                } else if step.next_price != target {
                    remaining
                } else {
                    step.amount_in
                };
                remaining -= consumed;
                total_out += step.amount_out - if fee_on_input { 0 } else { fee_amount };
                let pd = fee_amount * 120000 / 1_000_000;
                let fd = fee_amount * 40000 / 1_000_000;
                protocol += pd;
                fund += fd;
                growth += (u128::from(fee_amount - pd - fd) << 64) / 1_000_000_000_000;
                current = step.next_price;
                count += 1;
                target_tick += if left { -1 } else { 1 };
            }
            assert!(
                count > 5,
                "must exercise many uninitialized spacing boundaries"
            );
            let q = quote(&p, &input.to_string(), left);
            let post = q.next_pool.raydium_clmm.as_ref().unwrap();
            assert_eq!(q.amount_out.to_string(), total_out.to_string());
            assert_eq!(post.sqrt_price_x64.0, num_bigint::BigUint::from(current));
            assert_eq!(
                post.tick_current,
                tick_from_sqrt_price(&num_bigint::BigUint::from(current)).unwrap()
            );
            assert_eq!(dynamic(&q.next_pool)["volatility_accumulator"], accumulator);
            let before_growth = u128::try_from(if on0 {
                &s.fee_growth_global0.0
            } else {
                &s.fee_growth_global1.0
            })
            .unwrap();
            assert_eq!(
                if on0 {
                    post.protocol_fees0
                } else {
                    post.protocol_fees1
                },
                protocol
            );
            assert_eq!(
                if on0 {
                    post.fund_fees0
                } else {
                    post.fund_fees1
                },
                fund
            );
            assert_eq!(
                if on0 {
                    &post.fee_growth_global0.0
                } else {
                    &post.fee_growth_global1.0
                },
                &num_bigint::BigUint::from(before_growth + growth)
            );
        }
    }
}

#[test]
fn dynamic_raw_vault_overflow_and_underflow_are_atomic_errors() {
    for mode in 0..=2 {
        for left in [false, true] {
            for raw_input in [false, true] {
                let mut v = pool_value();
                v["raydium_clmm"]["collect_fee_on"] = json!(mode);
                let field = match (left, raw_input) {
                    (true, true) | (false, false) => "raw_vault0",
                    _ => "raw_vault1",
                };
                v["raydium_clmm"][field] = json!(if raw_input { u64::MAX } else { 0 });
                let p = decode(v);
                let before = p.clone();
                let q = std::panic::catch_unwind(|| {
                    quote_exact_in(&p, &"100000000".parse().unwrap(), left)
                });
                assert!(q.is_ok(), "panic in {field}");
                assert_eq!(q.unwrap().unwrap_err().0, "RAYCLMM_ARITHMETIC");
                assert_eq!(p, before);
            }
        }
    }
}

#[test]
fn dynamic_growth_wraps_with_the_same_delta_as_an_unwrapped_quote() {
    for mode in 0..=2 {
        for left in [false, true] {
            let mut v = pool_value();
            v["raydium_clmm"]["collect_fee_on"] = json!(mode);
            let normal = decode(v.clone());
            let q = quote(&normal, "100000000", left);
            let on0 = mode == 1 || (mode == 0 && left);
            let key = if on0 {
                "fee_growth_global0"
            } else {
                "fee_growth_global1"
            };
            let before = normal.raydium_clmm.as_ref().unwrap();
            let after = q.next_pool.raydium_clmm.as_ref().unwrap();
            let initial = u128::try_from(if on0 {
                &before.fee_growth_global0.0
            } else {
                &before.fee_growth_global1.0
            })
            .unwrap();
            let final_growth = u128::try_from(if on0 {
                &after.fee_growth_global0.0
            } else {
                &after.fee_growth_global1.0
            })
            .unwrap();
            let delta = final_growth - initial;
            assert!(delta > 1);
            v["raydium_clmm"][key] = json!((u128::MAX - 1).to_string());
            let wrapped = quote(&decode(v), "100000000", left);
            let after = wrapped.next_pool.raydium_clmm.as_ref().unwrap();
            assert_eq!(
                if on0 {
                    &after.fee_growth_global0.0
                } else {
                    &after.fee_growth_global1.0
                },
                &num_bigint::BigUint::from((u128::MAX - 1).wrapping_add(delta))
            );
            assert_eq!(wrapped.amount_out, q.amount_out);
            assert_eq!(dynamic(&wrapped.next_pool), dynamic(&q.next_pool));
        }
    }
}
