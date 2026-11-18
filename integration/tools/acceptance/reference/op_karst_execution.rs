//! I/O and bank installation only. All transitions use the unchanged pinned
//! official OpEvm, including its original Karst rules, crypto and fee handlers.
use alloy_consensus::{Transaction, TxEnvelope, transaction::SignerRecoverable};
use alloy_eips::{Decodable2718, Encodable2718, Typed2718};
use op_revm::revm::{
    ExecuteEvm,
    context::{BlockEnv, CfgEnv, TxEnv},
    database::InMemoryDB,
    database_interface::{Database, DatabaseCommit},
    primitives::{Address, B256, Bytes, U256, keccak256},
    state::{AccountInfo, Bytecode},
};
use op_revm::{DefaultOp, OpBuilder, OpContext, OpSpecId, OpTransaction};
use serde_json::{Value, json};
use std::{io::Read, str::FromStr};
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap()
}
fn address(s: &str) -> Address {
    s.parse().unwrap()
}
fn uint(s: &str) -> U256 {
    U256::from_str(s).unwrap()
}
fn raw(s: &str) -> Vec<u8> {
    hex::decode(s.strip_prefix("0x").unwrap_or(s)).unwrap()
}
fn install(fixture: &Value) -> InMemoryDB {
    let mut db = InMemoryDB::default();
    for a in fixture["accounts"].as_array().unwrap() {
        if !a["exists"].as_bool().unwrap() {
            continue;
        }
        let owner = address(text(a, "address"));
        let code = Bytecode::new_raw(raw(text(a, "code")).into());
        db.insert_account_info(
            owner,
            AccountInfo {
                balance: uint(text(a, "balance")),
                nonce: a["nonce"].as_u64().unwrap(),
                code_hash: code.hash_slow(),
                code: Some(code),
                ..Default::default()
            },
        );
        for s in a["storage"].as_array().unwrap() {
            db.insert_account_storage(owner, uint(text(s, "slot")), uint(text(s, "value")))
                .unwrap();
        }
    }
    db
}
fn snapshot(db: &mut InMemoryDB, fixture: &Value) -> Value {
    let mut accounts = serde_json::Map::new();
    for a in fixture["accounts"].as_array().unwrap() {
        let owner = address(text(a, "address"));
        let info = db.basic(owner).unwrap().unwrap_or_default();
        let code = db.code_by_hash(info.code_hash).unwrap().original_bytes();
        let mut storage = serde_json::Map::new();
        for s in a["storage"].as_array().unwrap() {
            let value = db.storage(owner, uint(text(s, "slot"))).unwrap();
            storage.insert(text(s, "slot").into(), json!(format!("{value:#066x}")));
        }
        accounts.insert(format!("{owner:#x}"),json!({"balance":info.balance.to_string(),"nonce":info.nonce,"code":format!("0x{}",hex::encode(code)),"storage":storage}));
    }
    Value::Object(accounts)
}
// Read-only observations on discarded copies. Preserve actual caller balance,
// GASPRICE and source nonce; use the original system-call path only for probes.
fn balance(
    db: &InMemoryDB,
    cfg: &CfgEnv<OpSpecId>,
    block: &BlockEnv,
    signed: &OpTransaction<TxEnv>,
    token: Address,
    owner: Address,
) -> U256 {
    let mut source = db.clone();
    let mut tx = signed.clone();
    let cfg = cfg.clone();
    tx.base.nonce = source.basic(tx.base.caller).unwrap().unwrap().nonce;
    tx.base.kind = op_revm::revm::primitives::TxKind::Call(token);
    tx.base.gas_limit = 200_000;
    let mut data = hex::decode("70a08231").unwrap();
    data.extend([0u8; 12]);
    data.extend(owner.as_slice());
    tx.base.data = data.into();
    let mut evm = OpContext::op()
        .with_db(source)
        .with_cfg(cfg)
        .with_block(block.clone())
        .with_tx(tx)
        .build_op();
    use op_revm::revm::handler::Handler;
    let mut handler = op_revm::handler::OpHandler::<
        _,
        op_revm::revm::context::result::EVMError<
            std::convert::Infallible,
            op_revm::OpTransactionError,
        >,
        op_revm::revm::handler::EthFrame<op_revm::revm::interpreter::interpreter::EthInterpreter>,
    >::new();
    let result = handler
        .run_system_call(&mut evm)
        .expect("original balanceOf probe");
    let state = evm.finalize();
    assert!(result.is_success());
    assert!(result.tx_gas_used() <= 200_000);
    for (owner, a) in &state {
        let before = evm
            .0
            .ctx
            .journaled_state
            .database
            .basic(*owner)
            .unwrap()
            .unwrap_or_default();
        assert_eq!(
            a.info.balance, before.balance,
            "balanceOf must not transfer native value"
        );
        assert!(
            a.storage.values().all(|slot| !slot.is_changed()),
            "balanceOf must not change storage"
        );
        assert!(!a.is_selfdestructed());
    }
    let bytes = result.output().unwrap();
    assert_eq!(bytes.len(), 32);
    U256::from_be_slice(bytes)
}

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .take(128 * 1024 * 1024)
        .read_to_string(&mut input)
        .unwrap();
    let inputs: Vec<Value> = serde_json::from_str(&input).unwrap();
    assert!(!inputs.is_empty() && inputs.len() <= 15000);
    let mut rows = Vec::new();
    for s in inputs {
        let fixture = &s["fixture"];
        let env = &fixture["execution_environment"];
        let wire = raw(text(&s, "signed_wire"));
        let mut cursor = wire.as_slice();
        let signed = TxEnvelope::decode_2718(&mut cursor).unwrap();
        assert!(cursor.is_empty());
        assert_eq!(signed.encoded_2718(), wire);
        assert!(matches!(signed.ty(), 0 | 1 | 2));
        let caller = signed.recover_signer().unwrap();
        assert_eq!(caller, address(text(&s, "caller")));
        let hash = keccak256(&wire);
        assert_eq!(hash, B256::from_str(text(&s, "transaction_hash")).unwrap());
        assert_eq!(signed.chain_id(), env["chain_id"].as_u64());
        let tx = TxEnv {
            tx_type: signed.ty(),
            caller,
            gas_limit: signed.gas_limit(),
            gas_price: signed.max_fee_per_gas(),
            kind: signed.kind(),
            value: signed.value(),
            data: signed.input().clone(),
            nonce: signed.nonce(),
            chain_id: signed.chain_id(),
            access_list: signed.access_list().cloned().unwrap_or_default(),
            gas_priority_fee: signed.max_priority_fee_per_gas(),
            ..Default::default()
        };
        let tx = OpTransaction {
            base: tx,
            enveloped_tx: Some(Bytes::from(wire.clone())),
            ..Default::default()
        };
        let spec = match s["rule"].as_str().unwrap() {
            "karst" => OpSpecId::KARST,
            "jovian" => OpSpecId::JOVIAN,
            _ => panic!("unsupported original rule"),
        };
        let mut cfg = CfgEnv::new_with_spec(spec);
        cfg.chain_id = env["chain_id"].as_u64().unwrap();
        let block = BlockEnv {
            number: U256::from(env["block_number"].as_u64().unwrap()),
            beneficiary: address(text(env, "coinbase")),
            timestamp: U256::from(env["timestamp"].as_u64().unwrap()),
            gas_limit: env["gas_limit"].as_u64().unwrap(),
            basefee: text(env, "base_fee").parse().unwrap(),
            prevrandao: Some(text(env, "prevrandao").parse().unwrap()),
            ..Default::default()
        };
        let db = install(fixture);
        let tokens = s.get("token_in").map(|_| {
            (
                address(text(&s, "token_in")),
                address(text(&s, "token_out")),
                address(text(&s, "recipient")),
            )
        });
        let token_before = tokens.map(|(sell, buy, recipient)| {
            (
                balance(&db, &cfg, &block, &tx, sell, caller),
                balance(&db, &cfg, &block, &tx, buy, recipient),
            )
        });
        let probe_cfg = cfg.clone();
        let probe_block = block.clone();
        let probe_tx = tx.clone();
        let mut before_db = db.clone();
        let before = snapshot(&mut before_db, fixture);
        let mut evm = OpContext::op()
            .with_db(db)
            .with_cfg(cfg)
            .with_block(block)
            .build_op();
        let result = evm.transact(tx);
        let mut row = json!({"id":s["id"],"transaction_hash":format!("{hash:#x}"),"caller":format!("{caller:#x}"),"before":before});
        match result {
            Ok(out) => {
                row["accepted"] = json!(true);
                row["failed"] = json!(!out.result.is_success());
                row["gas_used"] = json!(out.result.tx_gas_used());
                row["output"] = json!(hex::encode(
                    out.result.output().cloned().unwrap_or_default()
                ));
                row["status"] = json!(format!("{:?}", out.result));
                row["l1_fee"] = json!(
                    evm.0
                        .ctx
                        .chain
                        .calculate_tx_l1_cost(&wire, spec)
                        .to_string()
                );
                row["operator_fee"] = json!(
                    evm.0
                        .ctx
                        .chain
                        .operator_fee_charge(&wire, U256::from(out.result.tx_gas_used()), spec)
                        .to_string()
                );
                evm.0.ctx.journaled_state.database.commit(out.state);
            }
            Err(error) => {
                row["accepted"] = json!(false);
                row["error"] = json!(format!("{error:?}"));
            }
        }
        if row["accepted"] == true {
            if let (Some((sell, buy, recipient)), Some((sell_before, buy_before))) =
                (tokens, token_before)
            {
                let db = &evm.0.ctx.journaled_state.database;
                row["spent"] = json!(
                    sell_before
                        .checked_sub(balance(
                            db,
                            &probe_cfg,
                            &probe_block,
                            &probe_tx,
                            sell,
                            caller
                        ))
                        .unwrap()
                        .to_string()
                );
                row["bought"] = json!(
                    balance(db, &probe_cfg, &probe_block, &probe_tx, buy, recipient)
                        .checked_sub(buy_before)
                        .unwrap()
                        .to_string()
                );
                row["nonce_after"] = json!(
                    evm.0
                        .ctx
                        .journaled_state
                        .database
                        .basic(caller)
                        .unwrap()
                        .unwrap()
                        .nonce
                );
            }
        }
        row["accounts"] = snapshot(&mut evm.0.ctx.journaled_state.database, fixture);
        rows.push(row);
    }
    serde_json::to_writer(std::io::stdout(), &rows).unwrap();
}
