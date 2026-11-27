use revm::{
    db::InMemoryDB,
    primitives::{
        keccak256, AccountInfo, Address, Bytes, ExecutionResult, Output, SpecId, TxKind, U256,
    },
    Evm,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Build {
    manager: String,
    harness: String,
    hook: String,
}

fn address_word(address: Address) -> [u8; 32] {
    let mut word = [0; 32];
    word[12..].copy_from_slice(address.as_slice());
    word
}

fn selector(signature: &str) -> [u8; 4] {
    keccak256(signature)[..4].try_into().unwrap()
}

fn transact(db: &mut InMemoryDB, target: TxKind, data: Vec<u8>, value: u64) -> ExecutionResult {
    Evm::builder()
        .with_db(db)
        .with_spec_id(SpecId::CANCUN)
        .modify_block_env(|block| block.gas_limit = U256::from(30_000_000))
        .modify_tx_env(|tx| {
            tx.caller = Address::repeat_byte(0x11);
            tx.gas_limit = 20_000_000;
            tx.transact_to = target;
            tx.data = Bytes::from(data);
            tx.value = U256::from(value);
        })
        .build()
        .transact_commit()
        .unwrap()
}

fn deploy(db: &mut InMemoryDB, bytecode: &str, argument: Address) -> Address {
    let mut data = hex::decode(bytecode).unwrap();
    data.extend_from_slice(&address_word(argument));
    match transact(db, TxKind::Create, data, 0) {
        ExecutionResult::Success {
            output: Output::Create(_, Some(address)),
            ..
        } => address,
        failure => panic!("deployment failed: {failure:?}"),
    }
}

#[test]
#[ignore = "requires the pinned Cancun PoolManager build"]
fn original_pool_manager_settles_hook_deltas_and_native_currency() {
    let path = std::env::var("XONE_V4_MANAGER_SETTLEMENT_BUILD").unwrap();
    let build: Build = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut db = InMemoryDB::default();
    let caller = Address::repeat_byte(0x11);
    db.insert_account_info(
        caller,
        AccountInfo {
            balance: U256::from(10u128.pow(21)),
            ..Default::default()
        },
    );
    let manager = deploy(&mut db, &build.manager, caller);
    let harness = deploy(&mut db, &build.harness, manager);
    let mut hook = hex::decode(build.hook).unwrap();
    hook.extend_from_slice(&address_word(manager));
    let hash = keccak256(hook);
    let salt = (0..1_000_000u64)
        .map(|salt| U256::from(salt).to_be_bytes::<32>())
        .find(|salt| {
            let address = harness.create2(*salt, hash);
            u16::from_be_bytes([address[18], address[19]]) & 0x3fff == 0xcc
        })
        .expect("hook salt");
    let mut data = selector("installHook(bytes32)").to_vec();
    data.extend_from_slice(&salt);
    assert!(transact(&mut db, TxKind::Call(harness), data, 0).is_success());

    for native in [false, true] {
        for zero_for_one in [false, true] {
            for exact_in in [false, true] {
                let mut data = selector("run(bool,bool,bool,bool)").to_vec();
                for value in [native, zero_for_one, exact_in, false] {
                    data.extend_from_slice(&U256::from(u8::from(value)).to_be_bytes::<32>());
                }
                let value = if native { 10_000_000_000_000 } else { 0 };
                let result = transact(&mut db, TxKind::Call(harness), data.clone(), value);
                assert!(
                    result.is_success(),
                    "native={native} zero_for_one={zero_for_one} exact_in={exact_in}: {result:?}"
                );
                *data.last_mut().unwrap() = 1;
                let rejected = transact(&mut db, TxKind::Call(harness), data, value);
                assert!(matches!(rejected, ExecutionResult::Revert { output, .. }
                    if output.as_ref() == selector("CurrencyNotSettled()")));
            }
        }
    }
}
