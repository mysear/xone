//! Unit controls only, not an AMM oracle or V2 equivalence.
use serde_json::json;
use xone_simulation::{evm_fixture::*, revm_backend};
fn budget() -> SimulationBudget {
    SimulationBudget {
        max_fixture_json_bytes: 1_000_000,
        max_accounts: 64,
        max_storage_slots: 256,
        max_code_bytes: 64_000,
        max_image_bytes: 100_000,
        max_reads: 10_000,
        max_read_bytes: 2_000_000,
        max_instructions: 1_000_000,
        max_input_bytes: 4096,
        max_total_input_bytes: 64_000,
        max_transaction_gas: 1_000_000,
        probe_gas_limit: 100_000,
        max_total_gas: 5_000_000,
        max_memory_bytes: 1024 * 1024,
        max_output_bytes: 16 * 1024,
    }
}
fn fixture_json() -> serde_json::Value {
    json!({"schema_version":1,
"state_anchor":{"chain_key":"evm:31337","block_number":100,"block_hash":format!("0x{}","11".repeat(32)),"parent_hash":format!("0x{}","10".repeat(32)),"timestamp":1792886400u64,"genesis_hash":format!("0x{}","aa".repeat(32))},
"execution_environment":{"chain_id":31337,"fork":"shanghai","block_number":101,"block_hash":format!("0x{}","12".repeat(32)),"parent_hash":format!("0x{}","11".repeat(32)),"timestamp":1792886401u64,"coinbase":format!("0x{}","99".repeat(20)),"gas_limit":10_000_000,"base_fee":"1","prevrandao":format!("0x{}","55".repeat(32))},
"accounts":[{"address":format!("0x{}","22".repeat(20)),"exists":true,"balance":"123","nonce":7,"code":"0x00","storage":[{"slot":format!("0x{}","00".repeat(32)),"value":format!("0x{}","00".repeat(32))}]}],"block_hashes":[{"number":100,"hash":format!("0x{}","11".repeat(32))}]})
}
fn fixture() -> FixtureState {
    serde_json::from_value(fixture_json()).unwrap()
}
#[test]
fn wire_is_strict_and_rejects_native_case_metadata() {
    assert_eq!(
        parse_fixture_json(&serde_json::to_vec(&fixture_json()).unwrap(), &budget())
            .unwrap()
            .schema_version,
        1
    );
    for path in ["root", "account", "storage", "environment", "anchor"] {
        let mut v = fixture_json();
        match path {
            "root" => v["native_transaction"] = json!({}),
            "account" => v["accounts"][0]["extra"] = true.into(),
            "storage" => v["accounts"][0]["storage"][0]["extra"] = true.into(),
            "environment" => v["execution_environment"]["extra"] = true.into(),
            _ => v["state_anchor"]["extra"] = true.into(),
        };
        assert!(
            parse_fixture_json(&serde_json::to_vec(&v).unwrap(), &budget()).is_err(),
            "{path}"
        );
    }
    let mut v = fixture_json();
    v["accounts"][0]["balance"] = 123.into();
    assert!(parse_fixture_json(&serde_json::to_vec(&v).unwrap(), &budget()).is_err());
}
#[test]
fn declared_zero_preserved_and_undeclared_slot_unknown() {
    use revm::{
        primitives::{Address, U256},
        Database,
    };
    let mut db = revm_backend::StrictDB::new(
        fixture().to_image(&budget()).unwrap(),
        revm_backend::BoundReadLimits {
            max_reads: 10,
            max_bytes: 1000,
        },
        revm_backend::ExecutionControl::default(),
    );
    assert_eq!(
        db.storage(Address::repeat_byte(0x22), U256::ZERO).unwrap(),
        U256::ZERO
    );
    assert!(db
        .storage(Address::repeat_byte(0x22), U256::from(1))
        .is_err());
}
#[test]
fn duplicates_rejected_before_map_conversion() {
    let mut f = fixture();
    f.accounts.push(f.accounts[0].clone());
    assert_eq!(
        f.to_image(&budget()).unwrap_err().code,
        SimulationErrorCode::SimulationDuplicateAccount
    );
    let mut f = fixture();
    let row = f.accounts[0].storage[0].clone();
    f.accounts[0].storage.push(row);
    assert_eq!(
        f.to_image(&budget()).unwrap_err().code,
        SimulationErrorCode::SimulationDuplicateStorage
    );
    let mut f = fixture();
    f.block_hashes.push(f.block_hashes[0].clone());
    assert_eq!(
        f.to_image(&budget()).unwrap_err().code,
        SimulationErrorCode::SimulationDuplicateBlockHash
    );
}
#[test]
fn image_caps_apply_before_decode() {
    for k in 0..5 {
        let mut b = budget();
        match k {
            0 => b.max_accounts = 0,
            1 => b.max_storage_slots = 0,
            2 => b.max_code_bytes = 0,
            3 => b.max_image_bytes = 0,
            _ => b.max_fixture_json_bytes = 1,
        };
        let r = if k == 4 {
            parse_fixture_json(&serde_json::to_vec(&fixture_json()).unwrap(), &b).map(|_| ())
        } else {
            fixture().to_image(&b).map(|_| ())
        };
        assert_eq!(
            r.unwrap_err().code,
            SimulationErrorCode::SimulationFixtureLimit
        );
    }
}
#[test]
fn canonical_widths_schema_absence_and_amount_overflow_fail() {
    for k in 0..8 {
        let mut f = fixture();
        match k {
            0 => f.schema_version = 2,
            1 => f.accounts[0].address = "0x01".into(),
            2 => f.accounts[0].code = "0x0".into(),
            3 => f.accounts[0].storage[0].slot = "0x0".into(),
            4 => f.accounts[0].storage[0].value = format!("0x{}", "AA".repeat(32)),
            5 => f.accounts[0].exists = false,
            6 => f.state_anchor.genesis_hash = "0x00".into(),
            _ => f.block_hashes[0].hash = "0x00".into(),
        };
        assert!(f.to_image(&budget()).is_err(), "{k}");
    }
    let mut v = fixture_json();
    v["accounts"][0]["balance"] = "9".repeat(79).into();
    assert!(serde_json::from_value::<FixtureState>(v).is_err());
}

