//! Standalone coverage: no protocol dispatch or domain state registration is required.
#[path = "../src/balancer_stable.rs"]
mod balancer_stable;

use balancer_stable::{quote_exact_in, StableQuote};
use num_bigint::BigUint;
use num_traits::{One, Zero};

fn n(value: &str) -> BigUint {
    value.parse().unwrap()
}

fn run(
    balances: [&str; 2],
    amp: u64,
    factors: [&str; 2],
    fee: &str,
    input: &str,
    direction: bool,
) -> xone_domain::Result<StableQuote> {
    quote_exact_in(
        [&n(balances[0]), &n(balances[1])],
        &BigUint::from(amp),
        [&n(factors[0]), &n(factors[1])],
        &n(fee),
        &n(input),
        direction,
    )
}

const ONE: &str = "1000000000000000000";
const SIX_DECIMAL_SCALE: &str = "1000000000000000000000000000000";

#[test]
fn fixed_integer_vectors_preserve_rounding_and_full_vault_input() {
    // Independently calculated from the pinned Solidity operation order with Python integers.
    // These are transcription checks, not an executed Solidity oracle.
    let cases = [
        (
            ["1000000000000000000000000"; 2],
            100000,
            [ONE; 2],
            "1000000000000000",
            "1000000000000000000000",
            true,
            "998990118890048182640",
        ),
        (
            ["2000000000000", "3000000000000000000000000"],
            35791,
            [SIX_DECIMAL_SCALE, ONE],
            "3000000000000001",
            "123456789",
            true,
            "124541088015770204592",
        ),
        (
            ["2000000000000", "3000000000000000000000000"],
            35791,
            [SIX_DECIMAL_SCALE, ONE],
            "3000000000000001",
            "123456789012345678901",
            false,
            "121648332",
        ),
        // StableMath has no WeightedMath 30% max-in/max-out check.
        (
            ["1000000"; 2],
            1000,
            [ONE; 2],
            "0",
            "400000",
            true,
            "331091",
        ),
        (
            ["1000000"; 2],
            5000000,
            [ONE; 2],
            "0",
            "400000",
            false,
            "398336",
        ),
    ];
    for (balances, amp, factors, fee, input, direction, expected) in cases {
        let quote = run(balances, amp, factors, fee, input, direction).unwrap();
        assert_eq!(quote.amount_out, n(expected));
        let i = usize::from(!direction);
        let j = 1 - i;
        assert_eq!(quote.next_balances[i], n(balances[i]) + n(input));
        assert_eq!(quote.next_balances[j], n(balances[j]) - n(expected));
    }
}

// Independent bounded u128 transcription. It shares no production helpers and uses
// only small operands. Fixed point scales are normalized exactly for standard tokens.
fn reference(
    balances: [u128; 2],
    amp: u128,
    scales: [u128; 2],
    fee: u128,
    input: u128,
    direction: bool,
) -> Option<u128> {
    fn ceil(a: u128, b: u128) -> u128 {
        a.div_ceil(b)
    }
    let i = usize::from(!direction);
    let j = 1 - i;
    let mut x = [balances[0] * scales[0], balances[1] * scales[1]];
    let sum = x[0] + x[1];
    let ann = amp * 2;
    let mut d = sum;
    let mut converged = false;
    for _ in 0..255 {
        let mut p = d;
        for balance in x {
            p = p * d / (2 * balance);
        }
        let next = (ann * sum / 1000 + 2 * p) * d / ((ann - 1000) * d / 1000 + 3 * p);
        converged = next.abs_diff(d) <= 1;
        d = next;
        if converged {
            break;
        }
    }
    if !converged {
        return None;
    }
    x[i] += (input - ceil(input * fee, 10u128.pow(18))) * scales[i];
    let p = (x[0] * 2) * x[1] * 2 / d;
    let c = ceil(d * d, ann * p) * 1000 * x[j];
    let b = x[i] + (d / ann) * 1000;
    let mut y = ceil(d * d + c, d + b);
    for _ in 0..255 {
        let next = ceil(y * y + c, 2 * y + b - d);
        if next.abs_diff(y) <= 1 {
            return x[j]
                .checked_sub(next)?
                .checked_sub(1)
                .map(|out| out / scales[j])
                .filter(|out| *out > 0);
        }
        y = next;
    }
    None
}

