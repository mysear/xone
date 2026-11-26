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

fn meta_state() -> (
    Vec<BigUint>,
    Vec<BigUint>,
    Vec<BigUint>,
    BigUint,
    BigUint,
    BigUint,
) {
    (
        vec![n("1000000000000000000000000"); 2],
        vec![n("0"); 2],
        vec![n("1000000000000000000"); 2],
        n("10000"),
        n("4000000"),
        n("5000000000"),
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

#[test]
fn meta_underlying_composes_base_deposit_and_meta_swap() {
    let (base_balances, base_rates, base_amp, base_fee, base_admin, base_supply) = base_state();
    let (meta_balances, meta_admin_balances, meta_rates, meta_amp, meta_fee, meta_admin) =
        meta_state();
    let base = curve_multi::BaseLiquidityState {
        balances: base_balances,
        rates: base_rates,
        amplification: base_amp,
        fee: base_fee,
        admin_fee: base_admin,
        total_supply: base_supply,
    };
    let q = curve_multi::quote_meta_underlying_exact_in(
        &meta_balances,
        &meta_admin_balances,
        &meta_rates,
        &meta_amp,
        &meta_fee,
        &meta_admin,
        &base,
        &n("1000000000000000000000"),
        2,
        0,
    )
    .unwrap();
    assert!(q.amount_out > BigUint::from(0u8));
    assert!(q.next_meta_balances[1] > meta_balances[1]);
    assert!(q.next_base_total_supply > base.total_supply);
    assert!(q.base_admin_fee_amount > BigUint::from(0u8));
}

#[test]
fn meta_underlying_composes_meta_swap_and_base_withdrawal() {
    let (base_balances, base_rates, base_amp, base_fee, base_admin, base_supply) = base_state();
    let (meta_balances, meta_admin_balances, meta_rates, meta_amp, meta_fee, meta_admin) =
        meta_state();
    let base = curve_multi::BaseLiquidityState {
        balances: base_balances.clone(),
        rates: base_rates,
        amplification: base_amp,
        fee: base_fee,
        admin_fee: base_admin,
        total_supply: base_supply,
    };
    let q = curve_multi::quote_meta_underlying_exact_in(
        &meta_balances,
        &meta_admin_balances,
        &meta_rates,
        &meta_amp,
        &meta_fee,
        &meta_admin,
        &base,
        &n("1000000000000000000000"),
        0,
        2,
    )
    .unwrap();
    assert!(q.amount_out > BigUint::from(0u8));
    assert!(q.next_meta_balances[0] > meta_balances[0]);
    assert!(q.next_base_total_supply < base.total_supply);
    assert!(q.base_admin_fee_amount > BigUint::from(0u8));
}

#[test]
fn meta_underlying_routes_base_to_base_without_touching_meta_state() {
    let (base_balances, base_rates, base_amp, base_fee, base_admin, base_supply) = base_state();
    let (meta_balances, meta_admin_balances, meta_rates, meta_amp, meta_fee, meta_admin) =
        meta_state();
    let base = curve_multi::BaseLiquidityState {
        balances: base_balances,
        rates: base_rates,
        amplification: base_amp,
        fee: base_fee,
        admin_fee: base_admin,
        total_supply: base_supply,
    };
    let q = curve_multi::quote_meta_underlying_exact_in(
        &meta_balances,
        &meta_admin_balances,
        &meta_rates,
        &meta_amp,
        &meta_fee,
        &meta_admin,
        &base,
        &n("1000000000000000000000"),
        1,
        3,
    )
    .unwrap();
    assert!(q.amount_out > BigUint::from(0u8));
    assert_eq!(q.next_meta_balances, meta_balances);
    assert_eq!(q.next_meta_admin_balances, meta_admin_balances);
    assert_eq!(q.next_base_total_supply, base.total_supply);
}
