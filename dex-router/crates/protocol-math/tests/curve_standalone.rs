//! Standalone Plain2Basic tests; no crate-level export or router wiring required.
#[path = "../src/curve_plain.rs"]
mod curve_plain;

use curve_plain::{quote_exact_in, CurveQuote};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::Result;

const ONE: &str = "1000000000000000000";
const ADMIN: u64 = 5_000_000_000;
const FEE: u64 = 4_000_000;

fn n(value: &str) -> BigUint {
    value.parse().unwrap()
}

fn run(
    balances: [&str; 2],
    rates: [&str; 2],
    amp: u64,
    fee: u64,
    input: &str,
    direction: bool,
) -> Result<CurveQuote> {
    quote_exact_in(
        [&n(balances[0]), &n(balances[1])],
        [&n(rates[0]), &n(rates[1])],
        &BigUint::from(amp),
        fee,
        ADMIN,
        &n(input),
        direction,
    )
}

#[test]
fn equal_precision_exchange_removes_admin_fee_from_internal_balance() {
    let q = run(
        ["1000000000000000000000000"; 2],
        [ONE; 2],
        20000,
        FEE,
        "1000000000000000000000",
        true,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("999595026885489775669"));
    assert_eq!(
        q.next_balances,
        [
            n("1001000000000000000000000"),
            n("999000204974109531134741")
        ]
    );
    assert_eq!(
        n("1000000000000000000000000") - &q.next_balances[1] - &q.amount_out,
        n("199999004979089590")
    );
}

#[test]
fn six_decimal_output_downscales_after_fee_and_admin_fee_rounding() {
    let q = run(
        ["1000000000000000000000000", "1000000000000"],
        [ONE, "1000000000000000000000000000000"],
        20000,
        FEE,
        "1000000000000000000000",
        true,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("999595026"));
    assert_eq!(
        q.next_balances,
        [n("1001000000000000000000000"), n("999000204975")]
    );
    assert_eq!(
        n("1000000000000") - &q.next_balances[1] - &q.amount_out,
        n("199999")
    );
}

#[test]
fn reverse_direction_scales_input_and_preserves_raw_balance_order() {
    let q = run(
        ["1000000000000000000000000", "1000000000000"],
        [ONE, "1000000000000000000000000000000"],
        20000,
        FEE,
        "1000000000",
        false,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("999595026885489775669"));
    assert_eq!(
        q.next_balances,
        [n("999000204974109531134741"), n("1001000000000")]
    );
}

#[test]
fn get_d_divides_by_four_after_both_xp_divisions() {
    let q = run(["3", "14"], [ONE; 2], 100, FEE, "10", true).unwrap();
    assert_eq!(q.amount_out, n("11"));
    assert_eq!(q.next_balances, [n("13"), n("3")]);
}

#[test]
fn fractional_a_precise_and_odd_normalized_fee_use_floor_rounding() {
    let q = run(
        ["123456789", "987654321"],
        [ONE; 2],
        12345,
        100_000_000,
        "1234567",
        true,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("1317078"));
    assert_eq!(q.next_balances, [n("124691356"), n("986330592")]);
    assert_eq!(
        n("987654321") - &q.next_balances[1] - &q.amount_out,
        n("6651")
    );
}

#[test]
fn tiny_positive_input_can_succeed_with_zero_output() {
    let q = run(["1000000"; 2], [ONE; 2], 100, FEE, "1", true).unwrap();
    assert!(q.amount_out.is_zero());
    assert_eq!(q.next_balances, [n("1000001"), n("1000000")]);
}

#[test]
fn dy_minus_one_underflow_is_an_error_instead_of_saturating() {
    let error = run(["1000000"; 2], [ONE; 2], 100, FEE, "0", true).unwrap_err();
    assert_eq!(error.0, "CURVE_UINT256_UNDERFLOW");
}

#[test]
fn supported_a_precise_bounds_are_scaled_and_upper_bound_is_exclusive() {
    for amp in [0, 1, 99, 100_000_000, u64::MAX] {
        assert_eq!(
            run(["1000000"; 2], [ONE; 2], amp, FEE, "1000", true)
                .unwrap_err()
                .0,
            "CURVE_INVALID_AMPLIFICATION"
        );
    }
    for amp in [100, 101, 99_999_999] {
        assert!(run(["1000000"; 2], [ONE; 2], amp, FEE, "1000", true).is_ok());
    }
}

