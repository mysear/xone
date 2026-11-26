use num_bigint::BigUint;
use xone_protocol_math::curve_ng::{self, NgOracleState};

fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}

#[test]
fn ng_exact_out_replays_complete_oracle_poststate() {
    let balances = vec![
        n("1000000000000000000000000"),
        n("2000000000000000000000000"),
    ];
    let rates = vec![n("1000000000000000000"); 2];
    let state = NgOracleState {
        stored_balances: balances.clone(),
        admin_balances: vec![n("0"), n("0")],
        last_prices_packed: vec![n("0")],
        last_d_packed: n("0"),
        ma_last_time: n("0"),
        ma_exp_time: 0,
        d_ma_time: 0,
        timestamp: 1,
    };
    let input = n("10000000000000000000000");
    let exact_in = curve_ng::quote_full_exact_in(
        &balances,
        &n("10000"),
        &rates,
        &n("4000000"),
        &n("5000000000"),
        &n("20000000000"),
        &input,
        0,
        1,
        &state,
    )
    .unwrap();
    let exact_out = curve_ng::quote_full_exact_out(
        &balances,
        &n("10000"),
        &rates,
        &n("4000000"),
        &n("5000000000"),
        &n("20000000000"),
        &exact_in.amount_out,
        0,
        1,
        &state,
    )
    .unwrap();
    assert_eq!(exact_out.amount_in, input);
    assert_eq!(exact_out.amount_out, exact_in.amount_out);
    assert_eq!(exact_out.next_balances, exact_in.next_balances);
    assert_eq!(exact_out.next_state, exact_in.next_state);
}

#[test]
fn ng_exact_out_rejects_zero_and_unreachable_targets() {
    let balances = vec![
        n("1000000000000000000000000"),
        n("2000000000000000000000000"),
    ];
    let state = NgOracleState {
        stored_balances: balances.clone(),
        admin_balances: vec![n("0"), n("0")],
        last_prices_packed: vec![n("0")],
        last_d_packed: n("0"),
        ma_last_time: n("0"),
        ma_exp_time: 0,
        d_ma_time: 0,
        timestamp: 1,
    };
    let rates = vec![n("1000000000000000000"); 2];
    assert_eq!(
        curve_ng::quote_full_exact_out(
            &balances,
            &n("10000"),
            &rates,
            &n("4000000"),
            &n("5000000000"),
            &n("20000000000"),
            &n("0"),
            0,
            1,
            &state,
        )
        .unwrap_err()
        .0,
        "CURVE_EXACT_OUT_ZERO"
    );
    assert_eq!(
        curve_ng::quote_full_exact_out(
            &balances,
            &n("10000"),
            &rates,
            &n("4000000"),
            &n("5000000000"),
            &n("20000000000"),
            &n("3000000000000000000000000"),
            0,
            1,
            &state,
        )
        .unwrap_err()
        .0,
        "CURVE_EXACT_OUT_UNREACHABLE"
    );
}
