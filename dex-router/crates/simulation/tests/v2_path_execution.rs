//! Independent mined original-Router path transactions and complete declared banks.
use revm::primitives::{Address, BlockEnv, Bytes, SpecId, TxEnv, TxKind, B256, U256};
use serde_json::Value;
use std::{
    str::FromStr,
    sync::{Arc, OnceLock},
};
use xone_domain::{hash, scope_hash, v2_swap, Amount, BlockRef, Pool, SnapshotRef};
use xone_execution_plan::{BoundEvmExactOutExecution, Context, ExecutorIdentity};
use xone_execution_preparation::evm::{prepare_exact_out, FeeContext, FundingObservation};
use xone_optimizer::exact_out::{evaluate_allocation, ExactOutProposal};
use xone_routing::{Edge, Funds};
use xone_simulation::prepared_evm::{simulate_prepared_exact_out, PreparedExactOutExecution};
use xone_simulation::{evm_fixture::*, revm_backend as core};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../test/contracts/v2-path-execution.json"
        ))
        .unwrap()
    })
}
fn budget() -> SimulationBudget {
    SimulationBudget {
        max_fixture_json_bytes: 16 * 1024 * 1024,
        max_accounts: 256,
        max_storage_slots: 65536,
        max_code_bytes: 8 * 1024 * 1024,
        max_image_bytes: 16 * 1024 * 1024,
        max_reads: 32000,
        max_read_bytes: 32 * 1024 * 1024,
        max_instructions: 4_000_000,
        max_input_bytes: 128 * 1024,
        max_total_input_bytes: 1024 * 1024,
        max_transaction_gas: 15_000_000,
        probe_gas_limit: 200_000,
        max_total_gas: 15_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        max_output_bytes: 1024 * 1024,
    }
}
fn fixture(c: &Value) -> FixtureState {
    serde_json::from_value(c["fixture"].clone()).unwrap()
}
fn bound(c: &Value) -> (BoundEvmExactOutExecution, Vec<Pool>) {
    let f = fixture(c);
    let pools: Vec<Pool> = serde_json::from_value(c["pools"].clone()).unwrap();
    let tokens = c["path"].as_array().unwrap();
    let edges = tokens
        .windows(2)
        .map(|a| {
            let from = a[0].as_str().unwrap();
            let to = a[1].as_str().unwrap();
            let p = pools
                .iter()
                .find(|p| {
                    (p.token0 == from && p.token1 == to) || (p.token1 == from && p.token0 == to)
                })
                .unwrap();
            Edge {
                pool_id: p.pool_id.clone(),
                protocol: p.protocol.clone(),
                zero_for_one: p.token0 == from,
                from_token: None,
                to: to.into(),
            }
        })
        .collect();
    let context = Context {
        chain_key: "evm:31337".into(),
        caller: c["transaction"]["from"].as_str().unwrap().into(),
        recipient: c["recipient"].as_str().unwrap().into(),
        sell_token: tokens[0].as_str().unwrap().into(),
        buy_token: tokens.last().unwrap().as_str().unwrap().into(),
        slippage_bps: 0,
        deadline_ms: c["deadline_ms"].as_u64().unwrap(),
    };
    let target: Amount = c["amount_out"].as_str().unwrap().parse().unwrap();
    let maximum: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
    let allocation = evaluate_allocation(
        &pools,
        &[ExactOutProposal {
            edges,
            amount_out: target.clone(),
        }],
        &target,
        &maximum,
        Funds {
            sender: &context.caller,
            recipient: &context.recipient,
        },
    )
    .unwrap();
    let anchor = &f.state_anchor;
    let snapshot = SnapshotRef {
        block: BlockRef {
            chain_key: context.chain_key.clone(),
            number_or_slot: anchor.block_number,
            hash: anchor.block_hash.clone(),
            parent_hash: anchor.parent_hash.clone(),
            timestamp_ms: anchor.timestamp * 1000,
        },
        canonical_epoch: 1,
        journal_seq: 1,
        schema_version: 1,
        adapter_set_hash: hash(&vec![v2_swap::ADAPTER]).unwrap(),
        online_generation: 1,
        scope_hash: scope_hash(&pools).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let plan = xone_execution_plan::exact_out::build(
        &context,
        &allocation,
        &snapshot,
        "original-path-bank",
    )
    .unwrap();
    let executor: ExecutorIdentity = serde_json::from_value(c["executor"].clone()).unwrap();
    let b = xone_execution_plan::bind_v2_exact_out_execution(
        &plan,
        &pools,
        &executor,
        std::slice::from_ref(&executor.target),
    )
    .unwrap();
    (b, pools)
}
fn funding(c: &Value, b: &BoundEvmExactOutExecution) -> FundingObservation {
    let f = fixture(c);
    let caller = &b.plan.context.caller;
    let account = f.accounts.iter().find(|a| &a.address == caller).unwrap();
    let token = f
        .accounts
        .iter()
        .find(|a| a.address == b.plan.context.sell_token)
        .unwrap();
    let mut outer = [0u8; 64];
    outer[12..32].copy_from_slice(Address::from_str(caller).unwrap().as_slice());
    outer[63] = 1;
    let balance_slot = format!("0x{}", hex::encode(revm::primitives::keccak256(outer)));
    outer[63] = 2;
    let mut inner = [0u8; 64];
    inner[12..32].copy_from_slice(Address::from_str(&b.executor.target).unwrap().as_slice());
    inner[32..].copy_from_slice(revm::primitives::keccak256(outer).as_slice());
    let allowance_slot = format!("0x{}", hex::encode(revm::primitives::keccak256(inner)));
    let read = |slot: &str| {
        Amount::from_be_bytes(
            &hex::decode(&token.storage.iter().find(|s| s.slot == slot).unwrap().value[2..])
                .unwrap(),
        )
        .unwrap()
    };
    FundingObservation {
        snapshot_hash: hash(&b.plan.snapshot).unwrap(),
        caller: caller.clone(),
        sell_token: b.plan.context.sell_token.clone(),
        spender: b.executor.target.clone(),
        pending_nonce: account.nonce,
        native_balance: account.balance.clone(),
        token_balance: read(&balance_slot),
        allowance: read(&allowance_slot),
        base_fee_per_gas: f.execution_environment.base_fee,
    }
}
#[test]
fn original_twelve_path_receipts_match_all_native_accounts_and_declared_storage() {
    let mut count = 0;
    for c in corpus()["cases"].as_array().unwrap() {
        compare_mined(c, true, None);
        count += 1;
    }
    assert_eq!(count, 12);
}

#[test]
#[ignore = "requires current original ordered split H/H+1 banks"]
fn eight_ordered_split_receipts_match_all_logs_and_declared_poststate() {
    let report: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_ORDERED_V2_REPORT").unwrap()).unwrap(),
    )
    .unwrap();
    let bank = &report["executionBank"];
    let runs = bank["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 8);
    for (i, r) in runs.iter().enumerate() {
        let success = i % 2 == 0;
        assert_eq!(r["status"], u64::from(success));
        let c = serde_json::json!({"id":i,"fixture":{"schema_version":1,"state_anchor":bank["state_anchor"],"accounts":bank["accounts"],"block_hashes":[],"execution_environment":r["execution_environment"]},"transaction_context":r["transaction_context"],"transaction":r,"expected":{"gas_used":r["gasUsed"],"post_accounts":r["post_accounts"]}});
        compare_mined(&c, success, Some(&r["logs"]));
    }
}

#[test]
#[ignore = "requires mined caller approval followed by ordered split swap"]
fn approval_and_swap_use_consecutive_state_nonce_and_actual_fees() {
    let report: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_ORDERED_V2_REPORT").unwrap()).unwrap(),
    )
    .unwrap();
    let bank = &report["executionBank"]["approvalReplay"];
    let sequences = bank["sequences"].as_array().unwrap();
    assert_eq!(sequences.len(), 2);
    for (seq, expected_index) in sequences.iter().zip([0, 2]) {
        assert_eq!(seq["caseIndex"], expected_index);
        let runs = seq["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
        let fixture: FixtureState = serde_json::from_value(serde_json::json!({"schema_version":1,"state_anchor":bank["state_anchor"],"accounts":bank["accounts"],"block_hashes":[],"execution_environment":runs[0]["execution_environment"]})).unwrap();
        let mut image = fixture.to_image(&budget()).unwrap();
        let caller = Address::from_str(report["binding"]["caller"].as_str().unwrap()).unwrap();
        let before = image.data().accounts[&caller].as_ref().unwrap().balance;
        let nonce = image.data().accounts[&caller].as_ref().unwrap().nonce;
        let mut parent = fixture.state_anchor.block_hash;
        let mut fees = U256::ZERO;
        let binding = &report["binding"];
        let case = &binding["cases"][expected_index];
        let token = &case["legs"][0]["tokens"][0];
        let spender = binding["executor"]["target"].as_str().unwrap();
        let amount = U256::from_str(case["input"].as_str().unwrap()).unwrap();
        assert_eq!(runs[0]["to"], *token);
        assert_eq!(
            runs[0]["data"],
            format!("0x095ea7b3{:0>64}{amount:064x}", &spender[2..])
        );
        assert_eq!(runs[1]["to"], spender);
        assert_eq!(runs[1]["data"], case["data"]);
        for (offset, r) in runs.iter().enumerate() {
            let env: ExecutionEnvironment =
                serde_json::from_value(r["execution_environment"].clone()).unwrap();
            assert_eq!(env.parent_hash, parent);
            parent = env.block_hash.clone();
            assert_eq!(r["transaction_context"]["nonce"], nonce + offset as u64);
            assert_eq!(r["from"], binding["caller"]);
            assert_eq!(r["value"], "0");
            let c = serde_json::json!({"id":expected_index,"transaction_context":r["transaction_context"],"transaction":r,"expected":{"gas_used":r["gasUsed"],"post_accounts":r["post_accounts"]}});
            image = Arc::new(replay_on_image(&c, true, Some(&r["logs"]), &env, image));
            let price = U256::from_str(r["effectiveGasPrice"].as_str().unwrap()).unwrap();
            fees += price * U256::from_str(r["gasUsed"].as_str().unwrap()).unwrap();
            assert_eq!(
                before - image.data().accounts[&caller].as_ref().unwrap().balance,
                fees
            );
        }
    }
}

fn compare_mined(c: &Value, success: bool, logs: Option<&Value>) {
    let f = fixture(c);
    replay_on_image(
        c,
        success,
        logs,
        &f.execution_environment,
        f.to_image(&budget()).unwrap(),
    );
}

fn replay_on_image(
    c: &Value,
    success: bool,
    logs: Option<&Value>,
    env: &ExecutionEnvironment,
    image: Arc<core::StateImage>,
) -> core::StateImage {
    let tx: TransactionContext = serde_json::from_value(c["transaction_context"].clone()).unwrap();
    let original = image.as_ref().clone();
    let result = core::execute(
        image.clone(),
        core::TransactionRequest {
            fork: SpecId::SHANGHAI,
            chain_id: env.chain_id,
            block: BlockEnv {
                number: U256::from(env.block_number),
                timestamp: U256::from(env.timestamp),
                coinbase: Address::from_str(&env.coinbase).unwrap(),
                gas_limit: U256::from(env.gas_limit),
                basefee: U256::from_str(&env.base_fee.to_string()).unwrap(),
                prevrandao: Some(B256::from_str(&env.prevrandao).unwrap()),
                ..Default::default()
            },
            tx: TxEnv {
                caller: Address::from_str(c["transaction"]["from"].as_str().unwrap()).unwrap(),
                transact_to: TxKind::Call(
                    Address::from_str(c["transaction"]["to"].as_str().unwrap()).unwrap(),
                ),
                data: Bytes::from(
                    hex::decode(&c["transaction"]["data"].as_str().unwrap()[2..]).unwrap(),
                ),
                nonce: Some(tx.nonce),
                chain_id: Some(tx.chain_id),
                gas_limit: tx.gas_limit,
                gas_price: U256::from_str(&tx.gas_price.to_string()).unwrap(),
                gas_priority_fee: tx
                    .max_priority_fee_per_gas
                    .as_ref()
                    .map(|p| U256::from_str(&p.to_string()).unwrap()),
                ..Default::default()
            },
        },
        core::ExecutionLimits {
            reads: core::BoundReadLimits {
                max_reads: 32000,
                max_bytes: 32 * 1024 * 1024,
            },
            max_gas_limit: 15_000_000,
            max_instructions: 4_000_000,
            max_input_bytes: 128 * 1024,
            max_memory_bytes: 64 * 1024 * 1024,
        },
        Default::default(),
    )
    .unwrap();
    let execution = &result.result_and_state.result;
    assert_eq!(execution.is_success(), success, "{:?}", execution);
    if !success {
        assert!(matches!(
            execution,
            revm::primitives::ExecutionResult::Revert { .. }
        ));
    }
    if let Some(logs) = logs {
        let actual:Vec<_>=execution.logs().iter().map(|log|serde_json::json!({"address":format!("{:#x}",log.address),"topics":log.data.topics().iter().map(|t|format!("{:#x}",t)).collect::<Vec<_>>(),"data":format!("0x{}",hex::encode(&log.data.data))})).collect();
        assert_eq!(serde_json::json!(actual), *logs);
    }
    assert_eq!(
        result.result_and_state.result.gas_used().to_string(),
        c["expected"]["gas_used"]
    );
    let post = image.apply(&result.result_and_state.state).unwrap();
    for account in
        serde_json::from_value::<Vec<FixtureAccount>>(c["expected"]["post_accounts"].clone())
            .unwrap()
    {
        let addr = Address::from_str(&account.address).unwrap();
        let actual = post.data().accounts[&addr].as_ref().unwrap();
        assert_eq!(
            actual.balance,
            U256::from_str(&account.balance.to_string()).unwrap(),
            "{} {} balance",
            c["id"],
            account.address
        );
        assert_eq!(actual.nonce, account.nonce);
        assert_eq!(
            actual.code_hash,
            revm::primitives::keccak256(hex::decode(&account.code[2..]).unwrap())
        );
        for storage in account.storage {
            assert_eq!(
                post.data().storage[&(addr, U256::from_str(&storage.slot).unwrap())],
                U256::from_str(&storage.value).unwrap(),
                "{} {} {}",
                c["id"],
                account.address,
                storage.slot
            );
        }
    }
    assert_eq!(image.as_ref(), &original);
    post
}
#[test]
fn exact_out_six_original_simple_paths_and_preparations_match_mined_transactions() {
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "EXACT_OUT")
    {
        let (b, p) = bound(c);
        let f = fixture(c);
        let tx: TransactionContext =
            serde_json::from_value(c["transaction_context"].clone()).unwrap();
        assert_eq!(b.transaction.data, c["transaction"]["data"]);
        let out = simulate_bound_exact_out(
            &b,
            &p,
            std::slice::from_ref(&b.executor.target),
            &f,
            &tx,
            &budget(),
            Default::default(),
        )
        .unwrap();
        assert!(out.matches_plan);
        assert!(!out.verified);
        assert_eq!(
            out.execution.gas_used.to_string(),
            c["expected"]["gas_used"]
        );
        assert_eq!(
            out.actual_input_consumed.to_string(),
            c["expected"]["actual_input"]
        );
        assert_eq!(
            out.actual_buy_amount.to_string(),
            c["expected"]["actual_output"]
        );
        for id in &b.touched_pool_ids {
            assert!(out
                .observations
                .iter()
                .any(|o| &o.target == id && o.phase == "pair_lock_before"));
            assert!(out
                .observations
                .iter()
                .any(|o| &o.target == id && o.phase == "pair_reserves_after"));
        }
        let observed = funding(c, &b);
        let fees = FeeContext {
            chain_id: tx.chain_id,
            nonce: tx.nonce,
            gas_limit: tx.gas_limit,
            max_fee_per_gas: tx.gas_price.clone(),
            max_priority_fee_per_gas: tx.gas_price.clone(),
            max_network_fee: Amount::checked(&tx.gas_price.0 * tx.gas_limit).unwrap(),
        };
        let component = prepare_exact_out(
            &b,
            &p,
            std::slice::from_ref(&b.executor.target),
            &observed,
            &fees,
            f.execution_environment.timestamp * 1000,
        )
        .unwrap();
        let prepared = simulate_prepared_exact_out(
            PreparedExactOutExecution {
                bound: &b,
                pools: &p,
                allowed_targets: std::slice::from_ref(&b.executor.target),
                funding: &observed,
                fees: &fees,
                preparation: &component,
            },
            &f,
            &budget(),
            Default::default(),
        )
        .unwrap();
        assert!(prepared.simulation.matches_plan);
        assert_eq!(
            prepared.simulation.execution.post_state_digest,
            out.execution.post_state_digest
        );
        count += 1;
    }
    assert_eq!(count, 6);
}
#[test]
fn exact_out_every_hop_bank_identity_code_state_and_shared_budgets_are_checked() {
    let c = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "EXACT_OUT" && c["path"].as_array().unwrap().len() == 4)
        .unwrap();
    let (b, p) = bound(c);
    let tx = serde_json::from_value(c["transaction_context"].clone()).unwrap();
    for (hop, id) in b.touched_pool_ids.iter().enumerate() {
        for mode in 0..4 {
            let mut f = fixture(c);
            let mut pools = p.clone();
            match mode {
                0 => f.accounts.retain(|a| &a.address != id),
                1 => {
                    f.accounts
                        .iter_mut()
                        .find(|a| &a.address == id)
                        .unwrap()
                        .code = "0x00".into()
                }
                2 => f
                    .accounts
                    .iter_mut()
                    .find(|a| &a.address == id)
                    .unwrap()
                    .storage
                    .clear(),
                _ => {
                    pools
                        .iter_mut()
                        .find(|p| &p.pool_id == id)
                        .unwrap()
                        .v2
                        .as_mut()
                        .unwrap()
                        .swap
                        .as_mut()
                        .unwrap()
                        .bank_block_hash = format!("0x{}", "ff".repeat(32))
                }
            }
            assert!(
                simulate_bound_exact_out(
                    &b,
                    &pools,
                    std::slice::from_ref(&b.executor.target),
                    &f,
                    &tx,
                    &budget(),
                    Default::default()
                )
                .is_err(),
                "hop={hop} mode={mode}"
            );
        }
    }
    let f = fixture(c);
    let out = simulate_bound_exact_out(
        &b,
        &p,
        std::slice::from_ref(&b.executor.target),
        &f,
        &tx,
        &budget(),
        Default::default(),
    )
    .unwrap();
    for mode in 0..3 {
        let mut limits = budget();
        match mode {
            0 => limits.max_reads = out.usage.reads - 1,
            1 => limits.max_read_bytes = out.usage.read_bytes - 1,
            _ => limits.max_instructions = out.usage.instructions - 1,
        };
        let e = simulate_bound_exact_out(
            &b,
            &p,
            std::slice::from_ref(&b.executor.target),
            &f,
            &tx,
            &limits,
            Default::default(),
        )
        .unwrap_err();
        assert_eq!(e.code, SimulationErrorCode::SimulationBudgetExceeded);
    }
}

