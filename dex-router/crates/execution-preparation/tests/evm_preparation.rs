#[path = "support/evm_context.rs"]
mod common;
use common::*;
use xone_domain::{hash, Amount};
use xone_execution_preparation::evm::{prepare, FeeContext, FundingObservation};
fn contexts(
    reverse: bool,
) -> (
    xone_execution_plan::BoundEvmExecution,
    Vec<xone_domain::Pool>,
    FundingObservation,
    FeeContext,
    u64,
) {
    let (plan, pools) = fixture(reverse);
    let executor = executor();
    let bound =
        xone_execution_plan::bind_v2_execution(&plan, &pools, &executor, &[ROUTER.into()]).unwrap();
    let now = plan.snapshot.block.timestamp_ms + 1;
    let observed = FundingObservation {
        snapshot_hash: hash(&plan.snapshot).unwrap(),
        caller: plan.context.caller.clone(),
        sell_token: plan.context.sell_token.clone(),
        spender: executor.target,
        pending_nonce: 18,
        native_balance: "10000000000000000000".parse().unwrap(),
        token_balance: "100000000000000000000000".parse().unwrap(),
        allowance: "100000000000000000000000".parse().unwrap(),
        base_fee_per_gas: "1000000000".parse().unwrap(),
    };
    let fees = FeeContext {
        chain_id: plan.context.chain_key[4..].parse().unwrap(),
        nonce: 18,
        gas_limit: 500000,
        max_fee_per_gas: "2000000000".parse().unwrap(),
        max_priority_fee_per_gas: "1000000000".parse().unwrap(),
        max_network_fee: "1000000000000000000".parse().unwrap(),
    };
    (bound, pools, observed, fees, now)
}
#[test]
fn unsigned_exact_input_keeps_uint256_amounts_and_full_bound_plan() {
    for reverse in [false, true] {
        let (bound, pools, observed, fees, now) = contexts(reverse);
        let before = serde_json::to_value((&bound, &pools, &observed, &fees)).unwrap();
        let p = prepare(&bound, &pools, &[ROUTER.into()], &observed, &fees, now).unwrap();
        assert!(p.ready);
        assert!(p.required_input.0.bits() > 64);
        assert!(!p.signed && !p.submitted && !p.independent_production_verification);
        assert_eq!(p.max_network_fee, "1000000000000000".parse().unwrap());
        assert_eq!(p.transaction.as_ref().unwrap().data, bound.transaction.data);
        assert!(p.unsigned_transaction.as_ref().unwrap().starts_with("0x02"));
        let mut value = serde_json::to_value(&p).unwrap();
        value.as_object_mut().unwrap().remove("preparation_hash");
        assert_eq!(hash(&value).unwrap(), p.preparation_hash);
        assert_eq!(
            before,
            serde_json::to_value((&bound, &pools, &observed, &fees)).unwrap()
        );
        if let Ok(directory) = std::env::var("XONE_EVM_DRAFT_OUTPUT") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(std::path::Path::new(&directory).join(format!("unsigned-{reverse}.json")),serde_json::to_vec_pretty(&serde_json::json!({"bound":bound,"preparation":p,"observed":observed,"fees":fees})).unwrap()).unwrap();
        }
    }
}
#[test]
fn insufficient_native_tokens_or_allowance_never_emits_unsigned_wire() {
    let (bound, pools, observed, fees, now) = contexts(false);
    for mode in [0, 1, 2, 3] {
        let mut source = observed.clone();
        if mode == 0 || mode == 3 {
            source.native_balance = Amount::zero();
        }
        if mode == 1 || mode == 3 {
            source.token_balance = Amount::zero();
        }
        if mode == 2 || mode == 3 {
            source.allowance = Amount::zero();
        }
        let p = prepare(&bound, &pools, &[ROUTER.into()], &source, &fees, now).unwrap();
        assert!(!p.ready);
        assert!(
            p.transaction.is_none()
                && p.unsigned_transaction.is_none()
                && p.rpc_transaction.is_none()
        );
        assert_eq!(p.blockers.len(), if mode == 3 { 3 } else { 1 });
        assert!(!p.signed && !p.submitted);
    }
}
#[test]
fn changed_chain_nonce_capture_identity_and_expiry_reject() {
    let (bound, pools, observed, fees, now) = contexts(false);
    let mut wrong = fees.clone();
    wrong.chain_id += 1;
    assert_eq!(
        prepare(&bound, &pools, &[ROUTER.into()], &observed, &wrong, now)
            .unwrap_err()
            .0,
        "PREPARATION_EVM_CHAIN"
    );
    let mut wrong = fees.clone();
    wrong.nonce += 1;
    assert_eq!(
        prepare(&bound, &pools, &[ROUTER.into()], &observed, &wrong, now)
            .unwrap_err()
            .0,
        "PREPARATION_EVM_NONCE_CHANGED"
    );
    for mode in [0, 1, 2, 3] {
        let mut wrong = observed.clone();
        match mode {
            0 => wrong.snapshot_hash = "00".repeat(32),
            1 => wrong.caller = "0x0000000000000000000000000000000000001234".into(),
            2 => wrong.sell_token = "0x0000000000000000000000000000000000001234".into(),
            _ => wrong.spender = "0x0000000000000000000000000000000000001234".into(),
        };
        assert_eq!(
            prepare(&bound, &pools, &[ROUTER.into()], &wrong, &fees, now)
                .unwrap_err()
                .0,
            "PREPARATION_EVM_OBSERVATION_BINDING"
        );
    }
    assert_eq!(
        prepare(
            &bound,
            &pools,
            &[ROUTER.into()],
            &observed,
            &fees,
            bound.plan.context.deadline_ms
        )
        .unwrap_err()
        .0,
        "QUOTE_EXPIRED"
    );
    assert_eq!(
        prepare(
            &bound,
            &pools,
            &[ROUTER.into()],
            &observed,
            &fees,
            bound.plan.snapshot.block.timestamp_ms - 1
        )
        .unwrap_err()
        .0,
        "QUOTE_EXPIRED"
    );
}
#[test]
fn fee_caps_priority_basefee_and_uint256_overflow_reject() {
    let (bound, pools, observed, fees, now) = contexts(false);
    for mode in [0, 1, 2, 3, 4] {
        let mut wrong = fees.clone();
        match mode {
            0 => wrong.gas_limit = 20000,
            1 => wrong.gas_limit = 15000001,
            2 => wrong.max_priority_fee_per_gas = "3000000000".parse().unwrap(),
            3 => wrong.max_fee_per_gas = Amount::zero(),
            _ => wrong.max_fee_per_gas = "999999999".parse().unwrap(),
        };
        assert_eq!(
            prepare(&bound, &pools, &[ROUTER.into()], &observed, &wrong, now)
                .unwrap_err()
                .0,
            "PREPARATION_EVM_FEE_CONTEXT"
        );
    }
    let mut wrong = fees.clone();
    wrong.max_network_fee = Amount::zero();
    assert_eq!(
        prepare(&bound, &pools, &[ROUTER.into()], &observed, &wrong, now)
            .unwrap_err()
            .0,
        "PREPARATION_EVM_FEE_BUDGET"
    );
    let mut wrong = fees.clone();
    wrong.max_fee_per_gas =
        Amount((num_bigint::BigUint::from(1u8) << 256usize) - num_bigint::BigUint::from(1u8));
    wrong.max_network_fee = wrong.max_fee_per_gas.clone();
    assert!(prepare(&bound, &pools, &[ROUTER.into()], &observed, &wrong, now).is_err());
}
#[test]
fn tampered_plan_or_calldata_does_not_borrow_a_valid_binding() {
    let (bound, pools, observed, fees, now) = contexts(false);
    for mode in [0, 1, 2] {
        let mut wrong = bound.clone();
        match mode {
            0 => wrong.transaction.data = "0x00".into(),
            1 => wrong.plan.context.recipient = CALLER.into(),
            _ => wrong.binding_hash = "00".repeat(32),
        };
        assert!(prepare(&wrong, &pools, &[ROUTER.into()], &observed, &fees, now).is_err());
    }
    assert!(prepare(&bound, &pools, &[], &observed, &fees, now).is_err());
}

