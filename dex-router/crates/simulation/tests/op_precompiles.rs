//! Execute original signed transactions, not standalone crypto calls alone.
use revm::primitives::*;
use serde_json::Value;
use std::{str::FromStr, sync::Arc};
use xone_simulation::{op_transaction::authenticate_user, revm_backend::*};
fn address(s: &str) -> Address {
    s.parse().unwrap()
}
fn number(s: &str) -> U256 {
    U256::from_str(s).unwrap()
}
#[test]
#[ignore = "requires original signed OP precompile transitions"]
fn signed_op_precompiles_match_full_original_transitions_fees_and_fork_dispatch() {
    let v: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_PRECOMPILE_VECTORS").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(v["passed"], true);
    let mut count = 0;
    for row in v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r.get("transition").is_some())
    {
        let s = &row["input"];
        let original = &row["transition"];
        let a = hex::decode(s["attributes"].as_str().unwrap()).unwrap();
        let fork = match s["fork"].as_str().unwrap() {
            "regolith" => OpFork::Regolith,
            "canyon" => OpFork::Canyon,
            "ecotone" => OpFork::Ecotone,
            "fjord" => OpFork::Fjord,
            "granite" => OpFork::Granite,
            "holocene" => OpFork::Holocene,
            "isthmus" => OpFork::Isthmus,
            "jovian" => OpFork::Jovian,
            _ => panic!(),
        };
        let mut d = ImageData::default();
        for (addr, balance) in original["before"].as_object().unwrap() {
            d.accounts.insert(
                address(addr),
                Some(AccountInfo {
                    balance: number(balance.as_str().unwrap()),
                    code: None,
                    ..Default::default()
                }),
            );
        }
        let l1 = address("0x4200000000000000000000000000000000000015");
        d.accounts.insert(
            l1,
            Some(AccountInfo {
                code: None,
                ..Default::default()
            }),
        );
        if matches!(fork, OpFork::Regolith | OpFork::Canyon) {
            for (slot, start) in [(1, 68), (5, 196), (6, 228)] {
                d.storage.insert(
                    (l1, U256::from(slot)),
                    U256::from_be_slice(&a[start..start + 32]),
                );
            }
        } else {
            d.storage
                .insert((l1, U256::from(1)), U256::from_be_slice(&a[36..68]));
            d.storage
                .insert((l1, U256::from(7)), U256::from_be_slice(&a[68..100]));
            let mut scalars = [0; 32];
            scalars[16..24].copy_from_slice(&a[4..12]);
            d.storage
                .insert((l1, U256::from(3)), U256::from_be_bytes(scalars));
            if a.len() >= 176 {
                let mut op = [0; 32];
                op[20..32].copy_from_slice(&a[164..176]);
                d.storage
                    .insert((l1, U256::from(8)), U256::from_be_bytes(op));
            }
        }
        let wire = hex::decode(original["signed_wire"].as_str().unwrap()).unwrap();
        let tx = authenticate_user(
            10,
            original["transaction_hash"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            address(original["caller"].as_str().unwrap()),
            &wire,
            2_000_000,
        )
        .unwrap();
        let q = OpExecutionRequest {
            fork,
            transaction: tx,
            attributes_calldata: a,
            block: BlockEnv {
                number: U256::from(100),
                timestamp: U256::from(s["timestamp"].as_u64().unwrap()),
                coinbase: address(original["beneficiary"].as_str().unwrap()),
                basefee: U256::from(7),
                gas_limit: U256::from(60_000_000),
                prevrandao: Some(B256::ZERO),
                blob_excess_gas_and_price: Some(BlobExcessGasAndPrice::new(0, false)),
                ..Default::default()
            },
        };
        let limits = ExecutionLimits {
            reads: BoundReadLimits {
                max_reads: 100,
                max_bytes: 1_000_000,
            },
            max_gas_limit: 60_000_000,
            max_input_bytes: 2_000_000,
            max_instructions: 1000,
            max_memory_bytes: 1_000_000,
        };
        let result = execute_op_user(
            Arc::new(StateImage::new(d.clone()).unwrap()),
            q,
            limits,
            ExecutionControl::default(),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", s["id"]));
        let out = result.execution.result_and_state;
        assert_eq!(
            out.result.is_success(),
            original["success"].as_bool().unwrap(),
            "{}",
            s["id"]
        );
        assert_eq!(
            out.result.gas_used(),
            original["gas_used"].as_u64().unwrap(),
            "{}",
            s["id"]
        );
        assert_eq!(
            hex::encode(out.result.output().map_or(&[][..], |b| b.as_ref())),
            original["output"].as_str().unwrap(),
            "{}",
            s["id"]
        );
        for (addr, balance) in original["after"].as_object().unwrap() {
            let key = address(addr);
            let actual = out
                .state
                .get(&key)
                .map_or(d.accounts[&key].as_ref().unwrap().balance, |v| {
                    v.info.balance
                });
            assert_eq!(
                actual,
                number(balance.as_str().unwrap()),
                "{} {addr}",
                s["id"]
            );
        }
        assert_eq!(
            out.state[&address(original["caller"].as_str().unwrap())]
                .info
                .nonce,
            1
        );
        assert!(result.execution.telemetry.reads.reads <= 100);
        count += 1;
    }
    assert_eq!(count as u64, v["signed_transitions"].as_u64().unwrap());
    assert!(count >= 100);
}
