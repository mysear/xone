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

#[test]
fn complete_original_router_eip1559_matches_legacy_overlay_and_measured_native_fees() {
    let mut count = 0;
    for c in corpus()["cases"].as_array().unwrap() {
        if c["service_eligible"] != true {
            continue;
        }
        let (b, p, f, mut tx) = bound(c);
        let legacy = simulate(&b, &p, &f, &tx, &budget()).unwrap();
        let price = tx.gas_price.clone();
        tx.max_priority_fee_per_gas = Some(price.clone());
        // Priority is clipped to the cap minus the explicitly captured base fee.
        let dynamic = simulate(&b, &p, &f, &tx, &budget()).unwrap();
        assert_eq!(
            legacy.execution.post_state_digest,
            dynamic.execution.post_state_digest
        );
        assert_eq!(legacy.actual_buy_amount, dynamic.actual_buy_amount);
        let fees = dynamic.native_fees.unwrap();
        assert_eq!(fees.effective_gas_price, price);
        assert_eq!(
            &fees.caller_before.0 - &fees.caller_after.0,
            fees.execution_fee.0
        );
        assert_eq!(
            &fees.coinbase_after.0 - &fees.coinbase_before.0,
            fees.priority_fee_paid.0
        );
        assert_eq!(
            &fees.base_fee_burned.0 + &fees.priority_fee_paid.0,
            fees.execution_fee.0
        );
        count += 1;
    }
    assert_eq!(count, 16);
}

