#[allow(dead_code)]
#[path = "../src/raydium_clmm.rs"]
mod raydium_clmm;
use num_bigint::BigUint;
use xone_domain::{rayclmm_math::sqrt_price_from_tick, Amount, Pool};
pub use xone_protocol_math::Swap;
fn pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap()
}
fn amount(n: u64) -> Amount {
    Amount(BigUint::from(n))
}
#[test]
fn full_input_updates_raw_fee_ledgers_and_pool_epoch_without_mutation() {
    for left in [true, false] {
        let p = pool();
        p.validate().unwrap();
        let before = p.raydium_clmm.as_ref().unwrap();
        let q = raydium_clmm::quote_exact_in(&p, &amount(100000), left).unwrap();
        let s = q.next_pool.raydium_clmm.as_ref().unwrap();
        let out = u64::try_from(&q.amount_out.0).unwrap();
        assert_eq!(q.consumed_in, amount(100000));
        assert_eq!(q.remaining_in, Amount::zero());
        assert!(out > 0);
        assert_eq!(
            (s.raw_vault0, s.raw_vault1),
            if left {
                (before.raw_vault0 + 100000, before.raw_vault1 - out)
            } else {
                (before.raw_vault0 - out, before.raw_vault1 + 100000)
            }
        );
        assert_eq!(s.recent_epoch, s.bank_epoch);
        assert_eq!(s.observation_recent_epoch, before.observation_recent_epoch);
        assert_eq!(s.tick_arrays, before.tick_arrays);
        let (protocol, fund, growth) = if left {
            (s.protocol_fees0, s.fund_fees0, &s.fee_growth_global0)
        } else {
            (s.protocol_fees1, s.fund_fees1, &s.fee_growth_global1)
        };
        assert_eq!((protocol, fund), (36, 12));
        assert_eq!(
            growth.0,
            (BigUint::from(252u32) << 64usize) / BigUint::from(1000000000u64)
        );
        assert_eq!(p, pool());
        q.next_pool.validate().unwrap();
    }
}
#[test]
fn empty_current_array_uses_first_initialized_bitmap_array() {
    let mut p = pool();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.tick_arrays[1].ticks.clear();
    s.bitmap[8] &= !1;
    let q = raydium_clmm::quote_exact_in(&p, &amount(100000), true).unwrap();
    assert!(q.amount_out > Amount::zero());
    // Right scan has no initialized array in this direction, so no fabricated
    // empty target or unproved liquidity may be used.
    assert!(raydium_clmm::quote_exact_in(&p, &amount(100000), false).is_err());
}
#[test]
fn bitmap_outside_proof_is_allowed_until_needed() {
    let mut p = pool();
    p.raydium_clmm.as_mut().unwrap().bitmap[8] |= 1 << 2;
    p.validate().unwrap();
    assert!(raydium_clmm::quote_exact_in(&p, &amount(100000), false).is_ok());
    assert!(raydium_clmm::quote_exact_in(&p, &amount(10000000), false).is_err());
    let s = p.raydium_clmm.as_mut().unwrap();
    s.tick_arrays[1].ticks.clear();
    s.bitmap[8] &= !1;
    assert!(raydium_clmm::quote_exact_in(&p, &amount(100000), false).is_err());
}
#[test]
fn crossing_updates_only_crossed_array_epoch_and_wrapping_outside_growth() {
    let mut p = pool();
    let s = p.raydium_clmm.as_mut().unwrap();
    let mut further = s.tick_arrays[0].ticks[0].clone();
    further.tick = -256;
    further.liquidity_net = "0".into();
    s.tick_arrays[0].ticks[0].liquidity_net = "0".into();
    s.tick_arrays[0].ticks[0].fee_growth_outside1 = amount(5);
    s.tick_arrays[0].ticks.insert(0, further);
    s.fee_growth_global0 = Amount(BigUint::from(u128::MAX - 100));
    s.fee_growth_global1 = amount(3);
    let q = raydium_clmm::quote_exact_in(&p, &amount(7000000), true).unwrap();
    let n = q.next_pool.raydium_clmm.as_ref().unwrap();
    assert!(n.tick_current < -128 && n.tick_current > -256);
    assert_eq!(n.tick_arrays[0].recent_epoch, n.bank_epoch);
    assert_eq!(n.tick_arrays[1].recent_epoch, 4);
    assert_eq!(n.tick_arrays[2].recent_epoch, 4);
    assert_eq!(
        n.tick_arrays[0].ticks[1].fee_growth_outside1.0,
        BigUint::from(u128::MAX - 1)
    );
    assert!(n.fee_growth_global0.0 < BigUint::from(u128::MAX - 100));
    assert_eq!(n.liquidity.0, BigUint::from(1000000000u64));
}
#[test]
fn observation_threshold_old_tick_wrapping_and_ring_rollover() {
    for delta in [0u32, 14, 15, 30] {
        let mut p = pool();
        let s = p.raydium_clmm.as_mut().unwrap();
        s.tick_current = 64;
        s.sqrt_price_x64 = Amount(sqrt_price_from_tick(64).unwrap());
        s.observation_initialized = true;
        s.observation_index = 99;
        s.observations[99].block_timestamp = s.bank_unix_timestamp as u32 - delta;
        s.observations[99].tick_cumulative = i64::MAX - 10;
        let q = raydium_clmm::quote_exact_in(&p, &amount(100000), true).unwrap();
        let n = q.next_pool.raydium_clmm.unwrap();
        assert_eq!(n.observation_index, if delta >= 15 { 0 } else { 99 });
        assert_eq!(n.observation_recent_epoch, 4);
        if delta >= 15 {
            assert_eq!(
                n.observations[0].block_timestamp,
                n.bank_unix_timestamp as u32
            );
            assert_eq!(
                n.observations[0].tick_cumulative,
                (i64::MAX - 10).wrapping_add(64 * i64::from(delta))
            );
        } else {
            assert_eq!(n.observations, p.raydium_clmm.unwrap().observations);
        }
    }
}
#[test]
fn same_tick_keeps_uninitialized_observation_unchanged() {
    let p = pool();
    let q = raydium_clmm::quote_exact_in(&p, &amount(100), false).unwrap();
    let n = q.next_pool.raydium_clmm.unwrap();
    assert_eq!(n.tick_current, 0);
    assert!(!n.observation_initialized);
    assert_eq!(n.observations, p.raydium_clmm.unwrap().observations);
}
#[test]
fn budgets_insufficient_proof_width_open_time_and_checked_transfers_reject() {
    let p = pool();
    assert!(raydium_clmm::quote_exact_in(&p, &Amount::zero(), true).is_err());
    assert!(
        raydium_clmm::quote_exact_in(&p, &Amount(BigUint::from(u64::MAX) + 1u8), true).is_err()
    );
    assert!(raydium_clmm::quote_exact_in(&p, &amount(1), true).is_err());
    assert!(raydium_clmm::quote_exact_in(&p, &amount(10000000), true).is_err());
    for (raw0, raw1) in [(u64::MAX, 1000000000000), (1000000000000, 1)] {
        let mut bad = p.clone();
        let s = bad.raydium_clmm.as_mut().unwrap();
        s.raw_vault0 = raw0;
        s.raw_vault1 = raw1;
        assert!(raydium_clmm::quote_exact_in(&bad, &amount(100000), true).is_err());
    }
    let mut bad = p.clone();
    let s = bad.raydium_clmm.as_mut().unwrap();
    s.open_time = s.bank_unix_timestamp;
    assert!(raydium_clmm::quote_exact_in(&bad, &amount(100000), true).is_err());
    let mut bad = p.clone();
    bad.raydium_clmm.as_mut().unwrap().protocol_fees0 = u64::MAX;
    assert!(raydium_clmm::quote_exact_in(&bad, &amount(100000), true).is_err());
    let mut bad = p;
    bad.raydium_clmm.as_mut().unwrap().fund_fees1 = u64::MAX;
    assert!(raydium_clmm::quote_exact_in(&bad, &amount(100000), false).is_err());
}
#[test]
fn fees_floor_per_step_before_summing() {
    let mut p = pool();
    let s = p.raydium_clmm.as_mut().unwrap();
    let mut lower = s.tick_arrays[0].ticks[0].clone();
    lower.tick = -256;
    lower.liquidity_net = "0".into();
    s.tick_arrays[0].ticks[0].liquidity_net = "0".into();
    s.tick_arrays[0].ticks.insert(0, lower);
    let price = 1u128 << 64;
    let target = u128::try_from(sqrt_price_from_tick(-128).unwrap()).unwrap();
    let first = raydium_clmm::compute_swap(7000000, 3000, 1000000000, price, target, true).unwrap();
    let second = raydium_clmm::compute_swap(
        7000000 - first.amount_in - first.fee_amount,
        3000,
        1000000000,
        target,
        u128::try_from(sqrt_price_from_tick(-256).unwrap()).unwrap(),
        true,
    )
    .unwrap();
    let expected_protocol =
        first.fee_amount * 120000 / 1000000 + second.fee_amount * 120000 / 1000000;
    let expected_fund = first.fee_amount * 40000 / 1000000 + second.fee_amount * 40000 / 1000000;
    let q = raydium_clmm::quote_exact_in(&p, &amount(7000000), true).unwrap();
    let n = q.next_pool.raydium_clmm.unwrap();
    assert_eq!(
        (n.protocol_fees0, n.fund_fees0),
        (expected_protocol, expected_fund)
    );
    assert_ne!(
        (first.fee_amount + second.fee_amount) * 40000 / 1000000,
        expected_fund
    );
    let mut limited = p;
    limited.raydium_clmm.as_mut().unwrap().max_steps = 1;
    assert!(raydium_clmm::quote_exact_in(&limited, &amount(7000000), true).is_err());
}
#[test]
fn zero_liquidity_crosses_into_liquidity_without_spending_input() {
    let mut p = pool();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.liquidity = Amount::zero();
    s.tick_arrays[0].ticks[0].liquidity_net = "-1000000000".into();
    let mut further = s.tick_arrays[0].ticks[0].clone();
    further.tick = -256;
    further.liquidity_net = "0".into();
    s.tick_arrays[0].ticks.insert(0, further);
    let q = raydium_clmm::quote_exact_in(&p, &amount(100000), true).unwrap();
    let n = q.next_pool.raydium_clmm.unwrap();
    assert!(q.amount_out > Amount::zero());
    assert!(n.tick_current < -128);
    assert_eq!(
        n.tick_arrays[0].ticks[1].fee_growth_outside0,
        Amount::zero()
    );
    assert_eq!(n.liquidity, amount(1000000000));
    assert_eq!(n.tick_arrays[0].recent_epoch, n.bank_epoch);
}
#[test]
fn saturating_observation_time_and_clock_u32_limit() {
    let mut p = pool();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.bank_unix_timestamp = u64::from(u32::MAX);
    s.observation_initialized = true;
    s.observations[0].block_timestamp = u32::MAX;
    let q = raydium_clmm::quote_exact_in(&p, &amount(100000), true).unwrap();
    let n = q.next_pool.raydium_clmm.unwrap();
    assert_eq!(n.observation_index, 0);
    assert_eq!(
        n.observations,
        p.raydium_clmm.as_ref().unwrap().observations
    );
    p.raydium_clmm.as_mut().unwrap().bank_unix_timestamp += 1;
    assert!(raydium_clmm::quote_exact_in(&p, &amount(100000), true).is_err());
}
#[test]
fn current_bitmap_index_and_next_selection_use_all_sixteen_words() {
    for (index, left) in [(-511, true), (510, false)] {
        let mut p = pool();
        let s = p.raydium_clmm.as_mut().unwrap();
        s.tick_spacing = 1;
        s.bitmap.fill(0);
        s.tick_current = index * 60 + 30;
        s.sqrt_price_x64 = Amount(sqrt_price_from_tick(s.tick_current).unwrap());
        let middle = s.tick_arrays[1].ticks[0].clone();
        for (i, a) in s.tick_arrays.iter_mut().enumerate() {
            a.start_tick_index = (index + i as i32 - 1) * 60;
            a.address = test_array_pda(&p.pool_id, a.start_tick_index);
            a.ticks.clear();
            if i == if left { 0 } else { 2 } {
                let mut t = middle.clone();
                t.tick = a.start_tick_index + 30;
                t.liquidity_net = "0".into();
                a.ticks.push(t);
                let n = (a.start_tick_index / 60 + 512) as usize;
                s.bitmap[n / 64] |= 1u64 << (n % 64);
            }
        }
        p.validate().unwrap();
        let q = raydium_clmm::quote_exact_in(&p, &amount(100000), left).unwrap();
        assert!(q.amount_out > Amount::zero());
    }
}
#[test]
fn original_checked_release_rejects_negating_i128_min_on_left_crossing() {
    let mut p = pool();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.tick_current = -128;
    s.sqrt_price_x64 = Amount(sqrt_price_from_tick(-128).unwrap());
    s.liquidity = Amount(BigUint::from((1u128 << 127) + 1000000000));
    s.tick_arrays[0].ticks[0].liquidity_net = i128::MIN.to_string();
    s.tick_arrays[0].ticks[0].liquidity_gross = Amount(BigUint::from(1u128 << 127));
    let mut further = s.tick_arrays[0].ticks[0].clone();
    further.tick = -256;
    further.liquidity_net = "0".into();
    s.tick_arrays[0].ticks.insert(0, further);
    p.validate().unwrap();
    // Upstream profile.release has overflow-checks=true; liquidity_net.neg()
    // cannot silently wrap at i128::MIN.
    assert!(raydium_clmm::quote_exact_in(&p, &amount(100000), true).is_err());
}

// Numeric bitmap probes still carry real Solana account identities.
fn test_array_pda(pool: &str, start: i32) -> String {
    use sha2::{Digest, Sha256};
    let pool = bs58::decode(pool).into_vec().unwrap();
    let program = bs58::decode(xone_domain::raydium_clmm::PROGRAM_ID)
        .into_vec()
        .unwrap();
    for bump in (0..=255u8).rev() {
        let mut h = Sha256::new();
        h.update(b"tick_array");
        h.update(&pool);
        h.update(start.to_be_bytes());
        h.update([bump]);
        h.update(&program);
        h.update(b"ProgramDerivedAddress");
        let raw: [u8; 32] = h.finalize().into();
        if curve25519_dalek::edwards::CompressedEdwardsY(raw)
            .decompress()
            .is_none()
        {
            return bs58::encode(raw).into_string();
        }
    }
    panic!("PDA missing")
}