#[test]
fn independent_small_integer_reference_both_directions_and_decimal_scales() {
    let mut successes = 0;
    let mut rejections = 0;
    for balances in [[100_000, 100_000], [123_457, 765_431], [13, 21]] {
        for amp in [1000, 35791, 100000, 5000000] {
            for scales in [[1u128, 1u128], [10, 1], [1, 100]] {
                for fee in [0, 1, 3_000_000_000_000_001, 999_999_999_999_999_999] {
                    for input in [1, 2, 17, 40000] {
                        for direction in [true, false] {
                            let factor0 = BigUint::from(scales[0]) * n(ONE);
                            let factor1 = BigUint::from(scales[1]) * n(ONE);
                            let actual = quote_exact_in(
                                [&BigUint::from(balances[0]), &BigUint::from(balances[1])],
                                &BigUint::from(amp),
                                [&factor0, &factor1],
                                &BigUint::from(fee),
                                &BigUint::from(input),
                                direction,
                            );
                            let expected = reference(balances, amp, scales, fee, input, direction);
                            match expected {
                                Some(value) => {
                                    assert_eq!(actual.unwrap().amount_out, BigUint::from(value), "{balances:?} A={amp} scales={scales:?} fee={fee} input={input} dir={direction}");
                                    successes += 1;
                                }
                                None => {
                                    assert!(
                                        actual.is_err(),
                                        "expected rejection: {balances:?} A={amp} input={input}"
                                    );
                                    rejections += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(successes + rejections, 1152);
    assert!(successes > 100);
    assert!(rejections > 100);
}

#[test]
fn rejects_zero_empty_fee_and_parameter_boundaries() {
    let check = |balances, amp, factors, fee, input, expected| {
        assert_eq!(
            run(balances, amp, factors, fee, input, true).unwrap_err().0,
            expected
        );
    };
    check(["1000000"; 2], 100000, [ONE; 2], "0", "0", "ZERO_INPUT");
    for balances in [["0", "1000000"], ["1000000", "0"], ["0", "0"]] {
        check(
            balances,
            100000,
            [ONE; 2],
            "0",
            "100",
            "INSUFFICIENT_LIQUIDITY",
        );
    }
    for amp in [0, 999, 5000001] {
        check(
            ["1000000"; 2],
            amp,
            [ONE; 2],
            "0",
            "100",
            "BALANCER_INVALID_AMPLIFICATION",
        );
    }
    for factor in [
        "0",
        "1",
        "999999999999999999",
        "1100000000000000000",
        "10000000000000000000000000000000000000",
    ] {
        for factors in [[factor, ONE], [ONE, factor]] {
            check(
                ["1000000"; 2],
                100000,
                factors,
                "0",
                "100",
                "BALANCER_INVALID_SCALING_FACTOR",
            );
        }
    }
    check(
        ["1000000"; 2],
        100000,
        [ONE; 2],
        "1000000000000000001",
        "100",
        "BALANCER_INVALID_SWAP_FEE",
    );
    check(
        ["1000000"; 2],
        100000,
        [ONE; 2],
        ONE,
        "100",
        "INSUFFICIENT_OUTPUT",
    );
    // A fee of one fixed-point unit still consumes a full raw unit on a one-unit input.
    check(
        ["1000000"; 2],
        100000,
        [ONE; 2],
        "1",
        "1",
        "INSUFFICIENT_OUTPUT",
    );
    check(["1"; 2], 100000, [ONE; 2], "0", "1", "INSUFFICIENT_OUTPUT");
}

#[test]
fn checks_uint256_intermediates_and_vault_uint112_balances() {
    let one = n(ONE);
    let amp = BigUint::from(100000u64);
    let zero = BigUint::zero();
    let unit = BigUint::one();
    let too_large = &unit << 256usize;
    assert_eq!(
        quote_exact_in([&too_large, &unit], &amp, [&one; 2], &zero, &unit, true)
            .unwrap_err()
            .0,
        "BALANCER_INTERMEDIATE_OVERFLOW"
    );
    assert_eq!(
        quote_exact_in([&unit; 2], &amp, [&one; 2], &zero, &too_large, true)
            .unwrap_err()
            .0,
        "BALANCER_INTERMEDIATE_OVERFLOW"
    );
    let max_balance = (&unit << 112usize) - &unit;
    let large = BigUint::from(10u64).pow(24);
    for direction in [true, false] {
        assert_eq!(
            quote_exact_in([&max_balance; 2], &amp, [&one; 2], &zero, &unit, direction)
                .unwrap_err()
                .0,
            "BALANCER_RESERVE_OVERFLOW"
        );
        assert_eq!(
            quote_exact_in(
                [&(&max_balance + &unit), &large],
                &amp,
                [&one; 2],
                &zero,
                &unit,
                direction
            )
            .unwrap_err()
            .0,
            "BALANCER_RESERVE_OVERFLOW"
        );
    }
    // Scaling products fit uint256, but the invariant's later multiplications do not.
    let balance = &unit << 100usize;
    let scale = BigUint::from(10u64).pow(36);
    assert_eq!(
        quote_exact_in([&balance; 2], &amp, [&scale; 2], &zero, &unit, true)
            .unwrap_err()
            .0,
        "BALANCER_INTERMEDIATE_OVERFLOW"
    );
    // Division must not rescue a multiply that Solidity would already reject.
    assert_eq!(
        quote_exact_in(
            [&unit, &(&max_balance - &unit)],
            &amp,
            [&one; 2],
            &zero,
            &unit,
            true
        )
        .unwrap_err()
        .0,
        "BALANCER_INTERMEDIATE_OVERFLOW"
    );
}

#[test]
fn reversing_token_order_preserves_quote_and_balance_mapping() {
    let a = run(
        ["2000000", "30000000"],
        35791,
        [ONE, "10000000000000000000"],
        "3000000000000001",
        "12345",
        true,
    )
    .unwrap();
    let b = run(
        ["30000000", "2000000"],
        35791,
        ["10000000000000000000", ONE],
        "3000000000000001",
        "12345",
        false,
    )
    .unwrap();
    assert_eq!(a.amount_out, b.amount_out);
    assert_eq!(
        a.next_balances,
        [b.next_balances[1].clone(), b.next_balances[0].clone()]
    );
}

#[test]
fn invariant_nonconvergence_is_bounded_and_explicit() {
    // The pinned integer recurrence oscillates between 144133504112 and
    // 144133504129. Every arithmetic intermediate fits uint256.
    let quote = run(
        ["6167572322113", "2015"],
        5000000,
        [ONE; 2],
        "0",
        "100",
        true,
    );
    assert_eq!(quote.unwrap_err().0, "BALANCER_INVARIANT_DID_NOT_CONVERGE");
}

#[test]
fn accepts_exact_vault_limit_and_all_standard_decimal_factors() {
    let one = n(ONE);
    let amp = BigUint::from(100000u64);
    let input = BigUint::from(10000u64);
    let zero = BigUint::zero();
    let balance = BigUint::from(1000000u64);
    for decimals_difference in 0..=18 {
        let scale = &one * BigUint::from(10u32).pow(decimals_difference);
        for direction in [true, false] {
            let q =
                quote_exact_in([&balance; 2], &amp, [&scale; 2], &zero, &input, direction).unwrap();
            assert!(q.amount_out > zero);
            assert!(q.amount_out < input);
            assert_eq!(q.next_balances[usize::from(!direction)], &balance + &input);
        }
    }
    let max_balance = (BigUint::one() << 112usize) - BigUint::one();
    let balance = &max_balance - &input;
    for direction in [true, false] {
        let q = quote_exact_in([&balance; 2], &amp, [&one; 2], &zero, &input, direction).unwrap();
        assert_eq!(q.next_balances[usize::from(!direction)], max_balance);
        assert!(q.amount_out > zero);
    }
}
