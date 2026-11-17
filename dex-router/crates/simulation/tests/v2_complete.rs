//! Frozen original-bytecode oracle, plus distinct standard-50bps BOUND transactions.
use revm::primitives::{Address, BlockEnv, Bytes, SpecId, TxEnv, TxKind, B256, U256};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    str::FromStr,
    sync::OnceLock,
    time::{Duration, Instant},
};
use xone_domain::{hash, scope_hash, v2_swap, Amount, BlockRef, Pool, SnapshotRef};
use xone_execution_plan::{bind_v2_execution, build, BoundEvmExecution, Context, ExecutorIdentity};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};
use xone_simulation::{
    evm_fixture::*,
    revm_backend::{self as core, ExecutionControl, ExecutionLimits},
};

fn corpus() -> &'static Value {
    static BANK: OnceLock<Value> = OnceLock::new();
    BANK.get_or_init(|| {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/v2-complete.json"
        ))
        .unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            "63e8260ea53bec9cc9226f7e64e01b463af04a30d7afb773415134e591f494ee"
        );
        serde_json::from_slice(&bytes).unwrap()
    })
}
fn budget() -> SimulationBudget {
    SimulationBudget {
        max_fixture_json_bytes: 8 * 1024 * 1024,
        max_accounts: 2048,
        max_storage_slots: 65536,
        max_code_bytes: 4 * 1024 * 1024,
        max_image_bytes: 8 * 1024 * 1024,
        max_reads: 8192,
        max_read_bytes: 16 * 1024 * 1024,
        max_instructions: 2_000_000,
        max_input_bytes: 128 * 1024,
        max_total_input_bytes: 1024 * 1024,
        max_transaction_gas: 15_000_000,
        probe_gas_limit: 200_000,
        max_total_gas: 15_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        max_output_bytes: 64 * 1024,
    }
}
fn fixture(c: &Value) -> FixtureState {
    serde_json::from_value(json!({"schema_version":1,"state_anchor":c["state_anchor"],"execution_environment":c["execution_environment"],"accounts":c["accounts"],"block_hashes":[]})).unwrap()
}
fn bound(
    c: &Value,
) -> (
    BoundEvmExecution,
    Vec<Pool>,
    FixtureState,
    TransactionContext,
) {
    let f = fixture(c);
    let pool: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let amount: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
    let route = AllocatedRoute {
        path: PathQuote {
            edges: vec![Edge {
                pool_id: pool.pool_id.clone(),
                protocol: pool.protocol.clone(),
                zero_for_one: c["token_in"] == pool.token0,
                from_token: None,
                to: c["token_out"].as_str().unwrap().into(),
            }],
            tokens: vec![
                c["token_in"].as_str().unwrap().into(),
                c["token_out"].as_str().unwrap().into(),
            ],
            amount_in: amount.clone(),
            amount_out: Amount::zero(),
        },
        amount_in: amount,
        amount_out: Amount::zero(),
    };
    let pools = vec![pool];
    let allocation = evaluate_allocation(&pools, &[route], &Amount::zero()).unwrap();
    let s = &f.state_anchor;
    let reference = SnapshotRef {
        block: BlockRef {
            chain_key: s.chain_key.clone(),
            number_or_slot: s.block_number,
            hash: s.block_hash.clone(),
            parent_hash: s.parent_hash.clone(),
            timestamp_ms: s.timestamp * 1000,
        },
        canonical_epoch: 1,
        journal_seq: 1,
        schema_version: 1,
        adapter_set_hash: hash(&vec![&pools[0].adapter_version]).unwrap(),
        online_generation: 1,
        scope_hash: scope_hash(&pools).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let plan = build(
        &Context {
            chain_key: s.chain_key.clone(),
            caller: c["transaction"]["from"].as_str().unwrap().into(),
            recipient: c["recipient"].as_str().unwrap().into(),
            sell_token: c["token_in"].as_str().unwrap().into(),
            buy_token: c["token_out"].as_str().unwrap().into(),
            slippage_bps: 50,
            deadline_ms: f.execution_environment.timestamp * 1000 + 60_000,
        },
        &allocation,
        &reference,
        "v2-complete-native",
    )
    .unwrap();
    let router = corpus()["reference"]["runtime_identities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["address"] == c["transaction"]["to"])
        .unwrap();
    let executor = ExecutorIdentity {
        target: c["transaction"]["to"].as_str().unwrap().into(),
        code_sha256: router["runtime_code_sha256"].as_str().unwrap().into(),
        factory: pools[0].v2.as_ref().unwrap().factory.clone(),
        factory_code_sha256: v2_swap::FACTORY_CODE_SHA256.into(),
        adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
    };
    let b = bind_v2_execution(
        &plan,
        &pools,
        &executor,
        std::slice::from_ref(&executor.target),
    )
    .unwrap();
    (
        b,
        pools,
        f,
        serde_json::from_value(c["transaction_context"].clone()).unwrap(),
    )
}
fn simulate(
    b: &BoundEvmExecution,
    p: &[Pool],
    f: &FixtureState,
    t: &TransactionContext,
    lim: &SimulationBudget,
) -> Result<SimulationOutcome> {
    simulate_bound(
        b,
        p,
        std::slice::from_ref(&b.executor.target),
        f,
        t,
        lim,
        ExecutionControl::default(),
    )
}
fn addr(s: &str) -> Address {
    Address::from_str(s).unwrap()
}
fn num(s: &str) -> U256 {
    U256::from_str_radix(s, 10).unwrap()
}
fn slot(n: u64) -> String {
    format!("0x{n:064x}")
}
fn observation<'a>(o: &'a SimulationOutcome, phase: &str) -> &'a Observation {
    o.observations
        .iter()
        .find(|o| o.phase == phase)
        .unwrap_or_else(|| panic!("missing observation {phase}"))
}
fn observed(o: &SimulationOutcome, phase: &str) -> U256 {
    U256::from_str_radix(&observation(o, phase).output[2..], 16).unwrap()
}
fn assert_post(o: &SimulationOutcome, expected: &Value) {
    let p: Pool = serde_json::from_value(expected.clone()).unwrap();
    let s = p.v2.as_ref().unwrap().swap.as_ref().unwrap();
    let reserves = hex::decode(&observation(o, "pair_reserves_after").output[2..]).unwrap();
    assert_eq!(
        U256::from_be_slice(&reserves[..32]),
        num(&p.reserve0.to_string())
    );
    assert_eq!(
        U256::from_be_slice(&reserves[32..64]),
        num(&p.reserve1.to_string())
    );
    assert_eq!(
        U256::from_be_slice(&reserves[64..]),
        U256::from(s.block_timestamp_last)
    );
    for (phase, value) in [
        ("pair_balance0_after", &s.raw_balance0),
        ("pair_balance1_after", &s.raw_balance1),
        ("pair_price0_cumulative_after", &s.price0_cumulative_last),
        ("pair_price1_cumulative_after", &s.price1_cumulative_last),
    ] {
        assert_eq!(observed(o, phase), num(&value.to_string()), "{phase}");
    }
    assert_eq!(observed(o, "pair_lock_after"), U256::from(1));
    for phase in ["pair_lock_before", "pair_lock_after"] {
        let lock = serde_json::to_value(observation(o, phase)).unwrap();
        assert_eq!(lock["type"], "STORAGE");
        assert_eq!(lock["slot"], slot(12));
        assert!(lock.get("calldata").is_none_or(|v| v == ""));
        assert_eq!(lock["usage"]["reads"], 1);
        assert_eq!(lock["usage"]["read_bytes"], 84);
        assert_eq!(lock["usage"]["gas_used"], 0);
        assert_eq!(lock["usage"]["executions"], 0);
    }
}

