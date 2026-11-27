use super::*;
use revm::{
    primitives::{BlockEnv, Bytecode, TxEnv, B256},
    Database, DatabaseRef,
};
use std::{collections::BTreeSet, convert::Infallible};
use xone_domain::{
    hash,
    infinity_bin::InfinityBinState,
    scope_hash,
    singleton::{SingletonAdvancedState, INFINITY_SETTLEMENT_HOOK_MODEL},
    Amount, Pool, SnapshotRef, SCHEMA_VERSION,
};
use xone_execution_plan::{self as plans, v4_single, Context};
use xone_optimizer::{
    exact_out::{evaluate_allocation, ExactOutProposal},
    AllocatedRoute, Allocation,
};
use xone_routing::{Edge, Funds, PathQuote};
use xone_simulation::evm_fixture::{
    simulate_singleton, EvmStatus, FixtureState, SimulationBudget, SingletonPlan,
    TransactionContext,
};

#[derive(Default)]
struct RecordedReads {
    accounts: BTreeSet<Address>,
    storage: BTreeMap<Address, BTreeSet<U256>>,
    hashes: BTreeSet<u64>,
}

struct Recording<'a> {
    db: &'a InMemoryDB,
    reads: &'a mut RecordedReads,
}
impl Database for Recording<'_> {
    type Error = Infallible;
    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        self.reads.accounts.insert(address);
        self.db.basic_ref(address)
    }
    fn code_by_hash(&mut self, hash: B256) -> Result<Bytecode, Self::Error> {
        self.db.code_by_hash_ref(hash)
    }
    fn storage(&mut self, address: Address, slot: U256) -> Result<U256, Self::Error> {
        self.reads.storage.entry(address).or_default().insert(slot);
        self.db.storage_ref(address, slot)
    }
    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        self.reads.hashes.insert(number);
        self.db.block_hash_ref(number)
    }
}

