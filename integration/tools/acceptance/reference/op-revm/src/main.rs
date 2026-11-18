// Independent unchanged REVM OP handlers over the same completed initial state.
// This reference executable is not the product execution adapter.
use revm::{primitives::*, InMemoryDB};
use serde_json::{json, Value};
use std::str::FromStr;
fn addr(v: &str) -> Address {
    Address::from_str(v).unwrap()
}
fn num(v: &str) -> U256 {
    U256::from_str(v).unwrap()
}
fn main() {
    let v: Value =
        serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let mut checked = 0;
    let mut unsupported_jovian = 0;
    let mut failures = Vec::new();
    for r in v["cases"].as_array().unwrap() {
        let s = &r["input"];
        let rule = s["rule"].as_str().unwrap();
        let spec = match rule {
            "regolith" => SpecId::REGOLITH,
            "ecotone" => SpecId::ECOTONE,
            "fjord" => SpecId::FJORD,
            "isthmus" => SpecId::ISTHMUS,
            "jovian" => {
                unsupported_jovian += 1;
                continue;
            }
            _ => panic!(),
        };
        let mut db = InMemoryDB::default();
        for (a, n) in r["actual"]["before"].as_object().unwrap() {
            db.insert_account_info(
                addr(a),
                AccountInfo {
                    balance: num(n.as_str().unwrap()),
                    ..Default::default()
                },
            );
        }
        let target = addr("0x1000000000000000000000000000000000000001");
        let outcome = s["outcome"].as_str().unwrap();
        let code = match outcome {
            "success" => "00",
            "refund" => "600060005500",
            "revert" => "600160005560006000fd",
            "halt" => "6001600055fe",
            _ => panic!(),
        };
        let code = Bytecode::new_raw(Bytes::from(hex::decode(code).unwrap()));
        db.insert_account_info(
            target,
            AccountInfo {
                code_hash: code.hash_slow(),
                code: Some(code),
                ..Default::default()
            },
        );
        db.insert_account_storage(
            target,
            U256::ZERO,
            U256::from(if outcome == "refund" { 1 } else { 0 }),
        )
        .unwrap();
        let l1 = addr("0x4200000000000000000000000000000000000015");
        db.insert_account_info(l1, AccountInfo::default());
        let a = hex::decode(s["attributes"].as_str().unwrap()).unwrap();
        let mut put = |i: u64, b: &[u8]| {
            db.insert_account_storage(l1, U256::from(i), U256::from_be_slice(b))
                .unwrap()
        };
        if rule == "regolith" {
            put(1, &a[68..100]);
            put(5, &a[196..228]);
            put(6, &a[228..260]);
        } else {
            put(1, &a[36..68]);
            put(7, &a[68..100]);
            let mut scalars = [0; 32];
            scalars[16..24].copy_from_slice(&a[4..12]);
            put(3, &scalars);
            if a.len() >= 176 {
                let mut op = [0; 32];
                op[20..32].copy_from_slice(&a[164..176]);
                put(8, &op);
            }
        }
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
        let tx = TxEnv {
            caller: addr(r["payer"].as_str().unwrap()),
            gas_limit: s["gas_limit"].as_u64().unwrap(),
            gas_price: num(s["maximum"].as_str().unwrap()),
            gas_priority_fee: if s["kind"] == "dynamic" {
                Some(num(s["priority"].as_str().unwrap()))
            } else {
                None
            },
            transact_to: TxKind::Call(target),
            value: U256::ZERO,
            data: Bytes::from(vec![0, 1, 2, 0, 3]),
            nonce: Some(0),
            chain_id: Some(10),
            optimism: OptimismFields {
                enveloped_tx: Some(Bytes::from(
                    hex::decode(r["signed_wire"].as_str().unwrap()).unwrap(),
                )),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut evm = revm::Evm::builder()
            .with_db(db)
            .with_spec_id(spec)
            .modify_cfg_env(|c| c.chain_id = 10)
            .with_block_env(block)
            .with_tx_env(tx)
            .optimism()
            .build();
        match evm.transact() {
            Ok(actual) => {
                let gas = actual.result.gas_used();
                let payer = addr(r["payer"].as_str().unwrap());
                if actual.state[&payer].info.nonce != r["actual"]["nonce_after"].as_u64().unwrap()
                    || actual.result.is_success() == r["actual"]["failed"].as_bool().unwrap()
                {
                    failures
                        .push(json!({"id":s["id"],"error":"nonce or execution status differs"}));
                }
                let slot = actual
                    .state
                    .get(&target)
                    .and_then(|a| a.storage.get(&U256::ZERO))
                    .map(|slot| slot.present_value())
                    .unwrap_or(evm.context.evm.db.accounts[&target].storage[&U256::ZERO]);
                if slot != U256::ZERO {
                    failures.push(json!({"id":s["id"],"error":"storage rollback/refund differs"}));
                }
                for (a, n) in r["actual"]["after"].as_object().unwrap() {
                    let address = addr(a);
                    let value = actual
                        .state
                        .get(&address)
                        .map(|a| a.info.balance)
                        .unwrap_or(evm.context.evm.db.accounts[&address].info.balance);
                    if value != num(n.as_str().unwrap()) {
                        failures.push(json!({"id":s["id"],"account":a,"expected":n,"actual":value.to_string(),"gas":gas,"expected_gas":r["actual"]["gas_used"]}));
                    }
                }
                if gas != r["actual"]["gas_used"].as_u64().unwrap() {
                    failures.push(json!({"id":s["id"],"expected_gas":r["actual"]["gas_used"],"actual_gas":gas}));
                }
            }
            Err(e) => failures.push(json!({"id":s["id"],"error":format!("{e:?}")})),
        }
        checked += 1;
    }
    let passed = checked == 256 && unsupported_jovian == 64 && failures.is_empty();
    println!(
        "{}",
        json!({"passed":passed,"checked":checked,"unsupported_jovian":unsupported_jovian,"failures":failures,"scope":"LOCAL_ORIGINAL_REVM_VS_OP_GETH_FEE_ONLY_NOT_PRODUCT_OP_EXECUTION"})
    );
    if !passed {
        std::process::exit(1);
    }
}
