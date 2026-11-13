use xone_domain::{Amount, Pool};
use xone_protocol_math::orca;
pub use xone_protocol_math::Swap;

fn pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap()
}
fn amount(n: u64) -> Amount {
    n.to_string().parse().unwrap()
}

#[test]
fn full_exact_in_updates_raw_vaults_timestamp_and_directional_fee_state() {
    for left in [true, false] {
        let p = pool();
        p.validate().unwrap();
        let before = p.orca_whirlpool.as_ref().unwrap();
        let q = orca::quote_exact_in(&p, &amount(100000), left).unwrap();
        assert_eq!(q.consumed_in, amount(100000));
        assert_eq!(q.remaining_in, Amount::zero());
        assert!(q.amount_out > Amount::zero());
        let out = u64::try_from(&q.amount_out.0).unwrap();
        let s = q.next_pool.orca_whirlpool.as_ref().unwrap();
        assert_eq!(
            (s.raw_vault0, s.raw_vault1),
            if left {
                (before.raw_vault0 + 100000, before.raw_vault1 - out)
            } else {
                (before.raw_vault0 - out, before.raw_vault1 + 100000)
            }
        );
        assert_eq!(s.reward_last_updated_timestamp, s.bank_unix_timestamp);
        assert_eq!(
            (&q.next_pool.reserve0, &q.next_pool.reserve1),
            (&Amount::zero(), &Amount::zero())
        );
        assert_eq!(
            if left {
                &s.fee_growth_global1
            } else {
                &s.fee_growth_global0
            },
            &Amount::zero()
        );
        q.next_pool.validate().unwrap();
        assert_eq!(p, pool(), "quote must not mutate its input");
    }
}

#[test]
fn fixed_dynamic_and_empty_array_scan_have_identical_amounts() {
    let mut p = pool();
    let s = p.orca_whirlpool.as_mut().unwrap();
    for a in &mut s.tick_arrays {
        a.ticks.clear();
        a.initialized_bitmap = Amount::zero();
    }
    s.max_steps = 1;
    for left in [true, false] {
        let fixed = orca::quote_exact_in(&p, &amount(10000000), left).unwrap();
        let mut dynamic = p.clone();
        for a in &mut dynamic.orca_whirlpool.as_mut().unwrap().tick_arrays {
            a.layout = "dynamic".into();
        }
        let d = orca::quote_exact_in(&dynamic, &amount(10000000), left).unwrap();
        assert_eq!(d.amount_out, fixed.amount_out);
        assert_eq!(
            d.next_pool.orca_whirlpool.as_ref().unwrap().sqrt_price_x64,
            fixed
                .next_pool
                .orca_whirlpool
                .as_ref()
                .unwrap()
                .sqrt_price_x64
        );
    }
}

#[test]
fn b_to_a_first_search_uses_shifted_array_near_end() {
    let mut p = pool();
    let s = p.orca_whirlpool.as_mut().unwrap();
    s.tick_current_index = 5570;
    s.sqrt_price_x64 = Amount(xone_domain::orca_math::sqrt_price_from_tick(5570).unwrap());
    s.max_steps = 1;
    p.validate().unwrap();
    assert!(orca::quote_exact_in(&p, &amount(1000), false).is_ok());
}

#[test]
fn proof_budget_zero_output_u64_and_vault_failures_are_closed() {
    let p = pool();
    assert!(orca::quote_exact_in(&p, &Amount::zero(), true).is_err());
    assert!(orca::quote_exact_in(&p, &amount(1), true).is_err());
    assert!(orca::quote_exact_in(&p, &"18446744073709551616".parse().unwrap(), true).is_err());
    for left in [true, false] {
        assert!(orca::quote_exact_in(&p, &amount(u64::MAX), left).is_err());
        let mut overflow = p.clone();
        let s = overflow.orca_whirlpool.as_mut().unwrap();
        if left {
            s.raw_vault0 = u64::MAX;
        } else {
            s.raw_vault1 = u64::MAX;
        }
        assert!(orca::quote_exact_in(&overflow, &amount(1000), left).is_err());
        let mut underflow = p.clone();
        let s = underflow.orca_whirlpool.as_mut().unwrap();
        if left {
            s.raw_vault1 = 0;
        } else {
            s.raw_vault0 = 0;
        }
        assert!(orca::quote_exact_in(&underflow, &amount(1000), left).is_err());
        let mut budget = p.clone();
        let s = budget.orca_whirlpool.as_mut().unwrap();
        for a in &mut s.tick_arrays {
            for t in &mut a.ticks {
                t.liquidity_net = "0".into();
            }
        }
        s.max_steps = 1;
        assert!(orca::quote_exact_in(&budget, &amount(10000000), left).is_err());
    }
}

