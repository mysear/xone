use serde_json::json;
use xone_domain::{hash, scope_hash, BlockRef, Pool, Snapshot, SnapshotRef, SCHEMA_VERSION};
use xone_observability::now_ms;

pub const CHAIN: &str = "evm:31337";

pub fn snapshot(sequence: u64) -> Snapshot {
    let pool: Pool = serde_json::from_value(json!({
        "chain_key": CHAIN,
        "pool_id": "0x0000000000000000000000000000000000000003",
        "protocol": "uniswap-v2",
        "token0": "0x0000000000000000000000000000000000000001",
        "token1": "0x0000000000000000000000000000000000000002",
        "reserve0": "1000000",
        "reserve1": "2000000",
        "fee_numerator": 3,
        "fee_denominator": 1000,
        "adapter_version": "uniswap-v2/1",
        "token_behavior": "standard"
    }))
    .unwrap();
    let adapter_set_hash = hash(&[&pool.adapter_version]).unwrap();
    let pools = vec![pool];
    Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: CHAIN.into(),
                number_or_slot: sequence * 100,
                hash: format!("0x{sequence:064x}"),
                parent_hash: format!("0x{:064x}", sequence - 1),
                timestamp_ms: now_ms(),
            },
            canonical_epoch: 1,
            journal_seq: sequence,
            schema_version: SCHEMA_VERSION,
            adapter_set_hash,
            online_generation: sequence,
            scope_hash: scope_hash(&pools).unwrap(),
            writer_epoch: 1,
            owner_service: "chain-parser-go".into(),
            owner_assignment_version: 1,
        },
        pools,
        source: "retention-test-owner".into(),
        history_gaps: Vec::new(),
    }
}
