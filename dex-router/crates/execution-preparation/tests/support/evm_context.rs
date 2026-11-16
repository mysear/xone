use xone_domain::{hash, protocols::V2State, scope_hash, Amount, Pool, Snapshot};
use xone_execution_plan::*;
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};

pub const CALLER: &str = "0x90f8bf6a479f320ead074411a4b0e7944ea8c9c1";
const RECIPIENT: &str = "0x0000000000000000000000000000000000000022";
const TOKEN0: &str = "0x0000000000000000000000000000000000000033";
const TOKEN1: &str = "0x0000000000000000000000000000000000000044";
const PAIR: &str = "0x0000000000000000000000000000000000000055";
const FACTORY: &str = "0x0000000000000000000000000000000000000066";
pub const ROUTER: &str = "0x0000000000000000000000000000000000000077";

pub fn fixture(reverse: bool) -> (Plan, Vec<Pool>) {
    // Local in-memory quote state only; this fixture proves no EVM execution.
    let mut snapshot: Snapshot = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/fixtures/evm/baseline.json"
    )))
    .unwrap();
    snapshot.pools.truncate(1);
    let pool = &mut snapshot.pools[0];
    pool.pool_id = PAIR.into();
    pool.token0 = TOKEN0.into();
    pool.token1 = TOKEN1.into();
    pool.reserve0 = "1000000000000000000000000".parse().unwrap();
    pool.reserve1 = "2000000000000000000000000".parse().unwrap();
    pool.v2 = Some(V2State {
        swap: None,
        factory: FACTORY.into(),
        code_sha256: "ab".repeat(32),
    });
    snapshot.reference.scope_hash = scope_hash(&snapshot.pools).unwrap();
    snapshot.reference.adapter_set_hash = hash(&vec!["uniswap-v2/1"]).unwrap();
    let (sell, buy) = if reverse {
        (TOKEN1, TOKEN0)
    } else {
        (TOKEN0, TOKEN1)
    };
    let amount: Amount = "10000000000000000000000".parse().unwrap();
    let routes = vec![AllocatedRoute {
        path: PathQuote {
            amount_in: amount.clone(),
            edges: vec![Edge {
                pool_id: PAIR.into(),
                protocol: "uniswap-v2".into(),
                zero_for_one: !reverse,
                from_token: None,
                to: buy.into(),
            }],
            tokens: vec![sell.into(), buy.into()],
            amount_out: Amount::zero(),
        },
        amount_in: amount,
        amount_out: Amount::zero(),
    }];
    let allocation = evaluate_allocation(&snapshot.pools, &routes, &Amount::zero()).unwrap();
    let context = Context {
        chain_key: snapshot.reference.block.chain_key.clone(),
        caller: CALLER.into(),
        recipient: RECIPIENT.into(),
        sell_token: sell.into(),
        buy_token: buy.into(),
        slippage_bps: 50,
        deadline_ms: snapshot.reference.block.timestamp_ms + 60_000,
    };
    (
        build(&context, &allocation, &snapshot.reference, "policy").unwrap(),
        snapshot.pools,
    )
}

pub fn executor() -> ExecutorIdentity {
    ExecutorIdentity {
        target: ROUTER.into(),
        code_sha256: "cd".repeat(32),
        factory: FACTORY.into(),
        factory_code_sha256: "ef".repeat(32),
        adapter_version: "uniswap-v2-router02/1.1.0-beta.0".into(),
    }
}