use revm::primitives::{keccak256, U256};
use sha2::{Digest, Sha256};
use xone_domain::{hash, protocols::V2State, scope_hash, Amount, Pool, Snapshot};
use xone_execution_plan::{bind_v2_execution, build, BoundEvmExecution, Context, ExecutorIdentity};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};
const CALLER: &str = "0x0000000000000000000000000000000000000011";
const RECIPIENT: &str = "0x0000000000000000000000000000000000000022";
const TOKEN0: &str = "0x0000000000000000000000000000000000000033";
const TOKEN1: &str = "0x0000000000000000000000000000000000000044";
const PAIR: &str = "0x0000000000000000000000000000000000000055";
const FACTORY: &str = "0x0000000000000000000000000000000000000066";
const ROUTER: &str = "0x0000000000000000000000000000000000000077";
fn selector(s: &str) -> [u8; 4] {
    keccak256(s.as_bytes())[..4].try_into().unwrap()
}
fn addr_word(s: &str) -> [u8; 32] {
    let mut b = [0; 32];
    b[12..].copy_from_slice(&hex::decode(&s[2..]).unwrap());
    b
}
fn num(n: u64) -> [u8; 32] {
    U256::from(n).to_be_bytes()
}
fn ret(words: &[[u8; 32]]) -> Vec<u8> {
    let mut code = vec![];
    for (i, w) in words.iter().enumerate() {
        code.push(0x7f);
        code.extend(w);
        code.extend([0x60, (i * 32) as u8, 0x52]);
    }
    code.extend([0x60, (words.len() * 32) as u8, 0x5f, 0xf3]);
    code
}
fn dispatch(entries: Vec<(&str, Vec<u8>)>, default: Vec<u8>) -> Vec<u8> {
    let mut code = vec![0x5f, 0x35, 0x60, 0xe0, 0x1c];
    let mut positions = vec![];
    for (sig, _) in &entries {
        code.extend([0x80, 0x63]);
        code.extend(selector(sig));
        code.extend([0x14, 0x61, 0, 0, 0x57]);
        positions.push(code.len() - 3);
    }
    code.push(0x50);
    code.extend(default);
    for ((_, body), p) in entries.into_iter().zip(positions) {
        let dst = (code.len() as u16).to_be_bytes();
        code[p..p + 2].copy_from_slice(&dst);
        code.extend([0x5b, 0x50]);
        code.extend(body);
    }
    code
}
fn token_code() -> Vec<u8> {
    // Deliberately unauthenticated setter is solely a unit control, not standard token code.
    dispatch(
        vec![(
            "balanceOf(address)",
            vec![0x60, 4, 0x35, 0x54, 0x5f, 0x52, 0x60, 32, 0x5f, 0xf3],
        )],
        vec![0x60, 36, 0x35, 0x60, 4, 0x35, 0x55, 0],
    )
}
fn set_balance(code: &mut Vec<u8>, token: &str, owner: &str, value: u64) {
    let mut sel = [0; 32];
    sel[..4].copy_from_slice(&selector("setBalance(address,uint256)"));
    for (offset, w) in [(0, sel), (4, addr_word(owner)), (36, num(value))] {
        code.push(0x7f);
        code.extend(w);
        code.extend([0x60, offset, 0x52]);
    }
    code.extend([0x5f, 0x5f, 0x60, 68, 0x5f, 0x5f, 0x73]);
    code.extend(hex::decode(&token[2..]).unwrap());
    code.extend([0x62, 1, 0x86, 0xa0, 0xf1, 0x50]);
}
fn account(
    addr: &str,
    code: Vec<u8>,
    balance: u64,
    nonce: u64,
    slots: Vec<([u8; 32], [u8; 32])>,
) -> FixtureAccount {
    FixtureAccount {
        address: addr.into(),
        exists: true,
        balance: balance.to_string().parse().unwrap(),
        nonce,
        code: format!("0x{}", hex::encode(code)),
        storage: slots
            .into_iter()
            .map(|(slot, value)| StorageEntry {
                slot: format!("0x{}", hex::encode(slot)),
                value: format!("0x{}", hex::encode(value)),
            })
            .collect(),
    }
}
fn sha(code: &[u8]) -> String {
    hex::encode(Sha256::digest(code))
}
struct Case {
    bound: BoundEvmExecution,
    pools: Vec<Pool>,
    fixture: FixtureState,
    tx: TransactionContext,
}
fn case(buy_delta: u64, revert: bool) -> Case {
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.pools.truncate(1);
    let mut pair_entries = vec![
        ("factory()", ret(&[addr_word(FACTORY)])),
        ("token0()", ret(&[addr_word(TOKEN0)])),
        ("token1()", ret(&[addr_word(TOKEN1)])),
    ];
    pair_entries.push((
        "getReserves()",
        ret(&[num(1_000_000), num(2_000_000), num(1792886400)]),
    ));
    let pair_code = dispatch(pair_entries, vec![0x5f, 0x5f, 0xfd]);
    let factory_code = ret(&[addr_word(PAIR)]);
    let mut swap = vec![];
    set_balance(&mut swap, TOKEN0, CALLER, 90_000);
    set_balance(&mut swap, TOKEN1, RECIPIENT, 7 + buy_delta);
    swap.extend(if revert {
        vec![0x5f, 0x5f, 0xfd]
    } else {
        vec![0]
    });
    let router_code = dispatch(vec![("factory()", ret(&[addr_word(FACTORY)]))], swap);
    let pool = &mut snapshot.pools[0];
    pool.pool_id = PAIR.into();
    pool.token0 = TOKEN0.into();
    pool.token1 = TOKEN1.into();
    pool.reserve1 = "2000000".parse().unwrap();
    pool.v2 = Some(V2State {
        swap: None,
        factory: FACTORY.into(),
        code_sha256: sha(&pair_code),
    });
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    snapshot.reference.adapter_set_hash = hash(&vec!["uniswap-v2/1"]).unwrap();
    let sell: Amount = "10000".parse().unwrap();
    let routes = vec![AllocatedRoute {
        path: PathQuote {
            amount_in: sell.clone(),
            amount_out: Amount::zero(),
            tokens: vec![TOKEN0.into(), TOKEN1.into()],
            edges: vec![Edge {
                pool_id: PAIR.into(),
                protocol: "uniswap-v2".into(),
                zero_for_one: true,
                from_token: None,
                to: TOKEN1.into(),
            }],
        },
        amount_in: sell,
        amount_out: Amount::zero(),
    }];
    let allocation = evaluate_allocation(&snapshot.pools, &routes, &Amount::zero()).unwrap();
    let plan = build(
        &Context {
            chain_key: "evm:31337".into(),
            caller: CALLER.into(),
            recipient: RECIPIENT.into(),
            sell_token: TOKEN0.into(),
            buy_token: TOKEN1.into(),
            slippage_bps: 50,
            deadline_ms: snapshot.reference.block.timestamp_ms + 60_000,
        },
        &allocation,
        &snapshot.reference,
        "policy",
    )
    .unwrap();
    let bound = bind_v2_execution(
        &plan,
        &snapshot.pools,
        &ExecutorIdentity {
            target: ROUTER.into(),
            code_sha256: sha(&router_code),
            factory: FACTORY.into(),
            factory_code_sha256: sha(&factory_code),
            adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
        },
        &[ROUTER.into()],
    )
    .unwrap();
    let mut f = fixture();
    let coinbase = f.execution_environment.coinbase.clone();
    f.accounts = vec![
        account(CALLER, vec![], 1_000_000_000_000, 7, vec![]),
        account(RECIPIENT, vec![], 0, 0, vec![]),
        account(&coinbase, vec![], 0, 0, vec![]),
        account(ROUTER, router_code, 0, 1, vec![]),
        account(FACTORY, factory_code, 0, 1, vec![]),
        account(PAIR, pair_code, 0, 1, vec![]),
        account(
            TOKEN0,
            token_code(),
            0,
            1,
            vec![
                (addr_word(CALLER), num(100_000)),
                (addr_word(PAIR), num(1_000_000)),
            ],
        ),
        account(
            TOKEN1,
            token_code(),
            0,
            1,
            vec![
                (addr_word(RECIPIENT), num(7)),
                (addr_word(PAIR), num(2_000_000)),
            ],
        ),
    ];
    Case {
        bound,
        pools: snapshot.pools,
        fixture: f,
        tx: TransactionContext {
            nonce: 7,
            gas_limit: 500_000,
            gas_price: "3".parse().unwrap(),
            chain_id: 31337,
        },
    }
}
fn run(c: &Case) -> Result<SimulationOutcome> {
    simulate_bound(
        &c.bound,
        &c.pools,
        &[ROUTER.into()],
        &c.fixture,
        &c.tx,
        &budget(),
        revm_backend::ExecutionControl::default(),
    )
}
#[test]
fn source_anchor_and_execution_environment_are_never_mixed() {
    for k in 0..10 {
        let mut c = case(5, false);
        match k {
            0 => c.fixture.state_anchor.block_number += 1,
            1 => c.fixture.state_anchor.block_hash = format!("0x{}", "77".repeat(32)),
            2 => c.fixture.state_anchor.parent_hash = format!("0x{}", "77".repeat(32)),
            3 => c.fixture.state_anchor.timestamp += 1,
            4 => c.fixture.execution_environment.block_number = 100,
            5 => c.fixture.execution_environment.parent_hash = format!("0x{}", "10".repeat(32)),
            6 => c.fixture.execution_environment.fork = "cancun".into(),
            7 => c.fixture.execution_environment.chain_id = 1,
            8 => c.fixture.execution_environment.timestamp -= 100,
            _ => c.fixture.state_anchor.timestamp = u64::MAX,
        };
        assert!(run(&c).is_err(), "{k}");
    }
}
#[test]
fn actual_balances_not_quotes_and_probe_nonce_changes_are_discarded() {
    let c = case(5, false);
    let before = serde_json::to_value(&c.fixture).unwrap();
    let out = run(&c).unwrap();
    assert_eq!(out.execution.status, EvmStatus::Success);
    assert_eq!(out.actual_buy_amount.to_string(), "5");
    assert_eq!(out.actual_input_consumed.to_string(), "10000");
    assert!(!out.matches_plan);
    assert_ne!(
        out.actual_buy_amount,
        c.bound.plan.allocation.gross_buy_amount
    );
    assert_eq!(out.balances.caller_sell_before.to_string(), "100000");
    assert_eq!(out.balances.caller_sell_after.to_string(), "90000");
    assert_eq!(out.balances.recipient_buy_before.to_string(), "7");
    assert_eq!(out.balances.recipient_buy_after.to_string(), "12");
    assert_eq!(out.execution.sender_nonce_before, 7);
    assert_eq!(out.execution.sender_nonce_after, 8);
    assert_eq!(out.authority, DataAuthority::ProvidedLocalState);
    assert_eq!(serde_json::to_value(&c.fixture).unwrap(), before);
    assert_eq!(out.observations.len(), 12);
    assert_eq!(out.usage.executions, 13);
    assert_eq!(out.receipt_hash.len(), 64);
}
#[test]
fn ordinary_revert_keeps_actual_gas_nonce_and_zero_deltas() {
    let out = run(&case(5, true)).unwrap();
    assert_eq!(out.execution.status, EvmStatus::Revert);
    assert!(!out.matches_plan);
    assert_eq!(out.actual_buy_amount, Amount::zero());
    assert_eq!(out.actual_input_consumed, Amount::zero());
    assert_eq!(out.execution.sender_nonce_after, 8);
    assert!(out.execution.gas_used > 21000);
}
#[test]
fn runtime_sha256_checks_router_factory_and_pair() {
    for target in [ROUTER, FACTORY, PAIR] {
        let mut c = case(5, false);
        c.fixture
            .accounts
            .iter_mut()
            .find(|a| a.address == target)
            .unwrap()
            .code = "0x00".into();
        assert_eq!(
            run(&c).unwrap_err().code,
            SimulationErrorCode::SimulationCodeMismatch
        );
    }
}
#[test]
fn complete_pool_binding_and_actual_bound_transaction_are_revalidated() {
    for k in 0..3 {
        let mut c = case(5, false);
        match k {
            0 => c.pools[0].reserve0 = "900000".parse().unwrap(),
            1 => c.bound.transaction.from = RECIPIENT.into(),
            _ => c.bound.transaction.data = "0x00".into(),
        };
        assert_eq!(
            run(&c).unwrap_err().code,
            SimulationErrorCode::SimulationBindingMismatch
        );
    }
}
#[test]
fn missing_state_and_malformed_balance_queries_fail_closed() {
    for k in 0..3 {
        let mut c = case(5, false);
        if k == 0 {
            c.fixture.accounts.retain(|a| a.address != TOKEN0);
        } else if k == 1 {
            c.fixture
                .accounts
                .iter_mut()
                .find(|a| a.address == TOKEN0)
                .unwrap()
                .storage
                .clear();
        } else {
            c.fixture
                .accounts
                .iter_mut()
                .find(|a| a.address == TOKEN0)
                .unwrap()
                .code = "0x00".into();
        }
        assert_eq!(
            run(&c).unwrap_err().code,
            SimulationErrorCode::SimulationObservationUnavailable
        );
    }
}
#[test]
fn aggregate_budgets_do_not_reset_between_probes() {
    let c = case(5, false);
    let out = run(&c).unwrap();
    for k in 0..5 {
        let mut b = budget();
        match k {
            0 => b.max_reads = out.usage.reads - 1,
            1 => b.max_read_bytes = out.usage.read_bytes - 1,
            2 => b.max_instructions = out.usage.instructions - 1,
            3 => b.max_total_input_bytes = out.usage.input_bytes - 1,
            _ => b.max_total_gas = out.usage.gas_used - 1,
        };
        let e = simulate_bound(
            &c.bound,
            &c.pools,
            &[ROUTER.into()],
            &c.fixture,
            &c.tx,
            &b,
            revm_backend::ExecutionControl::default(),
        )
        .unwrap_err();
        assert_eq!(
            e.code,
            SimulationErrorCode::SimulationBudgetExceeded,
            "{k}: {e:?}"
        );
    }
}
#[test]
fn cancellation_and_output_limits_return_no_receipt() {
    let c = case(5, false);
    let control = revm_backend::ExecutionControl {
        cancellation: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        ..Default::default()
    };
    assert_eq!(
        simulate_bound(
            &c.bound,
            &c.pools,
            &[ROUTER.into()],
            &c.fixture,
            &c.tx,
            &budget(),
            control
        )
        .unwrap_err()
        .code,
        SimulationErrorCode::SimulationCancelled
    );
    let mut b = budget();
    b.max_output_bytes = 31;
    assert_eq!(
        simulate_bound(
            &c.bound,
            &c.pools,
            &[ROUTER.into()],
            &c.fixture,
            &c.tx,
            &b,
            Default::default()
        )
        .unwrap_err()
        .code,
        SimulationErrorCode::SimulationOutputLimit
    );
}

