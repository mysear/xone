use revm::{
    primitives::{
        keccak256, AccountInfo, Address, BlockEnv, Bytes, EVMError, ExecutionResult, HaltReason,
        SpecId, TxEnv, TxKind, B256, KECCAK_EMPTY, U256,
    },
    Database,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use xone_simulation::revm_backend::*;
fn sender() -> Address {
    Address::repeat_byte(0x11)
}
fn target() -> Address {
    Address::repeat_byte(0x22)
}
fn miner() -> Address {
    Address::repeat_byte(0x33)
}
fn info(balance: u64, nonce: u64, code_hash: B256) -> AccountInfo {
    AccountInfo {
        balance: U256::from(balance),
        nonce,
        code_hash,
        code: None,
    }
}
fn fixture(code: &[u8]) -> ImageData {
    let mut d = ImageData::default();
    let hash = keccak256(code);
    d.accounts
        .insert(sender(), Some(info(1_000_000_000_000, 7, KECCAK_EMPTY)));
    d.accounts.insert(target(), Some(info(0, 1, hash)));
    d.accounts.insert(miner(), Some(info(0, 0, KECCAK_EMPTY)));
    d.bytecodes.insert(hash, Bytes::copy_from_slice(code));
    d.storage.insert((target(), U256::ZERO), U256::ZERO);
    d
}
fn image(code: &[u8]) -> Arc<StateImage> {
    Arc::new(StateImage::new(fixture(code)).unwrap())
}
fn request() -> TransactionRequest {
    TransactionRequest {
        fork: SpecId::SHANGHAI,
        chain_id: 31337,
        block: BlockEnv {
            number: U256::from(100),
            timestamp: U256::from(1_700_000_000u64),
            coinbase: miner(),
            gas_limit: U256::from(10_000_000_000u64),
            basefee: U256::from(1),
            prevrandao: Some(B256::repeat_byte(0x55)),
            blob_excess_gas_and_price: None,
            ..BlockEnv::default()
        },
        tx: TxEnv {
            caller: sender(),
            transact_to: TxKind::Call(target()),
            nonce: Some(7),
            chain_id: Some(31337),
            gas_limit: 1_000_000,
            gas_price: U256::from(3),
            ..TxEnv::default()
        },
    }
}
fn limits() -> ExecutionLimits {
    ExecutionLimits {
        reads: BoundReadLimits {
            max_reads: 1000,
            max_bytes: 1_000_000,
        },
        max_gas_limit: 10_000_000_000,
        max_instructions: 1_000_000,
        max_input_bytes: 128_000,
        max_memory_bytes: 64 * 1024 * 1024,
    }
}
fn run(image: Arc<StateImage>) -> Result<ExecutionOutput, ExecutionError> {
    execute(image, request(), limits(), ExecutionControl::default())
}
fn reason(code: &[u8], limits: ExecutionLimits, control: ExecutionControl) -> ExecutionError {
    execute(image(code), request(), limits, control).unwrap_err()
}
const STORE_LOAD: &[u8] = &[
    0x60, 0x2a, 0x5f, 0x55, 0x5f, 0x54, 0x5f, 0x52, 0x60, 0x20, 0x5f, 0xf3,
];
const LOOP: &[u8] = &[0x5b, 0x5f, 0x56];
#[test]
fn actual_empty_call_charges_sender_gas_nonce_and_miner_tip() {
    let original = image(&[]);
    let saved = original.clone();
    let out = run(original).unwrap();
    assert!(out.result_and_state.result.is_success());
    assert_eq!(out.result_and_state.result.gas_used(), 21000);
    let sender_state = &out.result_and_state.state[&sender()];
    assert_eq!(sender_state.info.nonce, 8);
    assert_eq!(
        sender_state.info.balance,
        U256::from(1_000_000_000_000u64 - 63000)
    );
    assert_eq!(
        out.result_and_state.state[&miner()].info.balance,
        U256::from(42000)
    );
    assert_eq!(saved.data().accounts[&sender()].as_ref().unwrap().nonce, 7);
    assert_eq!(out.telemetry.calls, 1);
    assert_eq!(out.telemetry.instructions, 0);
}
#[test]
fn actual_store_load_returns_word_and_copy_overlay_preserves_original() {
    let base = image(STORE_LOAD);
    let before = base.as_ref().clone();
    let out = run(base.clone()).unwrap();
    let raw = &out.result_and_state;
    assert!(raw.result.is_success());
    assert_eq!(
        U256::from_be_slice(raw.result.output().unwrap()),
        U256::from(42)
    );
    assert_eq!(
        raw.state[&target()].storage[&U256::ZERO].present_value(),
        U256::from(42)
    );
    let post = base.apply(&raw.state).unwrap();
    assert_eq!(base.as_ref(), &before);
    assert_eq!(post.data().storage[&(target(), U256::ZERO)], U256::from(42));
    let mut followup = request();
    followup.tx.nonce = Some(8);
    assert!(execute(
        Arc::new(post),
        followup,
        limits(),
        ExecutionControl::default()
    )
    .unwrap()
    .result_and_state
    .result
    .is_success());
    assert_eq!(out.telemetry.initializations, 1);
    assert_eq!(out.telemetry.instructions, 10);
}
#[test]
fn revert_undoes_storage_but_keeps_real_nonce_and_gas_effects() {
    let base = image(&[
        0x60, 0x2a, 0x5f, 0x55, 0x60, 0x2a, 0x5f, 0x52, 0x60, 0x20, 0x5f, 0xfd,
    ]);
    let out = run(base.clone()).unwrap();
    assert!(matches!(
        out.result_and_state.result,
        ExecutionResult::Revert { .. }
    ));
    assert_eq!(
        U256::from_be_slice(out.result_and_state.result.output().unwrap()),
        U256::from(42)
    );
    let post = base.apply(&out.result_and_state.state).unwrap();
    assert_eq!(post.data().storage[&(target(), U256::ZERO)], U256::ZERO);
    assert_eq!(post.data().accounts[&sender()].as_ref().unwrap().nonce, 8);
    assert!(
        post.data().accounts[&sender()].as_ref().unwrap().balance
            < base.data().accounts[&sender()].as_ref().unwrap().balance
    );
}
#[test]
fn missing_account_slot_code_and_blockhash_are_execution_errors() {
    for (mut d, expected) in [
        (fixture(&[0x5f, 0x54, 0]), "unknown_storage"),
        (fixture(&[0x60, 99, 0x40, 0]), "unknown_block_hash"),
        (fixture(&[0]), "unknown_code"),
        (fixture(&[0]), "unknown_account"),
    ] {
        match expected {
            "unknown_storage" => d.storage.clear(),
            "unknown_code" => d.bytecodes.clear(),
            "unknown_account" => {
                d.accounts.remove(&target());
                d.storage.clear();
            }
            _ => {}
        }
        assert_eq!(
            run(Arc::new(StateImage::new(d).unwrap()))
                .unwrap_err()
                .reason(),
            expected
        );
    }
}
#[test]
fn declared_absent_call_target_is_valid_empty_call() {
    let mut d = fixture(&[0]);
    d.accounts.insert(target(), None);
    d.storage.clear();
    assert!(run(Arc::new(StateImage::new(d).unwrap()))
        .unwrap()
        .result_and_state
        .result
        .is_success());
}
#[test]
fn prior_block_hash_is_actual_and_out_of_window_zero_is_evm_semantics() {
    let mut d = fixture(&[0x60, 99, 0x40, 0x5f, 0x52, 0x60, 32, 0x5f, 0xf3]);
    d.block_hashes.insert(99, B256::repeat_byte(0x44));
    let out = run(Arc::new(StateImage::new(d).unwrap())).unwrap();
    assert_eq!(
        out.result_and_state.result.output().unwrap().as_ref(),
        &[0x44; 32]
    );
    assert_eq!(out.telemetry.reads.block_hash, 1);
    let out = run(image(&[0x60, 100, 0x40, 0x5f, 0x52, 0x60, 32, 0x5f, 0xf3])).unwrap();
    assert_eq!(
        out.result_and_state.result.output().unwrap().as_ref(),
        &[0; 32]
    );
    assert_eq!(out.telemetry.reads.block_hash, 0);
}
#[test]
fn cancellation_before_entry_and_expired_deadline_fail_without_reads() {
    let control = ExecutionControl {
        cancellation: Arc::new(AtomicBool::new(true)),
        ..ExecutionControl::default()
    };
    let err = reason(LOOP, limits(), control);
    assert_eq!(err.reason(), "cancelled");
    assert_eq!(err.telemetry.reads.reads, 0);
    let control = ExecutionControl {
        deadline: Some(Instant::now()),
        ..ExecutionControl::default()
    };
    assert_eq!(reason(LOOP, limits(), control).reason(), "deadline");
}
#[test]
fn instruction_budget_is_exact_fatal_and_not_a_fabricated_receipt() {
    let mut l = limits();
    l.max_instructions = 37;
    let err = reason(LOOP, l, ExecutionControl::default());
    assert_eq!(err.reason(), "instruction_limit");
    assert_eq!(err.telemetry.instructions, 37);
    assert_eq!(err.telemetry.initializations, 1);
    assert_eq!(err.telemetry.calls, 1);
    let mut l = limits();
    l.max_instructions = 0;
    let err = reason(&[], l, ExecutionControl::default());
    assert_eq!(err.reason(), "instruction_limit");
    assert_eq!(err.telemetry.calls, 1);
    let mut l = limits();
    l.max_instructions = 1;
    assert!(
        execute(image(&[0]), request(), l, ExecutionControl::default())
            .unwrap()
            .result_and_state
            .result
            .is_success()
    );
}
#[test]
fn cancellation_after_observed_live_steps_interrupts_loop() {
    let progress = Arc::new(AtomicU64::new(0));
    let flag = Arc::new(AtomicBool::new(false));
    let control = ExecutionControl {
        cancellation: flag.clone(),
        progress: Some(progress.clone()),
        deadline: Some(Instant::now() + Duration::from_secs(5)),
    };
    let worker = std::thread::spawn(move || {
        let mut r = request();
        r.tx.gas_limit = 1_000_000_000;
        let mut l = limits();
        l.max_instructions = 100_000_000;
        execute(image(LOOP), r, l, control)
    });
    let until = Instant::now() + Duration::from_secs(3);
    while progress.load(Ordering::Acquire) < 1024 && Instant::now() < until {
        std::thread::yield_now();
    }
    let observed = progress.load(Ordering::Acquire);
    flag.store(true, Ordering::Release);
    let err = worker.join().unwrap().unwrap_err();
    assert!(observed >= 1024, "worker never entered interpreter");
    assert_eq!(err.reason(), "cancelled");
    assert!(err.telemetry.instructions >= observed);
    assert!(err.telemetry.instructions < 100_000_000);
}
#[test]
fn deadline_interrupts_live_loop_with_real_steps() {
    let control = ExecutionControl {
        deadline: Some(Instant::now() + Duration::from_millis(40)),
        ..ExecutionControl::default()
    };
    let mut r = request();
    r.tx.gas_limit = 1_000_000_000;
    let mut l = limits();
    l.max_instructions = 100_000_000;
    let err = execute(image(LOOP), r, l, control).unwrap_err();
    assert_eq!(err.reason(), "deadline");
    assert!(err.telemetry.instructions > 0);
}
#[test]
fn engine_read_and_byte_exhaustion_are_typed() {
    let mut l = limits();
    l.reads.max_reads = 0;
    assert_eq!(
        reason(STORE_LOAD, l, ExecutionControl::default()).reason(),
        "read_limit"
    );
    let mut l = limits();
    l.reads.max_bytes = 0;
    assert_eq!(
        reason(STORE_LOAD, l, ExecutionControl::default()).reason(),
        "byte_limit"
    );
    // Code itself must be metered; compare receipt-identical programs of different length.
    let small = run(image(&[0])).unwrap().telemetry.reads.bytes;
    let long = [0u8; 8192];
    let mut l = limits();
    l.reads.max_bytes = small + 100;
    assert_eq!(
        reason(&long, l, ExecutionControl::default()).reason(),
        "byte_limit"
    );
}
#[test]
fn gas_ceiling_intrinsic_gas_and_ordinary_out_of_gas_are_distinct() {
    let mut r = request();
    r.tx.gas_limit = 0;
    assert_eq!(
        execute(image(&[0]), r, limits(), ExecutionControl::default())
            .unwrap_err()
            .reason(),
        "gas_limit"
    );
    let mut l = limits();
    l.max_gas_limit = 500;
    assert_eq!(
        reason(&[0], l, ExecutionControl::default()).reason(),
        "gas_limit"
    );
    let mut r = request();
    r.tx.gas_limit = 21001;
    let out = execute(image(LOOP), r, limits(), ExecutionControl::default()).unwrap();
    assert!(matches!(
        out.result_and_state.result,
        ExecutionResult::Halt {
            reason: HaltReason::OutOfGas(_),
            gas_used: 21001
        }
    ));
    let mut r = request();
    r.tx.gas_limit = 20000;
    assert_eq!(
        execute(image(&[0]), r, limits(), ExecutionControl::default())
            .unwrap_err()
            .reason(),
        "transaction_validation"
    );
}
#[test]
fn actual_sender_nonce_balance_fees_and_block_gas_are_validated() {
    for mode in 0..5 {
        let mut r = request();
        let mut d = fixture(&[0]);
        match mode {
            0 => r.tx.nonce = Some(8),
            1 => {
                d.accounts
                    .get_mut(&sender())
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .balance = U256::ZERO
            }
            2 => r.tx.gas_price = U256::ZERO,
            3 => r.block.gas_limit = U256::from(21000),
            _ => r.tx.nonce = Some(6),
        }
        let err = execute(
            Arc::new(StateImage::new(d).unwrap()),
            r,
            limits(),
            ExecutionControl::default(),
        )
        .unwrap_err();
        assert_eq!(err.reason(), "transaction_validation");
        assert!(
            matches!(err.failure, ExecutionFailure::Evm(error) if matches!(*error, EVMError::Transaction(_)))
        );
    }
}
#[test]
fn fork_is_explicit_and_only_shanghai_is_supported() {
    for fork in [
        SpecId::FRONTIER,
        SpecId::MERGE,
        SpecId::CANCUN,
        SpecId::PRAGUE,
        SpecId::LATEST,
    ] {
        let mut r = request();
        r.fork = fork;
        let err = execute(image(&[0]), r, limits(), ExecutionControl::default()).unwrap_err();
        assert_eq!(err.reason(), "unsupported_fork");
        assert_eq!(err.telemetry.reads.reads, 0);
    }
    for opcode in [0x49, 0x4a, 0x5c, 0x5d, 0x5e] {
        let out = run(image(&[opcode, 0])).unwrap();
        assert!(
            matches!(
                out.result_and_state.result,
                ExecutionResult::Halt {
                    reason: HaltReason::NotActivated | HaltReason::OpcodeNotFound,
                    ..
                }
            ),
            "higher fork opcode {opcode:x} unexpectedly enabled or halted for the wrong reason"
        );
    }
}
#[test]
fn explicit_chain_and_nonce_are_required() {
    for mode in 0..3 {
        let mut r = request();
        let expected = match mode {
            0 => {
                r.tx.nonce = None;
                "missing_nonce"
            }
            1 => {
                r.tx.chain_id = None;
                "missing_chain_id"
            }
            _ => {
                r.tx.chain_id = Some(1);
                "chain_id_mismatch"
            }
        };
        assert_eq!(
            execute(image(&[0]), r, limits(), ExecutionControl::default())
                .unwrap_err()
                .reason(),
            expected
        );
    }
}
#[test]
fn requested_chain_block_and_sender_context_reach_actual_opcodes() {
    let ops = [0x46, 0x43, 0x42, 0x41, 0x48, 0x44, 0x32, 0x33];
    let mut code = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        code.extend([*op, 0x60, (i * 32) as u8, 0x52]);
    }
    code.extend([0x61, 1, 0, 0x5f, 0xf3]);
    let out = run(image(&code)).unwrap();
    let output = out.result_and_state.result.output().unwrap();
    let words: Vec<_> = output.chunks_exact(32).map(U256::from_be_slice).collect();
    let addr = |a: Address| U256::from_be_slice(a.as_slice());
    assert_eq!(
        words,
        [
            U256::from(31337),
            U256::from(100),
            U256::from(1_700_000_000u64),
            addr(miner()),
            U256::from(1),
            U256::from_be_slice(&[0x55; 32]),
            addr(sender()),
            addr(sender())
        ]
    );
}
// The caller deliberately ignores a CALL failure. Exhaustion in a nested frame must still abort the whole transaction.
fn call_program(callee: Address) -> Vec<u8> {
    let mut code = vec![0x5f, 0x5f, 0x5f, 0x5f, 0x5f, 0x73];
    code.extend(callee.as_slice());
    code.extend([0x5a, 0xf1, 0x50, 0x60, 0x63, 0x5f, 0x55, 0]);
    code
}
fn nested(callee_code: &[u8]) -> Arc<StateImage> {
    let callee = Address::repeat_byte(0x44);
    let mut d = fixture(&call_program(callee));
    let hash = keccak256(callee_code);
    d.accounts.insert(callee, Some(info(0, 1, hash)));
    d.bytecodes
        .insert(hash, Bytes::copy_from_slice(callee_code));
    d.storage.insert((callee, U256::ZERO), U256::ZERO);
    Arc::new(StateImage::new(d).unwrap())
}
#[test]
fn nested_storage_call_commits_real_effects_on_copy() {
    let callee = Address::repeat_byte(0x44);
    let base = nested(&[0x60, 42, 0x5f, 0x55, 0]);
    let out = run(base.clone()).unwrap();
    assert!(out.result_and_state.result.is_success());
    let post = base.apply(&out.result_and_state.state).unwrap();
    assert_eq!(post.data().storage[&(callee, U256::ZERO)], U256::from(42));
    assert_eq!(post.data().storage[&(target(), U256::ZERO)], U256::from(99));
    assert_eq!(base.data().storage[&(callee, U256::ZERO)], U256::ZERO);
    assert_eq!(out.telemetry.calls, 2);
    assert_eq!(out.telemetry.initializations, 2);
}
#[test]
fn nested_loop_budget_abort_cannot_be_swallowed_by_parent_call() {
    let mut l = limits();
    l.max_instructions = 64;
    let err = execute(nested(LOOP), request(), l, ExecutionControl::default()).unwrap_err();
    assert_eq!(err.reason(), "instruction_limit");
    assert_eq!(err.telemetry.instructions, 64);
    assert_eq!(err.telemetry.calls, 2);
}
#[test]
fn call_boundary_enforces_exhaustion_before_entering_nested_interpreter() {
    let mut l = limits();
    l.max_instructions = 8; // five PUSH0 + PUSH20 + GAS + CALL
    let err = execute(nested(&[0]), request(), l, ExecutionControl::default()).unwrap_err();
    assert_eq!(err.reason(), "instruction_limit");
    assert_eq!(err.telemetry.calls, 2);
    assert_eq!(err.telemetry.initializations, 1);
}
#[test]
fn applied_overlay_keeps_unknown_slots_strict() {
    let base = image(STORE_LOAD);
    let out = run(base.clone()).unwrap();
    let copy = Arc::new(base.apply(&out.result_and_state.state).unwrap());
    let mut db = StrictDB::new(copy, limits().reads, ExecutionControl::default());
    assert_eq!(
        db.storage(target(), U256::from(1)),
        Err(DbError::UnknownStorage {
            address: target(),
            slot: U256::from(1)
        })
    );
}

