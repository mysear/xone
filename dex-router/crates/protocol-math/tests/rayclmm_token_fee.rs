//! Direction-specific token fees from the pinned original program.
use num_bigint::BigUint;
use xone_domain::{rayclmm_math::sqrt_price_from_tick, Amount, Pool};
use xone_protocol_math::raydium_clmm;

fn amount(n: u64) -> Amount {
    Amount(BigUint::from(n))
}
fn pool(mode: u32) -> Pool {
    let mut p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    if mode != 0 {
        p.protocol = "raydium-clmm-token-fee".into();
        p.adapter_version = "raydium-clmm/6d7a0f82-spl-token-fee-1".into();
    }
    p.raydium_clmm.as_mut().unwrap().collect_fee_on = mode;
    p.validate().unwrap();
    p
}
fn output_pool(left: bool) -> Pool {
    pool(if left { 2 } else { 1 })
}
fn quote(p: &Pool, n: u64, left: bool) -> xone_protocol_math::Swap {
    let before = p.clone();
    let result = raydium_clmm::quote_exact_in(p, &amount(n), left).unwrap();
    assert_eq!(p, &before, "quote mutated caller");
    assert_eq!(result.consumed_in, amount(n));
    assert_eq!(result.remaining_in, Amount::zero());
    result
}
fn fee(gross: u64, rate: u32) -> u64 {
    (u128::from(gross) * u128::from(rate)).div_ceil(1_000_000) as u64
}
fn gross_quote(p: &Pool, n: u64, left: bool) -> xone_protocol_math::Swap {
    let mut free = p.clone();
    free.fee_numerator = 0;
    free.raydium_clmm.as_mut().unwrap().trade_fee_rate = 0;
    quote(&free, n, left)
}

#[test]
fn output_fee_uses_full_input_for_price_and_net_output_for_raw_transfers() {
    for left in [true, false] {
        let p = output_pool(left);
        let gross = gross_quote(&p, 100_000, left);
        let gross_out = u64::try_from(&gross.amount_out.0).unwrap();
        let q = quote(&p, 100_000, left);
        let s = q.next_pool.raydium_clmm.as_ref().unwrap();
        let before = p.raydium_clmm.as_ref().unwrap();
        let net = gross_out - fee(gross_out, before.trade_fee_rate);
        assert_eq!(q.amount_out, amount(net));
        assert_eq!(
            s.sqrt_price_x64,
            gross
                .next_pool
                .raydium_clmm
                .as_ref()
                .unwrap()
                .sqrt_price_x64
        );
        assert_eq!(
            (s.raw_vault0, s.raw_vault1),
            if left {
                (before.raw_vault0 + 100_000, before.raw_vault1 - net)
            } else {
                (before.raw_vault0 - net, before.raw_vault1 + 100_000)
            }
        );
    }
}

#[test]
fn output_fees_credit_only_the_selected_token_ledger_and_growth() {
    for left in [true, false] {
        let p = output_pool(left);
        let gross = gross_quote(&p, 100_000, left);
        let total = fee(u64::try_from(&gross.amount_out.0).unwrap(), 3000);
        let protocol = total * 120_000 / 1_000_000;
        let fund = total * 40_000 / 1_000_000;
        let growth = Amount(
            BigUint::from(u128::from(total - protocol - fund) << 64)
                / BigUint::from(1_000_000_000u64),
        );
        let q = quote(&p, 100_000, left);
        let s = q.next_pool.raydium_clmm.as_ref().unwrap();
        let (pd, fd, g, other_pd, other_fd, other_g) = if left {
            (
                s.protocol_fees1,
                s.fund_fees1,
                &s.fee_growth_global1,
                s.protocol_fees0,
                s.fund_fees0,
                &s.fee_growth_global0,
            )
        } else {
            (
                s.protocol_fees0,
                s.fund_fees0,
                &s.fee_growth_global0,
                s.protocol_fees1,
                s.fund_fees1,
                &s.fee_growth_global1,
            )
        };
        assert_eq!((pd, fd, g), (protocol, fund, &growth));
        assert_eq!((other_pd, other_fd, other_g), (0, 0, &Amount::zero()));
    }
}

#[test]
fn input_side_modes_preserve_legacy_full_post_pool() {
    for left in [true, false] {
        let legacy = quote(&pool(0), 100_000, left);
        let mut q = quote(&pool(if left { 1 } else { 2 }), 100_000, left);
        q.next_pool.protocol = legacy.next_pool.protocol.clone();
        q.next_pool.adapter_version = legacy.next_pool.adapter_version.clone();
        q.next_pool.raydium_clmm.as_mut().unwrap().collect_fee_on = 0;
        assert_eq!(q.amount_out, legacy.amount_out);
        assert_eq!(q.next_pool, legacy.next_pool);
    }
}

#[test]
fn output_fee_rounds_up_for_tiny_gross_output() {
    for left in [true, false] {
        let p = output_pool(left);
        let gross = gross_quote(&p, 3, left);
        assert_eq!(gross.amount_out, amount(2));
        assert_eq!(quote(&p, 3, left).amount_out, amount(1));
        assert!(
            raydium_clmm::quote_exact_in(&p, &amount(2), left).is_err(),
            "ceil fee leaves no net output"
        );
    }
}

