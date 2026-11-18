use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::Amount;
use xone_execution_preparation::{
    op_chain_cost::{fee_only_cost, Address, ExecutionInputs, PackedRules, RollupInputs},
    op_l1_cost, op_packed_cost,
};

fn amount(s: &str) -> Amount {
    s.parse().unwrap()
}
fn address(s: &str) -> Address {
    hex::decode(s.trim_start_matches("0x"))
        .unwrap()
        .try_into()
        .unwrap()
}
fn balances(v: &Value) -> BTreeMap<Address, Amount> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(a, n)| (address(a), amount(n.as_str().unwrap())))
        .collect()
}
fn rules(s: &str) -> PackedRules {
    match s {
        "ecotone" => PackedRules::Ecotone,
        "fjord" => PackedRules::Fjord,
        "isthmus" => PackedRules::Isthmus,
        "jovian" => PackedRules::Jovian,
        _ => panic!("unexpected rule"),
    }
}
fn with_cost(row: &Value, test: impl FnOnce(xone_execution_preparation::op_chain_cost::ChainCost)) {
    let input = &row["input"];
    let attr = hex::decode(input["attributes"].as_str().unwrap()).unwrap();
    let wire = hex::decode(row["signed_wire"].as_str().unwrap()).unwrap();
    let base = amount(input["base_fee"].as_str().unwrap());
    let maximum = amount(input["maximum"].as_str().unwrap());
    let priority = amount(input["priority"].as_str().unwrap());
    let value = Amount::zero();
    let execution = ExecutionInputs {
        gas_used: row["actual"]["gas_used"].as_u64().unwrap(),
        gas_limit: input["gas_limit"].as_u64().unwrap(),
        base_fee: &base,
        maximum_gas_price: &maximum,
        priority_gas_price: (input["kind"] == "dynamic").then_some(&priority),
        value: &value,
    };
    let regolith;
    let packed;
    let rollup = if input["rule"] == "regolith" {
        regolith =
            op_l1_cost::decode_parameters("evm:10", input["timestamp"].as_u64().unwrap(), &attr)
                .unwrap();
        RollupInputs::Regolith(&regolith)
    } else {
        packed = op_packed_cost::decode_packed_attributes(&attr).unwrap();
        RollupInputs::Packed {
            parameters: &packed,
            rules: rules(input["rule"].as_str().unwrap()),
        }
    };
    test(fee_only_cost(&execution, rollup, &wire, wire.len()).unwrap());
}
fn reference() -> Value {
    let path = std::env::var("XONE_OP_CHAIN_COST_VECTORS")
        .expect("full original op-geth transition reference required");
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(v["passed"], true);
    assert_eq!(v["scenario_count"], 320);
    assert_eq!(v["upstream_commit"], op_l1_cost::UPSTREAM);
    v
}

#[test]
#[ignore = "requires full unchanged original op-geth transition results"]
fn original_signed_op_transitions_match_full_fees_vaults_refunds_and_funding() {
    let original = reference();
    let mut coverage = BTreeMap::<String, usize>::new();
    for row in original["cases"].as_array().unwrap() {
        with_cost(row, |cost| {
            let v = serde_json::to_value(&cost).unwrap();
            let gas = row["actual"]["gas_used"].as_u64().unwrap();
            let limit = row["input"]["gas_limit"].as_u64().unwrap();
            let n = |key: &str| amount(v[key].as_str().unwrap());
            let expected = |key: &str| amount(row[key].as_str().unwrap());
            assert_eq!(n("maximum_funding"), expected("maximum_funding"));
            assert_eq!(cost.maximum_funding(), &expected("maximum_funding"));
            assert_eq!(n("effective_gas_price"), expected("effective_gas_price"));
            assert_eq!(n("l1_data_fee"), expected("l1_fee"));
            assert_eq!(
                n("operator_gas_limit_reserve"),
                expected("operator_maximum")
            );
            assert_eq!(
                n("operator_fee"),
                amount(row["actual"]["operator_fee"].as_str().unwrap())
            );
            assert_eq!(
                n("base_fee_to_vault").0,
                amount(row["input"]["base_fee"].as_str().unwrap()).0 * gas
            );
            assert_eq!(
                n("execution_fee").0,
                expected("effective_gas_price").0 * gas
            );
            assert_eq!(
                n("execution_gas_refund").0,
                expected("effective_gas_price").0 * (limit - gas)
            );
            assert_eq!(
                n("operator_gas_refund").0,
                &n("operator_gas_limit_reserve").0 - &n("operator_fee").0
            );
            assert_eq!(
                &n("gross_charge").0 + &n("total_gas_refund").0,
                n("upfront_charge").0
            );
            let payer = address(row["payer"].as_str().unwrap());
            let beneficiary = address(row["beneficiary"].as_str().unwrap());
            for key in ["actual", "exact_funding"] {
                let before = balances(&row[key]["before"]);
                let after = balances(&row[key]["after"]);
                let audited = cost
                    .audit_fee_only_balances(payer, beneficiary, &before, &after)
                    .unwrap();
                assert_eq!(
                    audited.net_native_debit.0,
                    &before[&payer].0 - &after[&payer].0
                );
                assert_eq!(
                    &audited.net_native_debit.0 + &audited.fee_credits_returned_to_payer.0,
                    audited.gross_charge.0
                );
                assert_eq!(
                    audited.fee_credits_returned_to_payer,
                    if payer == beneficiary {
                        n("priority_fee_to_beneficiary")
                    } else {
                        Amount::zero()
                    }
                );
            }
            if !row["below_funding"].is_null() {
                let before = balances(&row["below_funding"]["before"]);
                let after = balances(&row["below_funding"]["after"]);
                assert!(cost
                    .audit_fee_only_balances(payer, beneficiary, &before, &after)
                    .unwrap_err()
                    .to_string()
                    .contains("MAXIMUM_FUNDING"));
            }
            *coverage
                .entry(row["input"]["rule"].as_str().unwrap().into())
                .or_default() += 1;
        });
    }
    assert_eq!(coverage.len(), 5);
    assert!(coverage.values().all(|n| *n == 64));
}