#[test]
fn protocol_owed_and_fee_growth_wrap_at_original_widths() {
    let mut p = pool();
    let s = p.orca_whirlpool.as_mut().unwrap();
    s.protocol_fee_rate = 2500;
    s.protocol_fee_owed0 = u64::MAX - 50;
    s.fee_growth_global0 = u128::MAX.to_string().parse().unwrap();
    let q = orca::quote_exact_in(&p, &amount(100000), true).unwrap();
    let n = q.next_pool.orca_whirlpool.unwrap();
    assert_eq!(n.protocol_fee_owed0, 24); // floor(300 * 2500/10000) = 75
    assert_eq!(
        n.fee_growth_global0,
        amount(((225u128 << 64) / 1000000000 - 1) as u64)
    );
}

#[test]
fn crossing_tracks_both_outside_fees_and_reversing_restores_other_global() {
    for left in [true, false] {
        let mut p = pool();
        let s = p.orca_whirlpool.as_mut().unwrap();
        s.fee_growth_global0 = "1009".parse().unwrap();
        s.fee_growth_global1 = "1013".parse().unwrap();
        for a in &mut s.tick_arrays {
            for t in &mut a.ticks {
                t.liquidity_net = "0".into();
                t.fee_growth_outside0 = "37".parse().unwrap();
                t.fee_growth_outside1 = "53".parse().unwrap();
            }
        }
        let q = orca::quote_exact_in(&p, &amount(10000000), left).unwrap();
        let target = if left { -128 } else { 128 };
        let crossed = q
            .next_pool
            .orca_whirlpool
            .as_ref()
            .unwrap()
            .tick_arrays
            .iter()
            .flat_map(|a| &a.ticks)
            .find(|t| t.tick == target)
            .unwrap();
        assert_eq!(
            if left {
                &crossed.fee_growth_outside1
            } else {
                &crossed.fee_growth_outside0
            },
            &amount(if left { 1013 - 53 } else { 1009 - 37 })
        );
        let reverse = orca::quote_exact_in(&q.next_pool, &amount(11000000), !left).unwrap();
        let crossed_again = reverse
            .next_pool
            .orca_whirlpool
            .as_ref()
            .unwrap()
            .tick_arrays
            .iter()
            .flat_map(|a| &a.ticks)
            .find(|t| t.tick == target)
            .unwrap();
        // The unchanged side of the reverse trade uses the full global from
        // the first trade; subtraction must occur at each individual cross.
        let first_global = if left {
            &q.next_pool
                .orca_whirlpool
                .as_ref()
                .unwrap()
                .fee_growth_global0
        } else {
            &q.next_pool
                .orca_whirlpool
                .as_ref()
                .unwrap()
                .fee_growth_global1
        };
        let first_outside = if left {
            &crossed.fee_growth_outside0
        } else {
            &crossed.fee_growth_outside1
        };
        assert_eq!(
            if left {
                &crossed_again.fee_growth_outside0
            } else {
                &crossed_again.fee_growth_outside1
            },
            &Amount(&first_global.0 - &first_outside.0)
        );
    }
}

#[test]
fn original_sparse_builder_selects_at_most_three_of_seven_arrays() {
    let mut p = pool();
    let state = p.orca_whirlpool.as_mut().unwrap();
    let template = state.tick_arrays[0].clone();
    let keys = [
        "EWn7dE93GeQJu72WEkEmC5MZpm5FhiJzkcJEf1xpRdWP",
        "EahQmXc3rwhY3CH1g3ZgUx8L4vHTNmzpK1xtiQ1RAxq6",
        "Eechuq54TEzmBHXX7Ltbmpu6K5Vf3qgdsRdYmn41vJ9o",
        "EiY148Y53YHzKNn2YeDX4hfrZEhriuNTRqJCqA6cfdUW",
        "EnTJCS15dqbDTU2XywYSMaScoPv4Py4GzExrtY9DQxoD",
        "ErNbLjU6E8tSbZH3REsMeTDP3Z8G52k6YedWwvBpAJ7v",
        "EvHtV2w6pSBfjeXYrYCGwKz9HiLTk6Rv74JB1JEQudSd",
    ];
    state.tick_arrays = (-3..=3)
        .zip(keys)
        .map(|(i, address)| {
            let mut a = template.clone();
            a.start_tick_index = i * 5632;
            a.address = address.into();
            a.ticks.clear();
            a.initialized_bitmap = Amount::zero();
            a
        })
        .collect();
    p.validate().unwrap();
    assert!(
        orca::quote_exact_in(&p, &amount(2000000000), false).is_err(),
        "must not silently use a fourth right array"
    );
    assert!(
        orca::quote_exact_in(&p, &amount(1000000000), true).is_err(),
        "must not silently use a fourth left array"
    );
}
