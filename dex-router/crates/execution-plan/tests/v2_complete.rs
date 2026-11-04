//! Exact new-profile admission against the local original-contract bank.
use serde_json::Value;
use std::sync::OnceLock;
use xone_domain::{hash, scope_hash, v2_swap, Amount, BlockRef, Pool, SnapshotRef};
use xone_execution_plan::*;
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};

fn corpus() -> &'static Value {
    static BANK: OnceLock<Value> = OnceLock::new();
    BANK.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/v2-complete.json"
            ))
            .unwrap(),
        )
        .unwrap()
    })
}
fn fixture(c: &Value) -> (Plan, Vec<Pool>, ExecutorIdentity) {
    let pool: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let pools = vec![pool.clone()];
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
    let allocation = evaluate_allocation(&pools, &[route], &Amount::zero()).unwrap();
    let s = &c["state_anchor"];
    let reference = SnapshotRef {
        block: BlockRef {
            chain_key: pool.chain_key.clone(),
            number_or_slot: s["block_number"].as_u64().unwrap(),
            hash: s["block_hash"].as_str().unwrap().into(),
            parent_hash: s["parent_hash"].as_str().unwrap().into(),
            timestamp_ms: s["timestamp"].as_u64().unwrap() * 1000,
        },
        canonical_epoch: 1,
        journal_seq: 1,
        schema_version: 1,
        adapter_set_hash: hash(&vec![&pool.adapter_version]).unwrap(),
        online_generation: 1,
        scope_hash: scope_hash(&pools).unwrap(),
        writer_epoch: 1,
        owner_service: "chain-parser-go".into(),
        owner_assignment_version: 1,
    };
    let context = Context {
        chain_key: pool.chain_key,
        caller: c["transaction"]["from"].as_str().unwrap().into(),
        recipient: c["recipient"].as_str().unwrap().into(),
        sell_token: c["token_in"].as_str().unwrap().into(),
        buy_token: c["token_out"].as_str().unwrap().into(),
        slippage_bps: 50,
        deadline_ms: reference.block.timestamp_ms + 60_000,
    };
    let identities = corpus()["reference"]["runtime_identities"]
        .as_array()
        .unwrap();
    let router = identities
        .iter()
        .find(|i| i["address"] == c["transaction"]["to"])
        .unwrap();
    let executor = ExecutorIdentity {
        target: c["transaction"]["to"].as_str().unwrap().into(),
        code_sha256: router["runtime_code_sha256"].as_str().unwrap().into(),
        factory: pool.v2.as_ref().unwrap().factory.clone(),
        factory_code_sha256: v2_swap::FACTORY_CODE_SHA256.into(),
        adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
    };
    (
        build(&context, &allocation, &reference, "v2-complete-test").unwrap(),
        pools,
        executor,
    )
}
fn recommit(p: &mut Plan) {
    *p = build(
        &p.context,
        &p.allocation,
        &p.snapshot,
        &p.economic_policy_hash,
    )
    .unwrap();
}

#[test]
fn complete_profile_binds_all_16_service_cases_with_exact_router02_calldata() {
    let mut count = 0;
    for c in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["service_eligible"] == true)
    {
        let (plan, pools, executor) = fixture(c);
        let allowed = [executor.target.clone()];
        let bound = bind_v2_execution(&plan, &pools, &executor, &allowed)
            .unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        validate_bound_execution(&bound, &pools, &allowed).unwrap();
        let data = hex::decode(&bound.transaction.data[2..]).unwrap();
        assert_eq!(&data[..4], &[0x38, 0xed, 0x17, 0x39]);
        assert_eq!(data.len(), 260);
        assert_eq!(
            &data[4..36],
            &plan.allocation.sell_amount.to_be_bytes().unwrap()
        );
        assert_eq!(&data[36..68], &plan.min_buy_amount.to_be_bytes().unwrap());
        assert_eq!(data[99], 160);
        assert_eq!(
            &data[112..132],
            &hex::decode(&plan.context.recipient[2..]).unwrap()
        );
        assert_eq!(data[195], 2);
        assert_ne!(
            bound.transaction.data,
            c["transaction"]["data"].as_str().unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn complete_factory_commitment_cannot_select_an_arbitrary_executor_factory_runtime() {
    let (p, pools, mut e) = fixture(&corpus()["cases"][0]);
    e.factory_code_sha256 = "ab".repeat(32);
    assert!(bind_v2_execution(&p, &pools, &e, &[e.target.clone()]).is_err());
}

#[test]
fn complete_binding_rebuild_rejects_state_calldata_scope_math_and_expiry() {
    let (plan, pools, e) = fixture(&corpus()["cases"][2]);
    let allowed = [e.target.clone()];
    let bound = bind_v2_execution(&plan, &pools, &e, &allowed).unwrap();
    let mut changed_pools = pools.clone();
    changed_pools[0]
        .v2
        .as_mut()
        .unwrap()
        .swap
        .as_mut()
        .unwrap()
        .raw_balance0 = "1".parse().unwrap();
    assert!(validate_bound_execution(&bound, &changed_pools, &allowed).is_err());
    for key in ["data", "pool_state_hash", "binding_hash"] {
        let mut b = bound.clone();
        match key {
            "data" => b.transaction.data = format!("{}00", b.transaction.data),
            "pool_state_hash" => b.pool_state_hash = "ab".repeat(32),
            _ => b.binding_hash = "ab".repeat(32),
        }
        assert!(
            validate_bound_execution(&b, &pools, &allowed).is_err(),
            "{key}"
        );
    }
    for key in [
        "math",
        "expiry",
        "scope",
        "adapter",
        "family",
        "missing_swap",
    ] {
        let mut p = plan.clone();
        let mut ps = pools.clone();
        match key {
            "math" => p.allocation.gross_buy_amount = "1".parse().unwrap(),
            "expiry" => p.context.deadline_ms = p.snapshot.block.timestamp_ms - 1,
            "scope" => p.snapshot.scope_hash = "ab".repeat(32),
            "adapter" => ps[0].adapter_version = "uniswap-v2/1".into(),
            "family" => ps[0].protocol = "sushiswap-v2".into(),
            _ => ps[0].v2.as_mut().unwrap().swap = None,
        }
        recommit(&mut p);
        assert!(bind_v2_execution(&p, &ps, &e, &allowed).is_err(), "{key}");
    }
}