#[test]
fn complete_bound_executes_16_service_cases_and_observes_real_pair_poststate() {
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == true)
    {
        let (b, p, f, t) = bound(c);
        let o =
            simulate(&b, &p, &f, &t, &budget()).unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        assert_eq!(o.execution.status, EvmStatus::Success, "{}", c["name"]);
        assert!(o.matches_plan, "{}", c["name"]);
        assert_eq!(o.execution.sender_nonce_before, t.nonce);
        assert_eq!(o.execution.sender_nonce_after, t.nonce + 1);
        assert_eq!(
            o.actual_buy_amount.to_string(),
            c["expected"]["amount_out"].as_str().unwrap()
        );
        assert_eq!(
            o.actual_input_consumed.to_string(),
            c["amount_in"].as_str().unwrap()
        );
        assert!(!o.verified);
        assert_ne!(
            b.transaction.data,
            c["transaction"]["data"].as_str().unwrap()
        );
        // Different minOut/deadline calldata has its own measured gas; no original-gas equality claim.
        assert_post(&o, &c["expected"]["pool_after"]);
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn complete_source_raw_timestamp_and_cumulatives_are_independently_rejected() {
    for field in [
        "raw_balance0",
        "raw_balance1",
        "block_timestamp_last",
        "price0_cumulative_last",
        "price1_cumulative_last",
    ] {
        let mut c = corpus()["cases"][0].clone();
        let old = &c["pool"]["v2"]["swap"][field];
        c["pool"]["v2"]["swap"][field] = if field == "block_timestamp_last" {
            (old.as_u64().unwrap() + 1).into()
        } else {
            (num(old.as_str().unwrap()) + U256::from(1))
                .to_string()
                .into()
        };
        let (b, p, f, t) = bound(&c);
        let e = simulate(&b, &p, &f, &t, &budget()).expect_err(field);
        assert_eq!(
            e.code,
            SimulationErrorCode::SimulationObservationMismatch,
            "{field}: {e}"
        );
    }
}

#[test]
fn complete_strict_unknown_and_non_one_lock_slots_are_rejected_before_swap() {
    let (b, p, f, t) = bound(&corpus()["cases"][0]);
    for n in [8, 9, 10, 12] {
        let mut f = f.clone();
        f.accounts
            .iter_mut()
            .find(|a| a.address == p[0].pool_id)
            .unwrap()
            .storage
            .retain(|s| s.slot != slot(n));
        let e = simulate(&b, &p, &f, &t, &budget()).expect_err("unknown cannot be zero");
        assert_eq!(
            e.code,
            SimulationErrorCode::SimulationObservationUnavailable,
            "slot {n}: {e}"
        );
        assert!(e.detail.contains("unknown_storage"), "{e}");
        assert!(e.usage.reads > 0);
    }
    for lock in [0, 2, 256] {
        let mut f = f.clone();
        f.accounts
            .iter_mut()
            .find(|a| a.address == p[0].pool_id)
            .unwrap()
            .storage
            .iter_mut()
            .find(|s| s.slot == slot(12))
            .unwrap()
            .value = slot(lock);
        let e = simulate(&b, &p, &f, &t, &budget()).expect_err("lock must be exactly 1");
        assert_eq!(
            e.code,
            SimulationErrorCode::SimulationObservationMismatch,
            "{e}"
        );
        assert_eq!(e.phase, "pair_lock_before");
    }
}

#[test]
fn complete_last_timestamp_width_and_bank_h_cannot_be_forged() {
    let mut c = corpus()["cases"][0].clone();
    c["pool"]["v2"]["swap"]["block_timestamp_last"] = (u64::from(u32::MAX) + 1).into();
    assert!(serde_json::from_value::<Pool>(c["pool"].clone()).is_err());
    for field in [
        "bank_block_number",
        "bank_block_hash",
        "bank_unix_timestamp",
        "factory_code_sha256",
        "unlocked",
    ] {
        let (mut b, mut p, f, t) = bound(&corpus()["cases"][0]);
        let s = p[0].v2.as_mut().unwrap().swap.as_mut().unwrap();
        match field {
            "bank_block_number" => s.bank_block_number += 1,
            "bank_block_hash" => s.bank_block_hash = format!("0x{}", "ab".repeat(32)),
            "bank_unix_timestamp" => s.bank_unix_timestamp += 1,
            "factory_code_sha256" => s.factory_code_sha256 = "ab".repeat(32),
            _ => s.unlocked = false,
        }
        b.plan.snapshot.scope_hash = scope_hash(&p).unwrap();
        b.plan = build(
            &b.plan.context,
            &b.plan.allocation,
            &b.plan.snapshot,
            &b.plan.economic_policy_hash,
        )
        .unwrap();
        assert!(
            bind_v2_execution(
                &b.plan,
                &p,
                &b.executor,
                std::slice::from_ref(&b.executor.target)
            )
            .is_err(),
            "{field}"
        );
        assert!(simulate(&b, &p, &f, &t, &budget()).is_err(), "{field}");
    }
}

#[test]
fn complete_lock_read_shares_aggregate_read_and_byte_budget() {
    let (b, p, f, t) = bound(&corpus()["cases"][0]);
    let full = simulate(&b, &p, &f, &t, &budget()).unwrap();
    let pre = observation(&full, "pair_lock_before");
    let index = full
        .observations
        .iter()
        .position(|o| std::ptr::eq(o, pre))
        .unwrap();
    let reads: u64 = full.observations[..index]
        .iter()
        .map(|o| o.usage.reads)
        .sum();
    let bytes: u64 = full.observations[..index]
        .iter()
        .map(|o| o.usage.read_bytes)
        .sum();
    for byte_limit in [false, true] {
        let mut lim = budget();
        if byte_limit {
            lim.max_read_bytes = bytes + 83;
        } else {
            lim.max_reads = reads;
        }
        let e = simulate(&b, &p, &f, &t, &lim).unwrap_err();
        assert_eq!(e.code, SimulationErrorCode::SimulationBudgetExceeded, "{e}");
        assert_eq!(e.phase, "pair_lock_before");
        assert_eq!(e.usage.reads, reads + 1);
        assert_eq!(e.usage.read_bytes, bytes + 84);
    }
    let mut exact = budget();
    exact.max_reads = full.usage.reads;
    exact.max_read_bytes = full.usage.read_bytes;
    assert!(simulate(&b, &p, &f, &t, &exact).is_ok());
    let mut late = budget();
    late.max_reads = full.usage.reads - 1;
    let e = simulate(&b, &p, &f, &t, &late).unwrap_err();
    assert_eq!(e.code, SimulationErrorCode::SimulationBudgetExceeded);
    assert_eq!(e.phase, "pair_lock_after");
    let cancel = ExecutionControl::default();
    cancel
        .cancellation
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        simulate_bound(
            &b,
            &p,
            std::slice::from_ref(&b.executor.target),
            &f,
            &t,
            &budget(),
            cancel
        )
        .unwrap_err()
        .code,
        SimulationErrorCode::SimulationCancelled
    );
    let expired = ExecutionControl {
        deadline: Some(Instant::now() - Duration::from_secs(1)),
        ..Default::default()
    };
    assert_eq!(
        simulate_bound(
            &b,
            &p,
            std::slice::from_ref(&b.executor.target),
            &f,
            &t,
            &budget(),
            expired
        )
        .unwrap_err()
        .code,
        SimulationErrorCode::SimulationDeadline
    );
}

