//! Reuse the complete original-transition assertions without changing legacy matrices.
#[path = "op_execution.rs"]
mod execution;
use revm::primitives::*;
use serde_json::Value;
use std::sync::Arc;
use xone_simulation::revm_backend::*;
fn vectors() -> Value {
    serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_JOVIAN_VECTORS").unwrap()).unwrap(),
    )
    .unwrap()
}
#[test]
#[ignore = "requires original historical Jovian signed transitions"]
fn jovian_signed_transitions_funding_refunds_aliases_and_source_fee_audit_match_original() {
    let v = vectors();
    assert_eq!(v["cases"].as_array().unwrap().len(), 64);
    assert!(v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["input"]["rule"] == "jovian" && r["input"]["chain"] == 10));
    execution::check_original_execution(&v, 1, 128, 63);
    execution::check_original_fee_audit(&v, 128);
}
#[test]
#[ignore = "requires original historical Jovian signed transitions"]
fn jovian_source_units_fork_boundaries_and_rehashed_fee_substitutions_fail_closed() {
    let v = vectors();
    let r = &v["cases"][0];
    for timestamp in [1_764_691_200, 1_764_691_201, 1_783_526_401, 1_799_999_999] {
        let (d, mut q) = execution::setup(r, "actual");
        q.block.timestamp = U256::from(timestamp as u64);
        assert_eq!(
            execution::run(d, q).unwrap_err().reason(),
            "op_execution_context"
        );
    }
    for which in 0..9 {
        let (mut d, mut q) = execution::setup(r, "actual");
        match which {
            0 => q.attributes_calldata[0] ^= 1,
            1 => {
                q.attributes_calldata.pop();
            }
            2 => q.attributes_calldata.push(0),
            3 => q.attributes_calldata[164] ^= 1,
            4 => q.fork = OpFork::Isthmus,
            5 => {
                d.storage.remove(&(
                    "0x4200000000000000000000000000000000000015"
                        .parse()
                        .unwrap(),
                    U256::from(8),
                ));
            }
            6 => q.attributes_calldata[175] ^= 1,
            7 => {
                d.accounts
                    .get_mut(&q.transaction.environment().caller)
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .nonce += 1;
            }
            _ => {
                d.accounts
                    .get_mut(&q.transaction.environment().caller)
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .balance = r["maximum_funding"]
                    .as_str()
                    .unwrap()
                    .parse::<U256>()
                    .unwrap()
                    - U256::from(1);
            }
        }
        assert!(execution::run(d, q).is_err(), "{which}");
    }
    let (d, q) = execution::setup(r, "actual");
    let out = execution::run(d, q).unwrap();
    let report = execution::fee_report(r, "actual", &out);
    for key in report["chain_cost"].as_object().unwrap().keys() {
        let mut changed = report.clone();
        changed["chain_cost"][key] = "0".into();
        if changed == report {
            changed["chain_cost"][key] = "1".into();
        }
        let (d, q) = execution::setup(r, "actual");
        audit_op_fee_measurements(
            Arc::new(StateImage::new(d).unwrap()),
            &q,
            r["actual"]["gas_used"].as_u64().unwrap(),
            &changed,
            execution::limits(),
            ExecutionControl::default(),
        )
        .unwrap_err();
    }
}