#[test]
fn factory_fee_bounds_are_inclusive() {
    for fee in [0, FEE - 1, 100_000_001, 10_000_000_000, u64::MAX] {
        assert_eq!(
            run(["1000000"; 2], [ONE; 2], 20000, fee, "1000", true)
                .unwrap_err()
                .0,
            "CURVE_INVALID_FEE"
        );
    }
    assert!(run(["1000000"; 2], [ONE; 2], 20000, 100_000_000, "1000", true).is_ok());
}

#[test]
fn admin_fee_must_match_plain2basic_constant() {
    for admin in [0, ADMIN - 1, ADMIN + 1, 10_000_000_000, u64::MAX] {
        assert_eq!(
            quote_exact_in(
                [&n("1000000"); 2],
                [&n(ONE); 2],
                &n("20000"),
                FEE,
                admin,
                &n("1000"),
                true
            )
            .unwrap_err()
            .0,
            "CURVE_INVALID_ADMIN_FEE"
        );
    }
}

#[test]
fn zero_rates_and_zero_balances_fail_closed() {
    for direction in [true, false] {
        for index in 0..2 {
            let mut rates = [ONE; 2];
            rates[index] = "0";
            assert_eq!(
                run(["1000000"; 2], rates, 20000, FEE, "1000", direction)
                    .unwrap_err()
                    .0,
                "CURVE_INVALID_RATE"
            );
            let mut balances = ["1000000"; 2];
            balances[index] = "0";
            assert_eq!(
                run(balances, [ONE; 2], 20000, FEE, "1000", direction)
                    .unwrap_err()
                    .0,
                "CURVE_DIVISION_BY_ZERO"
            );
        }
    }
}

#[test]
fn every_biguint_argument_is_checked_as_uint256() {
    let too_large = BigUint::one() << 256usize;
    let normal = n("1000000");
    let rate = n(ONE);
    let amp = n("20000");
    let input = n("1000");
    for index in 0..6 {
        let mut args = [&normal, &normal, &rate, &rate, &amp, &input];
        args[index] = &too_large;
        assert_eq!(
            quote_exact_in(
                [args[0], args[1]],
                [args[2], args[3]],
                args[4],
                FEE,
                ADMIN,
                args[5],
                true
            )
            .unwrap_err()
            .0,
            "CURVE_UINT256_OVERFLOW"
        );
    }
}

#[test]
fn multiplication_overflow_reverts_even_if_final_quotient_fits() {
    let max = (BigUint::one() << 256usize) - BigUint::one();
    let b = n("1000000");
    let r = n(ONE);
    let amp = n("20000");
    assert_eq!(
        quote_exact_in([&max, &b], [&r; 2], &amp, FEE, ADMIN, &BigUint::one(), true)
            .unwrap_err()
            .0,
        "CURVE_UINT256_OVERFLOW"
    );
    assert_eq!(
        quote_exact_in([&b; 2], [&r; 2], &amp, FEE, ADMIN, &max, true)
            .unwrap_err()
            .0,
        "CURVE_UINT256_OVERFLOW"
    );
    // Normalizing is safe, but the first D*D intermediate is too large.
    let b = BigUint::from(10u32).pow(58);
    assert_eq!(
        quote_exact_in(
            [&b; 2],
            [&n("1"); 2],
            &amp,
            FEE,
            ADMIN,
            &BigUint::one(),
            true
        )
        .unwrap_err()
        .0,
        "CURVE_UINT256_OVERFLOW"
    );
}

#[test]
fn invariant_oscillation_stops_after_255_iterations() {
    assert_eq!(
        run(["92019805550901", "4500"], [ONE; 2], 100, FEE, "100", true)
            .unwrap_err()
            .0,
        "CURVE_D_DID_NOT_CONVERGE"
    );
}

#[test]
fn swapping_coin_order_preserves_both_output_and_post_state() {
    let forward = run(
        ["123456789", "987654321"],
        [ONE; 2],
        12345,
        FEE,
        "1234567",
        true,
    )
    .unwrap();
    let reverse = run(
        ["987654321", "123456789"],
        [ONE; 2],
        12345,
        FEE,
        "1234567",
        false,
    )
    .unwrap();
    assert_eq!(forward.amount_out, reverse.amount_out);
    assert_eq!(
        forward.next_balances,
        [
            reverse.next_balances[1].clone(),
            reverse.next_balances[0].clone()
        ]
    );
}
