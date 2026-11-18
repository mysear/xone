use super::*;
use revm::primitives::*;
use serde_json::Value;
use std::str::FromStr;
fn vectors() -> Value {
    serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_KARST_EXECUTION_VECTORS").unwrap()).unwrap(),
    )
    .unwrap()
}
fn number(s: &str) -> U256 {
    U256::from_str(s).unwrap()
}
fn setup(s: &Value) -> (Arc<StateImage>, TransactionRequest, Vec<u8>) {
    let mut d = super::super::ImageData::default();
    for a in s["fixture"]["accounts"].as_array().unwrap() {
        let owner: Address = a["address"].as_str().unwrap().parse().unwrap();
        let bytes = hex::decode(&a["code"].as_str().unwrap()[2..]).unwrap();
        let hash = keccak256(&bytes);
        d.bytecodes.insert(hash, bytes.into());
        d.accounts.insert(
            owner,
            Some(AccountInfo {
                balance: number(a["balance"].as_str().unwrap()),
                nonce: a["nonce"].as_u64().unwrap(),
                code_hash: hash,
                code: None,
            }),
        );
        for entry in a["storage"].as_array().unwrap() {
            d.storage.insert(
                (owner, number(entry["slot"].as_str().unwrap())),
                number(entry["value"].as_str().unwrap()),
            );
        }
    }
    let wire = hex::decode(s["signed_wire"].as_str().unwrap().trim_start_matches("0x")).unwrap();
    let tx = crate::op_transaction::authenticate_user(
        10,
        s["transaction_hash"].as_str().unwrap().parse().unwrap(),
        s["caller"].as_str().unwrap().parse().unwrap(),
        &wire,
        wire.len(),
    )
    .unwrap()
    .into_environment();
    let e = &s["fixture"]["execution_environment"];
    let block = BlockEnv {
        number: U256::from(e["block_number"].as_u64().unwrap()),
        timestamp: U256::from(e["timestamp"].as_u64().unwrap()),
        coinbase: e["coinbase"].as_str().unwrap().parse().unwrap(),
        gas_limit: U256::from(e["gas_limit"].as_u64().unwrap()),
        basefee: number(e["base_fee"].as_str().unwrap()),
        prevrandao: Some(e["prevrandao"].as_str().unwrap().parse().unwrap()),
        blob_excess_gas_and_price: Some(BlobExcessGasAndPrice::new(0, true)),
        ..Default::default()
    };
    (
        Arc::new(StateImage::new(d).unwrap()),
        TransactionRequest {
            fork: OpFork::Karst.spec(),
            chain_id: 10,
            block,
            tx,
        },
        hex::decode(s["attributes"].as_str().unwrap()).unwrap(),
    )
}
fn limits() -> ExecutionLimits {
    ExecutionLimits {
        reads: super::super::BoundReadLimits {
            max_reads: 8192,
            max_bytes: 16 * 1024 * 1024,
        },
        max_gas_limit: 30_000_000,
        max_instructions: 100_000,
        max_input_bytes: 128 * 1024,
        max_memory_bytes: 65536,
    }
}
#[test]
#[ignore = "requires fixed complete original Karst signed transitions"]
fn karst_execution_components_match_full_original_signed_gas_fees_and_poststate() {
    let v = vectors();
    assert_eq!(v["passed"], true);
    assert_eq!(v["current_vm_admitted"], false);
    let inputs = v["inputs"].as_array().unwrap();
    let rows = v["cases"].as_array().unwrap();
    assert_eq!(inputs.len(), 468);
    assert_eq!(rows.len(), 468);
    let (mut accepted, mut rejected) = (0, 0);
    for (s, r) in inputs.iter().zip(rows) {
        let (image, q, attributes) = setup(s);
        let tx = q.tx.clone();
        let block = q.block.clone();
        // Only schedule admission is bypassed here. Signed authentication, strict
        // bank/funding, the actual product engine, all handlers and controls run.
        let out = execute_mode(
            image.clone(),
            q,
            limits(),
            ExecutionControl::default(),
            false,
            Some((&attributes, OpFork::Karst)),
        );
        if r["accepted"] == false {
            let error = out.unwrap_err();
            if s["gas_boundary"]
                .as_u64()
                .is_some_and(|g| g > super::super::karst_execution::TX_GAS_LIMIT_CAP)
            {
                assert_eq!(error.reason(), "gas_limit");
                assert_eq!(error.telemetry.reads.reads, 0);
            } else {
                assert_eq!(error.reason(), "op_execution_context");
            }
            rejected += 1;
            continue;
        }
        let out = out.unwrap_or_else(|e| panic!("{}: {e}", s["id"]));
        let result = &out.result_and_state.result;
        assert_eq!(
            result.gas_used(),
            r["gas_used"].as_u64().unwrap(),
            "{}",
            s["id"]
        );
        assert_eq!(
            result.is_success(),
            !r["failed"].as_bool().unwrap(),
            "{}",
            s["id"]
        );
        assert_eq!(
            hex::encode(result.output().cloned().unwrap_or_default()),
            r["output"].as_str().unwrap(),
            "{}",
            s["id"]
        );
        let post = image.apply(&out.result_and_state.state).unwrap();
        for (owner, a) in r["accounts"].as_object().unwrap() {
            let owner: Address = owner.parse().unwrap();
            let actual = post.data().accounts[&owner].as_ref().unwrap();
            assert_eq!(
                actual.balance.to_string(),
                a["balance"].as_str().unwrap(),
                "{} {owner}",
                s["id"]
            );
            assert_eq!(
                actual.nonce,
                a["nonce"].as_u64().unwrap(),
                "{} {owner}",
                s["id"]
            );
            assert_eq!(
                actual.code_hash,
                keccak256(hex::decode(&a["code"].as_str().unwrap()[2..]).unwrap()),
                "{} {owner}",
                s["id"]
            );
            for (slot, value) in a["storage"].as_object().unwrap() {
                assert_eq!(
                    post.data().storage[&(owner, number(slot))],
                    number(value.as_str().unwrap()),
                    "{} {owner} {slot}",
                    s["id"]
                );
            }
        }
        let (cost, payer) = super::super::op_fees::audit(
            &image,
            &out,
            &tx,
            block.coinbase,
            block.basefee,
            block.timestamp.to::<u64>(),
            OpFork::Karst,
            &attributes,
        )
        .unwrap_or_else(|e| panic!("{} {e:?}", s["id"]));
        let cost = serde_json::to_value(cost).unwrap();
        assert_eq!(cost["l1_data_fee"], r["l1_fee"], "{}", s["id"]);
        assert_eq!(cost["operator_fee"], r["operator_fee"], "{}", s["id"]);
        if let Some(maximum) = s.get("maximum_funding") {
            assert_eq!(cost["maximum_funding"], *maximum, "{}", s["id"]);
        }
        assert_eq!(
            payer.net_native_debit.to_string(),
            (image.data().accounts[&tx.caller].as_ref().unwrap().balance
                - post.data().accounts[&tx.caller].as_ref().unwrap().balance)
                .to_string()
        );
        accepted += 1;
    }
    assert_eq!((accepted, rejected), (401, 67));
}
#[test]
#[ignore = "requires fixed complete original Karst signed transitions"]
fn karst_clz_never_bypasses_instruction_control_or_transaction_cap() {
    let v = vectors();
    let s = v["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "clz-loop")
        .unwrap();
    for maximum in [1, 2, 3, 4, 5, 8, 16, 32] {
        let (image, q, a) = setup(s);
        let mut budget = limits();
        budget.max_instructions = maximum;
        let error = execute_mode(
            image,
            q,
            budget,
            ExecutionControl::default(),
            false,
            Some((&a, OpFork::Karst)),
        )
        .unwrap_err();
        assert_eq!(error.reason(), "instruction_limit");
        assert_eq!(error.telemetry.instructions, maximum);
    }
    let (image, q, a) = setup(s);
    let control = ExecutionControl::default();
    control
        .cancellation
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        execute_mode(
            image,
            q,
            limits(),
            control,
            false,
            Some((&a, OpFork::Karst))
        )
        .unwrap_err()
        .reason(),
        "cancelled"
    );
    let (image, q, a) = setup(s);
    let control = ExecutionControl {
        deadline: Some(std::time::Instant::now()),
        ..Default::default()
    };
    assert_eq!(
        execute_mode(
            image,
            q,
            limits(),
            control,
            false,
            Some((&a, OpFork::Karst))
        )
        .unwrap_err()
        .reason(),
        "deadline"
    );
    // The activation block remains outside ordinary-user execution.
    let (image, q, a) = setup(s);
    let wire = hex::decode(s["signed_wire"].as_str().unwrap().trim_start_matches("0x")).unwrap();
    let transaction = crate::op_transaction::authenticate_user(
        10,
        s["transaction_hash"].as_str().unwrap().parse().unwrap(),
        q.tx.caller,
        &wire,
        wire.len(),
    )
    .unwrap();
    assert_eq!(
        execute_op_user(
            image,
            OpExecutionRequest {
                fork: OpFork::Karst,
                block: BlockEnv {
                    timestamp: U256::from(1_783_526_401u64),
                    ..q.block
                },
                transaction,
                attributes_calldata: a
            },
            limits(),
            ExecutionControl::default()
        )
        .unwrap_err()
        .reason(),
        "op_execution_context"
    );
}

