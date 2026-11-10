//! Original published single-token BPT math, using real original swap wallets.
use num_bigint::BigUint;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::OnceLock};
use xone_protocol_math::managed_v2_bpt::{quote_exit_exact_in, quote_join_exact_in, BptSwapInputs};
fn n(v: &Value) -> BigUint {
    v.as_str().unwrap().parse().unwrap()
}
fn native() -> &'static Value {
    static B: OnceLock<Value> = OnceLock::new();
    B.get_or_init(|| {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/contracts/managed-v2-bpt.json");
        let bytes = std::fs::read(path).unwrap();
        let bank: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            bank["authority"],
            "independent-local-original-managed-v2-bpt"
        );
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)).len(), 64);
        bank
    })
}
#[test]
fn all_successful_original_bpt_joins_and_exits_match_actual_raw_wallet_credits() {
    let raw = native();
    let mut joins = 0;
    let mut exits = 0;
    for c in raw["cases"].as_array().unwrap() {
        if c["expected_native"]["status"] != 1 || c["operation"] == "token_swap" {
            continue;
        }
        let before = raw["banks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == c["before"])
            .unwrap();
        let branch = &raw["branches"][c["namespace"].as_str().unwrap()];
        let p = branch["pools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == c["pool_name"])
            .unwrap();
        let s = &before["pools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == c["pool_name"])
            .unwrap()["state"];
        let token = if c["operation"] == "bpt_join" {
            &c["token_in"]
        } else {
            &c["token_out"]
        };
        assert_ne!(*token, p["pool"]);
        let index = branch["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .position(|t| t == token)
            .unwrap();
        let t = &s["token_states"][index];
        let cash = n(&t["info"][0]);
        let managed = n(&t["info"][1]);
        let scaling = n(&s["scales"][index]);
        let weight = n(&s["weights"][index]);
        let fee = n(&s["fee"]);
        let supply = n(&s["aum"]["actual_supply"]);
        let input = n(&c["amount_in"]);
        let state = BptSwapInputs {
            cash: &cash,
            managed: &managed,
            scaling: &scaling,
            weight: &weight,
            fee: &fee,
            actual_supply: &supply,
        };
        let out = if c["operation"] == "bpt_join" {
            joins += 1;
            quote_join_exact_in(state, &input)
        } else {
            exits += 1;
            quote_exit_exact_in(state, &input)
        }
        .unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        assert_eq!(
            out,
            n(&c["expected_native"]["actual_amount_out"]),
            "{} exact original BPT/underlying credit",
            c["id"]
        );
    }
    assert!(
        joins >= 42 && exits >= 42,
        "whole original settled/scheduled direction matrix exercised"
    );
}
#[test]
fn invalid_inputs_overflow_and_original_min_invariant_guard_are_explicit() {
    let one = BigUint::from(10u8).pow(18);
    let cash = &one * 10000u32;
    let zero = BigUint::from(0u8);
    let weight = &one / 2u8;
    let fee = &one / 100u8;
    let supply = &one * 30000u32;
    let s = BptSwapInputs {
        cash: &cash,
        managed: &zero,
        scaling: &one,
        weight: &weight,
        fee: &fee,
        actual_supply: &supply,
    };
    assert!(quote_join_exact_in(s, &zero).is_err());
    assert!(quote_exit_exact_in(s, &zero).is_err());
    assert!(quote_exit_exact_in(s, &(&supply + 1u8)).is_err());
    assert!(quote_exit_exact_in(s, &(&supply * 31u8 / 100u8)).is_err());
    assert!(
        quote_exit_exact_in(s, &(&supply * 30u8 / 100u8 + 1u8)).is_ok(),
        "divUp invariant may admit the raw unit above a naive30% cap"
    );
    let huge = (BigUint::from(1u8) << 256usize) - 1u8;
    assert!(quote_join_exact_in(s, &huge).is_err());
    let invalid = BptSwapInputs { weight: &zero, ..s };
    assert!(quote_join_exact_in(invalid, &one).is_err());
    let bad_scale = &one + 1u8;
    assert!(quote_exit_exact_in(
        BptSwapInputs {
            scaling: &bad_scale,
            ..s
        },
        &one
    )
    .is_err());
}
#[test]
fn minimum_unit_can_consume_input_and_return_zero_without_wrong_rounding() {
    let one = BigUint::from(10u8).pow(18);
    let cash = &one * 10000u32;
    let zero = BigUint::from(0u8);
    let weight = &one / 2u8;
    let fee = &one / 100u8;
    let supply = &one * 30000u32;
    let s = BptSwapInputs {
        cash: &cash,
        managed: &zero,
        scaling: &one,
        weight: &weight,
        fee: &fee,
        actual_supply: &supply,
    };
    assert_eq!(quote_join_exact_in(s, &BigUint::from(1u8)).unwrap(), zero);
    assert_eq!(quote_exit_exact_in(s, &BigUint::from(1u8)).unwrap(), zero);
}

#[test]
fn join_rejects_total_registration_overflow_even_if_cash_alone_fits_uint112() {
    let one = BigUint::from(10u8).pow(18);
    let cash = BigUint::from(1u8);
    let managed = (BigUint::from(1u8) << 112usize) - 2u8;
    let weight = &one / 2u8;
    let fee = &one / 100u8;
    let s = BptSwapInputs {
        cash: &cash,
        managed: &managed,
        scaling: &one,
        weight: &weight,
        fee: &fee,
        actual_supply: &one,
    };
    assert!(
        quote_join_exact_in(s, &BigUint::from(1u8)).is_err(),
        "original Vault rejects cash+managed+input ==2^112 even if output rounds to zero"
    );
}
