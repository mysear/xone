use num_bigint::BigUint;
use xone_protocol_math::aerodrome;
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
#[test]
fn volatile_fee_rounding_and_fee_removed_from_reserve() {
    let q = aerodrome::quote_exact_in(
        [&n("1000"), &n("1000")],
        [&n("1000000000000000000"), &n("1000000000000000000")],
        30,
        &n("10"),
        true,
        false,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("9"));
    assert_eq!(q.next_balances[0], n("1010"));
    let q = aerodrome::quote_exact_in(
        [&n("1000000"), &n("1000000")],
        [&n("1000000000000000000"), &n("1000000000000000000")],
        30,
        &n("10000"),
        true,
        false,
    )
    .unwrap();
    assert_eq!(q.amount_out, n("9871"));
    assert_eq!(q.next_balances[0], n("1009970"));
}
#[test]
fn stable_decimal_normalization_and_invariant() {
    for zero in [true, false] {
        let q = aerodrome::quote_exact_in(
            [&n("1000000000000000000000000"), &n("1000000000000")],
            [&n("1000000000000000000"), &n("1000000")],
            4,
            &n(if zero {
                "1000000000000000000"
            } else {
                "1000000"
            }),
            zero,
            true,
        )
        .unwrap();
        assert!(q.amount_out > n("0"));
    }
}