fn input_bound(c: &Value) -> (xone_execution_plan::BoundEvmExecution, Vec<Pool>) {
    let (output, pools) = bound(c);
    let amount: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
    let route = &output.plan.allocation.routes[0];
    let allocation = xone_optimizer::evaluate_allocation(
        &pools,
        &[xone_optimizer::AllocatedRoute {
            path: xone_routing::PathQuote {
                edges: route.edges.clone(),
                tokens: route.tokens.clone(),
                amount_in: amount.clone(),
                amount_out: Amount::zero(),
            },
            amount_in: amount,
            amount_out: Amount::zero(),
        }],
        &Amount::zero(),
    )
    .unwrap();
    let plan = xone_execution_plan::build(
        &output.plan.context,
        &allocation,
        &output.plan.snapshot,
        "original-input-path-bank",
    )
    .unwrap();
    let b = xone_execution_plan::bind_v2_execution(
        &plan,
        &pools,
        &output.executor,
        std::slice::from_ref(&output.executor.target),
    )
    .unwrap();
    (b, pools)
}
#[test]
fn exact_in_six_original_simple_paths_bind_prepare_and_execute_mined_wire() {
    use xone_simulation::prepared_evm::{simulate_prepared, PreparedExecution};
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "EXACT_IN")
    {
        let (b, p) = input_bound(c);
        let (shadow, _) = bound(c);
        let f = fixture(c);
        let tx: TransactionContext =
            serde_json::from_value(c["transaction_context"].clone()).unwrap();
        assert_eq!(b.transaction.data, c["transaction"]["data"]);
        let out = simulate_bound(
            &b,
            &p,
            std::slice::from_ref(&b.executor.target),
            &f,
            &tx,
            &budget(),
            Default::default(),
        )
        .unwrap();
        assert!(out.matches_plan);
        assert_eq!(
            out.execution.gas_used.to_string(),
            c["expected"]["gas_used"]
        );
        assert_eq!(
            out.actual_input_consumed.to_string(),
            c["expected"]["actual_input"]
        );
        assert_eq!(
            out.actual_buy_amount.to_string(),
            c["expected"]["actual_output"]
        );
        for id in &b.touched_pool_ids {
            assert!(out
                .observations
                .iter()
                .any(|o| &o.target == id && o.phase == "pair_reserves_after"));
        }
        let observed = funding(c, &shadow);
        let fees = FeeContext {
            chain_id: tx.chain_id,
            nonce: tx.nonce,
            gas_limit: tx.gas_limit,
            max_fee_per_gas: tx.gas_price.clone(),
            max_priority_fee_per_gas: tx.gas_price.clone(),
            max_network_fee: Amount::checked(&tx.gas_price.0 * tx.gas_limit).unwrap(),
        };
        let component = xone_execution_preparation::evm::prepare(
            &b,
            &p,
            std::slice::from_ref(&b.executor.target),
            &observed,
            &fees,
            f.execution_environment.timestamp * 1000,
        )
        .unwrap();
        let prepared = simulate_prepared(
            PreparedExecution {
                bound: &b,
                pools: &p,
                allowed_targets: std::slice::from_ref(&b.executor.target),
                funding: &observed,
                fees: &fees,
                preparation: &component,
            },
            &f,
            &budget(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            prepared.simulation.execution.post_state_digest,
            out.execution.post_state_digest
        );
        assert_eq!(prepared.simulation.actual_buy_amount, out.actual_buy_amount);
        count += 1;
    }
    assert_eq!(count, 6);
}
#[test]
fn exact_in_path_rehashed_cycles_wrong_endpoints_and_later_hop_identities_reject() {
    let c = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "EXACT_IN" && c["path"].as_array().unwrap().len() == 4)
        .unwrap();
    let (b, p) = input_bound(c);
    for mode in 0..9 {
        let mut plan = b.plan.clone();
        let mut pools = p.clone();
        match mode {
            0 => {
                plan.allocation.routes[0].path.tokens[1] =
                    plan.allocation.routes[0].path.tokens[0].clone()
            }
            1 => {
                plan.allocation.routes[0].path.edges[1].pool_id =
                    plan.allocation.routes[0].path.edges[0].pool_id.clone()
            }
            2 => {
                plan.allocation.routes[0].path.edges[2].zero_for_one =
                    !plan.allocation.routes[0].path.edges[2].zero_for_one
            }
            3 => plan.allocation.routes[0].path.edges[1].to = b.plan.context.sell_token.clone(),
            4 => plan.allocation.routes[0]
                .path
                .tokens
                .pop()
                .map(|_| ())
                .unwrap(),
            5 => {
                pools
                    .iter_mut()
                    .find(|pool| pool.pool_id == b.touched_pool_ids[1])
                    .unwrap()
                    .v2
                    .as_mut()
                    .unwrap()
                    .factory = b.plan.context.caller.clone()
            }
            6 => {
                pools
                    .iter_mut()
                    .find(|pool| pool.pool_id == b.touched_pool_ids[2])
                    .unwrap()
                    .v2
                    .as_mut()
                    .unwrap()
                    .swap
                    .as_mut()
                    .unwrap()
                    .bank_block_hash = format!("0x{}", "ff".repeat(32))
            }
            7 => {
                pools
                    .iter_mut()
                    .find(|pool| pool.pool_id == b.touched_pool_ids[1])
                    .unwrap()
                    .token_behavior = "transfer-fee".into()
            }
            _ => plan.allocation.routes[0].amount_out = Amount::zero(),
        }
        plan.plan_hash=hash(&serde_json::json!({"context":plan.context,"snapshot":plan.snapshot,
            "allocation":plan.allocation,"min_buy_amount":plan.min_buy_amount,"economic_policy_hash":plan.economic_policy_hash})).unwrap();
        if mode != 8 {
            xone_execution_plan::verify_plan_hash(&plan).unwrap();
        }
        assert!(
            xone_execution_plan::bind_v2_execution(
                &plan,
                &pools,
                &b.executor,
                std::slice::from_ref(&b.executor.target)
            )
            .is_err(),
            "{mode}"
        );
    }
}
