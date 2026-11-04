use num_bigint::BigUint;
use xone_protocol_math::balancer_weighted::{quote_exact_in, quote_exact_in_weighted};
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
#[test]
fn immutable_two_token_weights_are_not_assumed_equal() {
    let e = n("1000000000000000000");
    let b = [n("1000000000000000000000000"), n("1000000000000")];
    let s = [e.clone(), n("1000000000000000000000000000000")];
    let fee = n("1000000000000000");
    let input = n("1000000000000000000");
    let half = n("500000000000000000");
    let equal = quote_exact_in([&b[0], &b[1]], [&s[0], &s[1]], &fee, &input, true).unwrap();
    assert_eq!(
        quote_exact_in_weighted(
            [&b[0], &b[1]],
            [&s[0], &s[1]],
            [&half, &half],
            &fee,
            &input,
            true
        )
        .unwrap(),
        equal
    );
    let heavy = n("800000000000000000");
    let light = &e - &heavy;
    let q = quote_exact_in_weighted(
        [&b[0], &b[1]],
        [&s[0], &s[1]],
        [&heavy, &light],
        &fee,
        &input,
        true,
    )
    .unwrap();
    assert!(q.amount_out > equal.amount_out);
    assert_eq!(q.next_balances[0], &b[0] + &input);
    for bad in [
        n("0"),
        n("9999999999999999"),
        e.clone(),
        (BigUint::from(1u8) << 256usize),
    ] {
        assert!(quote_exact_in_weighted(
            [&b[0], &b[1]],
            [&s[0], &s[1]],
            [&bad, &half],
            &fee,
            &input,
            true
        )
        .is_err());
    }
}

#[test]
fn exact_in_can_withdraw_more_than_thirty_percent_for_asymmetric_weights() {
    let b = [
        n("1000000000000000000000000"),
        n("1000000000000000000000000"),
    ];
    let e = n("1000000000000000000");
    let w = [n("990000000000000000"), n("10000000000000000")];
    let fee = n("1000000000000000");
    let input = n("300000000000000000000000");
    let q = quote_exact_in_weighted([&b[0], &b[1]], [&e, &e], [&w[0], &w[1]], &fee, &input, true)
        .unwrap();
    assert!(q.amount_out * 10u8 > &b[1] * 3u8);
}