#[test]
fn calldata_byte_ceiling_is_checked_before_state_reads_and_allows_exact_limit() {
    let code = &[0x36, 0x5f, 0x52, 0x60, 32, 0x5f, 0xf3]; // return actual CALLDATASIZE
    let mut r = request();
    r.tx.data = Bytes::from(vec![0xab; 64]);
    let mut l = limits();
    l.max_input_bytes = 64;
    let out = execute(image(code), r.clone(), l, ExecutionControl::default()).unwrap();
    assert_eq!(
        U256::from_be_slice(out.result_and_state.result.output().unwrap()),
        U256::from(64)
    );
    l.max_input_bytes = 63;
    let err = execute(image(code), r, l, ExecutionControl::default()).unwrap_err();
    assert_eq!(err.reason(), "input_limit");
    assert_eq!(err.telemetry.reads.reads, 0);
}

#[test]
fn allowance_like_guard_reads_real_caller_mapping_and_missing_allowance_fails() {
    // Handcoded guard for mapping(address => uint256) at slot 1; deliberately not an ERC20/V2 model.
    let mut code = vec![
        0x60, 10, 0x33, 0x5f, 0x52, 0x60, 1, 0x60, 32, 0x52, 0x60, 64, 0x5f, 0x20, 0x54, 0x10,
    ];
    let branch = code.len();
    code.extend([0x60, 0, 0x57]);
    code.extend([0x60, 42, 0x5f, 0x52, 0x60, 32, 0x5f, 0xf3]);
    code[branch + 1] = code.len() as u8;
    code.extend([0x5b, 0x5f, 0x5f, 0xfd]);
    let mut key = [0u8; 64];
    key[12..32].copy_from_slice(sender().as_slice());
    key[63] = 1;
    let slot = U256::from_be_slice(keccak256(key).as_slice());
    for allowance in [0u64, 9, 10, 11] {
        let mut d = fixture(&code);
        d.storage.insert((target(), slot), U256::from(allowance));
        let out = run(Arc::new(StateImage::new(d).unwrap())).unwrap();
        assert_eq!(out.result_and_state.result.is_success(), allowance >= 10);
        if allowance < 10 {
            assert!(matches!(
                out.result_and_state.result,
                ExecutionResult::Revert { .. }
            ));
        }
        assert_eq!(out.telemetry.reads.storage, 1);
    }
    assert_eq!(run(image(&code)).unwrap_err().reason(), "unknown_storage");
}

