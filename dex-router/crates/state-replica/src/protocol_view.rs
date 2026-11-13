//! Immutable per-protocol projection of one committed parent QuoteView.
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use xone_domain::{hash, Pool, Result, SnapshotRef};
use xone_protocol_math::capabilities::{describe, ProtocolCapabilities};
use xone_routing::Graph;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolQuoteView {
    pub protocol: String,
    pub adapter_version: String,
    pub parent_view_id: String,
    pub view_id: String,
    pub state_hash: String,
    pub graph_version: u64,
    pub snapshot_ref: SnapshotRef,
    pub pool_ids: Vec<String>,
    pub available_pool_ids: Vec<String>,
    pub excluded_pools: BTreeMap<String, String>,
    pub capabilities: ProtocolCapabilities,
    #[serde(skip)]
    valid: Arc<AtomicBool>,
}
impl ProtocolQuoteView {
    pub fn is_valid(&self) -> bool {
        self.valid.load(Ordering::Acquire)
    }
}
pub(super) fn partition(
    pools: &[Pool],
    graph: &Graph,
    reference: &SnapshotRef,
    graph_version: u64,
    parent_view_id: &str,
    valid: Arc<AtomicBool>,
) -> Result<BTreeMap<String, ProtocolQuoteView>> {
    let mut groups: BTreeMap<(&str, &str), Vec<&Pool>> = BTreeMap::new();
    for pool in pools {
        groups
            .entry((&pool.protocol, &pool.adapter_version))
            .or_default()
            .push(pool);
    }
    let available: BTreeSet<&str> = graph
        .adjacency
        .values()
        .flatten()
        .map(|e| e.pool_id.as_str())
        .collect();
    let mut output = BTreeMap::new();
    for ((protocol, adapter), mut selected) in groups {
        selected.sort_by(|a, b| a.pool_id.cmp(&b.pool_id));
        let capabilities = describe(selected[0])?;
        let state_hash = hash(&selected)?;
        let view_id = hash(
            &serde_json::json!({"schemaVersion":1,"parentViewId":parent_view_id,"reference":reference,"protocol":protocol,"adapterVersion":adapter,"stateHash":state_hash,"graphVersion":graph_version,"capabilities":capabilities}),
        )?;
        let pool_ids = selected.iter().map(|p| p.pool_id.clone()).collect();
        let available_pool_ids = selected
            .iter()
            .filter(|p| available.contains(p.pool_id.as_str()))
            .map(|p| p.pool_id.clone())
            .collect();
        let excluded_pools = selected
            .iter()
            .filter_map(|p| {
                graph
                    .excluded_pools
                    .get(&p.pool_id)
                    .cloned()
                    .or_else(|| {
                        (!available.contains(p.pool_id.as_str()))
                            .then(|| "NO_SEARCHABLE_EDGE".into())
                    })
                    .map(|why| (p.pool_id.clone(), why))
            })
            .collect();
        output.insert(
            format!("{protocol}@{adapter}"),
            ProtocolQuoteView {
                protocol: protocol.into(),
                adapter_version: adapter.into(),
                parent_view_id: parent_view_id.into(),
                view_id,
                state_hash,
                graph_version,
                snapshot_ref: reference.clone(),
                pool_ids,
                available_pool_ids,
                excluded_pools,
                capabilities,
                valid: valid.clone(),
            },
        );
    }
    Ok(output)
}
