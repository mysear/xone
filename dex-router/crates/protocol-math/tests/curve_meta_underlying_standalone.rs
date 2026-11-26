#![allow(dead_code)]

use num_bigint::BigUint;
use num_traits::Zero;

#[path = "../src/curve_multi.rs"]
mod curve_multi;

fn n(value: &str) -> BigUint {
    value.parse().unwrap()
}

fn base_state() -> (
    Vec<BigUint>,
    Vec<BigUint>,
    BigUint,
    BigUint,
    BigUint,
    BigUint,
) {
    (
        vec![n("1000000000000000000000000"); 3],
        vec![n("1000000000000000000"); 3],
        n("10000"),
        n("4000000"),
        n("5000000000"),
        n("3000000000000000000000000"),
    )
}

#[test]
fn one_sided_base_deposit_mints_lp_and_tracks_admin_balance() {
    let (balances, rates, amp, fee, admin, supply) = base_state();
    let q = curve_multi::quote_base_add_liquidity_one_coin(
        &balances,
        &rates,
        &amp,
        &fee,
        &admin,
        &supply,
        &n("1000000000000000000000"),
        0,
    )
    .unwrap();
    assert!(q.amount_out > BigUint::from(0u8));
    assert_eq!(q.next_balances.len(), 3);
    assert_eq!(q.next_total_supply, supply + &q.amount_out);
    assert!(q.fee_amounts.iter().any(|value| !value.is_zero()));
    assert!(q.admin_fee_amount > BigUint::from(0u8));
}

#[test]
fn one_sided_base_withdrawal_burns_lp_and_preserves_supply_accounting() {
    let (balances, rates, amp, fee, admin, supply) = base_state();
    let q = curve_multi::quote_base_remove_liquidity_one_coin(
        &balances,
        &rates,
        &amp,
        &fee,
        &admin,
        &supply,
        &n("1000000000000000000000"),
        1,
    )
    .unwrap();
    assert!(q.amount_out > BigUint::from(0u8));
    assert_eq!(q.next_total_supply, supply - n("1000000000000000000000"));
    assert!(q.next_balances[1] < balances[1]);
    assert!(q.fee_amounts.iter().any(|value| !value.is_zero()));
    assert!(q.admin_fee_amount > BigUint::from(0u8));
}

#[test]
fn base_liquidity_rejects_initial_one_sided_deposit_and_invalid_burn() {
    let (balances, rates, amp, fee, admin, _) = base_state();
    assert!(curve_multi::quote_base_add_liquidity_one_coin(
        &balances,
        &rates,
        &amp,
        &fee,
        &admin,
        &BigUint::from(0u8),
        &n("1"),
        0,
    )
    .is_err());
    assert!(curve_multi::quote_base_remove_liquidity_one_coin(
        &balances,
        &rates,
        &amp,
        &fee,
        &admin,
        &n("100"),
        &n("100"),
        0,
    )
    .is_err());
}