struct PreparedCase {
    bound: BoundEvmExecution,
    pools: Vec<Pool>,
    fixture: FixtureState,
    funding: xone_execution_preparation::evm::FundingObservation,
    fees: xone_execution_preparation::evm::FeeContext,
    component: xone_execution_preparation::evm::Preparation,
}
impl PreparedCase {
    fn input(&self) -> xone_simulation::prepared_evm::PreparedExecution<'_> {
        xone_simulation::prepared_evm::PreparedExecution {
            bound: &self.bound,
            pools: &self.pools,
            allowed_targets: std::slice::from_ref(&self.bound.executor.target),
            funding: &self.funding,
            fees: &self.fees,
            preparation: &self.component,
        }
    }
    fn prepare(&mut self) {
        self.component = xone_execution_preparation::evm::prepare(
            &self.bound,
            &self.pools,
            std::slice::from_ref(&self.bound.executor.target),
            &self.funding,
            &self.fees,
            self.fixture.execution_environment.timestamp * 1000,
        )
        .unwrap();
    }
}
fn original_allowance(f: &FixtureState, token: &str, owner: &str, spender: &str) -> Amount {
    // The retained original reference ERC20 declares balances at slot 1 and allowances at slot 2.
    let mut outer = [0u8; 64];
    outer[12..32].copy_from_slice(Address::from_str(owner).unwrap().as_slice());
    outer[63] = 2;
    let mut inner = [0u8; 64];
    inner[12..32].copy_from_slice(Address::from_str(spender).unwrap().as_slice());
    inner[32..].copy_from_slice(revm::primitives::keccak256(outer).as_slice());
    let slot = format!("0x{}", hex::encode(revm::primitives::keccak256(inner)));
    let account = f.accounts.iter().find(|a| a.address == token).unwrap();
    let value = &account
        .storage
        .iter()
        .find(|s| s.slot == slot)
        .unwrap()
        .value;
    Amount::from_be_bytes(&hex::decode(&value[2..]).unwrap()).unwrap()
}
fn prepared_contexts(c: &Value) -> PreparedCase {
    use xone_execution_preparation::evm::{FeeContext, FundingObservation};
    let (b, p, f, tx) = bound(c);
    let before = simulate(&b, &p, &f, &tx, &budget()).unwrap();
    let native = f
        .accounts
        .iter()
        .find(|a| a.address == b.transaction.from)
        .unwrap();
    let funding = FundingObservation {
        snapshot_hash: hash(&b.plan.snapshot).unwrap(),
        caller: b.plan.context.caller.clone(),
        sell_token: b.plan.context.sell_token.clone(),
        spender: b.executor.target.clone(),
        pending_nonce: tx.nonce,
        native_balance: native.balance.clone(),
        token_balance: before.balances.caller_sell_before,
        allowance: original_allowance(
            &f,
            &b.plan.context.sell_token,
            &b.plan.context.caller,
            &b.executor.target,
        ),
        base_fee_per_gas: f.execution_environment.base_fee.clone(),
    };
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
        &funding,
        &fees,
        f.execution_environment.timestamp * 1000,
    )
    .unwrap();
    PreparedCase {
        bound: b,
        pools: p,
        fixture: f,
        funding,
        fees,
        component,
    }
}
fn prepared_case() -> PreparedCase {
    let c = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true)
        .unwrap();
    prepared_contexts(c)
}
#[test]
fn prepared_original_router_executes_exact_unsigned_component_in_16_banks() {
    use xone_simulation::prepared_evm::simulate_prepared;
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == true)
    {
        let case = prepared_contexts(c);
        let before = serde_json::to_value(&case.component).unwrap();
        let out =
            simulate_prepared(case.input(), &case.fixture, &budget(), Default::default()).unwrap();
        assert!(out.simulation.matches_plan);
        assert!(!out.simulation.verified);
        assert_eq!(out.preparation_hash, case.component.preparation_hash);
        assert_eq!(
            out.simulation.actual_buy_amount.to_string(),
            c["expected"]["amount_out"].as_str().unwrap()
        );
        assert!(out
            .simulation
            .observations
            .iter()
            .any(|o| o.phase == "caller_allowance_before"));
        let raw = hex::decode(&case.component.unsigned_transaction.as_ref().unwrap()[2..]).unwrap();
        assert_eq!(
            out.unsigned_transaction_hash,
            hex::encode(Sha256::digest(raw))
        );
        assert_eq!(serde_json::to_value(&case.component).unwrap(), before);
        let mut value = serde_json::to_value(&out).unwrap();
        value.as_object_mut().unwrap().remove("commitment_hash");
        assert_eq!(hash(&value).unwrap(), out.commitment_hash);
        count += 1;
    }
    assert_eq!(count, 16);
}
#[test]
fn prepared_tampering_even_with_rehashed_component_never_starts_execution() {
    use xone_simulation::prepared_evm::simulate_prepared;
    let mut case = prepared_case();
    let original = serde_json::to_value(&case.component).unwrap();
    for mode in 0..25 {
        let mut value = original.clone();
        match mode {
            0 => value["plan_hash"] = json!("other"),
            1 => value["binding_hash"] = json!("other"),
            2 => value["snapshot_hash"] = json!("other"),
            3 => value["funding_observation_hash"] = json!("other"),
            4 => value["transaction"]["nonce"] = json!((case.fees.nonce + 1).to_string()),
            5 => value["transaction"]["gas_limit"] = json!((case.fees.gas_limit - 1).to_string()),
            6 => value["transaction"]["max_fee_per_gas"] = json!("0"),
            7 => value["transaction"]["max_priority_fee_per_gas"] = json!("0"),
            8 => value["transaction"]["from"] = json!(case.bound.executor.factory),
            9 => value["transaction"]["to"] = json!(case.bound.executor.factory),
            10 => value["transaction"]["value"] = json!("1"),
            11 => value["transaction"]["data"] = json!("0x"),
            12 => value["transaction"]["chain_id"] = json!("1"),
            13 => value["unsigned_transaction"] = json!("0x02"),
            14 => value["rpc_transaction"]["nonce"] = json!("0x0"),
            15 => value["rpc_transaction"]["data"] = json!("0x"),
            16 => value["required_signers"] = json!([]),
            17 => value["max_network_fee"] = json!("0"),
            18 => value["intrinsic_gas_lower_bound"] = json!("0"),
            19 => value["signed"] = json!(true),
            20 => value["submitted"] = json!(true),
            21 => value["independent_production_verification"] = json!(true),
            22 => value["ready"] = json!(false),
            23 => value["blockers"] = json!(["new blocker"]),
            _ => value["schema_version"] = json!(2),
        }
        value.as_object_mut().unwrap().remove("preparation_hash");
        value["preparation_hash"] = json!(hash(&value).unwrap());
        case.component = serde_json::from_value(value).unwrap();
        let error = simulate_prepared(case.input(), &case.fixture, &budget(), Default::default())
            .unwrap_err();
        assert_eq!(
            error.code,
            SimulationErrorCode::SimulationBindingMismatch,
            "{mode}"
        );
        assert_eq!(error.usage.executions, 0, "{mode}");
    }
}
#[test]
fn prepared_observation_context_limits_and_cancellation_fail_closed() {
    use xone_simulation::prepared_evm::simulate_prepared;
    for mode in 0..10 {
        let mut case = prepared_case();
        let mut lim = budget();
        let mut control = ExecutionControl::default();
        match mode {
            0 => {
                case.funding.base_fee_per_gas =
                    Amount::checked(&case.funding.base_fee_per_gas.0 + 1u64).unwrap()
            }
            1 => case.fees.gas_limit -= 1,
            2 => {
                case.funding.native_balance =
                    Amount::checked(&case.funding.native_balance.0 + 1u64).unwrap();
                case.prepare();
            }
            3 => {
                case.funding.token_balance =
                    Amount::checked(&case.funding.token_balance.0 + 1u64).unwrap();
                case.prepare();
            }
            4 => {
                case.funding.allowance = Amount::checked(&case.funding.allowance.0 - 1u64).unwrap();
                case.prepare();
            }
            5 => {
                case.funding.pending_nonce += 1;
                case.fees.nonce += 1;
                case.prepare();
            }
            6 => lim.max_fixture_json_bytes = 1,
            7 => control
                .cancellation
                .store(true, std::sync::atomic::Ordering::Relaxed),
            8 => control.deadline = Some(Instant::now()),
            _ => lim.max_input_bytes = 1,
        }
        let error = simulate_prepared(case.input(), &case.fixture, &lim, control).unwrap_err();
        if matches!(mode, 2..=5) {
            assert_eq!(
                error.code,
                SimulationErrorCode::SimulationObservationMismatch,
                "{mode}"
            );
            assert!(error.usage.executions > 0);
        } else {
            assert_eq!(error.usage.executions, 0);
        }
    }
}