fn execute_data(
    f: &FixtureState,
    from: &str,
    to: &str,
    data: &str,
    t: &TransactionContext,
) -> core::ExecutionOutput {
    let e = &f.execution_environment;
    let lim = budget();
    core::execute(
        f.to_image(&lim).unwrap(),
        core::TransactionRequest {
            fork: SpecId::SHANGHAI,
            chain_id: e.chain_id,
            block: BlockEnv {
                number: U256::from(e.block_number),
                timestamp: U256::from(e.timestamp),
                coinbase: addr(&e.coinbase),
                gas_limit: U256::from(e.gas_limit),
                basefee: num(&e.base_fee.to_string()),
                prevrandao: Some(B256::from_str(&e.prevrandao).unwrap()),
                ..Default::default()
            },
            tx: TxEnv {
                caller: addr(from),
                transact_to: TxKind::Call(addr(to)),
                data: Bytes::from(hex::decode(&data[2..]).unwrap()),
                nonce: Some(t.nonce),
                chain_id: Some(t.chain_id),
                gas_limit: t.gas_limit,
                gas_price: num(&t.gas_price.to_string()),
                ..Default::default()
            },
        },
        ExecutionLimits {
            reads: core::BoundReadLimits {
                max_reads: lim.max_reads,
                max_bytes: lim.max_read_bytes,
            },
            max_gas_limit: lim.max_transaction_gas,
            max_instructions: lim.max_instructions,
            max_input_bytes: lim.max_input_bytes,
            max_memory_bytes: lim.max_memory_bytes,
        },
        Default::default(),
    )
    .unwrap()
}

