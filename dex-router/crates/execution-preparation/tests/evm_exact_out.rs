use serde_json::{json, Value};
use xone_domain::{hash, scope_hash, Amount, BlockRef, Pool, SnapshotRef};
use xone_execution_plan::{
    bind_v2_exact_out_execution, validate_bound_exact_out_execution, BoundEvmExactOutExecution,
};
use xone_execution_plan::{
    exact_out::{self, ExactOutPlan},
    Context, ExecutorIdentity,
};
use xone_execution_preparation::evm::{self, FeeContext, FundingObservation};
use xone_optimizer::exact_out::{evaluate_allocation, ExactOutProposal};
use xone_routing::{Edge, Funds};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/v2-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn fixture(c: &Value, doc: &Value) -> (ExactOutPlan, Vec<Pool>, ExecutorIdentity) {
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
    let target: Amount = c["amount_out"].as_str().unwrap().parse().unwrap();
    let budget: Amount = c["max_input"].as_str().unwrap().parse().unwrap();
    let context = Context {
        chain_key: "evm:31337".into(),
        caller: doc["context"]["sender"].as_str().unwrap().into(),
        recipient: doc["context"]["recipient"].as_str().unwrap().into(),
        sell_token: tokens.first().unwrap().as_str().unwrap().into(),
        buy_token: tokens.last().unwrap().as_str().unwrap().into(),
        slippage_bps: 0,
        deadline_ms: doc["context"]["deadline"].as_u64().unwrap() * 1000,
    };
    let allocation = evaluate_allocation(
        &pools,
        &[ExactOutProposal {
            edges,
            amount_out: target.clone(),
        }],
        &target,
        &budget,
        Funds {
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
        adapter_set_hash: hash(&vec![xone_domain::v2_swap::ADAPTER]).unwrap(),
        online_generation: 1,
        scope_hash: scope_hash(&pools).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let plan = exact_out::build(
        &context,
        &allocation,
        &snapshot,
        "isolated-evm-output-draft",
    )
    .unwrap();
    let identities = &doc["identities"];
    let executor = ExecutorIdentity {
        target: identities["router"]["address"].as_str().unwrap().into(),
        code_sha256: identities["router"]["runtime_sha256"]
            .as_str()
            .unwrap()
            .into(),
        factory: identities["factory"]["address"].as_str().unwrap().into(),
        factory_code_sha256: identities["factory"]["runtime_sha256"]
            .as_str()
            .unwrap()
            .into(),
        adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
    };
    (plan, pools, executor)
}
fn contexts(bound: &BoundEvmExactOutExecution) -> (FundingObservation, FeeContext, u64) {
    let amount = &bound.plan.allocation.required_input;
    let observed = FundingObservation {
        snapshot_hash: hash(&bound.plan.snapshot).unwrap(),
        caller: bound.plan.context.caller.clone(),
        sell_token: bound.plan.context.sell_token.clone(),
        spender: bound.executor.target.clone(),
        pending_nonce: 7,
        native_balance: "10000000000000000000".parse().unwrap(),
        token_balance: amount.clone(),
        allowance: amount.clone(),
        base_fee_per_gas: "1000000000".parse().unwrap(),
    };
    let fees = FeeContext {
        chain_id: 31337,
        nonce: 7,
        gas_limit: 700000,
        max_fee_per_gas: "3000000000".parse().unwrap(),
        max_priority_fee_per_gas: "1000000000".parse().unwrap(),
        max_network_fee: "2100000000000000".parse().unwrap(),
    };
    (observed, fees, bound.plan.snapshot.block.timestamp_ms + 1)
}
#[test]
fn every_original_router_output_case_binds_complete_paths_and_uint256_preparation() {
    let doc = corpus();
    let mut count = 0;
    let mut multihop = 0;
    let mut wide = 0;
    for case in doc["cases"].as_array().unwrap() {
        let (plan, pools, executor) = fixture(case, &doc);
        let allowed = vec![executor.target.clone()];
        let bound = bind_v2_exact_out_execution(&plan, &pools, &executor, &allowed)
            .unwrap_or_else(|e| panic!("{}: {e}", case["id"]));
        validate_bound_exact_out_execution(&bound, &pools, &allowed).unwrap();
        let (o, f, now) = contexts(&bound);
        let prepared = evm::prepare_exact_out(&bound, &pools, &allowed, &o, &f, now).unwrap();
        assert!(prepared.preparation.ready);
        assert_eq!(
            prepared.preparation.required_input.to_string(),
            case["expected"]["required_input"]
        );
        assert_eq!(
            prepared.target_output.to_string(),
            case["expected"]["recipient_delta"]
        );
        assert_eq!(
            prepared.unused_quoted_input_budget.to_string(),
            case["expected"]["unused_input_budget"]
        );
        assert_eq!(
            prepared.preparation.transaction.as_ref().unwrap().data,
            bound.transaction.data
        );
        assert_eq!(
            hex::decode(&bound.transaction.data[2..6]).unwrap(),
            vec![0x88, 0x03]
        );
        assert_eq!(prepared.max_sell_amount, plan.max_sell_amount);
        assert_eq!(o.token_balance, plan.allocation.required_input);
        assert_eq!(o.allowance, plan.allocation.required_input); // Original Router pulls only its computed input; unused budget is not sent/refunded.
        if plan.allocation.required_input.0.bits() > 64 {
            wide += 1;
        }
        if plan.allocation.routes[0].edges.len() > 1 {
            multihop += 1;
        }
        let mut commitment = serde_json::to_value(&prepared).unwrap();
        commitment
            .as_object_mut()
            .unwrap()
            .remove("exact_out_preparation_hash");
        assert_eq!(
            hash(&commitment).unwrap(),
            prepared.exact_out_preparation_hash
        );
        if let Ok(directory) = std::env::var("XONE_EVM_EXACT_OUT_PREPARATION_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(std::path::Path::new(&directory).join(format!("output-{}.json",case["id"])),serde_json::to_vec_pretty(&json!({"case_id":case["id"],"bound":bound,"preparation":prepared,"observed":o,"fees":f})).unwrap()).unwrap();
        }
        count += 1;
    }
    assert_eq!(count, 90);
    assert!(multihop >= 30);
    assert!(wide >= 9);
}
#[test]
fn insufficient_actual_input_or_allowance_blocks_exact_output_wire() {
    let doc = corpus();
    let (p, pools, e) = fixture(&doc["cases"][0], &doc);
    let allowed = vec![e.target.clone()];
    let b = bind_v2_exact_out_execution(&p, &pools, &e, &allowed).unwrap();
    let (o, f, now) = contexts(&b);
    for mode in [0, 1] {
        let mut observation = o.clone();
        if mode == 0 {
            observation.token_balance = Amount::zero();
        } else {
            observation.allowance = Amount::zero();
        }
        let result = evm::prepare_exact_out(&b, &pools, &allowed, &observation, &f, now).unwrap();
        assert!(!result.preparation.ready);
        assert!(
            result.preparation.unsigned_transaction.is_none()
                && result.preparation.transaction.is_none()
        );
    }
}
#[test]
fn recomputed_economic_hash_cannot_hide_changed_required_input_or_factory() {
    let doc = corpus();
    let (plan, pools, e) = fixture(&doc["cases"][12], &doc);
    let allowed = vec![e.target.clone()];
    let mut a = plan.allocation.clone();
    a.required_input = Amount::checked(&a.required_input.0 + 1u8).unwrap();
    a.unused_input_budget = Amount::checked(&a.input_budget.0 - &a.required_input.0).unwrap();
    a.routes[0].amount_in = a.required_input.clone();
    a.routes[0].amounts[0] = a.required_input.clone();
    let forged = exact_out::build(
        &plan.context,
        &a,
        &plan.snapshot,
        &plan.economic_policy_hash,
    )
    .unwrap();
    assert_eq!(
        bind_v2_exact_out_execution(&forged, &pools, &e, &allowed)
            .unwrap_err()
            .0,
        "EXECUTION_ALLOCATION_MISMATCH"
    );
    let mut wrong = e.clone();
    wrong.factory = "0x0000000000000000000000000000000000001234".into();
    assert!(bind_v2_exact_out_execution(&plan, &pools, &wrong, &allowed).is_err());
    let mut wrong = e.clone();
    wrong.factory_code_sha256 = "12".repeat(32);
    assert_eq!(
        bind_v2_exact_out_execution(&plan, &pools, &wrong, &allowed)
            .unwrap_err()
            .0,
        "EXECUTION_FACTORY_CODE_MISMATCH"
    );
}
#[test]
fn stale_pool_source_cannot_use_a_valid_snapshot_or_pda_scope_hash() {
    let doc = corpus();
    let (plan, mut pools, e) = fixture(&doc["cases"][0], &doc);
    let allowed = vec![e.target.clone()];
    let touched = &plan.allocation.routes[0].edges[0].pool_id;
    let pool = pools.iter_mut().find(|p| &p.pool_id == touched).unwrap();
    pool.v2
        .as_mut()
        .unwrap()
        .swap
        .as_mut()
        .unwrap()
        .bank_block_number += 1;
    let mut changed = plan.clone();
    changed.snapshot.scope_hash = scope_hash(&pools).unwrap();
    changed = exact_out::build(
        &changed.context,
        &changed.allocation,
        &changed.snapshot,
        &changed.economic_policy_hash,
    )
    .unwrap();
    assert!(bind_v2_exact_out_execution(&changed, &pools, &e, &allowed).is_err());
}
#[test]
fn calldata_binding_or_budget_mutations_cannot_reuse_an_original_preparation() {
    let doc = corpus();
    let (plan, pools, e) = fixture(&doc["cases"][0], &doc);
    let allowed = vec![e.target.clone()];
    let b = bind_v2_exact_out_execution(&plan, &pools, &e, &allowed).unwrap();
    let (o, f, now) = contexts(&b);
    for mode in [0, 1, 2] {
        let mut wrong = b.clone();
        match mode {
            0 => wrong.transaction.data = "0x8803dbee".into(),
            1 => wrong.touched_pool_ids.clear(),
            _ => wrong.binding_hash = "00".repeat(32),
        };
        assert!(evm::prepare_exact_out(&wrong, &pools, &allowed, &o, &f, now).is_err());
    }
    assert!(bind_v2_exact_out_execution(&plan, &pools, &e, &[]).is_err());
}

#[test]
fn multiple_output_routes_need_an_explicit_execution_template() {
    let doc = corpus();
    let (plan, pools, e) = fixture(&doc["cases"][0], &doc);
    let output = Amount::checked(&plan.allocation.target_output.0 * 2u8).unwrap();
    let proposal = ExactOutProposal {
        edges: plan.allocation.routes[0].edges.clone(),
        amount_out: plan.allocation.target_output.clone(),
    };
    let allocation = evaluate_allocation(
        &pools,
        &[proposal.clone(), proposal],
        &output,
        &"1000000".parse().unwrap(),
        Funds {
            sender: &plan.context.caller,
            recipient: &plan.context.recipient,
        },
    )
    .unwrap();
    let split = exact_out::build(
        &plan.context,
        &allocation,
        &plan.snapshot,
        &plan.economic_policy_hash,
    )
    .unwrap();
    assert_eq!(
        bind_v2_exact_out_execution(&split, &pools, &e, std::slice::from_ref(&e.target))
            .unwrap_err()
            .0,
        "EXECUTION_ADAPTER_UNSUPPORTED"
    );
}

#[test]
fn nonce_exhaustion_rejects_exact_output_before_producing_wire() {
    let doc = corpus();
    let (plan, pools, executor) = fixture(&doc["cases"][0], &doc);
    let allowed = vec![executor.target.clone()];
    let bound = bind_v2_exact_out_execution(&plan, &pools, &executor, &allowed).unwrap();
    let (mut observed, mut fees, now) = contexts(&bound);
    observed.pending_nonce = u64::MAX;
    fees.nonce = u64::MAX;
    assert_eq!(
        evm::prepare_exact_out(&bound, &pools, &allowed, &observed, &fees, now)
            .unwrap_err()
            .0,
        "PREPARATION_EVM_NONCE_EXHAUSTED"
    );
}