#[test]
fn verified_is_always_false_and_true_cannot_be_deserialized() {
    let out = run(&case(5, false)).unwrap();
    let mut v = serde_json::to_value(&out).unwrap();
    assert_eq!(v["verified"], json!(false));
    v["verified"] = true.into();
    assert!(serde_json::from_value::<SimulationOutcome>(v).is_err());
}
fn rebind_control(c: &mut Case) {
    c.pools[0].v2.as_mut().unwrap().code_sha256 = sha(&hex::decode(
        &c.fixture
            .accounts
            .iter()
            .find(|a| a.address == PAIR)
            .unwrap()
            .code[2..],
    )
    .unwrap());
    c.bound.plan.snapshot.scope_hash = scope_hash(&c.pools).unwrap();
    let plan = build(
        &c.bound.plan.context,
        &c.bound.plan.allocation,
        &c.bound.plan.snapshot,
        &c.bound.plan.economic_policy_hash,
    )
    .unwrap();
    let mut executor = c.bound.executor.clone();
    executor.code_sha256 = sha(&hex::decode(
        &c.fixture
            .accounts
            .iter()
            .find(|a| a.address == ROUTER)
            .unwrap()
            .code[2..],
    )
    .unwrap());
    executor.factory_code_sha256 = sha(&hex::decode(
        &c.fixture
            .accounts
            .iter()
            .find(|a| a.address == FACTORY)
            .unwrap()
            .code[2..],
    )
    .unwrap());
    c.bound = bind_v2_execution(&plan, &c.pools, &executor, &[ROUTER.into()]).unwrap();
}
#[test]
fn runtime_identity_and_reserve_getters_are_checked_even_with_matching_code_hashes() {
    for k in 0..4 {
        let mut c = case(5, false);
        let (target, code) = match k {
            0 => (ROUTER, ret(&[addr_word(TOKEN0)])),
            1 => (FACTORY, ret(&[addr_word(TOKEN0)])),
            2 => (
                PAIR,
                dispatch(
                    vec![
                        ("factory()", ret(&[addr_word(FACTORY)])),
                        ("token0()", ret(&[addr_word(TOKEN1)])),
                    ],
                    vec![0],
                ),
            ),
            _ => (
                PAIR,
                dispatch(
                    vec![
                        ("factory()", ret(&[addr_word(FACTORY)])),
                        ("token0()", ret(&[addr_word(TOKEN0)])),
                        ("token1()", ret(&[addr_word(TOKEN1)])),
                        ("getReserves()", ret(&[num(1), num(2), num(0)])),
                    ],
                    vec![0],
                ),
            ),
        };
        c.fixture
            .accounts
            .iter_mut()
            .find(|a| a.address == target)
            .unwrap()
            .code = format!("0x{}", hex::encode(code));
        rebind_control(&mut c);
        assert_eq!(
            run(&c).unwrap_err().code,
            SimulationErrorCode::SimulationObservationMismatch,
            "{k}"
        );
    }
}
fn replace_router_control(c: &mut Case, sell_after: u64, buy_after: u64, halt: bool) {
    let mut body = vec![];
    set_balance(&mut body, TOKEN0, CALLER, sell_after);
    set_balance(&mut body, TOKEN1, RECIPIENT, buy_after);
    body.push(if halt { 0xfe } else { 0 });
    let code = dispatch(vec![("factory()", ret(&[addr_word(FACTORY)]))], body);
    c.fixture
        .accounts
        .iter_mut()
        .find(|a| a.address == ROUTER)
        .unwrap()
        .code = format!("0x{}", hex::encode(code));
    rebind_control(c);
}
#[test]
fn incorrect_balance_directions_fail_without_unsigned_underflow() {
    for (sell, buy) in [(100001, 12), (90000, 6)] {
        let mut c = case(5, false);
        replace_router_control(&mut c, sell, buy, false);
        assert_eq!(
            run(&c).unwrap_err().code,
            SimulationErrorCode::SimulationBalanceDirection
        );
    }
}
#[test]
fn ordinary_halt_retains_real_nonce_gas_and_zero_balances() {
    let mut c = case(5, false);
    replace_router_control(&mut c, 90000, 12, true);
    let out = run(&c).unwrap();
    assert_eq!(out.execution.status, EvmStatus::Halt);
    assert_eq!(out.execution.gas_used, c.tx.gas_limit);
    assert_eq!(out.execution.sender_nonce_after, 8);
    assert_eq!(out.actual_buy_amount, Amount::zero());
    assert!(!out.matches_plan);
    assert!(out.execution.halt_reason.is_some());
}
#[test]
fn receipt_hash_commits_source_context_limits_and_bound_plan() {
    let base = case(5, false);
    let expected = run(&base).unwrap();
    for k in 0..4 {
        let mut c = case(5, false);
        let mut b = budget();
        match k {
            0 => c.fixture.state_anchor.genesis_hash = format!("0x{}", "bb".repeat(32)),
            1 => c.tx.gas_price = "4".parse().unwrap(),
            2 => b.max_reads += 1,
            _ => {
                c.bound.plan.economic_policy_hash = "other policy".into();
                rebind_control(&mut c);
            }
        }
        let out = simulate_bound(
            &c.bound,
            &c.pools,
            &[ROUTER.into()],
            &c.fixture,
            &c.tx,
            &b,
            Default::default(),
        )
        .unwrap();
        assert_ne!(out.receipt_hash, expected.receipt_hash, "{k}");
    }
}
#[test]
fn native_cases_must_explicitly_extract_strict_fixture_fields() {
    let source: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/evm-execution.json")).unwrap();
    for c in source["cases"].as_array().unwrap() {
        let value = json!({"schema_version":c["schema_version"],"state_anchor":c["state_anchor"],"execution_environment":c["execution_environment"],"accounts":c["accounts"],"block_hashes":c["block_hashes"]});
        let mut b = budget();
        b.max_accounts = 256;
        b.max_storage_slots = 4096;
        b.max_code_bytes = 1024 * 1024;
        b.max_image_bytes = 2 * 1024 * 1024;
        b.max_fixture_json_bytes = 2 * 1024 * 1024;
        let f = parse_fixture_json(&serde_json::to_vec(&value).unwrap(), &b).unwrap();
        assert!(f.to_image(&b).is_ok());
    }
}