#[test]
fn original_24_transactions_match_real_gas_eth_nonces_code_and_all_declared_slots() {
    let mut success = 0;
    let mut revert = 0;
    for c in corpus()["cases"].as_array().unwrap() {
        let f = fixture(c);
        let t: TransactionContext =
            serde_json::from_value(c["transaction_context"].clone()).unwrap();
        let out = execute_data(
            &f,
            c["transaction"]["from"].as_str().unwrap(),
            c["transaction"]["to"].as_str().unwrap(),
            c["transaction"]["data"].as_str().unwrap(),
            &t,
        );
        assert_eq!(
            out.result_and_state.result.is_success(),
            c["expected"]["status"] == "success",
            "{}",
            c["name"]
        );
        if out.result_and_state.result.is_success() {
            success += 1
        } else {
            revert += 1
        }
        assert_eq!(
            out.result_and_state.result.gas_used(),
            c["expected"]["gas_used"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            "{}",
            c["name"]
        );
        let post = f
            .to_image(&budget())
            .unwrap()
            .apply(&out.result_and_state.state)
            .unwrap();
        let expected: Vec<FixtureAccount> =
            serde_json::from_value(c["expected"]["post_accounts"].clone()).unwrap();
        for a in expected {
            let address = addr(&a.address);
            let actual = post.data().accounts.get(&address).unwrap();
            if !a.exists {
                assert!(actual.is_none());
                continue;
            }
            let actual = actual.as_ref().unwrap();
            assert_eq!(
                actual.balance,
                num(&a.balance.to_string()),
                "{} {} ETH",
                c["name"],
                a.address
            );
            assert_eq!(actual.nonce, a.nonce, "{} {} nonce", c["name"], a.address);
            assert_eq!(
                actual.code_hash,
                revm::primitives::keccak256(hex::decode(&a.code[2..]).unwrap()),
                "{} code",
                c["name"]
            );
            for s in a.storage {
                assert_eq!(
                    post.data()
                        .storage
                        .get(&(address, U256::from_str_radix(&s.slot[2..], 16).unwrap()))
                        .copied(),
                    Some(U256::from_str_radix(&s.value[2..], 16).unwrap()),
                    "{} {} {}",
                    c["name"],
                    a.address,
                    s.slot
                );
            }
        }
    }
    assert_eq!((success, revert), (20, 4));
}

#[test]
fn complete_explicit_h_plus_one_clock_observes_real_cumulative_updates() {
    // Historical explicit simulation only. The fixture's service_eligible remains false.
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == false && c["expected"]["status"] == "success")
    {
        let (b, p, f, t) = bound(c);
        let o =
            simulate(&b, &p, &f, &t, &budget()).unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        assert_eq!(o.execution.status, EvmStatus::Success);
        assert!(o.matches_plan);
        assert_post(&o, &c["expected"]["pool_after"]);
        assert_eq!(
            b.plan.snapshot.block.timestamp_ms,
            f.state_anchor.timestamp * 1000
        );
        assert_eq!(
            p[0].v2
                .as_ref()
                .unwrap()
                .swap
                .as_ref()
                .unwrap()
                .bank_block_number,
            f.state_anchor.block_number
        );
        assert!(!o.verified);
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn complete_readonly_observations_preserve_real_swap_fees_and_nonce_with_tight_eth() {
    let (b, p, mut f, t) = bound(&corpus()["cases"][0]);
    let funds = U256::from(t.gas_limit) * num(&t.gas_price.to_string());
    f.accounts
        .iter_mut()
        .find(|a| a.address == b.transaction.from)
        .unwrap()
        .balance = funds.to_string().parse().unwrap();
    let direct = execute_data(
        &f,
        &b.transaction.from,
        &b.transaction.to,
        &b.transaction.data,
        &t,
    );
    let post = f
        .to_image(&budget())
        .unwrap()
        .apply(&direct.result_and_state.state)
        .unwrap();
    let real_caller = post.data().accounts[&addr(&b.transaction.from)]
        .as_ref()
        .unwrap();
    assert_eq!(
        real_caller.balance,
        funds
            - U256::from(direct.result_and_state.result.gas_used()) * num(&t.gas_price.to_string())
    );
    let o = simulate(&b, &p, &f, &t, &budget()).unwrap();
    assert!(o.matches_plan);
    assert_eq!(
        o.execution.gas_used,
        direct.result_and_state.result.gas_used()
    );
    assert_eq!(o.execution.sender_nonce_after, real_caller.nonce);
    // Underfunded actual swaps still fail ordinary transaction validation.
    f.accounts
        .iter_mut()
        .find(|a| a.address == b.transaction.from)
        .unwrap()
        .balance = Amount::zero();
    let err = simulate(&b, &p, &f, &t, &budget()).unwrap_err();
    assert_eq!(err.phase, "swap");
    assert_eq!(
        err.code,
        SimulationErrorCode::SimulationExecutionUnavailable
    );
}

#[test]
fn legacy_profile_stays_isolated_and_keeps_its_observation_wire() {
    let mut c = corpus()["cases"][0].clone();
    c["pool"]["protocol"] = "uniswap-v2".into();
    c["pool"]["adapter_version"] = "uniswap-v2/1".into();
    c["pool"]["v2"].as_object_mut().unwrap().remove("swap");
    let (b, p, mut f, t) = bound(&c);
    // Legacy does not use full-state preflight. The actual pair lock is still enforced by EVM.
    f.accounts
        .iter_mut()
        .find(|a| a.address == p[0].pool_id)
        .unwrap()
        .storage
        .iter_mut()
        .find(|s| s.slot == slot(12))
        .unwrap()
        .value = slot(0);
    let o = simulate(&b, &p, &f, &t, &budget()).unwrap();
    assert_eq!(o.execution.status, EvmStatus::Revert);
    assert!(!o
        .observations
        .iter()
        .any(|o| o.phase.ends_with("_cumulative_after")
            || o.phase == "pair_lock_before"
            || o.phase == "pair_lock_after"));
    for ob in o.observations {
        let wire = serde_json::to_value(&ob).unwrap();
        assert!(wire.get("type").is_none());
        assert!(wire.get("slot").is_none());
        assert_eq!(wire.as_object().unwrap().len(), 5);
        assert!(wire["calldata"].as_str().unwrap().starts_with("0x"));
    }
    c["pool"]["v2"]["swap"] = corpus()["cases"][0]["pool"]["v2"]["swap"].clone();
    assert!(serde_json::from_value::<Pool>(c["pool"].clone())
        .unwrap()
        .validate()
        .is_err());
}

#[test]
fn complete_actual_runtime_and_token_balance_storage_cannot_be_substituted() {
    let (b, p, f, t) = bound(&corpus()["cases"][0]);
    for target in [&p[0].pool_id, &b.executor.factory] {
        let mut bad = f.clone();
        let a = bad
            .accounts
            .iter_mut()
            .find(|a| &a.address == target)
            .unwrap();
        a.code.replace_range(2..4, "00");
        let e = simulate(&b, &p, &bad, &t, &budget()).unwrap_err();
        assert_eq!(
            e.code,
            SimulationErrorCode::SimulationCodeMismatch,
            "{target}: {e}"
        );
    }
    let s = p[0].v2.as_ref().unwrap().swap.as_ref().unwrap();
    for (token, raw) in [
        (&p[0].token0, &s.raw_balance0),
        (&p[0].token1, &s.raw_balance1),
    ] {
        let mut bad = f.clone();
        let a = bad
            .accounts
            .iter_mut()
            .find(|a| &a.address == token)
            .unwrap();
        let word = format!("0x{:064x}", num(&raw.to_string()));
        let index = a.storage.iter().position(|s| s.value == word).unwrap();
        a.storage.remove(index);
        let e = simulate(&b, &p, &bad, &t, &budget()).unwrap_err();
        assert_eq!(
            e.code,
            SimulationErrorCode::SimulationObservationUnavailable,
            "{token}: {e}"
        );
        assert!(e.detail.contains("unknown_storage"));
    }
    let mut bad = f;
    let packed = bad
        .accounts
        .iter_mut()
        .find(|a| a.address == p[0].pool_id)
        .unwrap()
        .storage
        .iter_mut()
        .find(|s| s.slot == slot(8))
        .unwrap();
    let value = U256::from_str_radix(&packed.value[2..], 16).unwrap() ^ (U256::from(1) << 224);
    packed.value = format!("0x{value:064x}");
    let e = simulate(&b, &p, &bad, &t, &budget()).unwrap_err();
    assert_eq!(e.code, SimulationErrorCode::SimulationObservationMismatch);
    assert_eq!(e.phase, "getReserves()");
}

#[test]
fn legacy_observation_missing_calldata_remains_invalid_and_storage_roundtrips() {
    let (b, p, f, t) = bound(&corpus()["cases"][0]);
    let o = simulate(&b, &p, &f, &t, &budget()).unwrap();
    let mut legacy = serde_json::to_value(&o.observations[0]).unwrap();
    legacy.as_object_mut().unwrap().remove("calldata");
    assert!(serde_json::from_value::<Observation>(legacy).is_err());
    let lock = serde_json::to_value(observation(&o, "pair_lock_after")).unwrap();
    let decoded: Observation = serde_json::from_value(lock.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), lock);
}

fn type2_context(transaction: &TransactionContext, priority: &Amount) -> Type2TransactionContext {
    Type2TransactionContext {
        transaction: transaction.clone(),
        max_priority_fee_per_gas: priority.clone(),
    }
}

#[test]
fn typed_exact_in_measures_native_fee_and_preserves_original_token_balance_deltas() {
    let priority: Amount = "1000000000".parse().unwrap();
    let mut count = 0;
    for case in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == true)
    {
        let (binding, pools, fixture, tx) = bound(case);
        let before = serde_json::to_value(&fixture).unwrap();
        let result = simulate_bound_type2(
            &binding,
            &pools,
            std::slice::from_ref(&binding.executor.target),
            &fixture,
            &type2_context(&tx, &priority),
            &budget(),
            ExecutionControl::default(),
        )
        .unwrap();
        assert!(result.outcome.matches_plan, "{}", case["name"]);
        assert_eq!(
            result.outcome.actual_buy_amount.to_string(),
            case["expected"]["amount_out"]
        );
        assert_eq!(
            result.outcome.actual_input_consumed,
            binding.plan.allocation.sell_amount
        );
        assert_eq!(result.actual_native_fee, result.actual_caller_native_debit);
        assert_eq!(result.caller_beneficiary_credit, Amount::zero());
        assert!(result.actual_native_fee > Amount::zero());
        assert_eq!(before, serde_json::to_value(&fixture).unwrap());
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn typed_exact_output_runs_original_router_and_measures_exact_target_and_input() {
    let priority: Amount = "1000000000".parse().unwrap();
    let mut count = 0;
    for case in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == true)
    {
        let (binding, pools, fixture, tx) = bound(case);
        let target = Amount::checked(&binding.plan.allocation.gross_buy_amount.0 / 2u8).unwrap();
        let proposal = xone_optimizer::exact_out::ExactOutProposal {
            edges: binding.plan.allocation.routes[0].path.edges.clone(),
            amount_out: target.clone(),
        };
        let allocation = xone_optimizer::exact_out::evaluate_allocation(
            &pools,
            &[proposal],
            &target,
            &binding.plan.allocation.sell_amount,
            xone_routing::Funds {
                sender: &binding.plan.context.caller,
                recipient: &binding.plan.context.recipient,
            },
        )
        .unwrap();
        let plan = xone_execution_plan::exact_out::build(
            &binding.plan.context,
            &allocation,
            &binding.plan.snapshot,
            &binding.plan.economic_policy_hash,
        )
        .unwrap();
        let output = xone_execution_plan::bind_v2_exact_out_execution(
            &plan,
            &pools,
            &binding.executor,
            std::slice::from_ref(&binding.executor.target),
        )
        .unwrap();
        let result = simulate_exact_out_type2(
            &output,
            &pools,
            std::slice::from_ref(&binding.executor.target),
            &fixture,
            &type2_context(&tx, &priority),
            &budget(),
            ExecutionControl::default(),
        )
        .unwrap();
        assert!(result.outcome.matches_plan, "{}", case["name"]);
        assert_eq!(result.outcome.actual_buy_amount, target);
        assert_eq!(
            result.outcome.actual_input_consumed,
            allocation.required_input
        );
        assert_eq!(result.actual_native_fee, result.actual_caller_native_debit);
        assert_eq!(result.outcome.refund, Amount::zero());
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn typed_execution_caller_as_beneficiary_separates_credit_from_actual_gas_charge() {
    let case = &corpus()["cases"][0];
    let (binding, pools, mut fixture, tx) = bound(case);
    fixture.execution_environment.coinbase = binding.plan.context.caller.clone();
    // Counterfactual execution environment over the retained source H. This is
    // deliberately not the original mined H+1 header or a chain proof.
    fixture.execution_environment.block_hash = format!("0x{}", "ce".repeat(32));
    let result = simulate_bound_type2(
        &binding,
        &pools,
        std::slice::from_ref(&binding.executor.target),
        &fixture,
        &type2_context(&tx, &"1000000000".parse().unwrap()),
        &budget(),
        ExecutionControl::default(),
    )
    .unwrap();
    assert!(result.outcome.matches_plan);
    assert!(result.caller_beneficiary_credit > Amount::zero());
    assert_eq!(
        result.actual_native_fee.0,
        &result.actual_caller_native_debit.0 + &result.caller_beneficiary_credit.0
    );
}

#[test]
fn typed_execution_fee_caps_and_missing_storage_fail_without_source_mutation() {
    let (binding, pools, fixture, tx) = bound(&corpus()["cases"][0]);
    let original = serde_json::to_value(&fixture).unwrap();
    let error = simulate_bound_type2(
        &binding,
        &pools,
        std::slice::from_ref(&binding.executor.target),
        &fixture,
        &type2_context(&tx, &Amount::checked(&tx.gas_price.0 + 1u8).unwrap()),
        &budget(),
        ExecutionControl::default(),
    )
    .unwrap_err();
    assert_eq!(
        error.code,
        SimulationErrorCode::SimulationTransactionContext
    );
    let mut missing = fixture.clone();
    missing
        .accounts
        .iter_mut()
        .find(|a| a.address == pools[0].pool_id)
        .unwrap()
        .storage
        .clear();
    assert!(simulate_bound_type2(
        &binding,
        &pools,
        std::slice::from_ref(&binding.executor.target),
        &missing,
        &type2_context(&tx, &Amount::zero()),
        &budget(),
        ExecutionControl::default()
    )
    .is_err());
    assert_eq!(original, serde_json::to_value(&fixture).unwrap());
}

#[test]
fn typed_revert_charges_only_actual_native_gas_and_rolls_back_token_transfers() {
    let (binding, pools, mut fixture, tx) = bound(&corpus()["cases"][0]);
    let caller = addr(&binding.plan.context.caller);
    let router = addr(&binding.executor.target);
    let mut owner_slot = [0u8; 64];
    owner_slot[12..32].copy_from_slice(caller.as_slice());
    owner_slot[63] = 2;
    let inner = revm::primitives::keccak256(owner_slot);
    let mut spender_slot = [0u8; 64];
    spender_slot[12..32].copy_from_slice(router.as_slice());
    spender_slot[32..].copy_from_slice(inner.as_slice());
    let slot = format!("{}", revm::primitives::keccak256(spender_slot));
    let account = fixture
        .accounts
        .iter_mut()
        .find(|a| a.address == binding.plan.context.sell_token)
        .unwrap();
    let allowance = account
        .storage
        .iter_mut()
        .find(|s| s.slot == slot)
        .expect("Original token allowance slot must be declared");
    allowance.value = format!("0x{}", "0".repeat(64));
    let source = serde_json::to_value(&fixture).unwrap();
    let result = simulate_bound_type2(
        &binding,
        &pools,
        std::slice::from_ref(&binding.executor.target),
        &fixture,
        &type2_context(&tx, &"1000000000".parse().unwrap()),
        &budget(),
        ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!(result.outcome.execution.status, EvmStatus::Revert);
    assert!(!result.outcome.matches_plan);
    assert_eq!(result.outcome.actual_input_consumed, Amount::zero());
    assert_eq!(result.outcome.actual_buy_amount, Amount::zero());
    assert!(result.actual_native_fee > Amount::zero());
    assert_eq!(result.actual_native_fee, result.actual_caller_native_debit);
    assert_eq!(result.outcome.execution.sender_nonce_after, tx.nonce + 1);
    assert_eq!(source, serde_json::to_value(&fixture).unwrap());
}

#[test]
fn typed_zero_fee_counterfactual_does_not_fabricate_a_native_network_charge() {
    let (binding, pools, mut fixture, mut tx) = bound(&corpus()["cases"][0]);
    // Explicit supplied H+1 environment, not a retained mined header.
    fixture.execution_environment.block_hash = format!("0x{}", "cf".repeat(32));
    fixture.execution_environment.base_fee = Amount::zero();
    tx.gas_price = Amount::zero();
    let result = simulate_bound_type2(
        &binding,
        &pools,
        std::slice::from_ref(&binding.executor.target),
        &fixture,
        &type2_context(&tx, &Amount::zero()),
        &budget(),
        ExecutionControl::default(),
    )
    .unwrap();
    assert!(result.outcome.matches_plan);
    assert_eq!(result.effective_gas_price, Amount::zero());
    assert_eq!(result.actual_native_fee, Amount::zero());
    assert_eq!(result.actual_caller_native_debit, Amount::zero());
    assert_eq!(result.caller_native_before, result.caller_native_after);
    assert_eq!(
        result.outcome.actual_buy_amount,
        binding.plan.allocation.gross_buy_amount
    );
}