#[test]
fn zero_nonce_priority_and_wide_fee_fields_keep_canonical_encoding() {
    let (bound, pools, mut observed, mut fees, now) = contexts(false);
    observed.pending_nonce = 0;
    fees.nonce = 0;
    fees.max_priority_fee_per_gas = Amount::zero();
    fees.max_fee_per_gas = Amount::checked(num_bigint::BigUint::from(1u8) << 180usize).unwrap();
    fees.max_network_fee = Amount::checked(num_bigint::BigUint::from(1u8) << 220usize).unwrap();
    observed.native_balance = fees.max_network_fee.clone();
    let prepared = prepare(&bound, &pools, &[ROUTER.into()], &observed, &fees, now).unwrap();
    assert!(prepared.ready);
    assert!(prepared.max_network_fee.0.bits() > 128);
    assert_eq!(prepared.transaction.as_ref().unwrap().nonce, 0);
    assert_eq!(
        prepared
            .transaction
            .as_ref()
            .unwrap()
            .max_priority_fee_per_gas,
        Amount::zero()
    );
    if let Ok(directory) = std::env::var("XONE_EVM_DRAFT_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("unsigned-zero-wide-fee.json"),serde_json::to_vec_pretty(&serde_json::json!({"bound":bound,"preparation":prepared,"observed":observed,"fees":fees})).unwrap()).unwrap();
    }
}