#[test]
fn output_side_counter_overflows_reject_without_mutation() {
    for left in [true, false] {
        for protocol in [true, false] {
            let mut p = output_pool(left);
            let s = p.raydium_clmm.as_mut().unwrap();
            match (left, protocol) {
                (true, true) => s.protocol_fees1 = u64::MAX,
                (true, false) => s.fund_fees1 = u64::MAX,
                (false, true) => s.protocol_fees0 = u64::MAX,
                (false, false) => s.fund_fees0 = u64::MAX,
            }
            p.validate().unwrap();
            let before = p.clone();
            assert!(raydium_clmm::quote_exact_in(&p, &amount(100_000), left).is_err());
            assert_eq!(p, before);
        }
    }
}

#[test]
fn output_side_unrelated_counter_at_max_does_not_overflow() {
    for left in [true, false] {
        let mut p = output_pool(left);
        let s = p.raydium_clmm.as_mut().unwrap();
        if left {
            s.protocol_fees0 = u64::MAX;
            s.fund_fees0 = u64::MAX;
        } else {
            s.protocol_fees1 = u64::MAX;
            s.fund_fees1 = u64::MAX;
        }
        quote(&p, 100_000, left);
    }
}

#[test]
fn zero_oversized_and_incomplete_input_are_rejected() {
    for left in [true, false] {
        let p = output_pool(left);
        for input in [
            Amount::zero(),
            Amount(BigUint::from(u64::MAX) + 1u32),
            amount(10_000_000),
        ] {
            assert!(raydium_clmm::quote_exact_in(&p, &input, left).is_err());
        }
    }
}

#[test]
fn output_partial_step_charges_all_input_despite_price_rounding_dust() {
    for left in [true, false] {
        let mut p = output_pool(left);
        p.raydium_clmm.as_mut().unwrap().liquidity = Amount(BigUint::from(1u128 << 70));
        let q = quote(&p, 100_000, left);
        let s = q.next_pool.raydium_clmm.as_ref().unwrap();
        let before = p.raydium_clmm.as_ref().unwrap();
        assert_eq!(
            if left {
                s.raw_vault0 - before.raw_vault0
            } else {
                s.raw_vault1 - before.raw_vault1
            },
            100_000
        );
        assert_eq!(q.amount_out, amount(if left { 99_668 } else { 99_667 })); // ceil fee 300
    }
}

#[test]
fn crossed_tick_outside_growth_follows_fee_token_and_fees_floor_per_step() {
    for left in [true, false] {
        let mut p = output_pool(left);
        let s = p.raydium_clmm.as_mut().unwrap();
        let a = if left { 0 } else { 1 };
        let mut further = s.tick_arrays[a].ticks[0].clone();
        further.tick = if left { -256 } else { 256 };
        further.liquidity_net = "0".into();
        s.tick_arrays[a].ticks[0].liquidity_net = "0".into();
        if left {
            s.tick_arrays[a].ticks.insert(0, further);
        } else {
            s.tick_arrays[a].ticks.push(further);
        }
        s.fee_growth_global0 = amount(17);
        s.fee_growth_global1 = amount(31);
        p.validate().unwrap();
        // Fee-free quotes reaching each boundary independently give the gross deltas.
        let target =
            u128::try_from(sqrt_price_from_tick(if left { -128 } else { 128 }).unwrap()).unwrap();
        let q64 = 1u128 << 64;
        let first_in = if left {
            (((BigUint::from(1_000_000_000u64) * BigUint::from(q64 - target)) << 64usize)
                + (BigUint::from(q64) * BigUint::from(target))
                - 1u32)
                / (BigUint::from(q64) * BigUint::from(target))
        } else {
            BigUint::from(1_000_000_000u64) * BigUint::from(target - q64) / BigUint::from(q64)
                + 1u32
        };
        let first_in = u64::try_from(first_in).unwrap();
        let first_gross = u64::try_from(gross_quote(&p, first_in, left).amount_out.0).unwrap();
        let gross = u64::try_from(gross_quote(&p, 7_000_000, left).amount_out.0).unwrap();
        let fees = [fee(first_gross, 3000), fee(gross - first_gross, 3000)];
        let pd: u64 = fees.iter().map(|v| v * 120_000 / 1_000_000).sum();
        let fd: u64 = fees.iter().map(|v| v * 40_000 / 1_000_000).sum();
        let q = quote(&p, 7_000_000, left);
        assert_eq!(q.amount_out, amount(gross - fees.iter().sum::<u64>()));
        let s = q.next_pool.raydium_clmm.as_ref().unwrap();
        let crossed = s.tick_arrays[a]
            .ticks
            .iter()
            .find(|t| t.tick == if left { -128 } else { 128 })
            .unwrap();
        let first_lp = fees[0] - fees[0] * 120_000 / 1_000_000 - fees[0] * 40_000 / 1_000_000;
        let growth = (u128::from(first_lp) << 64) / 1_000_000_000;
        if left {
            assert_eq!((s.protocol_fees1, s.fund_fees1), (pd, fd));
            assert_eq!(crossed.fee_growth_outside0, amount(17));
            assert_eq!(
                crossed.fee_growth_outside1,
                Amount(BigUint::from(31 + growth))
            );
        } else {
            assert_eq!((s.protocol_fees0, s.fund_fees0), (pd, fd));
            assert_eq!(
                crossed.fee_growth_outside0,
                Amount(BigUint::from(17 + growth))
            );
            assert_eq!(crossed.fee_growth_outside1, amount(31));
        }
    }
}

#[test]
fn public_legacy_step_signature_preserves_original_input_fee_vectors() {
    let q64 = 1u128 << 64;
    let s = raydium_clmm::step_exact_in(1000, 20_000, 1296, 9 * q64, 4 * q64, true).unwrap();
    assert_eq!(
        (s.amount_in, s.amount_out, s.next_price, s.fee_amount),
        (180, 6480, 4 * q64, 4)
    );
}
