use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use xone_domain::{hash, scope_hash, v2_swap, Amount, BlockRef, Pool, SnapshotRef};
use xone_execution_plan::{bind_v2_execution, build, BoundEvmExecution, Context, ExecutorIdentity};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};
use xone_simulation::evm_fixture::*;
pub fn corpus() -> &'static Value {
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
pub fn budget() -> SimulationBudget {
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
pub fn bound(
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
pub fn output_bound(
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