#[test]
#[ignore = "requires full unchanged original op-geth transition results"]
fn original_fee_accounts_reject_missing_extra_changed_sources_and_burn_substitution() {
    let original = reference();
    let mut rejected = 0;
    for row in original["cases"].as_array().unwrap() {
        with_cost(row, |cost| {
            let payer = address(row["payer"].as_str().unwrap());
            let beneficiary = address(row["beneficiary"].as_str().unwrap());
            let before = balances(&row["actual"]["before"]);
            let after = balances(&row["actual"]["after"]);
            for a in before.keys() {
                let mut changed = before.clone();
                changed.get_mut(a).unwrap().0 += 1u32;
                assert!(cost
                    .audit_fee_only_balances(payer, beneficiary, &changed, &after)
                    .is_err());
                let mut changed = after.clone();
                changed.get_mut(a).unwrap().0 += 1u32;
                assert!(cost
                    .audit_fee_only_balances(payer, beneficiary, &before, &changed)
                    .is_err());
                let mut changed = before.clone();
                changed.remove(a);
                assert!(cost
                    .audit_fee_only_balances(payer, beneficiary, &changed, &after)
                    .is_err());
                rejected += 3;
            }
            let mut changed = after.clone();
            changed.insert([0xff; 20], Amount::zero());
            assert!(cost
                .audit_fee_only_balances(payer, beneficiary, &before, &changed)
                .is_err());
            rejected += 1;
            if row["input"]["base_fee"] != "0" {
                let base_vault = xone_execution_preparation::op_chain_cost::BASE_FEE_VAULT;
                let mut changed = after.clone();
                changed.insert(base_vault, before[&base_vault].clone());
                assert!(cost
                    .audit_fee_only_balances(payer, beneficiary, &before, &changed)
                    .is_err());
                rejected += 1;
            }
        });
    }
    assert!(rejected > 4000);
}

#[test]
fn chain_fee_context_rejects_rules_native_value_unknown_fees_and_overflow() {
    let mut a = hex::decode("3db6be2b").unwrap();
    a.resize(178, 0);
    a[4..8].copy_from_slice(&1u32.to_be_bytes());
    a[68..100].copy_from_slice(&amount("1").to_be_bytes().unwrap());
    let p = op_packed_cost::decode_packed_attributes(&a).unwrap();
    let zero = Amount::zero();
    let one = amount("1");
    let huge =
        amount("115792089237316195423570985008687907853269984665640564039457584007913129639935");
    let mut e = ExecutionInputs {
        gas_used: 21000,
        gas_limit: 60000,
        base_fee: &one,
        maximum_gas_price: &one,
        priority_gas_price: Some(&zero),
        value: &zero,
    };
    let run = |e: &ExecutionInputs<'_>, rule, wire: &[u8], budget| {
        fee_only_cost(
            e,
            RollupInputs::Packed {
                parameters: &p,
                rules: rule,
            },
            wire,
            budget,
        )
    };
    assert!(run(&e, PackedRules::Jovian, &[2], 1).is_ok());
    for rule in [
        PackedRules::Ecotone,
        PackedRules::Fjord,
        PackedRules::Isthmus,
    ] {
        assert!(run(&e, rule, &[2], 1).is_err());
    }
    for wire in [vec![], vec![3], vec![0x7e], vec![0x80]] {
        assert!(run(&e, PackedRules::Jovian, &wire, 1).is_err());
    }
    assert!(run(&e, PackedRules::Jovian, &[2], 0).is_err());
    e.value = &one;
    assert!(run(&e, PackedRules::Jovian, &[2], 1).is_err());
    e.value = &zero;
    e.gas_used = 60001;
    assert!(run(&e, PackedRules::Jovian, &[2], 1).is_err());
    e.gas_used = 21000;
    e.maximum_gas_price = &huge;
    assert!(run(&e, PackedRules::Jovian, &[2], 1).is_err());
    e.maximum_gas_price = &zero;
    assert!(run(&e, PackedRules::Jovian, &[2], 1).is_err());

    // Each component can fit uint256 while the complete charge/reserve does
    // not. Never saturate the sum to an apparently affordable amount.
    a[4..8].copy_from_slice(&1_000_000u32.to_be_bytes());
    a[36..68].copy_from_slice(
        &Amount::checked(&huge.0 / 1600u32)
            .unwrap()
            .to_be_bytes()
            .unwrap(),
    );
    let wide = op_packed_cost::decode_packed_attributes(&a).unwrap();
    e.maximum_gas_price = &one;
    assert!(fee_only_cost(
        &e,
        RollupInputs::Packed {
            parameters: &wide,
            rules: PackedRules::Jovian,
        },
        &[2],
        1,
    )
    .is_err());
}