fn public_request(s: &Value, block: BlockEnv) -> OpExecutionRequest {
    let wire = hex::decode(s["signed_wire"].as_str().unwrap().trim_start_matches("0x")).unwrap();
    OpExecutionRequest {
        fork: OpFork::Karst,
        block,
        transaction: crate::op_transaction::authenticate_user(
            10,
            s["transaction_hash"].as_str().unwrap().parse().unwrap(),
            s["caller"].as_str().unwrap().parse().unwrap(),
            &wire,
            wire.len(),
        )
        .unwrap(),
        attributes_calldata: hex::decode(s["attributes"].as_str().unwrap()).unwrap(),
    }
}
#[test]
#[ignore = "requires full original Karst signed transitions and schedule qualification"]
fn karst_public_signed_execution_and_independent_parent_fee_audit_match_original() {
    let v = vectors();
    let (mut accepted, mut rejected) = (0, 0);
    for (s, r) in v["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(v["cases"].as_array().unwrap())
    {
        let (image, q, _) = setup(s);
        let request = public_request(s, q.block);
        let again = public_request(s, request.block.clone());
        let result = execute_op_user(
            image.clone(),
            request,
            limits(),
            ExecutionControl::default(),
        );
        if r["accepted"] == false {
            let error = result.unwrap_err();
            assert!(matches!(
                error.reason(),
                "gas_limit" | "op_execution_context"
            ));
            rejected += 1;
            continue;
        }
        let out = result.unwrap_or_else(|e| panic!("{}: {e}", s["id"]));
        assert_eq!(
            out.execution.result_and_state.result.gas_used(),
            r["gas_used"].as_u64().unwrap()
        );
        assert_eq!(
            out.execution.result_and_state.result.is_success(),
            !r["failed"].as_bool().unwrap()
        );
        let cost = serde_json::to_value(&out.costs).unwrap();
        assert_eq!(cost["l1_data_fee"], r["l1_fee"]);
        assert_eq!(cost["operator_fee"], r["operator_fee"]);
        let mut accounts = serde_json::Map::new();
        for owner in
            super::super::op_fees::accounts(again.transaction.environment(), again.block.coinbase)
        {
            let before = image.data().accounts[&owner]
                .as_ref()
                .map_or(U256::ZERO, |a| a.balance);
            let after = out
                .execution
                .result_and_state
                .state
                .get(&owner)
                .map_or(before, |a| a.info.balance);
            assert_eq!(
                after.to_string(),
                r["accounts"][format!("{owner:#x}")]["balance"]
                    .as_str()
                    .unwrap()
            );
            accounts.insert(
                format!("{owner:#x}"),
                serde_json::json!({"before":before.to_string(),"after":after.to_string()}),
            );
        }
        let report = serde_json::json!({"transaction_hash":format!("{:#x}",again.transaction.transaction_hash()),"transaction_type":again.transaction.transaction_type(),"chain_cost":cost,"payer_cost":out.payer_cost,"accounts":accounts});
        let audit = super::super::audit_op_fee_measurements(
            image,
            &again,
            r["gas_used"].as_u64().unwrap(),
            &report,
            limits(),
            ExecutionControl::default(),
        )
        .unwrap_or_else(|e| panic!("{} audit: {e}", s["id"]));
        assert_eq!(
            serde_json::to_value(audit.costs).unwrap(),
            report["chain_cost"]
        );
        assert_eq!(
            serde_json::to_value(audit.payer_cost).unwrap(),
            report["payer_cost"]
        );
        assert!(audit.telemetry.reads.reads > 0);
        accepted += 1;
    }
    assert_eq!((accepted, rejected), (401, 67));
}
#[test]
#[ignore = "requires original Karst sources"]
fn karst_schedule_width_and_parent_audit_cap_fail_before_source_reads() {
    let v = vectors();
    let s = &v["inputs"][0];
    for time in [
        1_783_526_400u64,
        1_783_526_401,
        1_799_683_201,
        1_799_683_202,
    ] {
        let (image, q, _) = setup(s);
        let mut request = public_request(s, q.block);
        request.block.timestamp = U256::from(time);
        let error =
            execute_op_user(image, request, limits(), ExecutionControl::default()).unwrap_err();
        assert_eq!(error.reason(), "op_execution_context");
        assert_eq!(error.telemetry.reads.reads, 0);
    }
    for field in 0..4 {
        let (image, mut q, a) = setup(s);
        match field {
            0 => q.block.basefee = U256::from(u64::MAX) + U256::from(1),
            1 => q.block.gas_limit = U256::from(u64::MAX) + U256::from(1),
            2 => q.tx.gas_price = U256::from(u128::MAX) + U256::from(1),
            _ => q.tx.gas_priority_fee = Some(U256::from(u128::MAX) + U256::from(1)),
        }
        let error = execute_mode(
            image,
            q,
            limits(),
            ExecutionControl::default(),
            false,
            Some((&a, OpFork::Karst)),
        )
        .unwrap_err();
        assert_eq!(error.reason(), "op_execution_context");
        assert_eq!(error.telemetry.reads.reads, 0);
    }
    let s = v["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "cap-0-16777217")
        .unwrap();
    let (image, q, _) = setup(s);
    let request = public_request(s, q.block);
    let report = serde_json::json!({"transaction_hash":format!("{:#x}",request.transaction.transaction_hash()),"transaction_type":request.transaction.transaction_type(),"chain_cost":{},"payer_cost":{},"accounts":{}});
    let error = super::super::audit_op_fee_measurements(
        image,
        &request,
        21000,
        &report,
        limits(),
        ExecutionControl::default(),
    )
    .unwrap_err();
    assert_eq!(error.reason(), "gas_limit");
    assert_eq!(error.telemetry.reads.reads, 0);
}