#[test]
fn memory_ceiling_is_fatal_without_publishing_a_partial_state() {
    // PUSH1 1; PUSH3 65536; MSTORE; STOP. Ordinary gas permits expansion,
    // but this request's independent memory budget does not.
    let code = [0x60, 0x01, 0x62, 0x01, 0x00, 0x00, 0x52, 0x00];
    let mut budget = limits();
    budget.max_memory_bytes = 65536;
    let error = execute(image(&code), request(), budget, ExecutionControl::default()).unwrap_err();
    assert_eq!(error.reason(), "memory_limit");
}

#[test]
fn zero_or_unreasonable_memory_policy_is_rejected_before_reads() {
    for maximum in [0, 256 * 1024 * 1024 + 1] {
        let mut budget = limits();
        budget.max_memory_bytes = maximum;
        let error =
            execute(image(&[0]), request(), budget, ExecutionControl::default()).unwrap_err();
        assert_eq!(error.reason(), "invalid_memory_limit");
        assert_eq!(error.telemetry.reads.reads, 0);
    }
}

#[test]
fn nested_memory_ceiling_cannot_be_swallowed_by_parent_call() {
    let code = [0x60, 0x01, 0x62, 0x01, 0x00, 0x00, 0x52, 0x00];
    let mut budget = limits();
    budget.max_memory_bytes = 65536;
    let error = execute(
        nested(&code),
        request(),
        budget,
        ExecutionControl::default(),
    )
    .unwrap_err();
    assert_eq!(error.reason(), "memory_limit");
    assert_eq!(error.telemetry.calls, 2);
}
