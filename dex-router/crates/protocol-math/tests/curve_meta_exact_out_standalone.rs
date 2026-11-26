#[allow(dead_code)]
#[path = "../src/curve_multi.rs"]
mod curve_multi;

use num_bigint::BigUint;

fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}

#[test]
fn meta_exact_out_replays_integer_poststate_and_admin_fee() {
    let balances = vec![n("1000000000000000000000000"); 2];
    let input = n("10000000000000000000000");
    let exact_in = curve_multi::quote_meta_exact_in(
        &balances,
        &n("10000"),
        &n("1000000000000000000"),
        &n("1000000000000000000"),
        &n("4000000"),
        &n("5000000000"),
        &input,
        0,
        1,
    )
    .unwrap();
    let exact_out = curve_multi::quote_meta_exact_out(
        &balances,
        &n("10000"),
        &n("1000000000000000000"),
        &n("1000000000000000000"),
        &n("4000000"),
        &n("5000000000"),
        &exact_in.amount_out,
        0,
        1,
    )
    .unwrap();
    assert_eq!(exact_out.amount_in, input);
    assert_eq!(exact_out.amount_out, exact_in.amount_out);
    assert_eq!(exact_out.next_balances, exact_in.next_balances);
    assert!(exact_out.admin_fee_amount > BigUint::from(0u8));
}

#[test]
fn meta_exact_out_rejects_zero_and_unreachable_targets() {
    let balances = vec![n("1000000000000000000000000"); 2];
    let rates = (
        &balances,
        &n("10000"),
        &n("1000000000000000000"),
        &n("1000000000000000000"),
        &n("4000000"),
        &n("5000000000"),
    );
    assert_eq!(
        curve_multi::quote_meta_exact_out(
            rates.0,
            rates.1,
            rates.2,
            rates.3,
            rates.4,
            rates.5,
            &n("0"),
            0,
            1,
        )
        .unwrap_err()
        .0,
        "CURVE_EXACT_OUT_ZERO"
    );
    assert_eq!(
        curve_multi::quote_meta_exact_out(
            rates.0,
            rates.1,
            rates.2,
            rates.3,
            rates.4,
            rates.5,
            &n("3000000000000000000000000"),
            0,
            1,
        )
        .unwrap_err()
        .0,
        "CURVE_EXACT_OUT_UNREACHABLE"
    );
}
