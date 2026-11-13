use serde_json::Value;
use xone_domain::{Pool, Snapshot};
use xone_execution_plan::Context;
use xone_execution_preparation::{prepare_solana, SolanaQuote, SolanaRequest, SolanaRpcConfig};
fn quote() -> SolanaQuote {
    let b: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-execution-bank.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let pool: Pool = serde_json::from_value(b["pool"].clone()).unwrap();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.reference.block.chain_key = pool.chain_key.clone();
    SolanaQuote {
        context: Context {
            chain_key: pool.chain_key.clone(),
            caller: b["caller"].as_str().unwrap().into(),
            recipient: b["recipient"].as_str().unwrap().into(),
            sell_token: pool.token0.clone(),
            buy_token: pool.token1.clone(),
            slippage_bps: 50,
            deadline_ms: u64::MAX,
        },
        pool,
        plan_hash: "f".repeat(64),
        quote_view_id: "test-view".into(),
        snapshot: snapshot.reference,
        zero: true,
        exact: true,
        amount: 1000,
        limit: 2000,
        expected_input: 699,
        expected_output: 1000,
    }
}
#[tokio::test]
async fn resolver_rejects_mismatched_quote_contract_before_any_rpc() {
    let config = SolanaRpcConfig {
        url: "not-an-rpc-url".into(),
        genesis_hash: "not-used".into(),
    };
    for mutation in 0..5 {
        let mut q = quote();
        match mutation {
            0 => q.context.buy_token = q.context.sell_token.clone(),
            1 => q.plan_hash = "invalid".into(),
            2 => q.snapshot.block.chain_key = "evm:1".into(),
            3 => q.amount = 999,
            4 => q.limit = 698,
            _ => unreachable!(),
        }
        assert_eq!(
            prepare_solana(&config, &q, SolanaRequest::default())
                .await
                .unwrap_err()
                .0,
            "PREPARATION_QUOTE_INVALID"
        );
    }
    let mut q = quote();
    q.context.deadline_ms = 0;
    assert_eq!(
        prepare_solana(&config, &q, SolanaRequest::default())
            .await
            .unwrap_err()
            .0,
        "QUOTE_EXPIRED"
    );
}