#[test]
fn transaction_input_cap_precedes_binding_copy() {
    let mut c = case(5, false);
    c.bound.transaction.data = format!("0x{}", "00".repeat(8192));
    let mut b = budget();
    b.max_input_bytes = 4096;
    let e = simulate_bound(
        &c.bound,
        &c.pools,
        &[ROUTER.into()],
        &c.fixture,
        &c.tx,
        &b,
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(e.code, SimulationErrorCode::SimulationBudgetExceeded);
    assert_eq!(e.usage.executions, 0);
}
#[test]
fn actual_caller_funding_and_swap_nonce_are_not_bypassed() {
    let mut c = case(5, false);
    c.fixture
        .accounts
        .iter_mut()
        .find(|a| a.address == CALLER)
        .unwrap()
        .balance = Amount::zero();
    let e = run(&c).unwrap_err();
    assert_eq!(e.code, SimulationErrorCode::SimulationExecutionUnavailable);
    assert_eq!(e.phase, "swap");
    assert!(
        e.usage.executions > 1,
        "read observations do not require transaction funding"
    );
    let mut c = case(5, false);
    c.tx.nonce += 1;
    let e = run(&c).unwrap_err();
    assert_eq!(e.code, SimulationErrorCode::SimulationExecutionUnavailable);
    assert!(e.usage.executions > 1);
}
#[test]
fn cancellation_inside_actual_getter_loop_discards_all_proof() {
    use std::{
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let mut c = case(5, false);
    c.fixture
        .accounts
        .iter_mut()
        .find(|a| a.address == ROUTER)
        .unwrap()
        .code = "0x5b5f56".into();
    rebind_control(&mut c);
    let flag = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicU64::new(0));
    let control = revm_backend::ExecutionControl {
        cancellation: flag.clone(),
        progress: Some(progress.clone()),
        deadline: Some(Instant::now() + Duration::from_secs(5)),
    };
    let worker = std::thread::spawn(move || {
        let mut b = budget();
        b.probe_gas_limit = 10_000_000;
        b.max_total_gas = 20_000_000;
        b.max_instructions = 100_000_000;
        simulate_bound(
            &c.bound,
            &c.pools,
            &[ROUTER.into()],
            &c.fixture,
            &c.tx,
            &b,
            control,
        )
    });
    let end = Instant::now() + Duration::from_secs(3);
    while progress.load(Ordering::Acquire) < 1024 && Instant::now() < end {
        std::thread::yield_now();
    }
    let observed = progress.load(Ordering::Acquire);
    flag.store(true, Ordering::Release);
    let e = worker.join().unwrap().unwrap_err();
    assert!(observed >= 1024);
    assert_eq!(e.code, SimulationErrorCode::SimulationCancelled);
    assert!(e.usage.instructions >= observed);
}

#[test]
fn unsynced_pair_surplus_is_observed_without_rejecting_actual_execution() {
    // Unit controls only: demonstrate guard behavior and observations, not V2 invariant math.
    let mut c = case(5, false);
    for (token, balance) in [(TOKEN0, 1_000_123), (TOKEN1, 2_000_456)] {
        c.fixture
            .accounts
            .iter_mut()
            .find(|a| a.address == token)
            .unwrap()
            .storage
            .iter_mut()
            .find(|row| row.slot == format!("0x{}", hex::encode(addr_word(PAIR))))
            .unwrap()
            .value = format!("0x{}", hex::encode(num(balance)));
    }
    let original = serde_json::to_value(&c.fixture).unwrap();
    let out = run(&c).expect(
        "provided raw surplus must not be rejected solely because it differs from stored reserves",
    );
    for (phase, balance) in [("pair_balance0", 1_000_123), ("pair_balance1", 2_000_456)] {
        let observed = out
            .observations
            .iter()
            .find(|observation| observation.phase == phase)
            .unwrap();
        assert_eq!(observed.output, format!("0x{}", hex::encode(num(balance))));
    }
    let reserves = out
        .observations
        .iter()
        .find(|observation| observation.phase == "getReserves()")
        .unwrap();
    let bytes = hex::decode(&reserves.output[2..]).unwrap();
    assert_eq!(U256::from_be_slice(&bytes[..32]), U256::from(1_000_000));
    assert_eq!(U256::from_be_slice(&bytes[32..64]), U256::from(2_000_000));
    assert_eq!(out.execution.status, EvmStatus::Success);
    assert_eq!(out.actual_input_consumed.to_string(), "10000");
    assert_eq!(out.actual_buy_amount.to_string(), "5");
    assert!(!out.matches_plan);
    assert!(!out.verified);
    assert_eq!(out.observations.len(), 12);
    assert_eq!(out.usage.executions, 13);
    assert_eq!(serde_json::to_value(&c.fixture).unwrap(), original);
}
