//! Original pinned compute_swap semantics, including its wide amount0 fallback.
#[allow(dead_code)]
#[path = "../src/raydium_clmm.rs"]
mod raydium_clmm;
pub use xone_protocol_math::Swap;
#[test]
fn original_q64_input_side_fee_steps() {
    let q = 1u128 << 64;
    for (amount, fee, left, target, expected) in [
        (1000, 20000, true, 4 * q, (180, 6480, 4 * q, 4)),
        (1000, 10000, true, 4 * q, (180, 6480, 4 * q, 2)),
        (
            2000,
            20000,
            false,
            16 * q,
            (1960, 20, 193918550355107200012, 40),
        ),
        (20000, 20000, false, 16 * q, (9072, 63, 16 * q, 186)),
    ] {
        let s = raydium_clmm::compute_swap(amount, fee, 1296, 9 * q, target, left).unwrap();
        assert_eq!(
            (s.amount_in, s.amount_out, s.next_price, s.fee_amount),
            expected
        );
    }
}
#[test]
fn zero_liquidity_reaches_target_with_zero_amounts() {
    for (left, target) in [(true, 4u128 << 64), (false, 16u128 << 64)] {
        let s = raydium_clmm::compute_swap(100, 20000, 0, 9u128 << 64, target, left).unwrap();
        assert_eq!(
            (s.amount_in, s.amount_out, s.next_price, s.fee_amount),
            (0, 0, target, 0)
        );
    }
}
#[test]
fn widened_u256_amount_zero_numerator_and_full_math_fallback_work() {
    // Raydium's full_math widens to U512 instead of rejecting L*delta<<64.
    // An Orca implementation rejects these original Raydium-valid steps.
    for left in [true, false] {
        let price = if left { 2u128 << 64 } else { 1u128 << 64 };
        let target = if left {
            4295048017
        } else {
            79226673521066979257578248090
        };
        let s = raydium_clmm::compute_swap(100000, 3000, u128::MAX, price, target, left).unwrap();
        assert_eq!(s.amount_in.checked_add(s.fee_amount).unwrap(), 100000);
        assert!(if left {
            s.next_price <= price
        } else {
            s.next_price >= price
        });
    }
}
#[test]
fn both_target_amounts_must_fit_u64_before_reaching_target() {
    let q = 1u128 << 64;
    for left in [true, false] {
        let target = if left {
            4295048017
        } else {
            79226673521066979257578248090
        };
        let s = raydium_clmm::compute_swap(100000, 3000, 1u128 << 70, q, target, left).unwrap();
        assert_eq!(s.amount_in + s.fee_amount, 100000);
        assert!(s.amount_out > 0);
        assert_ne!(s.next_price, target);
    }
}
#[test]
fn partial_fee_can_consume_input_without_moving_price() {
    let s = raydium_clmm::compute_swap(1, 1, u128::MAX, 1u128 << 64, 4295048017, true).unwrap();
    assert_eq!(
        (s.amount_in, s.amount_out, s.next_price, s.fee_amount),
        (0, 0, 1u128 << 64, 1)
    );
}
