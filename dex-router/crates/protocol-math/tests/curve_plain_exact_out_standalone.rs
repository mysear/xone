#[allow(dead_code)]
#[path = "../src/curve_multi.rs"]
mod curve_multi;

use num_bigint::BigUint;

fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}

#[test]
fn plain_exact_out_replays_three_and_four_coin_poststate() {
    for count in [3, 4] {
        let balances = vec![n("1000000000000000000000000"); count];
        let rates = vec![n("1000000000000000000"); count];
        let input = n("10000000000000000000000");
        let exact_in = curve_multi::quote_exact_in(
            &balances,
            &n("10000"),
            &rates,
            &n("4000000"),
            &n("5000000000"),
            &input,
            0,
            count - 1,
        )
        .unwrap();
        let exact_out = curve_multi::quote_exact_out(
            &balances,
            &n("10000"),
            &rates,
            &n("4000000"),
            &n("5000000000"),
            &exact_in.amount_out,
            0,
            count - 1,
        )
        .unwrap();
        assert_eq!(exact_out.amount_in, input);
        assert_eq!(exact_out.next_balances, exact_in.next_balances);
    }
}

#[test]
fn plain_exact_out_rejects_unreachable_target() {
    let balances = vec![n("1000000000000000000000000"); 3];
    let rates = vec![n("1000000000000000000"); 3];
    let result = curve_multi::quote_exact_out(
        &balances,
        &n("10000"),
        &rates,
        &n("4000000"),
        &n("5000000000"),
        &n("3000000000000000000000000"),
        0,
        2,
    );
    assert_eq!(result.unwrap_err().0, "CURVE_EXACT_OUT_UNREACHABLE");
}
