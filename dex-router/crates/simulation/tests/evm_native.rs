//! Published original contracts in Ganache are an independent execution oracle.
//! Expected amounts are consumed only after REVM has executed captured code/state.
use revm::primitives::{
    Address, BlockEnv, Bytes, ExecutionResult, SpecId, TxEnv, TxKind, B256, U256,
};
use serde_json::{json, Value};
use std::{str::FromStr, sync::Arc};
use xone_domain::{Amount, BlockRef, Pool, SnapshotRef};
use xone_execution_plan::{
    bind_v2_execution, build, validate_bound_execution, BoundEvmExecution, Context,
    ExecutorIdentity,
};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{search, Budget, Graph};
use xone_simulation::{
    evm_fixture::*,
    revm_backend::{self as core, ExecutionControl, ExecutionLimits},
};
fn corpus() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/evm-execution.json")).unwrap()
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
    serde_json::from_value(json!({"schema_version":1,"state_anchor":c["state_anchor"],"execution_environment":c["execution_environment"],"accounts":c["accounts"],"block_hashes":c["block_hashes"]})).unwrap()
}
fn number(s: &str) -> U256 {
    U256::from_str_radix(s, 10).unwrap()
}
fn address(s: &str) -> Address {
    Address::from_str(s).unwrap()
}
fn block(f: &FixtureState) -> BlockEnv {
    let e = &f.execution_environment;
    BlockEnv {
        number: U256::from(e.block_number),
        timestamp: U256::from(e.timestamp),
        coinbase: address(&e.coinbase),
        gas_limit: U256::from(e.gas_limit),
        basefee: number(&e.base_fee.to_string()),
        prevrandao: Some(B256::from_str(&e.prevrandao).unwrap()),
        ..Default::default()
    }
}
fn limits() -> ExecutionLimits {
    let b = budget();
    ExecutionLimits {
        reads: core::BoundReadLimits {
            max_reads: b.max_reads,
            max_bytes: b.max_read_bytes,
        },
        max_gas_limit: b.max_transaction_gas,
        max_instructions: b.max_instructions,
        max_input_bytes: b.max_input_bytes,
        max_memory_bytes: b.max_memory_bytes,
    }
}
fn query_balance(
    image: Arc<core::StateImage>,
    f: &FixtureState,
    c: &Value,
    token: &str,
    owner: &str,
) -> U256 {
    let caller = address(c["transaction"]["from"].as_str().unwrap());
    let nonce = image.data().accounts[&caller].as_ref().unwrap().nonce;
    let data = format!("70a08231{:0>64}", &owner[2..]);
    let request = core::TransactionRequest {
        fork: SpecId::SHANGHAI,
        chain_id: 31337,
        block: block(f),
        tx: TxEnv {
            caller,
            transact_to: TxKind::Call(address(token)),
            data: Bytes::from(hex::decode(data).unwrap()),
            nonce: Some(nonce),
            chain_id: Some(31337),
            gas_limit: 200_000,
            gas_price: number(c["transaction_context"]["gas_price"].as_str().unwrap()),
            ..Default::default()
        },
    };
    let out = core::execute(image, request, limits(), Default::default()).unwrap();
    assert!(out.result_and_state.result.is_success());
    let data = out.result_and_state.result.output().unwrap();
    assert_eq!(data.len(), 32);
    U256::from_be_slice(data)
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
    let pools = vec![pool];
    let amount: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
    let paths = search(
        &Graph::build(&pools).unwrap(),
        &pools,
        c["token_in"].as_str().unwrap(),
        c["token_out"].as_str().unwrap(),
        &amount,
        Budget {
            split_parts: 1,
            max_hops: 1,
            max_candidates: 2,
            max_expansions: 8,
        },
    )
    .unwrap();
    let path = paths
        .candidates
        .first()
        .unwrap_or_else(|| {
            panic!(
                "no original reference path {} sell={} buy={} input={} exhausted={}",
                c["name"], c["token_in"], c["token_out"], amount, paths.budget_exhausted
            )
        })
        .clone();
    let route = AllocatedRoute {
        path: path.clone(),
        amount_in: amount.clone(),
        amount_out: path.amount_out.clone(),
    };
    let allocation = evaluate_allocation(&pools, &[route], &Amount::zero()).unwrap();
    let source = &f.state_anchor;
    let reference = SnapshotRef {
        block: BlockRef {
            chain_key: source.chain_key.clone(),
            number_or_slot: source.block_number,
            hash: source.block_hash.clone(),
            parent_hash: source.parent_hash.clone(),
            timestamp_ms: source.timestamp * 1000,
        },
        canonical_epoch: 1,
        journal_seq: 1,
        schema_version: 1,
        adapter_set_hash: xone_domain::hash(&vec!["uniswap-v2/1"]).unwrap(),
        online_generation: 1,
        scope_hash: xone_domain::scope_hash(&pools).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let bytes = hex::decode(
        c["transaction"]["data"]
            .as_str()
            .unwrap()
            .strip_prefix("0x")
            .unwrap(),
    )
    .unwrap();
    let deadline = u64::try_from(U256::from_be_slice(&bytes[4 + 32 * 4..4 + 32 * 5])).unwrap();
    let plan = build(
        &Context {
            chain_key: source.chain_key.clone(),
            caller: c["transaction"]["from"].as_str().unwrap().into(),
            recipient: c["recipient"].as_str().unwrap().into(),
            sell_token: c["token_in"].as_str().unwrap().into(),
            buy_token: c["token_out"].as_str().unwrap().into(),
            slippage_bps: 50,
            deadline_ms: deadline * 1000,
        },
        &allocation,
        &reference,
        "native-original-v2-reference",
    )
    .unwrap();
    let executor: ExecutorIdentity = serde_json::from_value(c["executor"].clone()).unwrap();
    let allowed = vec![executor.target.clone()];
    let bound = bind_v2_execution(&plan, &pools, &executor, &allowed).unwrap();
    let tx = serde_json::from_value(c["transaction_context"].clone()).unwrap();
    (bound, pools, f, tx)
}
#[test]
fn original_ten_transactions_execute_captured_state_and_measure_real_token_deltas() {
    let doc = corpus();
    let cases = doc["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    let mut success = 0;
    let mut reverts = 0;
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let f = fixture(c);
        let image = f.to_image(&budget()).unwrap();
        let caller = c["transaction"]["from"].as_str().unwrap();
        let recipient = c["recipient"].as_str().unwrap();
        let sell = c["token_in"].as_str().unwrap();
        let buy = c["token_out"].as_str().unwrap();
        let sell_before = query_balance(image.clone(), &f, c, sell, caller);
        let buy_before = query_balance(image.clone(), &f, c, buy, recipient);
        let ctx: TransactionContext =
            serde_json::from_value(c["transaction_context"].clone()).unwrap();
        let request = core::TransactionRequest {
            fork: SpecId::SHANGHAI,
            chain_id: ctx.chain_id,
            block: block(&f),
            tx: TxEnv {
                caller: address(caller),
                transact_to: TxKind::Call(address(c["transaction"]["to"].as_str().unwrap())),
                data: Bytes::from(
                    hex::decode(&c["transaction"]["data"].as_str().unwrap()[2..]).unwrap(),
                ),
                nonce: Some(ctx.nonce),
                chain_id: Some(ctx.chain_id),
                gas_limit: ctx.gas_limit,
                gas_price: number(&ctx.gas_price.to_string()),
                ..Default::default()
            },
        };
        let out = core::execute(
            image.clone(),
            request,
            limits(),
            ExecutionControl::default(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let e = &c["expected"];
        assert_eq!(
            out.result_and_state.result.is_success(),
            e["status"] == "success",
            "status {name}"
        );
        assert_eq!(
            out.result_and_state.result.gas_used().to_string(),
            e["gas_used"].as_str().unwrap(),
            "gas {name}"
        );
        let post = Arc::new(image.apply(&out.result_and_state.state).unwrap());
        let sell_after = query_balance(post.clone(), &f, c, sell, caller);
        let buy_after = query_balance(post.clone(), &f, c, buy, recipient);
        for (actual, key) in [
            (sell_before, "caller_sell_before"),
            (sell_after, "caller_sell_after"),
            (buy_before, "recipient_buy_before"),
            (buy_after, "recipient_buy_after"),
        ] {
            assert_eq!(actual.to_string(), e[key].as_str().unwrap(), "{key} {name}");
        }
        assert_eq!(
            (sell_before - sell_after).to_string(),
            e["sender_input_consumed"].as_str().unwrap()
        );
        assert_eq!(
            (buy_after - buy_before).to_string(),
            e["amount_out"].as_str().unwrap()
        );
        assert_eq!(
            post.data().accounts[&address(caller)]
                .as_ref()
                .unwrap()
                .nonce,
            e["caller_nonce_after"].as_u64().unwrap()
        );
        let observed = e["post_accounts"]
            .as_array()
            .unwrap_or_else(|| panic!("original native post image missing {name}"));
        assert_eq!(observed.len(), c["accounts"].as_array().unwrap().len());
        for account in observed {
            let owner = address(account["address"].as_str().unwrap());
            let actual = post
                .data()
                .accounts
                .get(&owner)
                .unwrap_or_else(|| panic!("post account missing {name}: {owner}"));
            if account["exists"] == false {
                assert!(actual.is_none());
                continue;
            }
            let actual = actual.as_ref().unwrap();
            assert_eq!(
                actual.balance.to_string(),
                account["balance"].as_str().unwrap(),
                "native ETH balance {name}: {owner}"
            );
            assert_eq!(
                actual.nonce,
                account["nonce"].as_u64().unwrap(),
                "native nonce {name}: {owner}"
            );
            let code = hex::decode(&account["code"].as_str().unwrap()[2..]).unwrap();
            assert_eq!(
                actual.code_hash,
                revm::primitives::keccak256(code),
                "native post code {name}: {owner}"
            );
            for row in account["storage"].as_array().unwrap() {
                let slot = U256::from_str_radix(&row["slot"].as_str().unwrap()[2..], 16).unwrap();
                let expected =
                    U256::from_str_radix(&row["value"].as_str().unwrap()[2..], 16).unwrap();
                assert_eq!(
                    *post
                        .data()
                        .storage
                        .get(&(owner, slot))
                        .unwrap_or_else(|| panic!("missing post storage {name}: {owner}/{slot}")),
                    expected,
                    "native post storage {name}: {owner}/{slot}"
                );
            }
        }
        let pair = address(c["pool"]["pool_id"].as_str().unwrap());
        let packed = post.data().storage[&(pair, U256::from(8))];
        let mask: U256 = (U256::from(1) << 112usize) - U256::from(1);
        assert_eq!(
            (packed & mask).to_string(),
            e["pool_after"]["reserve0"].as_str().unwrap(),
            "actual post reserves {name}"
        );
        assert_eq!(
            ((packed >> 112usize) & mask).to_string(),
            e["pool_after"]["reserve1"].as_str().unwrap()
        );
        assert_eq!(
            u64::try_from(packed >> 224usize).unwrap(),
            e["pool_after"]["block_timestamp_last"].as_u64().unwrap()
        );
        if matches!(out.result_and_state.result, ExecutionResult::Success { .. }) {
            success += 1;
        } else {
            reverts += 1;
        }
    }
    assert_eq!((success, reverts), (6, 4));
}
#[test]
fn bound_original_v2_cases_use_exact_calldata_and_actual_balance_evidence() {
    let doc = corpus();
    let mut checked = 0;
    for original in doc["cases"].as_array().unwrap() {
        let mut diagnostic = original.clone();
        let name = original["name"].as_str().unwrap();
        if name == "deadline-revert" {
            continue;
        }
        if name == "fee-token-output-counterexample" {
            // The genuine simulation-only capability is blocked before quoting. Deliberately
            // misdeclare it in this adversarial test to prove measured deltas still refuse
            // verification even if an upstream capability declaration is incorrect.
            let genuine: Pool = serde_json::from_value(original["pool"].clone()).unwrap();
            let amount: Amount = original["amount_in"].as_str().unwrap().parse().unwrap();
            let candidates = search(
                &Graph::build(std::slice::from_ref(&genuine)).unwrap(),
                std::slice::from_ref(&genuine),
                original["token_in"].as_str().unwrap(),
                original["token_out"].as_str().unwrap(),
                &amount,
                Budget {
                    split_parts: 1,
                    max_hops: 1,
                    max_candidates: 2,
                    max_expansions: 8,
                },
            )
            .unwrap();
            assert!(candidates.candidates.is_empty());
            diagnostic["pool"]["token_behavior"] = json!("standard");
        }
        let c = &diagnostic;
        let (mut b, pools, f, tx) = bound(c);
        let allowed = vec![b.executor.target.clone()];
        let actual = c["transaction"]["data"].as_str().unwrap();
        if name == "min-out-revert" {
            b.transaction.data = actual.into();
            assert!(validate_bound_execution(&b, &pools, &allowed).is_err());
            continue;
        }
        assert_eq!(
            b.transaction.data, actual,
            "exact bound original calldata {name}"
        );
        let out = simulate_bound(&b, &pools, &allowed, &f, &tx, &budget(), Default::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let expected = &c["expected"];
        assert_eq!(
            out.actual_buy_amount.to_string(),
            expected["amount_out"].as_str().unwrap(),
            "recipient delta {name}"
        );
        assert_eq!(
            out.actual_input_consumed.to_string(),
            expected["sender_input_consumed"].as_str().unwrap()
        );
        assert_eq!(
            out.execution.gas_used.to_string(),
            expected["gas_used"].as_str().unwrap()
        );
        assert!(!out.verified);
        if name == "fee-token-output-counterexample" {
            assert!(!out.matches_plan);
            assert!(out.actual_buy_amount < b.plan.min_buy_amount);
        } else {
            assert_eq!(
                out.matches_plan,
                expected["status"] == "success",
                "matches {name}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn real_swap_spending_gas_does_not_need_another_probe_prepayment() {
    let doc = corpus();
    let case = &doc["cases"][0];
    let (bound, pools, mut fixture, mut context) = bound(case);
    context.gas_limit = 250_000;
    let caller = bound.plan.context.caller.clone();
    fixture
        .accounts
        .iter_mut()
        .find(|a| a.address == caller)
        .unwrap()
        .balance = "500000000000000".parse().unwrap();
    let output = simulate_bound(
        &bound,
        &pools,
        std::slice::from_ref(&bound.executor.target),
        &fixture,
        &context,
        &budget(),
        ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!(output.execution.status, EvmStatus::Success);
    assert!(output.matches_plan);
    assert_eq!(
        output.execution.gas_used.to_string(),
        case["expected"]["gas_used"].as_str().unwrap()
    );
    assert_eq!(
        output.actual_buy_amount.to_string(),
        case["expected"]["amount_out"].as_str().unwrap()
    );
}

#[test]
fn out_of_gas_receipt_survives_zero_native_balance_after_real_execution() {
    let doc = corpus();
    let case = &doc["cases"][0];
    let (bound, pools, mut fixture, mut context) = bound(case);
    let calldata = hex::decode(bound.transaction.data.strip_prefix("0x").unwrap()).unwrap();
    let intrinsic = 21_000
        + calldata
            .iter()
            .map(|b| if *b == 0 { 4u64 } else { 16u64 })
            .sum::<u64>();
    // One execution gas unit cannot pay the first Router opcode. Unlike an
    // exhausted nested CALL (which Router can turn into REVERT), this is HALT.
    context.gas_limit = intrinsic + 1;
    let upfront = context.gas_limit * context.gas_price.to_string().parse::<u64>().unwrap();
    let caller = bound.plan.context.caller.clone();
    fixture
        .accounts
        .iter_mut()
        .find(|a| a.address == caller)
        .unwrap()
        .balance = upfront.to_string().parse().unwrap();
    let output = simulate_bound(
        &bound,
        &pools,
        std::slice::from_ref(&bound.executor.target),
        &fixture,
        &context,
        &budget(),
        ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!(output.execution.status, EvmStatus::Halt);
    assert_eq!(output.execution.gas_used, context.gas_limit);
    assert!(!output.matches_plan);
    assert_eq!(output.actual_buy_amount.to_string(), "0");
    assert_eq!(output.actual_input_consumed.to_string(), "0");
}
