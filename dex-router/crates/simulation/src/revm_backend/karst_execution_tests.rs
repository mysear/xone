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
    // Public execution remains closed until schedule and normal API qualification.
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
                block: q.block,
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
