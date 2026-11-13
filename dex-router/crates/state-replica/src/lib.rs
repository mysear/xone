mod protocol_view;
pub use protocol_view::ProtocolQuoteView;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};
use xone_domain::{err, hash, Pool, Record, Result, Snapshot, SnapshotRef};
use xone_routing::Graph;
#[derive(Clone, Debug)]
pub struct QuoteView {
    pub reference: SnapshotRef,
    pub pools: Vec<Pool>,
    pub graph: Arc<Graph>,
    pub graph_version: u64,
    pub view_id: String,
    pub source: String,
    pub protocol_views: BTreeMap<String, ProtocolQuoteView>,
    valid: Arc<AtomicBool>,
}
impl QuoteView {
    pub fn is_valid(&self) -> bool {
        self.valid.load(Ordering::Acquire)
    }
    pub fn protocol_bindings<'a>(
        &self,
        pool_ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<BTreeMap<String, String>> {
        if !self.is_valid() {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
        let mut bindings = BTreeMap::new();
        for id in pool_ids {
            let p = self
                .pools
                .iter()
                .find(|p| p.pool_id == id)
                .ok_or_else(|| err("MISSING_POOL"))?;
            let key = format!("{}@{}", p.protocol, p.adapter_version);
            let view = self
                .protocol_views
                .get(&key)
                .ok_or_else(|| err("PROTOCOL_VIEW_MISSING"))?;
            if !view.is_valid()
                || !view
                    .available_pool_ids
                    .iter()
                    .any(|available| available == id)
            {
                return Err(err("QUOTE_STATE_CHANGED"));
            }
            bindings.insert(key, view.view_id.clone());
        }
        Ok(bindings)
    }
}
#[derive(Default)]
pub struct Replica {
    owners: BTreeMap<String, (String, u64)>,
    views: RwLock<BTreeMap<String, Arc<QuoteView>>>,
    reorgs: RwLock<BTreeMap<String, Record>>,
}
impl Replica {
    pub fn new(owners: Vec<(String, String, u64)>) -> Self {
        Self {
            owners: owners.into_iter().map(|(k, o, a)| (k, (o, a))).collect(),
            views: RwLock::new(BTreeMap::new()),
            reorgs: RwLock::new(BTreeMap::new()),
        }
    }
    fn validate(&self, s: &Snapshot) -> Result<()> {
        let (o, a) = self
            .owners
            .get(&s.reference.block.chain_key)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
        s.validate(o, *a)
    }
    /// Metadata only: the latest committed bank may be revoked or have no
    /// searchable edges. Quote/execution consumers must continue to use view().
    pub fn diagnostic_view(&self, chain: &str) -> Result<Arc<QuoteView>> {
        self.views
            .read()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .get(chain)
            .cloned()
            .ok_or_else(|| err("CHAIN_NOT_READY"))
    }
    pub fn view(&self, chain: &str) -> Result<Arc<QuoteView>> {
        let views = self.views.read().map_err(|_| err("STATE_LOCK_POISONED"))?;
        let v = views.get(chain).ok_or_else(|| err("CHAIN_NOT_READY"))?;
        if !v.is_valid() || v.graph.adjacency.is_empty() {
            return Err(err("CHAIN_NOT_READY"));
        }
        Ok(v.clone())
    }
    /// Last completely applied journal reference, including a non-ready barrier.
    /// This is a replay cursor, never proof that a quote scope is ready.
    pub fn last_reference(&self, chain: &str) -> Result<Option<SnapshotRef>> {
        let views = self.views.read().map_err(|_| err("STATE_LOCK_POISONED"))?;
        let reorgs = self.reorgs.read().map_err(|_| err("STATE_LOCK_POISONED"))?;
        Ok(reorgs
            .get(chain)
            .map(|r| r.reference.clone())
            .or_else(|| views.get(chain).map(|v| v.reference.clone())))
    }
    pub fn reorg_pending(&self, chain: &str) -> Result<bool> {
        Ok(self
            .reorgs
            .read()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .contains_key(chain))
    }
    pub fn chains(&self) -> Vec<String> {
        self.owners.keys().cloned().collect()
    }
    pub fn suspend(&self, chain: &str) {
        if let Ok(views) = self.views.read() {
            if let Some(v) = views.get(chain) {
                v.valid.store(false, Ordering::Release);
            }
        }
    }
    pub fn install_baseline(&self, s: Snapshot) -> Result<()> {
        self.install(s, false)
    }
    /// A complete owner-issued checkpoint may restore a local replica within the same generation.
    /// This does not allocate a generation; the caller must bind the snapshot to its committed cursor.
    pub fn restore_owner_checkpoint(&self, s: Snapshot) -> Result<()> {
        self.install(s, true)
    }
    fn install(&self, s: Snapshot, checkpoint: bool) -> Result<()> {
        self.validate(&s)?;
        let mut views = self.views.write().map_err(|_| err("STATE_LOCK_POISONED"))?;
        let mut reorgs = self
            .reorgs
            .write()
            .map_err(|_| err("STATE_LOCK_POISONED"))?;
        if let Some(begin) = reorgs.get(&s.reference.block.chain_key) {
            if s.reference.online_generation <= begin.reference.online_generation
                || s.reference.journal_seq <= begin.reference.journal_seq
                || s.reference.canonical_epoch < begin.reference.canonical_epoch
                || s.reference.writer_epoch < begin.reference.writer_epoch
            {
                return Err(err("REORG_IN_PROGRESS"));
            }
        }
        let previous = views.get(&s.reference.block.chain_key);
        if let Some(old) = previous {
            if old.reference == s.reference && old.pools != s.pools {
                return Err(err("SNAPSHOT_CONTENT_CONFLICT"));
            }
            if old.reference == s.reference
                && old.pools == s.pools
                && (!checkpoint || old.is_valid())
            {
                return if old.is_valid() {
                    Ok(())
                } else {
                    Err(err("REANCHOR_REQUIRED"))
                };
            }
            if (!checkpoint && s.reference.online_generation <= old.reference.online_generation
                || checkpoint && s.reference.online_generation < old.reference.online_generation)
                || s.reference.canonical_epoch < old.reference.canonical_epoch
                || s.reference.writer_epoch < old.reference.writer_epoch
                || (!checkpoint && s.reference.journal_seq <= old.reference.journal_seq
                    || checkpoint && s.reference.journal_seq < old.reference.journal_seq)
            {
                return Err(err("STALE_BASELINE"));
            }
        }
        let graph = Arc::new(Graph::build(&s.pools)?);
        if graph.adjacency.is_empty() {
            if let Some(old) = previous {
                old.valid.store(false, Ordering::Release);
            }
            return Err(err("NO_QUOTE_CAPABLE_SCOPE"));
        }
        let graph_version = previous.map_or(1, |old| old.graph_version + 1);
        let view_id = hash(&s.reference)?;
        let valid = Arc::new(AtomicBool::new(true));
        let protocol_views = protocol_view::partition(
            &s.pools,
            &graph,
            &s.reference,
            graph_version,
            &view_id,
            valid.clone(),
        )?;
        let view = Arc::new(QuoteView {
            protocol_views,
            reference: s.reference.clone(),
            pools: s.pools,
            graph,
            graph_version,
            view_id,
            source: s.source,
            valid,
        });
        if let Some(old) = previous {
            old.valid.store(false, Ordering::Release);
        }
        reorgs.remove(&s.reference.block.chain_key);
        views.insert(s.reference.block.chain_key.clone(), view);
        Ok(())
    }
    pub fn apply(&self, r: Record) -> Result<()> {
        if ![
            "BLOCK_COMMIT",
            "REORG_COMMIT",
            "REORG_BEGIN",
            "ONLINE_SCOPE_REVOKED",
        ]
        .contains(&r.kind.as_str())
        {
            return Err(err("RECORD_KIND_NOT_ONLINE"));
        }
        r.validate_payload()?;
        let mut views = self.views.write().map_err(|_| err("STATE_LOCK_POISONED"))?;
        let old = views
            .get(&r.reference.block.chain_key)
            .ok_or_else(|| err("BASELINE_REQUIRED"))?;
        let mut reorgs = self
            .reorgs
            .write()
            .map_err(|_| err("STATE_LOCK_POISONED"))?;
        let snapshot = Snapshot {
            reference: r.reference.clone(),
            pools: r.pools.clone(),
            source: old.source.clone(),
            history_gaps: Vec::new(),
        };
        self.validate(&snapshot)?;
        if r.reference.writer_epoch < old.reference.writer_epoch {
            return Err(err("WRITER_FENCED"));
        }
        if let Some(begin) = reorgs.get(&r.reference.block.chain_key) {
            if r.kind == begin.kind && r.reference == begin.reference && r.pools == begin.pools {
                return Ok(());
            }
            if begin.kind == "ONLINE_SCOPE_REVOKED" {
                if r.kind != "REORG_BEGIN"
                    || r.reference.journal_seq
                        != begin
                            .reference
                            .journal_seq
                            .checked_add(1)
                            .ok_or_else(|| err("SEQUENCE_OVERFLOW"))?
                    || r.reference.canonical_epoch
                        != begin
                            .reference
                            .canonical_epoch
                            .checked_add(1)
                            .ok_or_else(|| err("EPOCH_OVERFLOW"))?
                    || r.reference.block != begin.reference.block
                    || r.pools != begin.pools
                    || r.reference.writer_epoch != begin.reference.writer_epoch
                    || r.reference.online_generation != begin.reference.online_generation
                    || r.reference.scope_hash != begin.reference.scope_hash
                    || r.reference.adapter_set_hash != begin.reference.adapter_set_hash
                {
                    return Err(err("REORG_BEGIN_MISMATCH"));
                }
                reorgs.insert(r.reference.block.chain_key.clone(), r);
                return Ok(());
            }
            if r.kind != "REORG_COMMIT"
                || r.reference.journal_seq != begin.reference.journal_seq + 1
                || r.reference.canonical_epoch != begin.reference.canonical_epoch
                || r.reference.writer_epoch != begin.reference.writer_epoch
                || r.reference.online_generation != begin.reference.online_generation
                || r.reference.scope_hash != begin.reference.scope_hash
                || r.reference.adapter_set_hash != begin.reference.adapter_set_hash
            {
                return Err(err("REORG_COMMIT_MISMATCH"));
            }
        } else {
            if r.reference == old.reference
                && r.pools == old.pools
                && (r.kind == "BLOCK_COMMIT" || r.kind == "REORG_COMMIT")
            {
                return if old.is_valid() {
                    Ok(())
                } else {
                    Err(err("REANCHOR_REQUIRED"))
                };
            }
            if r.reference.journal_seq != old.reference.journal_seq + 1 {
                return Err(err("JOURNAL_SEQUENCE_GAP"));
            }
            if r.reference.online_generation != old.reference.online_generation {
                return Err(err("GENERATION_MISMATCH"));
            }
            if r.kind == "REORG_COMMIT" {
                return Err(err("REORG_BEGIN_REQUIRED"));
            }
            if r.kind == "REORG_BEGIN" {
                if r.reference.canonical_epoch
                    != old
                        .reference
                        .canonical_epoch
                        .checked_add(1)
                        .ok_or_else(|| err("EPOCH_OVERFLOW"))?
                    || r.reference.block != old.reference.block
                    || r.pools != old.pools
                    || r.reference.writer_epoch != old.reference.writer_epoch
                    || r.reference.adapter_set_hash != old.reference.adapter_set_hash
                    || r.reference.scope_hash != old.reference.scope_hash
                {
                    return Err(err("REORG_BEGIN_MISMATCH"));
                }
                old.valid.store(false, Ordering::Release);
                reorgs.insert(r.reference.block.chain_key.clone(), r);
                return Ok(());
            }
            if r.kind == "ONLINE_SCOPE_REVOKED" {
                if r.reference.canonical_epoch != old.reference.canonical_epoch
                    || r.reference.writer_epoch != old.reference.writer_epoch
                    || r.reference.block != old.reference.block
                    || r.pools != old.pools
                    || r.reference.scope_hash != old.reference.scope_hash
                    || r.reference.adapter_set_hash != old.reference.adapter_set_hash
                {
                    return Err(err("SCOPE_REVOCATION_MISMATCH"));
                }
                old.valid.store(false, Ordering::Release);
                reorgs.insert(r.reference.block.chain_key.clone(), r);
                return Ok(());
            }
            if r.reference.canonical_epoch != old.reference.canonical_epoch
                || r.reference.block.parent_hash != old.reference.block.hash
                || r.reference.block.number_or_slot != old.reference.block.number_or_slot + 1
                || r.reference.scope_hash != old.reference.scope_hash
                || r.reference.adapter_set_hash != old.reference.adapter_set_hash
            {
                return Err(err("BLOCK_CONTINUITY"));
            }
        }
        fn topology(pools: &[Pool]) -> Vec<(&str, Vec<&str>, &str, &str)> {
            pools
                .iter()
                .map(|p| {
                    (
                        p.pool_id.as_str(),
                        p.multi_asset
                            .as_ref()
                            .map(|s| {
                                s.assets
                                    .iter()
                                    .map(|a| a.token.as_str())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_else(|| vec![p.token0.as_str(), p.token1.as_str()]),
                        p.protocol.as_str(),
                        p.token_behavior.as_str(),
                    )
                })
                .collect()
        }
        // Candidate admission depends on the complete bank (for example,
        // shared Vault registrations), not just token endpoints/protocol labels.
        let admission_changed =
            xone_domain::unavailable_pool_reasons(r.pools.iter())? != old.graph.excluded_pools;
        let changed = topology(&old.pools) != topology(&r.pools) || admission_changed;
        let graph = if changed {
            Arc::new(Graph::build(&r.pools)?)
        } else {
            old.graph.clone()
        };
        let is_reorg = r.reference.canonical_epoch != old.reference.canonical_epoch;
        if !is_reorg && !old.is_valid() {
            return Err(err("REANCHOR_REQUIRED"));
        }
        let quote_capability_changed = graph.adjacency.is_empty() != old.graph.adjacency.is_empty();
        let invalidate_old = is_reorg || admission_changed || quote_capability_changed;
        let valid = if invalidate_old {
            // A complete canonical unit still advances the reader when no
            // quote edges are admitted. view() keeps that bank non-ready;
            // the next qualified unit may restore routes without rewinding.
            Arc::new(AtomicBool::new(true))
        } else {
            old.valid.clone()
        };
        let view_id = hash(&r.reference)?;
        let graph_version = old.graph_version + u64::from(changed);
        let protocol_views = protocol_view::partition(
            &r.pools,
            &graph,
            &r.reference,
            graph_version,
            &view_id,
            valid.clone(),
        )?;
        if invalidate_old {
            old.valid.store(false, Ordering::Release);
        }
        let next = Arc::new(QuoteView {
            protocol_views,
            reference: r.reference.clone(),
            pools: r.pools,
            graph,
            graph_version,
            view_id,
            source: old.source.clone(),
            valid,
        });
        reorgs.remove(&r.reference.block.chain_key);
        views.insert(r.reference.block.chain_key, next);
        Ok(())
    }
}
