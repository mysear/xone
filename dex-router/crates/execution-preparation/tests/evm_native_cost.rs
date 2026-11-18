use serde_json::Value;
use xone_domain::Amount;
use xone_execution_preparation::evm_native_cost::{measured_native_cost, reserve};

fn amount(value: &str) -> Amount {
    value.parse().unwrap()
}

#[test]
fn native_cost_uses_effective_price_and_keeps_reserve_separate_from_charge() {
    let gas = 50_000;
    for (base, cap, tip, effective) in [
        ("100", "200", Some("30"), "130"),
        ("100", "110", Some("30"), "110"),
        ("100", "200", None, "200"),
        ("0", "0", Some("0"), "0"),
    ] {
        let parsed_tip = tip.map(amount);
        let cost = measured_native_cost(
            gas,
            100_000,
            &amount(base),
            &amount(cap),
            parsed_tip.as_ref(),
        )
        .unwrap();
        assert_eq!(cost.effective_gas_price, amount(effective));
        assert_eq!(cost.execution_fee.0, amount(effective).0 * gas);
        assert_eq!(
            &cost.base_fee_burned.0 + &cost.priority_fee_paid.0,
            cost.execution_fee.0
        );
        assert_eq!(
            cost.gas_limit_reserve,
            reserve(100_000, &amount(cap)).unwrap()
        );
        assert!(cost.gas_limit_reserve >= cost.execution_fee);
    }
}

#[test]
fn native_cost_rejects_impossible_receipts_context_and_full_width_overflow() {
    for (used, limit, base, cap, tip) in [
        (0, 100_000, "1", "2", None),
        (20_999, 100_000, "1", "2", None),
        (100_001, 100_000, "1", "2", None),
        (21_000, 0, "1", "2", None),
        (21_000, 100_000, "3", "2", None),
        (21_000, 100_000, "1", "2", Some("3")),
    ] {
        assert!(measured_native_cost(
            used,
            limit,
            &amount(base),
            &amount(cap),
            tip.map(amount).as_ref()
        )
        .is_err());
    }
    let maximum =
        amount("115792089237316195423570985008687907853269984665640564039457584007913129639935");
    assert!(reserve(21_000, &maximum).is_err());
    assert!(measured_native_cost(21_000, 21_000, &amount("0"), &maximum, None).is_err());
    assert!(reserve(0, &amount("1")).is_err());
}

#[test]
#[ignore = "requires this run's independent Python integer fee vectors"]
fn current_python_native_cost_vectors_match_all_components_and_rejections() {
    let path = std::env::var("XONE_NATIVE_COST_VECTORS")
        .expect("current independent fee vectors required");
    let document: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let cases = document["cases"].as_array().unwrap();
    assert!(cases.len() >= 40);
    for row in cases {
        let used = row["gas_used"].as_str().unwrap().parse().unwrap();
        let limit = row["gas_limit"].as_str().unwrap().parse().unwrap();
        let base = amount(row["base_fee"].as_str().unwrap());
        let cap = amount(row["maximum"].as_str().unwrap());
        let tip = row["priority"].as_str().map(amount);
        let actual = measured_native_cost(used, limit, &base, &cap, tip.as_ref());
        if row["valid"] == false {
            assert!(actual.is_err(), "{}", row["id"]);
            continue;
        }
        let actual = actual.unwrap();
        let expected = &row["expected"];
        assert_eq!(
            actual.effective_gas_price,
            amount(expected["effective_gas_price"].as_str().unwrap())
        );
        assert_eq!(
            actual.execution_fee,
            amount(expected["execution_fee"].as_str().unwrap())
        );
        assert_eq!(
            actual.base_fee_burned,
            amount(expected["base_fee_burned"].as_str().unwrap())
        );
        assert_eq!(
            actual.priority_fee_paid,
            amount(expected["priority_fee_paid"].as_str().unwrap())
        );
        assert_eq!(
            actual.gas_limit_reserve,
            amount(expected["gas_limit_reserve"].as_str().unwrap())
        );
    }
}

