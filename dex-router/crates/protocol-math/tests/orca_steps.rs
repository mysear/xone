//! Numerical vectors are copied from the pinned original math/swap_math.rs tests.
pub use xone_protocol_math::orca_adaptive_online;
#[allow(dead_code)]
#[path = "../src/orca.rs"]
mod orca;
pub use xone_protocol_math::Swap;

#[test]
fn original_exact_in_step_vectors() {
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
        let s = orca::compute_swap(amount, fee, 1296, 9 * q, target, left).unwrap();
        assert_eq!(
            (s.amount_in, s.amount_out, s.next_price, s.fee_amount),
            expected
        );
    }
}

#[test]
fn empty_liquidity_moves_to_target_without_token_or_fee() {
    let q = 1u128 << 64;
    for (left, target) in [(true, 4 * q), (false, 16 * q)] {
        let s = orca::compute_swap(100, 20000, 0, 9 * q, target, left).unwrap();
        assert_eq!(
            (s.amount_in, s.amount_out, s.next_price, s.fee_amount),
            (0, 0, target, 0)
        );
    }
}

#[test]
fn delta_b_u128_overflow_is_unreachable_target_not_fatal_initially() {
    // Liquidity * full-range price difference overflows u128, but the actual
    // tiny move and output remain valid. Initial delta must use ExceedsMax.
    let s = orca::compute_swap(
        100000,
        3000,
        1u128 << 70,
        1u128 << 64,
        79226673515401279992447579055,
        false,
    )
    .unwrap();
    assert_eq!(s.amount_in + s.fee_amount, 100000);
    assert!(s.amount_out > 0);
}

#[test]
fn delta_a_checked_shift_and_next_a_numerator_are_hard_errors() {
    assert!(orca::compute_swap(
        100,
        3000,
        u128::MAX,
        79226673515401279992447579055,
        4295048016,
        true
    )
    .is_err());
}

#[test]
fn delta_a_u64_excess_can_use_a_small_valid_partial_step() {
    let s = orca::compute_swap(100000, 3000, 1u128 << 70, 1u128 << 64, 4295048016, true).unwrap();
    assert_eq!(s.amount_in + s.fee_amount, 100000);
    assert!(s.amount_out > 0);
    assert!(s.next_price > 4295048016);
}

#[test]
fn next_a_price_numerator_overflow_has_no_v3_fallback() {
    let price = 79226673515401279992447579055;
    // L * (price-target) << 64 fits, while L * price << 64 does not.
    assert!(orca::compute_swap(100, 3000, u128::MAX, price, price - 1000, true).is_err());
}
