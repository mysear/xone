use num_bigint::BigUint;
use serde_json::{json, Value};
use xone_domain::{multi_asset::MultiAssetState, Amount, Pool};
use xone_protocol_math::balancer_managed_fees::{quote_exact_in, quote_exact_out, ManagedFeeState};

fn amount(value: &Value) -> BigUint {
    value.as_str().unwrap().parse().unwrap()
}

fn amounts(value: &Value) -> Vec<BigUint> {
    value.as_array().unwrap().iter().map(amount).collect()
}

fn state(value: &Value) -> ManagedFeeState {
    ManagedFeeState {
        total_supply: amount(&value["total_supply"]),
        protocol_swap_fee: amount(&value["protocol_swap_fee"]),
        management_swap_fee: amount(&value["management_swap_fee"]),
        owner_balance: amount(&value["owner_balance"]),
        protocol_collector_balance: amount(&value["protocol_collector_balance"]),
        same_recipient: value["same_recipient"].as_bool().unwrap(),
    }
}

fn check(case: &Value) -> Option<(Vec<BigUint>, ManagedFeeState)> {
    let (quote, target) = match case.get("output") {
        Some(output) => (
            quote_exact_out as fn(&[BigUint], &[BigUint], &[BigUint], _, _, _, _, _) -> _,
            amount(output),
        ),
        None => (
            quote_exact_in as fn(&[BigUint], &[BigUint], &[BigUint], _, _, _, _, _) -> _,
            amount(&case["input"]),
        ),
    };
    let balances = amounts(&case["balances"]);
    let before = state(&case["state"]);
    let result: xone_domain::Result<xone_protocol_math::balancer_managed_fees::ManagedFeeQuote> =
        quote(
            &balances,
            &amounts(&case["weights"]),
            &amounts(&case["scales"]),
            &amount(&case["fee"]),
            &target,
            case["index_in"].as_u64().unwrap() as usize,
            case["index_out"].as_u64().unwrap() as usize,
            &before,
        );
    let expected = &case["expected"];
    if case.get("output").is_some() && target == BigUint::from(0u8) {
        assert_eq!(result.unwrap_err().0, "EXACT_OUT_ZERO_OUTPUT");
        return None;
    }
    if expected["reverted"] == true {
        assert!(result.is_err(), "original reverted: {case}");
        assert_eq!(before, state(&expected["next_state"]));
        assert_eq!(expected["rollback_verified"], true);
        return None;
    }
    let result = result.unwrap_or_else(|error| panic!("{error}: {case}"));
    if case.get("output").is_some() {
        assert_eq!(result.amount_in, amount(&expected["amount_in"]), "{case}");
        assert_eq!(result.amount_out, target);
    } else {
        assert_eq!(result.amount_in, target);
        assert_eq!(result.amount_out, amount(&expected["amount_out"]), "{case}");
    }
    assert_eq!(
        result.next_balances,
        amounts(&expected["next_balances"]),
        "{case}"
    );
    assert_eq!(result.next_state, state(&expected["next_state"]), "{case}");
    assert_eq!(
        result.protocol_bpt,
        amount(&expected["protocol_bpt"]),
        "{case}"
    );
    assert_eq!(
        result.management_bpt,
        amount(&expected["management_bpt"]),
        "{case}"
    );
    Some((result.next_balances, result.next_state))
}