fn output_bound(
    b: &BoundEvmExecution,
    pools: &[Pool],
    target: Amount,
    budget: Amount,
) -> xone_execution_plan::BoundEvmExactOutExecution {
    use xone_optimizer::exact_out::{evaluate_allocation, ExactOutProposal};
    let route = &b.plan.allocation.routes[0];
    let allocation = evaluate_allocation(
        pools,
        &[ExactOutProposal {
            edges: route.path.edges.clone(),
            amount_out: target.clone(),
        }],
        &target,
        &budget,
        xone_routing::Funds {
            sender: &b.plan.context.caller,
            recipient: &b.plan.context.recipient,
        },
    )
    .unwrap();
    let plan = xone_execution_plan::exact_out::build(
        &b.plan.context,
        &allocation,
        &b.plan.snapshot,
        "original-router-exact-out-bank",
    )
    .unwrap();
    xone_execution_plan::bind_v2_exact_out_execution(
        &plan,
        pools,
        &b.executor,
        std::slice::from_ref(&b.executor.target),
    )
    .unwrap()
}
#[test]
fn exact_out_original_router_16_banks_measure_required_input_and_unused_budget() {
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == true)
    {
        let (b, p, f, mut tx) = bound(c);
        let target = b.plan.allocation.gross_buy_amount.clone();
        let maximum = Amount::checked(&b.plan.allocation.sell_amount.0 * 2u64).unwrap();
        let exact = output_bound(&b, &p, target.clone(), maximum);
        tx.max_priority_fee_per_gas = Some("1".parse().unwrap());
        let out = simulate_bound_exact_out(
            &exact,
            &p,
            std::slice::from_ref(&b.executor.target),
            &f,
            &tx,
            &budget(),
            Default::default(),
        )
        .unwrap();
        assert!(out.matches_plan, "{}", c["name"]);
        assert_eq!(out.execution.status, EvmStatus::Success);
        assert_eq!(out.actual_buy_amount, target);
        assert_eq!(
            out.actual_input_consumed,
            exact.plan.allocation.required_input
        );
        let unused = out.unused_input_budget.unwrap();
        assert_eq!(
            &unused.0 + &out.actual_input_consumed.0,
            exact.plan.max_sell_amount.0
        );
        assert_eq!(out.refund, Amount::zero());
        assert_ne!(unused, Amount::zero());
        // Decode original Router's actual returned amounts; the recipient balance remains authority.
        let returned = hex::decode(&out.execution.output[2..]).unwrap();
        assert_eq!(returned.len(), 128);
        assert_eq!(
            Amount::from_be_bytes(&returned[64..96]).unwrap(),
            out.actual_input_consumed
        );
        assert_eq!(
            Amount::from_be_bytes(&returned[96..128]).unwrap(),
            out.actual_buy_amount
        );
        assert_eq!(
            out.native_fees.as_ref().unwrap().effective_gas_price,
            Amount::checked(&f.execution_environment.base_fee.0 + 1u64).unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 16);
}
#[test]
fn exact_out_original_router_rejects_mutations_and_missing_state() {
    let c = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true)
        .unwrap();
    let (b, p, f, tx) = bound(c);
    let exact = output_bound(
        &b,
        &p,
        b.plan.allocation.gross_buy_amount.clone(),
        Amount::checked(&b.plan.allocation.sell_amount.0 * 2u64).unwrap(),
    );
    for mode in 0..6 {
        let mut changed = exact.clone();
        let mut fixture = f.clone();
        match mode {
            0 => changed.transaction.data.replace_range(2..10, "38ed1739"),
            1 => changed.plan.max_sell_amount = "1".parse().unwrap(),
            2 => changed.plan.allocation.required_input = "1".parse().unwrap(),
            3 => fixture
                .accounts
                .retain(|a| a.address != b.plan.context.sell_token),
            4 => fixture
                .accounts
                .iter_mut()
                .find(|a| a.address == p[0].pool_id)
                .unwrap()
                .storage
                .clear(),
            _ => fixture.execution_environment.parent_hash = format!("0x{}", "ff".repeat(32)),
        }
        let error = simulate_bound_exact_out(
            &changed,
            &p,
            std::slice::from_ref(&b.executor.target),
            &fixture,
            &tx,
            &budget(),
            Default::default(),
        )
        .unwrap_err();
        assert!(!format!("{error}").is_empty());
    }
}