#[test]
fn nonce_exhaustion_rejects_the_unusable_u64_maximum() {
    let (bound, pools, mut observed, mut fees, now) = contexts(false);
    observed.pending_nonce = u64::MAX;
    fees.nonce = u64::MAX;
    assert_eq!(
        prepare(&bound, &pools, &[ROUTER.into()], &observed, &fees, now)
            .unwrap_err()
            .0,
        "PREPARATION_EVM_NONCE_EXHAUSTED"
    );
}

#[test]
fn nonce_last_usable_value_is_preserved_without_integer_narrowing() {
    let (bound, pools, mut observed, mut fees, now) = contexts(false);
    observed.pending_nonce = u64::MAX - 1;
    fees.nonce = u64::MAX - 1;
    let prepared = prepare(&bound, &pools, &[ROUTER.into()], &observed, &fees, now).unwrap();
    assert!(prepared.ready);
    assert_eq!(prepared.transaction.unwrap().nonce, u64::MAX - 1);
    assert!(prepared.unsigned_transaction.is_some());
}

#[test]
fn quantities_are_lossless_strings_and_rpc_uses_canonical_hex() {
    let (bound, pools, mut observed, mut fees, now) = contexts(false);
    fees.nonce = u64::MAX - 1;
    observed.pending_nonce = fees.nonce;
    fees.max_fee_per_gas = Amount::checked(num_bigint::BigUint::from(1u8) << 180usize).unwrap();
    fees.max_network_fee = Amount::checked(num_bigint::BigUint::from(1u8) << 220usize).unwrap();
    observed.native_balance = fees.max_network_fee.clone();
    let prepared = prepare(&bound, &pools, &[ROUTER.into()], &observed, &fees, now).unwrap();
    let transaction = prepared.transaction.as_ref().unwrap();
    let value = serde_json::to_value(transaction).unwrap();
    assert_eq!(value["nonce"], (u64::MAX - 1).to_string());
    assert_eq!(value["chain_id"], "31337");
    assert_eq!(value["gas_limit"], "500000");
    let decoded: xone_execution_preparation::evm::Eip1559Transaction =
        serde_json::from_value(value).unwrap();
    assert_eq!(decoded.nonce, u64::MAX - 1);
    let rpc = prepared.rpc_transaction.as_ref().unwrap();
    assert_eq!(rpc["type"], "0x2");
    assert_eq!(rpc["chainId"], "0x7a69");
    assert_eq!(rpc["nonce"], "0xfffffffffffffffe");
    assert_eq!(rpc["gas"], "0x7a120");
    assert_eq!(rpc["maxFeePerGas"], format!("0x1{}", "0".repeat(45)));
    assert_eq!(rpc["value"], "0x0");
    assert_eq!(rpc["accessList"], serde_json::json!([]));
    assert_eq!(*rpc, decoded.rpc_parameters());
    let observation = serde_json::to_value(&observed).unwrap();
    assert_eq!(observation["pending_nonce"], (u64::MAX - 1).to_string());
    let decoded: FundingObservation = serde_json::from_value(observation).unwrap();
    assert_eq!(decoded.pending_nonce, u64::MAX - 1);
}

#[test]
fn quantity_json_rejects_numbers_overflow_and_noncanonical_strings() {
    let (_, _, _, fees, _) = contexts(false);
    let value = serde_json::to_value(fees).unwrap();
    for field in ["chain_id", "nonce", "gas_limit"] {
        for invalid in [
            serde_json::json!(1),
            serde_json::json!("01"),
            serde_json::json!("-1"),
            serde_json::json!("+1"),
            serde_json::json!("1.0"),
            serde_json::json!(""),
            serde_json::json!("18446744073709551616"),
        ] {
            let mut changed = value.clone();
            changed[field] = invalid;
            assert!(serde_json::from_value::<FeeContext>(changed).is_err());
        }
    }
}
