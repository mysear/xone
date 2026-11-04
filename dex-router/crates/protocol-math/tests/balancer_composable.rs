#[path = "../src/balancer_composable.rs"]
mod balancer_composable;
use balancer_composable::{quote_bpt_exact_in_after_fees, quote_exact_in};
use num_bigint::BigUint;
fn n(v: &str) -> BigUint {
    v.parse().unwrap()
}
fn e() -> BigUint {
    n("1000000000000000000")
}
#[test]
fn regular_swap_drops_bpt_and_preserves_registered_order() {
    let b = vec![
        n("1000000000000000000000000"),
        n("2596148429267413814265248164610048"),
        n("1000000000000"),
        n("1000000000000000000000000"),
    ];
    let s = vec![e(), e(), n("1000000000000000000000000000000"), e()];
    let q = quote_exact_in(
        &b,
        &n("100000"),
        &s,
        &n("1000000000000000"),
        &n("1000000000000000000000"),
        0,
        2,
        1,
    )
    .unwrap();
    assert!(q.amount_out > n("990000000"));
    assert!(q.amount_out < n("1010000000"));
    assert_eq!(q.next_balances[1], b[1]);
    assert_eq!(q.next_balances[3], b[3]);
    assert_eq!(q.next_balances[0], &b[0] + n("1000000000000000000000"));
    assert!(quote_exact_in(&b, &n("100000"), &s, &n("1000000000000000"), &e(), 1, 2, 1).is_err());
}
#[test]
fn bpt_math_requires_explicit_post_protocol_fee_supply() {
    let b = vec![
        n("1000000000000000000000000"),
        n("2000000000000000000000000"),
        n("3000000000000000000000000"),
    ];
    let s = vec![e(); 3];
    let supply = n("6000000000000000000000000");
    let join = quote_bpt_exact_in_after_fees(
        &b,
        &n("100000"),
        &s,
        &n("1000000000000000"),
        &e(),
        &supply,
        0,
        true,
    )
    .unwrap();
    assert!(join.amount_out > n("990000000000000000"));
    assert_eq!(join.next_actual_supply, &supply + &join.amount_out);
    assert_eq!(join.next_underlying_balances[0], &b[0] + e());
    let exit = quote_bpt_exact_in_after_fees(
        &b,
        &n("100000"),
        &s,
        &n("1000000000000000"),
        &e(),
        &supply,
        2,
        false,
    )
    .unwrap();
    assert!(exit.amount_out > n("990000000000000000"));
    assert_eq!(exit.next_actual_supply, &supply - e());
    assert!(quote_bpt_exact_in_after_fees(
        &b,
        &n("100000"),
        &s,
        &n("1000000000000000"),
        &supply,
        &supply,
        2,
        false
    )
    .is_err());
}
#[test]
fn composable_guards_check_indices_rates_amp_and_integer_limits() {
    let balance = n("1000000000000000000000000");
    let b = vec![
        balance.clone(),
        n("2596148429267413814265248164610048"),
        balance,
    ];
    let scales = vec![e(); 3];
    let run = |b: &[BigUint], a: BigUint, s: &[BigUint], f: BigUint, input: BigUint, i, j, k| {
        quote_exact_in(b, &a, s, &f, &input, i, j, k)
    };
    assert!(run(&b, n("999"), &scales, n("0"), e(), 0, 2, 1).is_err());
    assert!(run(&b, n("5000001"), &scales, n("0"), e(), 0, 2, 1).is_err());
    assert!(run(&b, n("100000"), &scales, e() + 1u8, e(), 0, 2, 1).is_err());
    assert!(run(&b, n("100000"), &scales, n("0"), n("0"), 0, 2, 1).is_err());
    for (i, j, k) in [
        (0, 0, 1),
        (0, 2, 3),
        (0, 3, 1),
        (usize::MAX, 2, 1),
        (0, 1, 1),
    ] {
        assert!(run(&b, n("100000"), &scales, n("0"), e(), i, j, k).is_err());
    }
    assert!(run(&b, n("100000"), &scales[..2], n("0"), e(), 0, 2, 1).is_err());
    let mut bad = scales.clone();
    bad[2] = n("0");
    assert!(run(&b, n("100000"), &bad, n("0"), e(), 0, 2, 1).is_err());
    bad[2] = BigUint::from(1u8) << 256usize;
    assert!(run(&b, n("100000"), &bad, n("0"), e(), 0, 2, 1).is_err());
    bad[2] = BigUint::from(1u8) << 255usize;
    assert!(run(&b, n("100000"), &bad, n("0"), e(), 0, 2, 1).is_err());
    let mut bad = b.clone();
    bad[0] = BigUint::from(1u8) << 112usize;
    assert!(run(&bad, n("100000"), &scales, n("0"), e(), 0, 2, 1).is_err());
    bad[0] = (BigUint::from(1u8) << 112usize) - 1u8;
    assert!(run(&bad, n("100000"), &scales, n("0"), e(), 0, 2, 1).is_err());
    let raw = &b[..1];
    let s = &scales[..1];
    assert!(
        quote_bpt_exact_in_after_fees(raw, &n("100000"), s, &n("0"), &e(), &e(), 0, true).is_err()
    );
    assert!(quote_bpt_exact_in_after_fees(
        &b[..2],
        &n("100000"),
        &scales[..2],
        &n("0"),
        &e(),
        &n("0"),
        0,
        true
    )
    .is_err());
    assert!(quote_bpt_exact_in_after_fees(
        &b[..2],
        &n("100000"),
        &scales[..2],
        &n("0"),
        &e(),
        &(BigUint::from(1u8) << 256usize),
        0,
        true
    )
    .is_err());
}