#[test]
fn native_balance_conservation_handles_distinct_and_alias_beneficiary_without_double_fee() {
    use xone_execution_preparation::evm_native_cost::{reconcile_native_balances, NativeBalances};
    for base in ["0", "1", "10"] {
        let cost = measured_native_cost(
            21_000,
            100_000,
            &amount(base),
            &amount("20"),
            Some(&amount("2")),
        )
        .unwrap();
        let before = amount("10000000");
        let distinct = NativeBalances {
            payer_before: before.clone(),
            payer_after: Amount::checked(&before.0 - &cost.execution_fee.0).unwrap(),
            beneficiary_before: amount("17"),
            beneficiary_after: Amount::checked(amount("17").0 + &cost.priority_fee_paid.0).unwrap(),
        };
        reconcile_native_balances(&cost, &distinct, false).unwrap();
        let alias_after = Amount::checked(&before.0 - &cost.base_fee_burned.0).unwrap();
        let alias = NativeBalances {
            payer_before: before.clone(),
            payer_after: alias_after.clone(),
            beneficiary_before: before.clone(),
            beneficiary_after: alias_after,
        };
        reconcile_native_balances(&cost, &alias, true).unwrap();
        for field in 0..4 {
            let mut changed = alias.clone();
            let destination = match field {
                0 => &mut changed.payer_before,
                1 => &mut changed.payer_after,
                2 => &mut changed.beneficiary_before,
                _ => &mut changed.beneficiary_after,
            };
            *destination = Amount::checked(&destination.0 + 1u8).unwrap();
            assert!(reconcile_native_balances(&cost, &changed, true).is_err());
            let mut changed = distinct.clone();
            let destination = match field {
                0 => &mut changed.payer_before,
                1 => &mut changed.payer_after,
                2 => &mut changed.beneficiary_before,
                _ => &mut changed.beneficiary_after,
            };
            *destination = Amount::checked(&destination.0 + 1u8).unwrap();
            assert!(reconcile_native_balances(&cost, &changed, false).is_err());
        }
    }
}

#[test]
fn payer_cost_requires_maximum_funding_even_if_the_actual_fee_or_alias_debit_is_smaller() {
    use xone_execution_preparation::evm_native_cost::{
        audited_payer_cost, reconcile_native_balances, NativeBalances,
    };
    let cost = measured_native_cost(
        21_000,
        100_000,
        &amount("1"),
        &amount("10"),
        Some(&amount("2")),
    )
    .unwrap();
    let before = amount("999999");
    let insufficient = NativeBalances {
        payer_before: before.clone(),
        payer_after: Amount::checked(&before.0 - &cost.execution_fee.0).unwrap(),
        beneficiary_before: amount("0"),
        beneficiary_after: cost.priority_fee_paid.clone(),
    };
    reconcile_native_balances(&cost, &insufficient, false).unwrap();
    assert!(audited_payer_cost(&cost, &insufficient, false).is_err());
    for same in [false, true] {
        let before = cost.gas_limit_reserve.clone();
        let debit = if same {
            &cost.base_fee_burned
        } else {
            &cost.execution_fee
        };
        let after = Amount::checked(&before.0 - &debit.0).unwrap();
        let balances = NativeBalances {
            payer_before: before.clone(),
            payer_after: after.clone(),
            beneficiary_before: if same { before } else { amount("0") },
            beneficiary_after: if same {
                after
            } else {
                cost.priority_fee_paid.clone()
            },
        };
        let net = audited_payer_cost(&cost, &balances, same).unwrap();
        assert_eq!(
            &net.net_native_debit.0 + &net.priority_returned_to_payer.0,
            net.gross_execution_charge.0
        );
        assert_eq!(net.net_native_debit, *debit);
    }
    let free = measured_native_cost(
        21_000,
        100_000,
        &amount("0"),
        &amount("10"),
        Some(&amount("2")),
    )
    .unwrap();
    let alias = NativeBalances {
        payer_before: amount("0"),
        payer_after: amount("0"),
        beneficiary_before: amount("0"),
        beneficiary_after: amount("0"),
    };
    reconcile_native_balances(&free, &alias, true).unwrap();
    assert!(
        audited_payer_cost(&free, &alias, true).is_err(),
        "zero net debit does not remove the upfront funding rule"
    );
}
