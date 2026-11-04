use num_bigint::BigUint;
use xone_protocol_math::balancer_weighted::quote_exact_in;
fn n(v: &str) -> BigUint {
    v.parse().unwrap()
}
#[test]
fn weighted_rounds_as_native_fixed_point_not_v2_fraction() {
    let e = n("1000000000000000000");
    let b = [e.clone(), n("1000000000000000000000000")];
    let s = [e.clone(), e.clone()];
    assert!(quote_exact_in([&b[0], &b[1]], [&s[0], &s[1]], &n("0"), &n("1"), true).is_err());
    let q = quote_exact_in([&b[0], &b[1]], [&s[0], &s[1]], &n("0"), &n("2"), true).unwrap();
    assert_eq!(q.amount_out, n("1000000"));
    assert_eq!(q.next_balances, [&b[0] + n("2"), &b[1] - n("1000000")]);
}
#[test]
fn weighted_fee_cap_scaling_and_overflow_boundaries() {
    let e = n("1000000000000000000");
    let b = [n("1000000000000000000000000"), n("1000000000000")];
    let s = [e.clone(), n("1000000000000000000000000000000")];
    let quote = |input: &BigUint, fee: &BigUint| {
        quote_exact_in([&b[0], &b[1]], [&s[0], &s[1]], fee, input, true)
    };
    assert!(quote(&n("300000000000000000000000"), &n("0")).is_ok());
    assert!(quote(&n("300000000000000000000001"), &n("0")).is_err());
    assert!(quote(&n("300000000000000000000001"), &n("1000000000000000")).is_ok());
    assert!(quote(&n("0"), &n("0")).is_err());
    assert!(quote(&e, &e).is_err());
    let max = (BigUint::from(1u8) << 112usize) - BigUint::from(1u8);
    assert!(quote_exact_in([&max, &max], [&e, &e], &n("0"), &e, true).is_err());
    let huge_scale = n("1000000000000000000000000000000000000");
    assert!(quote_exact_in([&max, &max], [&huge_scale, &huge_scale], &n("0"), &e, true).is_err());
}