fn fixture(db: &InMemoryDB, reads: &RecordedReads, pool_hash: &str) -> FixtureState {
    let accounts: Vec<_> = reads
        .accounts
        .iter()
        .map(|&address| {
            let info = db.basic_ref(address).unwrap();
            let storage = if info.is_some() {
                reads.storage.get(&address).into_iter().flatten().map(|&slot| {
                    json!({"slot":format!("0x{slot:064x}"),"value":format!("0x{:064x}",db.storage_ref(address, slot).unwrap())})
                }).collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let code = info.as_ref().map_or_else(Bytes::new, |info| {
                db.code_by_hash_ref(info.code_hash).unwrap().original_bytes()
            });
            json!({
                "address": format!("{address:#x}"), "exists": info.is_some(),
                "balance": info.as_ref().map_or(U256::ZERO, |i| i.balance).to_string(),
                "nonce": info.map_or(0, |i| i.nonce),
                "code": format!("0x{}", hex::encode(code)), "storage": storage
            })
        })
        .collect();
    let hashes: Vec<_> = reads.hashes.iter().map(|&number| {
        json!({"number":number,"hash":format!("{:#x}",db.block_hash_ref(number).unwrap())})
    }).collect();
    serde_json::from_value(json!({
        "schema_version":1,
        "state_anchor":{"chain_key":"evm:31337","block_number":100,"block_hash":pool_hash,
            "parent_hash":format!("{:#x}",B256::repeat_byte(0xbc)),"timestamp":1000,
            "genesis_hash":format!("{:#x}",B256::repeat_byte(0xcd))},
        "execution_environment":{"chain_id":31337,"fork":"cancun","block_number":101,
            "block_hash":format!("{:#x}",B256::repeat_byte(0xde)),"parent_hash":pool_hash,
            "timestamp":1001,"coinbase":format!("{:#x}",Address::ZERO),"gas_limit":30_000_000,
            "base_fee":"10","prevrandao":format!("{:#x}",B256::ZERO),"excess_blob_gas":0},
        "accounts":accounts,"block_hashes":hashes
    }))
    .unwrap()
}

fn run_reference(
    db: &InMemoryDB,
    reads: &mut RecordedReads,
    transaction: &plans::Transaction,
    nonce: u64,
) -> ExecutionResult {
    let mut block = BlockEnv {
        number: U256::from(101),
        timestamp: U256::from(1001),
        gas_limit: U256::from(30_000_000),
        basefee: U256::from(10),
        ..Default::default()
    };
    block.set_blob_excess_gas_and_price(0, false);
    Evm::builder()
        .with_db(Recording { db, reads })
        .with_spec_id(SpecId::CANCUN)
        .modify_cfg_env(|cfg| cfg.chain_id = 31337)
        .with_block_env(block)
        .with_tx_env(TxEnv {
            caller: transaction.from.parse().unwrap(),
            nonce: Some(nonce),
            chain_id: Some(31337),
            transact_to: TxKind::Call(transaction.to.parse().unwrap()),
            value: U256::from_be_bytes(transaction.value.to_be_bytes().unwrap()),
            data: Bytes::from(hex::decode(&transaction.data[2..]).unwrap()),
            gas_limit: 1_500_000,
            gas_price: U256::from(30),
            gas_priority_fee: Some(U256::from(3)),
            ..Default::default()
        })
        .build()
        .transact()
        .unwrap()
        .result
}

fn cl_pool(
    tokens: [Address; 2],
    manager: Address,
    manager_digest: &str,
    hook: Address,
    bank_hash: &str,
    infinity: bool,
) -> Pool {
    let (protocol, profile, model, parameters) = if infinity {
        (
            "pancake-infinity-cl",
            "pancake-infinity-cl-advanced",
            INFINITY_SETTLEMENT_HOOK_MODEL,
            format!("0x{:064x}", (60u32 << 16) | 0xcc0),
        )
    } else {
        (
            "uniswap-v4",
            "uniswap-v4-advanced",
            "v4-settlement-hook-v1",
            String::new(),
        )
    };
    let mut state: SingletonAdvancedState = serde_json::from_value(json!({
        "protocol":protocol,
        "currency0":format!("{:#x}",tokens[0]),"currency1":format!("{:#x}",tokens[1]),
        "key_fee":0x800000,"snapshot_block_hash":bank_hash,"hook_model":model,
        "snapshot":{"manager":format!("{manager:#x}"),"manager_code_sha256":manager_digest,
            "pool_id":"","lp_fee":0,"protocol_fee0":0,"protocol_fee1":0,"hooks":format!("{hook:#x}"),
            "parameters":parameters,
            "core":{"sqrt_price_x96":"79228162514264337593543950336","tick":0,
                "liquidity":"1000000000000","tick_spacing":60,"max_steps":8,
                "tick_words":[{"word_pos":-1,"bitmap":(U256::from(1)<<254usize).to_string()},
                    {"word_pos":0,"bitmap":"4"}],
                "ticks":[{"tick":-120,"liquidity_gross":"1000000000000","liquidity_net":"1000000000000"},
                    {"tick":120,"liquidity_gross":"1000000000000","liquidity_net":"-1000000000000"}]}}
    })).unwrap();
    state.snapshot.pool_id = state.key_id().unwrap();
    serde_json::from_value(json!({
        "chain_key":"evm:31337","pool_id":format!("{manager:#x}/{}",state.snapshot.pool_id),
        "protocol":profile,"adapter_version":xone_domain::protocols::protocol(profile).unwrap().adapter,
        "token0":state.currency0,"token1":state.currency1,"reserve0":"0","reserve1":"0",
        "fee_numerator":0,"fee_denominator":1_000_000,"token_behavior":"standard",
        "singleton_advanced":state
    })).unwrap()
}

fn bin_pool(
    tokens: [Address; 2],
    manager: Address,
    manager_digest: &str,
    vault: (&str, &str),
    words: &[&[u8]],
    bank_hash: &str,
    token_digests: &BTreeMap<String, String>,
) -> Pool {
    let tokens = tokens.map(|token| format!("{token:#x}"));
    let bins: Vec<_> = (0..3)
        .map(|i| {
            json!({
                "id": 8_388_607 + i,
                "reserve0": u128::from_be_bytes(words[7+i][16..].try_into().unwrap()).to_string(),
                "reserve1": u128::from_be_bytes(words[7+i][..16].try_into().unwrap()).to_string(),
                "total_shares": U256::from_be_slice(words[10+i]).to_string(),
            })
        })
        .collect();
    let bit255 = (U256::from(1) << 255usize).to_string();
    let state: InfinityBinState = serde_json::from_value(json!({
        "manager":format!("{manager:#x}"),"manager_code_sha256":manager_digest,
        "vault":vault.0,"vault_code_sha256":vault.1,"pool_id":format!("0x{}",hex::encode(words[20])),
        "hooks":format!("0x{}",hex::encode(&words[2][12..])),"hook_model":INFINITY_SETTLEMENT_HOOK_MODEL,
        "parameters":InfinityBinState::key_parameters(25,INFINITY_SETTLEMENT_HOOK_MODEL),
        "token0_code_sha256":token_digests.get(&tokens[0]).map_or("",String::as_str),
        "token1_code_sha256":token_digests[&tokens[1]],"bin_step":25,"active_id":8388608,
        "lp_fee":0,"protocol_fee0":0,"protocol_fee1":0,"max_bins":8,"max_tree_words":8,"max_steps":8,
        "tree_level0":(U256::from(3) << 127usize).to_string(),
        "tree_level1":[{"index":127,"value":bit255},{"index":128,"value":"1"}],
        "tree_level2":[{"index":32767,"value":bit255},{"index":32768,"value":"3"}],
        "bins":bins,
        "app_reserve0":U256::from_be_slice(words[16]).to_string(),
        "app_reserve1":U256::from_be_slice(words[17]).to_string(),
        "protocol_fees0":"0","protocol_fees1":"0","block_number":100,"timestamp":1000,"block_hash":bank_hash
    })).unwrap();
    assert_eq!(state.key_id(&tokens[0], &tokens[1]).unwrap(), state.pool_id);
    serde_json::from_value(json!({
        "chain_key":"evm:31337","pool_id":format!("{manager:#x}/{}",state.pool_id),
        "protocol":xone_domain::infinity_bin::PROFILE,"adapter_version":xone_domain::infinity_bin::ADAPTER,
        "token0":tokens[0],"token1":tokens[1],"reserve0":"0","reserve1":"0",
        "fee_numerator":0,"fee_denominator":1_000_000,"token_behavior":"standard", "infinity_bin":state
    })).unwrap()
}

#[derive(Clone, Copy)]
pub(super) enum Venue {
    V4 { hook: Address },
    Infinity { vault: Address, bin: bool },
}

pub(super) fn verify(
    mut db: InMemoryDB,
    manager: Address,
    harness: Address,
    venue: Venue,
    executor: Address,
    cases: &[serde_json::Value],
) {
    let caller = Address::repeat_byte(0x11);
    let recipient = Address::with_last_byte(0xb4);
    let runtime_digest = |db: &InMemoryDB, address| {
        hex::encode(Sha256::digest(
            db.code_by_hash_ref(db.basic_ref(address).unwrap().unwrap().code_hash)
                .unwrap()
                .original_bytes(),
        ))
    };
    let manager_digest = runtime_digest(&db, manager);
    let executor_digest = runtime_digest(&db, executor);
    let executor_address = format!("{executor:#x}");
    let vault = match venue {
        Venue::Infinity { vault, .. } => Some((format!("{vault:#x}"), runtime_digest(&db, vault))),
        Venue::V4 { .. } => None,
    };
    let executor_identity = v4_single::Executor {
        address: &executor_address,
        code_sha256: &executor_digest,
        infinity_vault: vault.as_ref().map(|(a, d)| (a.as_str(), d.as_str())),
    };
    let bank_hash = format!("{:#x}", B256::repeat_byte(0xab));
    let budget = SimulationBudget {
        max_fixture_json_bytes: 1_000_000,
        max_accounts: 64,
        max_storage_slots: 1024,
        max_code_bytes: 400_000,
        max_image_bytes: 500_000,
        max_reads: 4000,
        max_read_bytes: 500_000,
        max_instructions: 200_000,
        max_input_bytes: 4096,
        max_total_input_bytes: 8192,
        max_transaction_gas: 1_500_000,
        probe_gas_limit: 100_000,
        max_total_gas: 2_000_000,
        max_memory_bytes: 262_144,
        max_output_bytes: 1024,
    };
    for native in [false, true] {
        let boolean = |flag| U256::from(u8::from(flag)).to_be_bytes::<32>();
        let prepare = match venue {
            Venue::V4 { .. } => {
                [selector("prepareReplay(bool)").as_slice(), &boolean(native)].concat()
            }
            Venue::Infinity { bin, .. } => [
                selector("prepareReplay(address,bool,bool)").as_slice(),
                &address_word(manager),
                &boolean(bin),
                &boolean(native),
            ]
            .concat(),
        };
        let prepared = transact(
            &mut db,
            TxKind::Call(harness),
            prepare,
            if native { 10_000_000_000_000 } else { 0 },
        );
        let raw = prepared.output().unwrap();
        assert!(prepared.is_success());
        let tokens = [
            Address::from_slice(&raw[12..32]),
            Address::from_slice(&raw[44..64]),
        ];
        let words: Vec<_> = raw.chunks_exact(32).collect();
        let hook = match venue {
            Venue::V4 { hook } => hook,
            Venue::Infinity { .. } => Address::from_slice(&words[2][12..]),
        };
        let token_digests = tokens
            .iter()
            .filter(|&&token| token != Address::ZERO)
            .map(|&token| (format!("{token:#x}"), runtime_digest(&db, token)))
            .collect();
        let pool = if matches!(venue, Venue::Infinity { bin: true, .. }) {
            bin_pool(
                tokens,
                manager,
                &manager_digest,
                executor_identity.infinity_vault.unwrap(),
                &words,
                &bank_hash,
                &token_digests,
            )
        } else {
            cl_pool(
                tokens,
                manager,
                &manager_digest,
                hook,
                &bank_hash,
                vault.is_some(),
            )
        };
        let pools = std::slice::from_ref(&pool);
        let snapshot: SnapshotRef = serde_json::from_value(json!({
            "block":{"chain_key":"evm:31337","number_or_slot":100,"hash":bank_hash,
                "parent_hash":format!("{:#x}",B256::repeat_byte(0xbc)),"timestamp_ms":1000000},
            "canonical_epoch":1,"journal_seq":1,"schema_version":SCHEMA_VERSION,
            "adapter_set_hash":hash(&[&pool.adapter_version]).unwrap(),"online_generation":1,
            "scope_hash":scope_hash(pools).unwrap(),"writer_epoch":1,
            "owner_service":"chain-parser-go","owner_assignment_version":1,
        }))
        .unwrap();
        for direction in [true, false] {
            let mut pre = db.clone();
            let currencies = if direction {
                tokens
            } else {
                [tokens[1], tokens[0]]
            };
            if currencies[0] != Address::ZERO {
                let approval = [
                    selector("approve(address,uint256)").as_slice(),
                    &address_word(executor),
                    &U256::from(10_000_000).to_be_bytes::<32>(),
                ]
                .concat();
                assert!(transact(&mut pre, TxKind::Call(currencies[0]), approval, 0).is_success());
            }
            let nonce = pre.basic_ref(caller).unwrap().unwrap().nonce;
            let tx = TransactionContext {
                nonce,
                gas_limit: 1_500_000,
                gas_price: "30".parse().unwrap(),
                max_priority_fee_per_gas: Some("3".parse().unwrap()),
                chain_id: 31337,
            };
            let context = Context {
                chain_key: pool.chain_key.clone(),
                caller: format!("{caller:#x}"),
                recipient: format!("{recipient:#x}"),
                sell_token: format!("{:#x}", currencies[0]),
                buy_token: format!("{:#x}", currencies[1]),
                slippage_bps: 100,
                deadline_ms: 2_000_000,
            };
            let edge = Edge {
                from_token: None,
                protocol: pool.protocol.clone(),
                pool_id: pool.pool_id.clone(),
                zero_for_one: direction,
                to: context.buy_token.clone(),
            };
            let amount: Amount = "1000000".parse().unwrap();
            let output = xone_routing::evaluate_path(
                &mut BTreeMap::from([(pool.pool_id.clone(), pool.clone())]),
                std::slice::from_ref(&edge),
                &amount,
            )
            .unwrap();
            let path = PathQuote {
                amount_in: amount.clone(),
                edges: vec![edge.clone()],
                tokens: vec![context.sell_token.clone(), context.buy_token.clone()],
                amount_out: output.clone(),
            };
            let allocation = Allocation {
                sell_amount: amount.clone(),
                gross_buy_amount: output.clone(),
                net_buy_amount: output.clone(),
                refund: Amount::zero(),
                routes: vec![AllocatedRoute {
                    path,
                    amount_in: amount.clone(),
                    amount_out: output,
                }],
            };
            let input = plans::build(&context, &allocation, &snapshot, "test").unwrap();
            let out_allocation = evaluate_allocation(
                pools,
                &[ExactOutProposal {
                    edges: vec![edge],
                    amount_out: amount.clone(),
                }],
                &amount,
                &"1100000".parse().unwrap(),
                Funds {
                    sender: &context.caller,
                    recipient: &context.recipient,
                },
            )
            .unwrap();
            let output =
                plans::exact_out::build(&context, &out_allocation, &snapshot, "test").unwrap();
            let mut tight_context = context.clone();
            tight_context.slippage_bps = 0;
            let mut tight_in = allocation.clone();
            tight_in.gross_buy_amount.0 += 1u8;
            tight_in.net_buy_amount = tight_in.gross_buy_amount.clone();
            tight_in.routes[0].amount_out = tight_in.gross_buy_amount.clone();
            tight_in.routes[0].path.amount_out = tight_in.gross_buy_amount.clone();
            let tight_in = plans::build(&tight_context, &tight_in, &snapshot, "test").unwrap();
            let mut tight_out = out_allocation.clone();
            tight_out.required_input.0 -= 1u8;
            tight_out.unused_input_budget.0 += 1u8;
            tight_out.routes[0].amount_in = tight_out.required_input.clone();
            tight_out.routes[0].amounts[0] = tight_out.required_input.clone();
            let tight_out =
                plans::exact_out::build(&tight_context, &tight_out, &snapshot, "test").unwrap();
            for (exact_in, plan, bound, tight_plan, tight_bound) in [
                (
                    true,
                    SingletonPlan::ExactIn(&input),
                    v4_single::bind_exact_in(&input, pools, executor_identity).unwrap(),
                    SingletonPlan::ExactIn(&tight_in),
                    v4_single::bind_exact_in(&tight_in, pools, executor_identity).unwrap(),
                ),
                (
                    false,
                    SingletonPlan::ExactOut(&output),
                    v4_single::bind_exact_out(&output, pools, executor_identity).unwrap(),
                    SingletonPlan::ExactOut(&tight_out),
                    v4_single::bind_exact_out(&tight_out, pools, executor_identity).unwrap(),
                ),
            ] {
                let mut reads = RecordedReads::default();
                reads.accounts.extend([recipient, Address::ZERO]);
                let original = run_reference(&pre, &mut reads, &bound.transaction, nonce);
                assert!(original.is_success(), "{original:?}");
                let rejected = run_reference(&pre, &mut reads, &tight_bound.transaction, nonce);
                assert!(
                    matches!(rejected, ExecutionResult::Revert { ref output, .. }
                    if output.as_ref() == selector("AmountMismatch()"))
                );
                let bank = fixture(&pre, &reads, &bank_hash);
                let result = simulate_singleton(
                    plan,
                    pools,
                    executor_identity,
                    &token_digests,
                    &bank,
                    &tx,
                    &budget,
                    Default::default(),
                )
                .unwrap();
                let expected = cases
                    .iter()
                    .find(|case| {
                        (case["currency0"] == format!("{:#x}", Address::ZERO)) == native
                            && case["zeroForOne"] == direction
                            && case["exactIn"] == exact_in
                            && case["bin"].as_bool().unwrap_or(false)
                                == matches!(venue, Venue::Infinity { bin: true, .. })
                    })
                    .unwrap();
                assert_eq!(result.execution.status, EvmStatus::Success);
                assert_eq!(result.execution.gas_used, original.gas_used());
                assert_eq!(json!(result.actual_input_consumed), expected["input"]);
                assert_eq!(json!(result.actual_buy_amount), expected["output"]);
                assert_eq!(result.execution.sender_nonce_after, nonce + 1);
                assert!(result.matches_plan);
                assert!(!result.verified);
                let rolled_back = simulate_singleton(
                    tight_plan,
                    pools,
                    executor_identity,
                    &token_digests,
                    &bank,
                    &tx,
                    &budget,
                    Default::default(),
                )
                .unwrap();
                assert_eq!(rolled_back.execution.status, EvmStatus::Revert);
                assert_eq!(rolled_back.execution.gas_used, rejected.gas_used());
                assert_eq!(rolled_back.actual_input_consumed, Amount::zero());
                assert_eq!(rolled_back.actual_buy_amount, Amount::zero());
                assert_eq!(rolled_back.refund, Amount::zero());
                assert!(!rolled_back.matches_plan);
                if native && direction && !exact_in {
                    assert_ne!(result.refund, Amount::zero());
                    assert_eq!(result.unused_input_budget, Some(result.refund));
                } else {
                    assert_eq!(result.refund, Amount::zero());
                }
                let mut missing = bank.clone();
                missing
                    .accounts
                    .iter_mut()
                    .find(|a| a.address == format!("{manager:#x}"))
                    .unwrap()
                    .storage
                    .clear();
                let unavailable = simulate_singleton(
                    plan,
                    pools,
                    executor_identity,
                    &token_digests,
                    &missing,
                    &tx,
                    &budget,
                    Default::default(),
                )
                .unwrap_err();
                assert!(
                    unavailable.detail.contains("unknown_storage"),
                    "{unavailable}"
                );
                let mut tampered_hook = bank;
                let code = &mut tampered_hook
                    .accounts
                    .iter_mut()
                    .find(|a| a.address == format!("{hook:#x}"))
                    .unwrap()
                    .code;
                code.push_str("00");
                let rejected = simulate_singleton(
                    plan,
                    pools,
                    executor_identity,
                    &token_digests,
                    &tampered_hook,
                    &tx,
                    &budget,
                    Default::default(),
                )
                .unwrap_err();
                assert_eq!(rejected.phase, "hook_runtime");
            }
        }
    }
}