#[test]
fn exact_out_original_router_reverts_leave_full_budget_and_charge_real_gas() {
    let c = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true)
        .unwrap();
    let (b, p, f, mut tx) = bound(c);
    tx.max_priority_fee_per_gas = Some("1".parse().unwrap());
    let exact = output_bound(
        &b,
        &p,
        b.plan.allocation.gross_buy_amount.clone(),
        Amount::checked(&b.plan.allocation.sell_amount.0 * 2u64).unwrap(),
    );
    for mode in 0..3 {
        let mut fixture = f.clone();
        if mode == 2 {
            fixture.execution_environment.timestamp = exact.plan.context.deadline_ms / 1000 + 1;
        } else {
            let mut outer = [0u8; 64];
            outer[12..32].copy_from_slice(
                Address::from_str(&b.plan.context.caller)
                    .unwrap()
                    .as_slice(),
            );
            let slot = if mode == 0 {
                outer[63] = 2;
                let mut inner = [0u8; 64];
                inner[12..32]
                    .copy_from_slice(Address::from_str(&b.executor.target).unwrap().as_slice());
                inner[32..].copy_from_slice(revm::primitives::keccak256(outer).as_slice());
                revm::primitives::keccak256(inner)
            } else {
                outer[63] = 1;
                revm::primitives::keccak256(outer)
            };
            let slot = format!("0x{}", hex::encode(slot));
            let account = fixture
                .accounts
                .iter_mut()
                .find(|a| a.address == b.plan.context.sell_token)
                .unwrap();
            account
                .storage
                .iter_mut()
                .find(|s| s.slot == slot)
                .unwrap()
                .value = format!("0x{}", "00".repeat(32));
        }
        let out = simulate_bound_exact_out(
            &exact,
            &p,
            std::slice::from_ref(&b.executor.target),
            &fixture,
            &tx,
            &budget(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(out.execution.status, EvmStatus::Revert, "{mode}");
        assert!(!out.matches_plan);
        assert_eq!(out.actual_input_consumed, Amount::zero());
        assert_eq!(out.actual_buy_amount, Amount::zero());
        assert_eq!(
            out.unused_input_budget.as_ref(),
            Some(&exact.plan.max_sell_amount)
        );
        assert_eq!(out.refund, Amount::zero());
        assert_eq!(out.execution.sender_nonce_after, tx.nonce + 1);
        assert_ne!(out.native_fees.unwrap().execution_fee, Amount::zero());
    }
}

#[test]
fn exact_out_valid_multi_pool_binding_still_rejects_unrelated_execution_bank() {
    use xone_optimizer::exact_out::{evaluate_allocation, ExactOutProposal};
    let doc: Value =
        serde_json::from_str(include_str!("../../../test/contracts/v2-exact-out.json")).unwrap();
    let c = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["path"].as_array().unwrap().len() > 2)
        .unwrap();
    let pools: Vec<Pool> = serde_json::from_value(c["pools"].clone()).unwrap();
    let tokens = c["path"].as_array().unwrap();
    let edges = tokens
        .windows(2)
        .map(|pair| {
            let from = pair[0].as_str().unwrap();
            let to = pair[1].as_str().unwrap();
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
        caller: doc["context"]["sender"].as_str().unwrap().into(),
        recipient: doc["context"]["recipient"].as_str().unwrap().into(),
        sell_token: tokens[0].as_str().unwrap().into(),
        buy_token: tokens.last().unwrap().as_str().unwrap().into(),
        slippage_bps: 0,
        deadline_ms: doc["context"]["deadline"].as_u64().unwrap() * 1000,
    };
    let target: Amount = c["amount_out"].as_str().unwrap().parse().unwrap();
    let maximum: Amount = c["max_input"].as_str().unwrap().parse().unwrap();
    let allocation = evaluate_allocation(
        &pools,
        &[ExactOutProposal {
            edges,
            amount_out: target.clone(),
        }],
        &target,
        &maximum,
        xone_routing::Funds {
            sender: &context.caller,
            recipient: &context.recipient,
        },
    )
    .unwrap();
    let h = &c["source"]["header"];
    let snapshot = SnapshotRef {
        block: BlockRef {
            chain_key: context.chain_key.clone(),
            number_or_slot: h["number"].as_u64().unwrap(),
            hash: h["hash"].as_str().unwrap().into(),
            parent_hash: h["parent_hash"].as_str().unwrap().into(),
            timestamp_ms: h["timestamp"].as_u64().unwrap() * 1000,
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
        "multi-pool-refusal",
    )
    .unwrap();
    let executor = ExecutorIdentity {
        target: doc["identities"]["router"]["address"]
            .as_str()
            .unwrap()
            .into(),
        code_sha256: doc["identities"]["router"]["runtime_sha256"]
            .as_str()
            .unwrap()
            .into(),
        factory: doc["identities"]["factory"]["address"]
            .as_str()
            .unwrap()
            .into(),
        factory_code_sha256: doc["identities"]["factory"]["runtime_sha256"]
            .as_str()
            .unwrap()
            .into(),
        adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
    };
    let exact = xone_execution_plan::bind_v2_exact_out_execution(
        &plan,
        &pools,
        &executor,
        std::slice::from_ref(&executor.target),
    )
    .unwrap();
    xone_execution_plan::validate_bound_exact_out_execution(
        &exact,
        &pools,
        std::slice::from_ref(&executor.target),
    )
    .unwrap();
    let first = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["service_eligible"] == true)
        .unwrap();
    let (_, _, fixture, tx) = bound(first);
    let error = simulate_bound_exact_out(
        &exact,
        &pools,
        std::slice::from_ref(&executor.target),
        &fixture,
        &tx,
        &budget(),
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, SimulationErrorCode::SimulationAnchorMismatch);
    assert_eq!(error.usage.executions, 0);
}

#[test]
fn prepared_exact_out_16_original_banks_bind_both_commitments_and_unused_input() {
    use xone_simulation::prepared_evm::{simulate_prepared_exact_out, PreparedExactOutExecution};
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == true)
    {
        let case = prepared_contexts(c);
        let exact = output_bound(
            &case.bound,
            &case.pools,
            case.bound.plan.allocation.gross_buy_amount.clone(),
            Amount::checked(&case.bound.plan.allocation.sell_amount.0 * 2u64).unwrap(),
        );
        let preparation = xone_execution_preparation::evm::prepare_exact_out(
            &exact,
            &case.pools,
            std::slice::from_ref(&exact.executor.target),
            &case.funding,
            &case.fees,
            case.fixture.execution_environment.timestamp * 1000,
        )
        .unwrap();
        let original = serde_json::to_value(&preparation).unwrap();
        let out = simulate_prepared_exact_out(
            PreparedExactOutExecution {
                bound: &exact,
                pools: &case.pools,
                allowed_targets: std::slice::from_ref(&exact.executor.target),
                funding: &case.funding,
                fees: &case.fees,
                preparation: &preparation,
            },
            &case.fixture,
            &budget(),
            Default::default(),
        )
        .unwrap();
        assert!(out.simulation.matches_plan);
        assert!(!out.simulation.verified);
        assert_eq!(
            out.preparation_hash,
            preparation.preparation.preparation_hash
        );
        assert_eq!(
            out.exact_out_preparation_hash.as_ref(),
            Some(&preparation.exact_out_preparation_hash)
        );
        assert_eq!(
            out.simulation.actual_input_consumed,
            exact.plan.allocation.required_input
        );
        assert_eq!(out.simulation.actual_buy_amount, preparation.target_output);
        let unspent = out.simulation.unused_input_budget.as_ref().unwrap();
        assert_eq!(
            &unspent.0 + &out.simulation.actual_input_consumed.0,
            exact.plan.max_sell_amount.0
        );
        assert_eq!(
            &preparation.unused_quoted_input_budget.0 + &out.simulation.actual_input_consumed.0,
            exact.plan.allocation.input_budget.0
        );
        // The request budget may exceed the slippage-limited encoded maximum.
        assert!(exact.plan.max_sell_amount <= exact.plan.allocation.input_budget);
        assert_eq!(out.simulation.refund, Amount::zero());
        let raw = hex::decode(
            &preparation
                .preparation
                .unsigned_transaction
                .as_ref()
                .unwrap()[2..],
        )
        .unwrap();
        assert_eq!(
            out.unsigned_transaction_hash,
            hex::encode(Sha256::digest(raw))
        );
        let mut value = serde_json::to_value(&out).unwrap();
        value.as_object_mut().unwrap().remove("commitment_hash");
        assert_eq!(hash(&value).unwrap(), out.commitment_hash);
        assert_eq!(serde_json::to_value(preparation).unwrap(), original);
        count += 1;
    }
    assert_eq!(count, 16);
}
#[test]
fn prepared_exact_out_rehashed_outer_inner_tampering_and_funding_fail_closed() {
    use xone_simulation::prepared_evm::{simulate_prepared_exact_out, PreparedExactOutExecution};
    let case = prepared_case();
    let exact = output_bound(
        &case.bound,
        &case.pools,
        case.bound.plan.allocation.gross_buy_amount.clone(),
        Amount::checked(&case.bound.plan.allocation.sell_amount.0 * 2u64).unwrap(),
    );
    let prepare = |funding: &xone_execution_preparation::evm::FundingObservation| {
        xone_execution_preparation::evm::prepare_exact_out(
            &exact,
            &case.pools,
            std::slice::from_ref(&exact.executor.target),
            funding,
            &case.fees,
            case.fixture.execution_environment.timestamp * 1000,
        )
        .unwrap()
    };
    let original = serde_json::to_value(prepare(&case.funding)).unwrap();
    for mode in 0..16 {
        let mut value = original.clone();
        let mut funding = case.funding.clone();
        match mode {
            0 => value["target_output"] = json!("1"),
            1 => value["max_sell_amount"] = json!("1"),
            2 => value["unused_quoted_input_budget"] = json!("0"),
            3 => value["preparation"]["transaction"]["data"] = json!("0x"),
            4 => value["preparation"]["transaction"]["nonce"] = json!("0"),
            5 => value["preparation"]["transaction"]["max_fee_per_gas"] = json!("0"),
            6 => value["preparation"]["transaction"]["to"] = json!(exact.executor.factory),
            7 => value["preparation"]["unsigned_transaction"] = json!("0x02"),
            8 => value["preparation"]["rpc_transaction"]["data"] = json!("0x"),
            9 => value["preparation"]["signed"] = json!(true),
            10 => value["preparation"]["ready"] = json!(false),
            11 => value["preparation"]["required_input"] = json!("1"),
            12 => {
                funding.native_balance = Amount::checked(&funding.native_balance.0 + 1u64).unwrap();
                value = serde_json::to_value(prepare(&funding)).unwrap();
            }
            13 => {
                funding.token_balance = Amount::checked(&funding.token_balance.0 + 1u64).unwrap();
                value = serde_json::to_value(prepare(&funding)).unwrap();
            }
            14 => {
                funding.allowance = Amount::checked(&funding.allowance.0 - 1u64).unwrap();
                value = serde_json::to_value(prepare(&funding)).unwrap();
            }
            _ => {
                funding.base_fee_per_gas =
                    Amount::checked(&funding.base_fee_per_gas.0 + 1u64).unwrap()
            }
        }
        value["preparation"]
            .as_object_mut()
            .unwrap()
            .remove("preparation_hash");
        value["preparation"]["preparation_hash"] = json!(hash(&value["preparation"]).unwrap());
        value
            .as_object_mut()
            .unwrap()
            .remove("exact_out_preparation_hash");
        value["exact_out_preparation_hash"] = json!(hash(&value).unwrap());
        let changed = serde_json::from_value(value).unwrap();
        let error = simulate_prepared_exact_out(
            PreparedExactOutExecution {
                bound: &exact,
                pools: &case.pools,
                allowed_targets: std::slice::from_ref(&exact.executor.target),
                funding: &funding,
                fees: &case.fees,
                preparation: &changed,
            },
            &case.fixture,
            &budget(),
            Default::default(),
        )
        .unwrap_err();
        if (12..15).contains(&mode) {
            assert_eq!(
                error.code,
                SimulationErrorCode::SimulationObservationMismatch
            );
        } else {
            assert_eq!(error.code, SimulationErrorCode::SimulationBindingMismatch);
            assert_eq!(error.usage.executions, 0);
        }
    }
}
#[test]
fn prepared_exact_out_limits_and_cancellation_precede_bank_execution() {
    use xone_simulation::prepared_evm::{simulate_prepared_exact_out, PreparedExactOutExecution};
    let case = prepared_case();
    let exact = output_bound(
        &case.bound,
        &case.pools,
        case.bound.plan.allocation.gross_buy_amount.clone(),
        Amount::checked(&case.bound.plan.allocation.sell_amount.0 * 2u64).unwrap(),
    );
    let preparation = xone_execution_preparation::evm::prepare_exact_out(
        &exact,
        &case.pools,
        std::slice::from_ref(&exact.executor.target),
        &case.funding,
        &case.fees,
        case.fixture.execution_environment.timestamp * 1000,
    )
    .unwrap();
    for mode in 0..4 {
        let mut lim = budget();
        let mut control = ExecutionControl::default();
        match mode {
            0 => lim.max_fixture_json_bytes = 1,
            1 => lim.max_input_bytes = 1,
            2 => control
                .cancellation
                .store(true, std::sync::atomic::Ordering::Relaxed),
            _ => control.deadline = Some(Instant::now()),
        };
        let error = simulate_prepared_exact_out(
            PreparedExactOutExecution {
                bound: &exact,
                pools: &case.pools,
                allowed_targets: std::slice::from_ref(&exact.executor.target),
                funding: &case.funding,
                fees: &case.fees,
                preparation: &preparation,
            },
            &case.fixture,
            &lim,
            control,
        )
        .unwrap_err();
        assert_eq!(error.usage.executions, 0);
    }
}
