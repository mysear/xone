use revm::{
    db::InMemoryDB,
    primitives::{
        keccak256, AccountInfo, Address, Bytes, ExecutionResult, Output, SpecId, TxKind, U256,
    },
    Evm,
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Build {
    manager: String,
    harness: String,
    hook: String,
    executor: String,
    infinity: InfinityBuild,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InfinityBuild {
    vault: String,
    cl_manager: String,
    bin_manager: String,
    bin_vault: String,
    harness: String,
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

fn deploy(db: &mut InMemoryDB, bytecode: &str, argument: impl Into<Option<Address>>) -> Address {
    let mut data = hex::decode(bytecode).unwrap();
    if let Some(argument) = argument.into() {
        data.extend_from_slice(&address_word(argument));
    }
    match transact(db, TxKind::Create, data, 0) {
        ExecutionResult::Success {
            output: Output::Create(_, Some(address)),
            ..
        } => address,
        failure => panic!("deployment failed: {failure:?}"),
    }
}

fn record_bin_bank(
    db: &mut InMemoryDB,
    manager: Address,
    vault: Address,
    hook: Address,
    currencies: [Address; 2],
    pool_id: [u8; 32],
) -> serde_json::Value {
    let accounts = [manager, vault, hook, currencies[0], currencies[1]];
    let codes: BTreeMap<_, _> = accounts
        .into_iter()
        .filter(|address| *address != Address::ZERO)
        .map(|address| {
            let code = &db.contracts[&db.accounts[&address].info.code_hash];
            (
                format!("{address:#x}"),
                format!("0x{}", hex::encode(code.original_bytes())),
            )
        })
        .collect();
    let mut calls = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut read = |address, signature, words: &[[u8; 32]]| {
        let mut calldata = selector(signature).to_vec();
        calldata.extend(words.iter().flatten().copied());
        let key = format!("0x{}", hex::encode(&calldata));
        match transact(db, TxKind::Call(address), calldata, 0) {
            ExecutionResult::Success {
                output: Output::Call(data),
                ..
            } => {
                let previous = calls
                    .entry(format!("{address:#x}"))
                    .or_default()
                    .insert(key, format!("0x{}", hex::encode(data)));
                assert!(previous.is_none());
            }
            failure => panic!("{signature}: {failure:?}"),
        }
    };
    read(manager, "vault()", &[]);
    read(manager, "paused()", &[]);
    read(vault, "isAppRegistered(address)", &[address_word(manager)]);
    read(manager, "poolIdToPoolKey(bytes32)", &[pool_id]);
    read(hook, "getHooksRegistrationBitmap()", &[]);
    for currency in currencies {
        if currency != Address::ZERO {
            read(currency, "decimals()", &[]);
        }
        let currency = address_word(currency);
        read(
            vault,
            "reservesOfApp(address,address)",
            &[address_word(manager), currency],
        );
        read(manager, "protocolFeesAccrued(address)", &[currency]);
    }
    read(manager, "getSlot0(bytes32)", &[pool_id]);
    let word = |n: u32| U256::from(n).to_be_bytes::<32>();
    let mapping = |key: [u8; 32], slot: [u8; 32]| keccak256([key, slot].concat()).0;
    let pool_base = U256::from_be_bytes(mapping(pool_id, word(5)));
    let slot = |n: u32| (pool_base + U256::from(n)).to_be_bytes::<32>();
    for n in [0, 4] {
        read(manager, "extsload(bytes32)", &[slot(n)]);
    }
    for (offset, indices) in [(5, [127, 128]), (6, [32767, 32768])] {
        for index in indices {
            read(
                manager,
                "extsload(bytes32)",
                &[mapping(word(index), slot(offset))],
            );
        }
    }
    for id in 8_388_607..=8_388_609 {
        read(manager, "getBin(bytes32,uint24)", &[pool_id, word(id)]);
        for n in [1, 2] {
            read(manager, "extsload(bytes32)", &[mapping(word(id), slot(n))]);
        }
        for direction in [0, 1] {
            read(
                manager,
                "getNextNonEmptyBin(bytes32,bool,uint24)",
                &[pool_id, word(direction), word(id)],
            );
        }
    }
    json!({"code": codes, "calls": calls})
}

#[test]
#[ignore = "requires the pinned Cancun Infinity CL/Bin/Vault build"]
fn original_infinity_managers_settle_hook_deltas_through_vault() {
    let path = std::env::var("XONE_V4_MANAGER_SETTLEMENT_BUILD").unwrap();
    let build: Build = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut db = InMemoryDB::default();
    db.insert_account_info(
        Address::repeat_byte(0x11),
        AccountInfo {
            balance: U256::from(10u128.pow(21)),
            ..Default::default()
        },
    );
    let mut cases = Vec::new();
    for (bin, manager_code, vault_code) in [
        (false, &build.infinity.cl_manager, &build.infinity.vault),
        (true, &build.infinity.bin_manager, &build.infinity.bin_vault),
    ] {
        let vault = deploy(&mut db, vault_code, None);
        let vault_code = &db.contracts[&db.accounts[&vault].info.code_hash];
        let vault_digest = hex::encode(Sha256::digest(vault_code.original_bytes()));
        let harness = deploy(&mut db, &build.infinity.harness, vault);
        let manager = deploy(&mut db, manager_code, vault);
        let manager_code = &db.contracts[&db.accounts[&manager].info.code_hash];
        let manager_digest = hex::encode(Sha256::digest(manager_code.original_bytes()));
        let mut register = selector("registerApp(address)").to_vec();
        register.extend_from_slice(&address_word(manager));
        assert!(transact(&mut db, TxKind::Call(vault), register, 0).is_success());
        for native in [false, true] {
            for zero in [false, true] {
                for exact_in in [false, true] {
                    let mut data = selector("run(address,bool,bool,bool,bool,bool)").to_vec();
                    data.extend_from_slice(&address_word(manager));
                    for option in [bin, native, zero, exact_in, false] {
                        data.extend_from_slice(&U256::from(u8::from(option)).to_be_bytes::<32>());
                    }
                    let value = if native { 10_000_000_000_000 } else { 0 };
                    let result = transact(&mut db, TxKind::Call(harness), data.clone(), value);
                    let ExecutionResult::Success {
                        output: Output::Call(raw),
                        ..
                    } = result
                    else {
                        panic!(
                            "bin={bin} native={native} zero={zero} exact in={exact_in}: {result:?}"
                        );
                    };
                    let words: Vec<_> = raw.chunks_exact(32).collect();
                    assert_eq!(words.len(), 21);
                    let delta0 = i128::from_be_bytes(words[3][16..].try_into().unwrap());
                    let delta1 = i128::from_be_bytes(words[4][16..].try_into().unwrap());
                    let (input, output) = if zero {
                        (-delta0, delta1)
                    } else {
                        (-delta1, delta0)
                    };
                    let hook = Address::from_slice(&words[2][12..]);
                    let hook_code = &db.contracts[&db.accounts[&hook].info.code_hash];
                    let hook_code = format!("0x{}", hex::encode(hook_code.original_bytes()));
                    let currencies = [
                        Address::from_slice(&words[0][12..]),
                        Address::from_slice(&words[1][12..]),
                    ];
                    let pool_id = words[20].try_into().unwrap();
                    let bank = (bin && zero && exact_in).then(|| {
                        record_bin_bank(&mut db, manager, vault, hook, currencies, pool_id)
                    });
                    let bins = |offset: usize| {
                        (0..3).map(|i| json!({
                            "id": 8_388_607 + i,
                            "reserve0": u128::from_be_bytes(words[offset + i][16..].try_into().unwrap()).to_string(),
                            "reserve1": u128::from_be_bytes(words[offset + i][..16].try_into().unwrap()).to_string(),
                            "total_shares": U256::from_be_slice(words[10 + i]).to_string(),
                        })).collect::<Vec<_>>()
                    };
                    cases.push(json!({
                        "currency0": format!("0x{}", hex::encode(&words[0][12..])),
                        "currency1": format!("0x{}", hex::encode(&words[1][12..])),
                        "manager": format!("{manager:#x}"),
                        "managerCodeSha256": manager_digest,
                        "vault": format!("{vault:#x}"),
                        "vaultCodeSha256": vault_digest,
                        "hook": format!("{hook:#x}"),
                        "hookCode": hook_code,
                        "poolId": format!("0x{}", hex::encode(pool_id)),
                        "rpcBankAfter": bank,
                        "bin": bin,
                        "zeroForOne": zero,
                        "exactIn": exact_in,
                        "input": input.to_string(),
                        "output": output.to_string(),
                        "stateAfter": U256::from_be_slice(words[5]).to_string(),
                        "tickAfter": i32::from_be_bytes(words[6][28..].try_into().unwrap()),
                        "binsBefore": bin.then(|| bins(7)),
                        "binsAfter": bin.then(|| bins(13)),
                        "appReserve0Before": U256::from_be_slice(words[16]).to_string(),
                        "appReserve1Before": U256::from_be_slice(words[17]).to_string(),
                        "appReserve0After": U256::from_be_slice(words[18]).to_string(),
                        "appReserve1After": U256::from_be_slice(words[19]).to_string(),
                    }));
                    *data.last_mut().unwrap() = 1;
                    let rejected = transact(&mut db, TxKind::Call(harness), data, value);
                    assert!(
                        matches!(&rejected, ExecutionResult::Revert { output, .. }
                        if output.as_ref() == selector("CurrencyNotSettled()")),
                        "{rejected:?}"
                    );
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("XONE_INFINITY_HOOK_QUOTES") {
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
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
    let executor = deploy(&mut db, &build.executor, manager);
    let mut callback = selector("unlockCallback(bytes)").to_vec();
    callback.extend_from_slice(&U256::from(32).to_be_bytes::<32>());
    callback.extend_from_slice(&[0; 32]);
    let rejected = transact(&mut db, TxKind::Call(executor), callback, 0);
    assert!(matches!(rejected, ExecutionResult::Revert { output, .. }
        if output.as_ref() == selector("UnauthorizedCallback()")));
    let mut hook_initcode = hex::decode(build.hook).unwrap();
    hook_initcode.extend_from_slice(&address_word(manager));
    let hash = keccak256(hook_initcode);
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
    let hook = harness.create2(salt, hash);
    let runtime = |db: &InMemoryDB, address| {
        db.contracts[&db.accounts[&address].info.code_hash].original_bytes()
    };
    let manager_code = runtime(&db, manager);
    let manager_sha256 = hex::encode(Sha256::digest(&manager_code));
    let executor_code = runtime(&db, executor);
    let mut token_code = None;

    let mut cases = Vec::new();
    for native in [false, true] {
        for zero_for_one in [false, true] {
            for exact_in in [false, true] {
                let mut data = selector("run(bool,bool,bool,bool,address)").to_vec();
                for value in [native, zero_for_one, exact_in, false] {
                    data.extend_from_slice(&U256::from(u8::from(value)).to_be_bytes::<32>());
                }
                let failure_flag = data.len() - 1;
                data.extend_from_slice(&address_word(Address::ZERO));
                let value = if native { 10_000_000_000_000 } else { 0 };
                let result = transact(&mut db, TxKind::Call(harness), data.clone(), value);
                assert!(
                    result.is_success(),
                    "native={native} zero_for_one={zero_for_one} exact_in={exact_in}: {result:?}"
                );
                let ExecutionResult::Success {
                    output: Output::Call(raw),
                    ..
                } = result
                else {
                    unreachable!("successful call")
                };
                let words: Vec<&[u8]> = raw.chunks_exact(32).collect();
                assert_eq!(words.len(), 6);
                token_code
                    .get_or_insert_with(|| runtime(&db, Address::from_slice(&words[1][12..])));
                let delta0 = i128::from_be_bytes(words[2][16..].try_into().unwrap());
                let delta1 = i128::from_be_bytes(words[3][16..].try_into().unwrap());
                let (input, output) = if zero_for_one {
                    (-delta0, delta1)
                } else {
                    (-delta1, delta0)
                };
                cases.push(json!({
                    "currency0": format!("0x{}", hex::encode(&words[0][12..])),
                    "currency1": format!("0x{}", hex::encode(&words[1][12..])),
                    "manager": format!("{manager:#x}"),
                    "managerCodeSha256": manager_sha256,
                    "hook": format!("{hook:#x}"),
                    "zeroForOne": zero_for_one,
                    "exactIn": exact_in,
                    "input": input.to_string(),
                    "output": output.to_string(),
                    "priceAfter": U256::from_be_slice(words[4]).to_string(),
                    "tickAfter": i32::from_be_bytes(words[5][28..].try_into().unwrap()),
                }));
                data[failure_flag] = 1;
                let rejected = transact(&mut db, TxKind::Call(harness), data.clone(), value);
                assert!(matches!(rejected, ExecutionResult::Revert { output, .. }
                    if output.as_ref() == selector("CurrencyNotSettled()")));
                data[failure_flag] = 0;
                data[failure_flag + 1..].copy_from_slice(&address_word(executor));
                let executed = transact(&mut db, TxKind::Call(harness), data.clone(), value);
                assert!(
                    matches!(&executed, ExecutionResult::Success {
                    output: Output::Call(actual), ..
                } if actual[64..] == raw[64..]),
                    "{executed:?}"
                );
                data[failure_flag] = 1;
                let rejected = transact(&mut db, TxKind::Call(harness), data, value);
                assert!(
                    matches!(&rejected, ExecutionResult::Revert { output, .. }
                    if output.as_ref() == selector("AmountMismatch()")),
                    "{rejected:?}"
                );
            }
        }
    }
    if let Some(path) = std::env::var_os("XONE_V4_HOOK_QUOTES") {
        let token_code = token_code.unwrap();
        let fixture = json!({
            "cases": cases,
            "executor": format!("{executor:#x}"),
            "executorCode": format!("0x{}", hex::encode(&executor_code)),
            "executorCodeSha256": hex::encode(Sha256::digest(&executor_code)),
            "managerCode": format!("0x{}", hex::encode(manager_code)),
            "tokenCode": format!("0x{}", hex::encode(&token_code)),
            "tokenCodeSha256": hex::encode(Sha256::digest(&token_code)),
        });
        std::fs::write(path, serde_json::to_vec_pretty(&fixture).unwrap()).unwrap();
    }
}
