use revm::primitives::*;
use serde_json::Value;
use std::{
    str::FromStr,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use xone_simulation::{op_transaction::authenticate_user, revm_backend::*};
fn addr(s: &str) -> Address {
    Address::from_str(s).unwrap()
}
fn num(s: &str) -> U256 {
    U256::from_str(s).unwrap()
}
fn target() -> Address {
    addr("0x1000000000000000000000000000000000000001")
}
fn l1() -> Address {
    addr("0x4200000000000000000000000000000000000015")
}
fn reference() -> Value {
    serde_json::from_slice(
        &std::fs::read(
            std::env::var("XONE_OP_EXECUTION_VECTORS")
                .expect("original execution reference required"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn fork(rule: &str) -> OpFork {
    match rule {
        "regolith" => OpFork::Regolith,
        "canyon" => OpFork::Canyon,
        "ecotone" => OpFork::Ecotone,
        "fjord" => OpFork::Fjord,
        "granite" => OpFork::Granite,
        "holocene" => OpFork::Holocene,
        "isthmus" => OpFork::Isthmus,
        _ => panic!(),
    }
}
fn setup(r: &Value, funding: &str) -> (ImageData, OpExecutionRequest) {
    let s = &r["input"];
    let a = hex::decode(s["attributes"].as_str().unwrap()).unwrap();
    let chain = s["chain"].as_u64().unwrap();
    let mut d = ImageData::default();
    for (address, n) in r[funding]["before"].as_object().unwrap() {
        d.accounts.insert(
            addr(address),
            Some(AccountInfo {
                balance: num(n.as_str().unwrap()),
                code: None,
                ..Default::default()
            }),
        );
    }
    let outcome = s["outcome"].as_str().unwrap();
    let code = hex::decode(match outcome {
        "success" => "00",
        "refund" => "600060005500",
        "revert" => "600160005560006000fd",
        "halt" => "6001600055fe",
        _ => panic!(),
    })
    .unwrap();
    let hash = keccak256(&code);
    d.bytecodes.insert(hash, Bytes::from(code));
    d.accounts.insert(
        target(),
        Some(AccountInfo {
            code_hash: hash,
            code: None,
            ..Default::default()
        }),
    );
    d.storage.insert(
        (target(), U256::ZERO),
        U256::from(u64::from(outcome == "refund")),
    );
    d.accounts.insert(
        l1(),
        Some(AccountInfo {
            code: None,
            ..Default::default()
        }),
    );
    if matches!(s["rule"].as_str().unwrap(), "regolith" | "canyon") {
        for (slot, start) in [(1, 68), (5, 196), (6, 228)] {
            d.storage.insert(
                (l1(), U256::from(slot)),
                U256::from_be_slice(&a[start..start + 32]),
            );
        }
    } else {
        d.storage
            .insert((l1(), U256::from(1)), U256::from_be_slice(&a[36..68]));
        d.storage
            .insert((l1(), U256::from(7)), U256::from_be_slice(&a[68..100]));
        let mut scalars = [0; 32];
        scalars[16..24].copy_from_slice(&a[4..12]);
        d.storage
            .insert((l1(), U256::from(3)), U256::from_be_bytes(scalars));
        if a.len() >= 176 {
            let mut op = [0; 32];
            op[20..32].copy_from_slice(&a[164..176]);
            d.storage
                .insert((l1(), U256::from(8)), U256::from_be_bytes(op));
        }
    }
    let wire = hex::decode(r["signed_wire"].as_str().unwrap()).unwrap();
    let transaction = authenticate_user(
        chain,
        B256::from_str(r["transaction_hash"].as_str().unwrap()).unwrap(),
        addr(r["payer"].as_str().unwrap()),
        &wire,
        wire.len(),
    )
    .unwrap();
    let block = BlockEnv {
        number: U256::from(100),
        timestamp: U256::from(s["timestamp"].as_u64().unwrap()),
        coinbase: addr(r["beneficiary"].as_str().unwrap()),
        gas_limit: U256::from(30000000),
        basefee: num(s["base_fee"].as_str().unwrap()),
        prevrandao: Some(B256::ZERO),
        blob_excess_gas_and_price: Some(BlobExcessGasAndPrice::new(0, false)),
        ..Default::default()
    };
    (
        d,
        OpExecutionRequest {
            fork: fork(s["rule"].as_str().unwrap()),
            block,
            transaction,
            attributes_calldata: a,
        },
    )
}
fn limits() -> ExecutionLimits {
    ExecutionLimits {
        reads: BoundReadLimits {
            max_reads: 1000,
            max_bytes: 1000000,
        },
        max_gas_limit: 1000000,
        max_instructions: 10000,
        max_input_bytes: 100000,
        max_memory_bytes: 65536,
    }
}
fn run(d: ImageData, r: OpExecutionRequest) -> Result<OpExecutionOutput, ExecutionError> {
    execute_op_user(
        Arc::new(StateImage::new(d).unwrap()),
        r,
        limits(),
        ExecutionControl::default(),
    )
}
#[test]
#[ignore = "requires full original signed OP transitions"]
fn authenticated_strict_op_execution_matches_original_gas_accounts_nonce_and_storage() {
    let v = reference();
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let mut accepted = 0;
    let mut rejected = 0;
    for r in v["cases"].as_array().unwrap() {
        *counts
            .entry(r["input"]["rule"].as_str().unwrap().into())
            .or_default() += 1;
        for funding in ["actual", "exact_funding"] {
            let (d, q) = setup(r, funding);
            let image = Arc::new(StateImage::new(d.clone()).unwrap());
            let out = execute_op_user(image.clone(), q, limits(), ExecutionControl::default())
                .unwrap_or_else(|e| panic!("{} {funding}: {e}", r["input"]["id"]));
            let actual = out.execution.result_and_state;
            assert_eq!(
                actual.result.gas_used(),
                r[funding]["gas_used"].as_u64().unwrap()
            );
            assert_ne!(
                actual.result.is_success(),
                r[funding]["failed"].as_bool().unwrap()
            );
            let payer = addr(r["payer"].as_str().unwrap());
            assert_eq!(actual.state[&payer].info.nonce, 1);
            for (a, n) in r[funding]["after"].as_object().unwrap() {
                let address = addr(a);
                let balance = actual
                    .state
                    .get(&address)
                    .map_or(d.accounts[&address].as_ref().unwrap().balance, |a| {
                        a.info.balance
                    });
                assert_eq!(
                    balance,
                    num(n.as_str().unwrap()),
                    "{} {a}",
                    r["input"]["id"]
                );
            }
            let slot = actual
                .state
                .get(&target())
                .and_then(|a| a.storage.get(&U256::ZERO))
                .map_or(d.storage[&(target(), U256::ZERO)], |s| s.present_value());
            assert_eq!(slot, U256::ZERO);
            assert_eq!(
                out.costs.maximum_funding().to_string(),
                r["maximum_funding"].as_str().unwrap()
            );
            assert_eq!(
                out.payer_cost.net_native_debit.to_string(),
                (num(r[funding]["before"][r["payer"].as_str().unwrap()]
                    .as_str()
                    .unwrap())
                    - num(r[funding]["after"][r["payer"].as_str().unwrap()]
                        .as_str()
                        .unwrap()))
                .to_string()
            );
            assert_eq!(image.data(), &d);
            assert!(out.execution.telemetry.reads.storage >= 3);
            accepted += 1;
        }
        if !r["below_funding"].is_null() {
            let (d, q) = setup(r, "below_funding");
            assert_eq!(run(d, q).unwrap_err().reason(), "op_execution_context");
            rejected += 1;
        }
    }
    assert_eq!(counts.len(), 7);
    assert_eq!(accepted, 896);
    assert_eq!(rejected, 441);
}
#[test]
#[ignore = "requires full original signed OP transitions"]
fn strict_op_context_missing_state_fee_binding_and_resource_guards_fail_closed() {
    let v = reference();
    let rows = v["cases"].as_array().unwrap();
    let mut checks = 0;
    for rule in [
        "regolith", "canyon", "ecotone", "fjord", "granite", "holocene", "isthmus",
    ] {
        let r = rows
            .iter()
            .find(|r| r["input"]["rule"] == rule && r["input"]["outcome"] == "refund")
            .unwrap();
        let (d, _) = setup(r, "actual");
        for key in d.storage.keys() {
            let (mut changed, q) = setup(r, "actual");
            changed.storage.remove(key);
            assert_eq!(run(changed, q).unwrap_err().reason(), "unknown_storage");
            checks += 1;
        }
        for key in d.accounts.keys() {
            let (mut changed, q) = setup(r, "actual");
            changed.accounts.remove(key);
            changed.storage.retain(|(a, _), _| a != key);
            assert_eq!(run(changed, q).unwrap_err().reason(), "unknown_account");
            checks += 1;
        }
        let (mut changed, q) = setup(r, "actual");
        changed.bytecodes.clear();
        assert_eq!(run(changed, q).unwrap_err().reason(), "unknown_code");
        for slot in d.storage.keys().filter(|(a, _)| *a == l1()) {
            let (mut changed, q) = setup(r, "actual");
            *changed.storage.get_mut(slot).unwrap() ^=
                U256::from(1) << if slot.1 == U256::from(3) { 64 } else { 0 };
            assert_eq!(
                run(changed, q).unwrap_err().reason(),
                "op_execution_context"
            );
        }
        for time in [
            0, 1704992401, 1710374401, 1720627201, 1726070401, 1736445601, 1746806401, 1764691201,
            1794993587,
        ] {
            let (d, mut q) = setup(r, "actual");
            q.block.timestamp = U256::from(time as u64);
            assert_eq!(run(d, q).unwrap_err().reason(), "op_execution_context");
        }
        let (d, mut q) = setup(r, "actual");
        q.attributes_calldata[0] ^= 1;
        assert_eq!(run(d, q).unwrap_err().reason(), "op_execution_context");
        for (which, expected) in [
            (0, "read_limit"),
            (1, "byte_limit"),
            (2, "instruction_limit"),
            (3, "input_limit"),
            (4, "gas_limit"),
            (5, "invalid_memory_limit"),
        ] {
            let (d, q) = setup(r, "actual");
            let mut l = limits();
            match which {
                0 => l.reads.max_reads = 0,
                1 => l.reads.max_bytes = 0,
                2 => l.max_instructions = 0,
                3 => l.max_input_bytes = 0,
                4 => l.max_gas_limit = 1,
                _ => l.max_memory_bytes = 0,
            };
            assert_eq!(
                execute_op_user(
                    Arc::new(StateImage::new(d).unwrap()),
                    q,
                    l,
                    ExecutionControl::default()
                )
                .unwrap_err()
                .reason(),
                expected
            );
        }
        for cancelled in [true, false] {
            let (d, q) = setup(r, "actual");
            let mut c = ExecutionControl::default();
            if cancelled {
                c.cancellation.store(true, Ordering::Release);
            } else {
                c.deadline = Some(Instant::now() - Duration::from_secs(1));
            }
            assert_eq!(
                execute_op_user(Arc::new(StateImage::new(d).unwrap()), q, limits(), c)
                    .unwrap_err()
                    .reason(),
                if cancelled { "cancelled" } else { "deadline" }
            );
        }
        let (d, q) = setup(r, "actual");
        let main = TransactionRequest {
            fork: SpecId::SHANGHAI,
            chain_id: q.transaction.environment().chain_id.unwrap(),
            block: q.block,
            tx: q.transaction.environment().clone(),
        };
        assert_eq!(
            execute(
                Arc::new(StateImage::new(d).unwrap()),
                main,
                limits(),
                ExecutionControl::default()
            )
            .unwrap_err()
            .reason(),
            "op_execution_context"
        );
    }
    assert!(checks > 70);
}
#[path = "support/op_user.rs"]
mod support;
#[test]
#[ignore = "requires full original signed OP transitions"]
fn op_native_value_creation_and_internal_transfers_stay_outside_fee_only_scope() {
    let v = reference();
    let r = &v["cases"][0];
    for create in [false, true] {
        let (d, mut q) = setup(r, "actual");
        let mut f = support::fields(2);
        if create {
            f[5] = vec![0x80];
        } else {
            f[6] = support::number(U256::from(1));
        }
        let wire = support::signed(2, &f);
        q.transaction = authenticate_user(
            10,
            support::hash(&wire),
            support::payer(),
            &wire,
            wire.len(),
        )
        .unwrap();
        assert_eq!(run(d, q).unwrap_err().reason(), "op_execution_context");
    }
    // A zero-value outer transaction still cannot hide a contract-funded CALL.
    let (mut d, q) = setup(r, "actual");
    let code =
        hex::decode("6000600060006000600173111111111111111111111111111111111111111161fffff100")
            .unwrap();
    let hash = keccak256(&code);
    d.bytecodes.insert(hash, Bytes::from(code));
    let target_info = d.accounts.get_mut(&target()).unwrap().as_mut().unwrap();
    target_info.balance = U256::from(100);
    target_info.code_hash = hash;
    d.accounts.insert(
        Address::repeat_byte(0x11),
        Some(AccountInfo {
            balance: U256::from(1),
            code: None,
            ..Default::default()
        }),
    );
    assert_eq!(run(d, q).unwrap_err().reason(), "op_execution_context");
}
#[test]
#[ignore = "requires full original signed OP transitions"]
fn op_runtime_memory_instructions_and_cooperative_cancellation_are_fatal() {
    let v = reference();
    let r = &v["cases"][0];
    let replace = |code: &[u8]| {
        let (mut d, q) = setup(r, "actual");
        let hash = keccak256(code);
        d.bytecodes.insert(hash, Bytes::copy_from_slice(code));
        d.accounts
            .get_mut(&target())
            .unwrap()
            .as_mut()
            .unwrap()
            .code_hash = hash;
        (Arc::new(StateImage::new(d).unwrap()), q)
    };
    let (d, q) = replace(&[0x60, 1, 0x61, 0x10, 0, 0x52, 0]);
    let mut l = limits();
    l.max_memory_bytes = 1024;
    assert_eq!(
        execute_op_user(d, q, l, ExecutionControl::default())
            .unwrap_err()
            .reason(),
        "memory_limit"
    );
    let (d, q) = replace(&[0x5b, 0x60, 0, 0x56]);
    let mut l = limits();
    l.max_instructions = 10;
    assert_eq!(
        execute_op_user(d, q, l, ExecutionControl::default())
            .unwrap_err()
            .reason(),
        "instruction_limit"
    );
    let (d, q) = replace(&[0x5b, 0x60, 0, 0x56]);
    let c = ExecutionControl {
        progress: Some(Arc::new(std::sync::atomic::AtomicU64::new(0))),
        ..Default::default()
    };
    let shared = c.clone();
    let mut l = limits();
    l.max_instructions = 1000000;
    let task = std::thread::spawn(move || execute_op_user(d, q, l, c));
    let deadline = Instant::now() + Duration::from_secs(5);
    while shared.progress.as_ref().unwrap().load(Ordering::Acquire) == 0 && !task.is_finished() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    shared.cancellation.store(true, Ordering::Release);
    assert_eq!(task.join().unwrap().unwrap_err().reason(), "cancelled");
}

fn fee_report(r: &Value, funding: &str, out: &OpExecutionOutput) -> Value {
    let mut accounts = serde_json::Map::new();
    for owner in r[funding]["before"].as_object().unwrap().keys() {
        accounts.insert(
            owner.to_lowercase(),
            serde_json::json!({"before":r[funding]["before"][owner],"after":r[funding]["after"][owner]}),
        );
    }
    serde_json::json!({
        "transaction_hash":r["transaction_hash"],"transaction_type":match r["input"]["kind"].as_str().unwrap() {"legacy"=>0,"access"=>1,"dynamic"=>2,_=>panic!()},
        "chain_cost":out.costs,"payer_cost":out.payer_cost,"accounts":accounts
    })
}

#[test]
#[ignore = "requires full original signed OP transitions"]
fn retained_signed_op_fee_audit_matches_original_roles_aliases_funding_and_refunds() {
    let v = reference();
    let mut count = 0;
    for r in v["cases"].as_array().unwrap() {
        for funding in ["actual", "exact_funding"] {
            let (d, q) = setup(r, funding);
            let image = Arc::new(StateImage::new(d.clone()).unwrap());
            let execution = run(d.clone(), q).unwrap();
            let (_, q) = setup(r, funding);
            let report = fee_report(r, funding, &execution);
            let audited = audit_op_fee_measurements(
                image.clone(),
                &q,
                r[funding]["gas_used"].as_u64().unwrap(),
                &report,
                limits(),
                ExecutionControl::default(),
            )
            .unwrap_or_else(|e| panic!("{} {funding}: {e}; report={report}", r["input"]["id"]));
            assert_eq!(
                serde_json::to_value(audited.costs).unwrap(),
                report["chain_cost"]
            );
            assert_eq!(
                serde_json::to_value(audited.payer_cost).unwrap(),
                report["payer_cost"]
            );
            assert_eq!(audited.telemetry.reads.basic, 6);
            assert!(audited.telemetry.reads.storage >= 3);
            assert_eq!(audited.telemetry.instructions, 0);
            assert_eq!(audited.telemetry.calls, 0);
            assert_eq!(image.data(), &d);
            count += 1;
        }
    }
    assert_eq!(count, 896);
}

#[test]
#[ignore = "requires full original signed OP transitions"]
fn retained_op_fee_audit_rejects_report_source_and_signature_substitutions() {
    let v = reference();
    let mut checks = 0;
    for rule in [
        "regolith", "canyon", "ecotone", "fjord", "granite", "holocene", "isthmus",
    ] {
        let r = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["input"]["rule"] == rule && r["input"]["outcome"] == "refund")
            .unwrap();
        let (d, q) = setup(r, "actual");
        let report = fee_report(r, "actual", &run(d, q).unwrap());
        let gas = r["actual"]["gas_used"].as_u64().unwrap();
        let check = |value: &Value| {
            let (d, q) = setup(r, "actual");
            audit_op_fee_measurements(
                Arc::new(StateImage::new(d).unwrap()),
                &q,
                gas,
                value,
                limits(),
                ExecutionControl::default(),
            )
            .unwrap_err()
        };
        for group in ["chain_cost", "payer_cost"] {
            for (key, old) in report[group].as_object().unwrap() {
                let mut changed = report.clone();
                changed[group][key] =
                    if let Some(n) = old.as_str().and_then(|s| s.parse::<U256>().ok()) {
                        (n + U256::from(1)).to_string().into()
                    } else {
                        "changed".into()
                    };
                let e = check(&changed);
                assert_eq!(e.reason(), "op_execution_context");
                assert!(e.telemetry.reads.reads >= 9);
                checks += 1;
                let mut missing = report.clone();
                missing[group].as_object_mut().unwrap().remove(key);
                check(&missing);
                checks += 1;
            }
            let mut unknown = report.clone();
            unknown[group]["unknown"] = "1".into();
            check(&unknown);
            checks += 1;
        }
        for (owner, balances) in report["accounts"].as_object().unwrap() {
            for field in ["before", "after"] {
                let mut changed = report.clone();
                changed["accounts"][owner][field] = (num(balances[field].as_str().unwrap())
                    + U256::from(1))
                .to_string()
                .into();
                check(&changed);
                checks += 1;
            }
            let mut changed = report.clone();
            for field in ["before", "after"] {
                changed["accounts"][owner][field] = (num(balances[field].as_str().unwrap())
                    + U256::from(1))
                .to_string()
                .into();
            }
            check(&changed);
            checks += 1;
            let mut missing = report.clone();
            missing["accounts"].as_object_mut().unwrap().remove(owner);
            check(&missing);
            checks += 1;
            let mut unknown = report.clone();
            unknown["accounts"][owner]["unknown"] = "1".into();
            check(&unknown);
            checks += 1;
            let mut numeric = report.clone();
            numeric["accounts"][owner]["before"] = 1.into();
            check(&numeric);
            checks += 1;
        }
        for (field, value) in [
            (
                "transaction_hash",
                Value::from(format!("0x{}", "00".repeat(32))),
            ),
            ("transaction_type", Value::from(99)),
            ("unknown", Value::from(1)),
        ] {
            let mut changed = report.clone();
            changed[field] = value;
            check(&changed);
            checks += 1;
        }
        let mut extra = report.clone();
        extra["accounts"][format!("0x{}", "99".repeat(20))] =
            serde_json::json!({"before":"0","after":"0"});
        check(&extra);
        checks += 1;
        for measured in [
            0,
            gas - 1,
            gas + 1,
            r["input"]["gas_limit"].as_u64().unwrap() + 1,
        ] {
            let (d, q) = setup(r, "actual");
            audit_op_fee_measurements(
                Arc::new(StateImage::new(d).unwrap()),
                &q,
                measured,
                &report,
                limits(),
                ExecutionControl::default(),
            )
            .unwrap_err();
            checks += 1;
        }
        let (d, _) = setup(r, "actual");
        for key in d.storage.keys().filter(|(a, _)| *a == l1()) {
            let (mut d, q) = setup(r, "actual");
            d.storage.remove(key);
            let e = audit_op_fee_measurements(
                Arc::new(StateImage::new(d).unwrap()),
                &q,
                gas,
                &report,
                limits(),
                ExecutionControl::default(),
            )
            .unwrap_err();
            assert_eq!(e.reason(), "unknown_storage");
            assert!(e.telemetry.reads.storage > 0);
            checks += 1;
        }
        for owner in report["accounts"].as_object().unwrap().keys() {
            for missing in [true, false] {
                let (mut changed, q) = setup(r, "actual");
                let owner = addr(owner);
                if missing {
                    changed.accounts.remove(&owner);
                    changed.storage.retain(|(a, _), _| *a != owner);
                } else {
                    changed
                        .accounts
                        .get_mut(&owner)
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .balance += U256::from(1);
                }
                audit_op_fee_measurements(
                    Arc::new(StateImage::new(changed).unwrap()),
                    &q,
                    gas,
                    &report,
                    limits(),
                    ExecutionControl::default(),
                )
                .unwrap_err();
                checks += 1;
            }
        }
        for kind in 0..7 {
            let (mut d, mut q) = setup(r, "actual");
            match kind {
                0 => {
                    d.accounts
                        .get_mut(&q.transaction.environment().caller)
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .nonce += 1;
                }
                1 => {
                    d.accounts
                        .get_mut(&q.transaction.environment().caller)
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .balance = U256::from_str(r["maximum_funding"].as_str().unwrap()).unwrap()
                        - U256::from(1);
                }
                2 => {
                    d.accounts.remove(&q.transaction.environment().caller);
                }
                3 => {
                    q.attributes_calldata[0] ^= 1;
                }
                4 => {
                    q.block.timestamp = U256::from(1_764_691_201);
                }
                5 => {
                    q.block.basefee += U256::from(1);
                }
                _ => {
                    let wire = hex::decode(
                        v["cases"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|c| {
                                c["transaction_hash"] != r["transaction_hash"]
                                    && c["input"]["chain"] == r["input"]["chain"]
                            })
                            .unwrap()["signed_wire"]
                            .as_str()
                            .unwrap(),
                    )
                    .unwrap();
                    q.transaction = authenticate_user(
                        q.transaction.environment().chain_id.unwrap(),
                        keccak256(&wire),
                        q.transaction.environment().caller,
                        &wire,
                        wire.len(),
                    )
                    .unwrap();
                }
            }
            audit_op_fee_measurements(
                Arc::new(StateImage::new(d).unwrap()),
                &q,
                gas,
                &report,
                limits(),
                ExecutionControl::default(),
            )
            .unwrap_err();
            checks += 1;
        }
    }
    assert!(checks > 650, "{checks}");
}

#[test]
#[ignore = "requires full original signed OP transitions"]
fn retained_op_fee_audit_charges_reads_preserves_failure_usage_and_honors_control() {
    let v = reference();
    let r = &v["cases"][0];
    let (d, q) = setup(r, "actual");
    let report = fee_report(r, "actual", &run(d, q).unwrap());
    let (d, q) = setup(r, "actual");
    let image = Arc::new(StateImage::new(d.clone()).unwrap());
    let gas = r["actual"]["gas_used"].as_u64().unwrap();
    let out = audit_op_fee_measurements(
        image.clone(),
        &q,
        gas,
        &report,
        limits(),
        ExecutionControl::default(),
    )
    .unwrap();
    let mut exact = limits();
    exact.reads.max_reads = out.telemetry.reads.reads;
    exact.reads.max_bytes = out.telemetry.reads.bytes;
    assert_eq!(
        audit_op_fee_measurements(
            image.clone(),
            &q,
            gas,
            &report,
            exact,
            ExecutionControl::default()
        )
        .unwrap()
        .telemetry,
        out.telemetry
    );
    for which in 0..4 {
        let mut l = exact;
        match which {
            0 => l.reads.max_reads -= 1,
            1 => l.reads.max_bytes -= 1,
            2 => l.max_gas_limit = 1,
            _ => l.max_input_bytes = 0,
        }
        let e = audit_op_fee_measurements(
            image.clone(),
            &q,
            gas,
            &report,
            l,
            ExecutionControl::default(),
        )
        .unwrap_err();
        assert_eq!(
            e.reason(),
            ["read_limit", "byte_limit", "gas_limit", "input_limit"][which]
        );
        if which < 2 {
            assert_eq!(e.telemetry.reads.reads, out.telemetry.reads.reads);
            assert_eq!(e.telemetry.reads.bytes, out.telemetry.reads.bytes);
        }
    }
    for stop in [true, false] {
        let mut c = ExecutionControl::default();
        if stop {
            c.cancellation.store(true, Ordering::Release);
        } else {
            c.deadline = Some(Instant::now() - Duration::from_secs(1));
        }
        let e = audit_op_fee_measurements(image.clone(), &q, gas, &report, exact, c).unwrap_err();
        assert_eq!(e.reason(), if stop { "cancelled" } else { "deadline" });
        assert_eq!(e.telemetry, Telemetry::default());
    }
    assert_eq!(image.data(), &d);
}
