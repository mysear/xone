#[path = "normal_bank.rs"]
mod normal_bank;

use xone_domain::{hash, scope_hash, BlockRef, Snapshot, SnapshotRef, SCHEMA_VERSION};
use xone_solana_ingest::rpc::decode_cpmm_at;

pub(crate) fn snapshot(timestamp_ms: u64) -> Snapshot {
    let (chain, accounts) = normal_bank::archive();
    let pool = decode_cpmm_at(&chain, &chain.pools[0], &accounts, timestamp_ms / 1000).unwrap();
    let adapter_set_hash = hash(&[&pool.adapter_version]).unwrap();
    let pools = vec![pool];
    Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: chain.key,
                number_or_slot: 100,
                hash: bs58::encode([100; 32]).into_string(),
                parent_hash: bs58::encode([99; 32]).into_string(),
                timestamp_ms,
            },
            canonical_epoch: 1,
            journal_seq: 1,
            schema_version: SCHEMA_VERSION,
            adapter_set_hash,
            online_generation: 1,
            scope_hash: scope_hash(&pools).unwrap(),
            writer_epoch: 1,
            owner_service: chain.owner_service,
            owner_assignment_version: chain.assignment_version,
        },
        pools,
        source: "OWNER_SCOPE_TEST".into(),
        history_gaps: vec![],
    }
}