#[test]
fn original_callbacks_match_amounts_cash_and_bpt_ledger() {
    let corpus = match std::env::var_os("XONE_MANAGED_FEE_CORPUS") {
        Some(path) => std::fs::read_to_string(path).unwrap(),
        None => include_str!("fixtures/managed_fee_exact_out.json").to_owned(),
    };
    let corpus: Value = serde_json::from_str(&corpus).unwrap();
    assert_eq!(
        corpus["reference"]["revision"],
        "e281ff40583c39bf1d46c322334b2b13936e0e33"
    );
    assert_eq!(corpus["reference"]["eip170_deployable"], false);
    let mut successful = 0;
    for case in corpus["cases"].as_array().unwrap() {
        successful += usize::from(check(case).is_some());
    }
    assert!(successful >= 6);
    for sequence in corpus["sequences"].as_array().unwrap() {
        let mut before = (amounts(&sequence["balances"]), state(&sequence["state"]));
        for step in sequence["steps"].as_array().unwrap() {
            assert_eq!(before, (amounts(&step["balances"]), state(&step["state"])));
            before = check(step).unwrap();
        }
    }
    for guard in corpus["guards"].as_array().unwrap() {
        let case = &corpus["cases"][guard["case_index"].as_u64().unwrap() as usize];
        assert_eq!(case["expected"]["reverted"], true);
        assert_eq!(case["expected"]["rollback_verified"], true);
    }
}

#[test]
fn pool_dispatch_preserves_complete_managed_fee_wire_state() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/managed_fee_exact_out.json")).unwrap();
    let step = &corpus["sequences"][0]["steps"][0];
    let address = |index| format!("0x{index:040x}");
    let pool_id = address(8);
    let digest = "1".repeat(64);
    let mut fees = step["state"].clone();
    let fields = fees.as_object_mut().unwrap();
    assert_eq!(fields.remove("same_recipient"), Some(json!(true)));
    fields.extend(
        json!({
            "owner": address(4), "protocol_collector": address(4),
            "timestamp": 1767225600, "block_number": 1, "block_hash": format!("0x{digest}")
        })
        .as_object()
        .unwrap()
        .clone(),
    );
    let pool: Pool = serde_json::from_value(json!({
        "chain_key": "evm:31337",
        "pool_id": pool_id,
        "protocol": "balancer-v2-managed-fees",
        "adapter_version": "balancer-v2-managed-fees/e281ff40-bpt-1",
        "token0": address(1), "token1": address(2),
        "reserve0": step["balances"][0], "reserve1": step["balances"][1],
        "fee_numerator": step["fee"].as_str().unwrap().parse::<u64>().unwrap(),
        "fee_denominator": 1_000_000_000_000_000_000u64,
        "token_behavior": "standard",
        "multi_asset": {
            "code_sha256": digest, "vault": address(3), "vault_code_sha256": digest,
            "pool_id": format!("{pool_id}0001{}", "0".repeat(20)),
            "assets": ([0, 1].map(|i| json!({
                "token": address(i + 1), "balance": step["balances"][i],
                "scaling": step["scales"][i], "code_sha256": digest
            }))),
            "weights": step["weights"], "amplification": "0", "admin_fee": 0,
            "managed_fees": fees
        }
    }))
    .unwrap();
    let mut pool = pool;
    let capability = xone_protocol_math::capabilities::describe(&pool).unwrap();
    assert_eq!(capability.quote_modes, ["EXACT_IN", "EXACT_OUT"]);
    assert!(!capability.execution_preparation && !capability.production_ready);
    for step in corpus["sequences"][0]["steps"].as_array().unwrap() {
        let output = Amount(amount(&step["output"]));
        let quote =
            xone_protocol_math::exact_out::quote_exact_out(&pool, &output, step["index_in"] == 0)
                .unwrap();
        let expected = &step["expected"];
        assert_eq!(quote.amount_in.0, amount(&expected["amount_in"]));
        assert_eq!(quote.amount_out, output);
        let next = quote.next_pool.multi_asset.as_ref().unwrap();
        let fees = next.managed_fees.as_ref().unwrap();
        assert_eq!(
            fees.total_supply.0,
            amount(&expected["next_state"]["total_supply"])
        );
        assert_eq!(
            fees.owner_balance.0,
            amount(&expected["next_state"]["owner_balance"])
        );
        assert_eq!(
            fees.protocol_collector_balance.0,
            amount(&expected["next_state"]["protocol_collector_balance"])
        );
        assert_eq!(
            MultiAssetState::from_proto(next.to_proto().unwrap()).unwrap(),
            *next
        );
        pool = quote.next_pool;
    }
}
