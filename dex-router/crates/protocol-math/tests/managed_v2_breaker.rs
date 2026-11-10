//! Independent integer circuit kernel against original stored/getter views.
use num_bigint::BigUint;
use serde_json::Value;
use std::path::Path;
use xone_protocol_math::managed_v2_breaker::{check_side, derive_view};
fn decimal(v: &Value) -> BigUint {
    v.as_str().unwrap().parse().unwrap()
}
fn word(s: &str) -> BigUint {
    if s == "0x" {
        0u8.into()
    } else {
        BigUint::parse_bytes(s.trim_start_matches("0x").as_bytes(), 16).unwrap()
    }
}
#[test]
fn every_original_circuit_view_matches_raw_cache_and_dynamic_pow_rounding() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/contracts/managed-v2-breaker.json");
    let raw: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        raw["authority"],
        "independent-local-original-managed-v2-breaker"
    );
    let mut seen = 0;
    let mut dynamic = 0;
    let mut enabled = 0;
    for bank in raw["banks"].as_array().unwrap() {
        let branch = &raw["branches"][bank["namespace"].as_str().unwrap()];
        for row in bank["pools"].as_array().unwrap() {
            let p = branch["pools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == row["name"])
                .unwrap();
            let state = &row["state"];
            for (index, token) in branch["tokens"].as_array().unwrap().iter().enumerate() {
                let stored = bank["control_storage"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| {
                        r["address"] == p["pool"] && r["kind"] == "circuit" && r["token"] == *token
                    })
                    .unwrap();
                let getter = state["getters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|g| {
                        g["to"] == p["pool"]
                            && g["method"] == "getCircuitBreakerState"
                            && g["args"][0] == *token
                    })
                    .unwrap();
                let raw_word = word(stored["value"].as_str().unwrap());
                let weight = decimal(&state["weights"][index]);
                let scaling = decimal(&state["scales"][index]);
                let got = derive_view(&raw_word, &weight, &scaling).unwrap();
                let expected = getter["decoded"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(decimal)
                    .collect::<Vec<_>>();
                assert_eq!(
                    got.as_values(),
                    expected,
                    "{} {} token{}",
                    bank["id"],
                    row["name"],
                    index
                );
                if raw_word != BigUint::from(0u8) {
                    enabled += 1;
                    if got.reference_weight != weight {
                        dynamic += 1;
                    }
                }
                seen += 1;
            }
        }
    }
    assert!(
        seen > 1000 && enabled > 100 && dynamic > 100,
        "{seen} views {enabled} installed {dynamic} dynamic"
    );
}
#[test]
fn disabled_bounds_skip_balance_division_and_abi_widths_stay_checked() {
    let zero = BigUint::from(0u8);
    let one = BigUint::from(10u8).pow(18);
    let weight = &one / 2u8;
    assert!(check_side(&zero, &one, &weight, &zero, true).is_ok());
    assert!(check_side(&zero, &one, &weight, &zero, false).is_ok());
    let invalid = BigUint::from(1u8) << 256usize;
    assert!(derive_view(&invalid, &weight, &one).is_err());
    assert!(check_side(&zero, &invalid, &weight, &one, true).is_err());
}
#[test]
fn lower_floor_upper_ceil_compare_the_internal_scaled_price_and_equality_passes() {
    let one = BigUint::from(10u8).pow(18);
    let weight = &one / 2u8;
    // Unit ratio cached exactly through chosen u32 max => ratio10*ONE.
    // Reference price ONE/10 => both derived boundaries ONE.
    let percentage = BigUint::from(1u8);
    let reference = &one / 10u8;
    let raw = &percentage
        | (&percentage << 16usize)
        | (&weight << 32usize)
        | (&reference << 96usize)
        | ((BigUint::from(u32::MAX)) << 192usize)
        | ((BigUint::from(u32::MAX)) << 224usize);
    let balance = &one;
    let supply = &one * 2u8;
    assert!(check_side(&raw, &supply, &weight, balance, true).is_ok());
    assert!(check_side(&raw, &supply, &weight, balance, false).is_ok());
    assert!(check_side(&raw, &(&supply - 1u8), &weight, balance, true).is_err());
    assert!(check_side(&raw, &(&supply + 1u8), &weight, balance, false).is_err());
    let huge = (BigUint::from(1u8) << 256usize) - 1u8;
    assert!(check_side(&raw, &huge, &weight, balance, true).is_err());
    assert!(check_side(&raw, &supply, &weight, &BigUint::from(0u8), true).is_err());
}

#[test]
fn calculation_keeps_internal_dust_before_wallet_output_policy_or_settlement() {
    use xone_protocol_math::{
        managed_v2_bpt::{calculate_exit_exact_in, quote_exit_exact_in, BptSwapInputs},
        managed_v2_swap::{calculate_regular, quote_regular, WeightedInputs},
    };
    let one = BigUint::from(10u8).pow(18);
    let zero = BigUint::from(0u8);
    let cash = BigUint::from(2u8);
    let scaling = &one * BigUint::from(10u8).pow(12);
    let weight = &one / 2u8;
    let fee = &one / 100u8;
    let b = BptSwapInputs {
        cash: &cash,
        managed: &zero,
        scaling: &scaling,
        weight: &weight,
        fee: &fee,
        actual_supply: &one,
    };
    let exit = calculate_exit_exact_in(b, &(&one / 10u8)).unwrap();
    assert_eq!(exit.amount_out, zero);
    assert!(exit.scaled_amount_out > zero);
    assert_eq!(quote_exit_exact_in(b, &(&one / 10u8)).unwrap(), zero);
    let balances = vec![&one * 10000u32, cash, &one * 10000u32];
    let managed = vec![zero.clone(); 3];
    let weights = vec![&one / 5u8, &one / 2u8, &one * 3u8 / 10u8];
    let scales = vec![one.clone(), scaling, one.clone()];
    let s = WeightedInputs {
        cash: &balances,
        managed: &managed,
        weights: &weights,
        scales: &scales,
        fee: &fee,
    };
    let regular = calculate_regular(s, &one, 0, 1).unwrap();
    assert_eq!(regular.amount_out, zero);
    assert!(regular.scaled_amount_out > zero);
    assert_eq!(regular.gross_input_scaled, one);
    assert!(
        quote_regular(s, &one, 0, 1).is_err(),
        "retained ordinary positive-output policy is unchanged"
    );
}
#[test]
fn join_calculation_precedes_vault_uint112_settlement() {
    use xone_protocol_math::managed_v2_bpt::{
        calculate_join_exact_in, quote_join_exact_in, BptSwapInputs,
    };
    let one = BigUint::from(10u8).pow(18);
    let cash = BigUint::from(1u8);
    let zero = BigUint::from(0u8);
    let fee = &one / 100u8;
    let weight = &one / 2u8;
    let supply = BigUint::from(2_000_000u32);
    let input = BigUint::from(1u8) << 112usize;
    let s = BptSwapInputs {
        cash: &cash,
        managed: &zero,
        scaling: &one,
        weight: &weight,
        fee: &fee,
        actual_supply: &supply,
    };
    assert!(calculate_join_exact_in(s, &input).unwrap().amount_out > zero);
    assert!(
        quote_join_exact_in(s, &input).is_err(),
        "normal quote still rejects un-packable Vault input"
    );
}

#[test]
fn breaker_profile_requires_exact_current_pow_view_before_pricing() {
    use xone_domain::{
        managed_v2_circuit_state::{
            ManagedV2CircuitState, ManagedV2CircuitToken, ADAPTER_VERSION, PROTOCOL,
        },
        Amount, Pool,
    };
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/contracts/managed-v2-bpt-wire.json");
    let v: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let mut p: Pool = serde_json::from_value(v["snapshot"]["pools"][0].clone()).unwrap();
    p.protocol = PROTOCOL.into();
    p.adapter_version = ADAPTER_VERSION.into();
    let s = p.multi_asset.as_mut().unwrap();
    let mut tokens = Vec::new();
    for asset in &s.assets {
        tokens.push(ManagedV2CircuitToken {
            token: asset.token.clone(),
            state_word: Amount::zero(),
            reference_bpt_price: Amount::zero(),
            reference_weight: Amount::zero(),
            lower_percentage: Amount::zero(),
            upper_percentage: Amount::zero(),
            lower_bpt_price_bound: Amount::zero(),
            upper_bpt_price_bound: Amount::zero(),
        });
    }
    s.managed_v2.as_mut().unwrap().circuit_breakers = Some(ManagedV2CircuitState { tokens });
    p.validate().unwrap();
    let a = &p.multi_asset.as_ref().unwrap().assets;
    let input: Amount = "100".parse().unwrap();
    let actual = xone_protocol_math::managed_v2_pool::quote_exact_in_tokens(
        &p,
        &input,
        &a[0].token,
        &a[1].token,
    )
    .unwrap();
    assert_eq!(actual.next_pool.protocol, PROTOCOL);
    assert!(actual
        .next_pool
        .multi_asset
        .unwrap()
        .managed_v2
        .unwrap()
        .execution_context
        .is_some());
}
