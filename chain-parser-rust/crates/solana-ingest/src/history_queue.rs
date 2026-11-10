//! Owner-local history capability. All keys written here are history-only.
use crate::store::Store;
use rocksdb::{WriteBatch, WriteOptions};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};
use xone_domain::{err, hash, Gap, Result, Snapshot};
const KEY: &[u8] = b"history/queue-v1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_jobs: usize,
    pub max_queue_bytes: usize,
    pub max_records: u64,
    pub max_history_bytes: u64,
    pub page_slots: u64,
    pub max_response_bytes: usize,
    pub max_batch_bytes: usize,
    pub lease_ms: u64,
    pub retry_min_ms: u64,
    pub retry_max_ms: u64,
    pub poll_ms: u64,
}
impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_jobs: 128,
            max_queue_bytes: 2 << 20,
            max_records: 4096,
            max_history_bytes: 64 << 20,
            page_slots: 128,
            max_response_bytes: 4 << 20,
            max_batch_bytes: 16 << 20,
            lease_ms: 30000,
            retry_min_ms: 250,
            retry_max_ms: 30000,
            poll_ms: 50,
        }
    }
}
impl HistoryLimits {
    pub fn validate(&self) -> Result<()> {
        if self.max_jobs == 0
            || self.max_jobs > 4096
            || self.max_queue_bytes < 4096
            || self.max_queue_bytes > 16 << 20
            || self.max_records == 0
            || self.max_history_bytes == 0
            || self.page_slots == 0
            || self.page_slots > 128
            || self.max_response_bytes == 0
            || self.max_response_bytes > 4 << 20
            || self.max_batch_bytes < self.max_response_bytes
            || self.max_batch_bytes > 16 << 20
            || self.lease_ms == 0
            || self.lease_ms > 300000
            || self.retry_min_ms == 0
            || self.retry_max_ms < self.retry_min_ms
            || self.poll_ms == 0
        {
            return Err(err("HISTORY_LIMITS"));
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct HistoryRegistration {
    pub chain_key: String,
    pub genesis_hash: String,
    pub source_fingerprint: String,
    pub enabled: bool,
    pub limits: HistoryLimits,
}
impl HistoryRegistration {
    pub fn new(
        chain: &str,
        genesis: &str,
        source: &str,
        enabled: bool,
        limits: HistoryLimits,
    ) -> Result<Self> {
        limits.validate()?;
        if chain.is_empty() || genesis.is_empty() {
            return Err(err("HISTORY_IDENTITY"));
        }
        Ok(Self {
            chain_key: chain.into(),
            genesis_hash: genesis.into(),
            source_fingerprint: hash(&("public-solana-rpc-history-v1", source))?,
            enabled,
            limits,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryDataset {
    pub chain_key: String,
    pub genesis_hash: String,
    pub scope_hash: String,
    pub decoder_hash: String,
    pub source_fingerprint: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryHead {
    pub slot: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum HistoryPhase {
    Pending,
    Running,
    Backoff,
    Complete,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryJob {
    pub id: String,
    pub from: u64,
    pub to: u64,
    pub next: Option<u64>,
    pub phase: HistoryPhase,
    pub attempt: u64,
    pub lease_until_ms: u64,
    pub due_ms: u64,
    pub last_error: String,
    pub claim_head: Option<HistoryHead>,
    pub bound_head: HistoryHead,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryClaim {
    pub id: String,
    pub epoch: String,
    pub session: u64,
    pub attempt: u64,
    pub lease_until_ms: u64,
    pub dataset: HistoryDataset,
    pub canonical_revision: u64,
    pub from: u64,
    pub to: u64,
    pub head: HistoryHead,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcReceipt {
    pub method: String,
    pub params: serde_json::Value,
    pub response: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryBlock {
    pub slot: u64,
    pub hash: String,
    pub parent_slot: u64,
    pub parent_hash: String,
    pub transactions: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryPage {
    pub dataset: HistoryDataset,
    pub canonical_revision: u64,
    pub history_epoch: String,
    pub verification: String,
    pub historical_accounts_complete: bool,
    pub skipped: Vec<Gap>,
    pub genesis_hash: String,
    pub source_fingerprint: String,
    pub from: u64,
    pub to: u64,
    pub head: HistoryHead,
    pub produced: Vec<HistoryBlock>,
    pub raw: Vec<RpcReceipt>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct State {
    version: u32,
    dataset: HistoryDataset,
    canonical_revision: u64,
    owner_canonical_epoch: u64,
    online_source_epoch: String,
    epoch: String,
    session: u64,
    head: HistoryHead,
    generation: u64,
    paused: bool,
    revoked: bool,
    enabled: bool,
    jobs: BTreeMap<String, HistoryJob>,
    covered: Vec<Gap>,
    tail_from: u64,
    gap_sync_pending: bool,
    records: u64,
    bytes: u64,
    last_error: String,
    source_status: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct HistoryStatus {
    pub dataset: Option<HistoryDataset>,
    pub canonical_revision: u64,
    pub owner_canonical_epoch: u64,
    pub online_generation: u64,
    pub head: Option<HistoryHead>,
    pub revoked: bool,
    pub epoch: String,
    pub jobs: Vec<HistoryJob>,
    pub covered_ranges: Vec<Gap>,
    pub missing_ranges: Vec<Gap>,
    pub coverage_policy: String,
    pub next_tail_to_enqueue: u64,
    pub gap_sync_pending: bool,
    pub records: u64,
    pub bytes: u64,
    pub last_error: String,
    pub source_status: String,
    pub enabled: bool,
    pub paused: bool,
    pub history_independent: bool,
    pub historical_accounts_complete: bool,
    pub limits: HistoryLimits,
}
fn read(db: &Store) -> Result<Option<State>> {
    db.db
        .get(KEY)
        .map_err(|_| err("HISTORY_READ"))?
        .map(|bytes| {
            let state: State =
                serde_json::from_slice(&bytes).map_err(|_| err("HISTORY_RECONCILE_REQUIRED"))?;
            if state.version != 1 {
                return Err(err("HISTORY_RECONCILE_REQUIRED"));
            }
            Ok(state)
        })
        .transpose()
}

fn encode(s: &State, limits: &HistoryLimits) -> Result<Vec<u8>> {
    let b = serde_json::to_vec(s).map_err(|_| err("HISTORY_ENCODING"))?;
    if s.jobs.len() > limits.max_jobs || b.len() > limits.max_queue_bytes {
        return Err(err("HISTORY_QUEUE_BUDGET"));
    }
    Ok(b)
}
fn write(db: &mut Store, batch: WriteBatch) -> Result<()> {
    if db.is_uncertain() || db.history_uncertain {
        return Err(err("HISTORY_REOPEN_REQUIRED"));
    }
    let mut options = WriteOptions::default();
    options.set_sync(true);
    if db.db.write_opt(batch, &options).is_err() {
        db.history_uncertain = true;
        return Err(err("HISTORY_IO_OUTCOME_UNKNOWN"));
    }
    Ok(())
}
fn save(db: &mut Store, s: &State, limits: &HistoryLimits) -> Result<()> {
    let mut batch = WriteBatch::default();
    batch.put(KEY, encode(s, limits)?);
    write(db, batch)
}
fn merge_covered(ranges: &mut Vec<Gap>, from: u64, to: u64) {
    ranges.push(Gap {
        from,
        to,
        reason: "PUBLIC_FINALIZED_TRANSACTIONS_META_RAW_V1".into(),
    });
    ranges.sort_by_key(|r| r.from);
    let mut merged: Vec<Gap> = Vec::new();
    for range in ranges.drain(..) {
        if let Some(last) = merged.last_mut() {
            if range.from <= last.to.saturating_add(1) {
                last.to = last.to.max(range.to);
                continue;
            }
        }
        merged.push(range);
    }
    *ranges = merged;
}
fn prune_completed(s: &mut State, limits: &HistoryLimits) {
    while encode(s, limits).is_err() {
        let Some(id) = s
            .jobs
            .values()
            .filter(|j| j.phase == HistoryPhase::Complete)
            .min_by_key(|j| j.from)
            .map(|j| j.id.clone())
        else {
            break;
        };
        s.jobs.remove(&id);
    }
}
fn add_ranges(s: &mut State, gaps: &[Gap], limits: &HistoryLimits) -> Result<()> {
    if gaps.len() as u64 > limits.max_records.saturating_add(limits.max_jobs as u64) {
        return Err(err("HISTORY_QUEUE_BUDGET"));
    }
    let mut ranges = Vec::new();
    for gap in gaps {
        if gap.from > gap.to || gap.to >= s.head.slot {
            return Err(err("HISTORY_FUTURE_OR_RANGE"));
        }
        ranges.push((gap.from, gap.to));
    }
    ranges.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (from, to) in ranges {
        if let Some(last) = merged.last_mut() {
            if from <= last.1.saturating_add(1) {
                last.1 = last.1.max(to);
                continue;
            }
        }
        merged.push((from, to));
    }
    for (from, to) in merged {
        let mut todo = vec![(from, to)];
        let existing = s
            .jobs
            .values()
            .map(|j| (j.from, j.to))
            .chain(s.covered.iter().map(|r| (r.from, r.to)))
            .collect::<Vec<_>>();
        for (old_from, old_to) in existing {
            let mut next = Vec::new();
            for (a, b) in todo {
                if old_to < a || old_from > b {
                    next.push((a, b));
                    continue;
                }
                if a < old_from {
                    next.push((a, old_from - 1));
                }
                if old_to < b {
                    next.push((old_to + 1, b));
                }
            }
            todo = next;
            if todo.len() > limits.max_jobs {
                return Err(err("HISTORY_QUEUE_BUDGET"));
            }
        }
        for (from, to) in todo {
            while s.jobs.len() >= limits.max_jobs {
                let Some(id) = s
                    .jobs
                    .values()
                    .filter(|j| j.phase == HistoryPhase::Complete)
                    .min_by_key(|j| j.from)
                    .map(|j| j.id.clone())
                else {
                    s.tail_from = s.tail_from.min(from);
                    return Err(err("HISTORY_QUEUE_BUDGET"));
                };
                s.jobs.remove(&id);
            }
            let id = hash(&(&s.dataset, s.canonical_revision, from, to))?;
            s.jobs.insert(
                id.clone(),
                HistoryJob {
                    id,
                    from,
                    to,
                    next: Some(from),
                    phase: HistoryPhase::Pending,
                    attempt: 0,
                    lease_until_ms: 0,
                    due_ms: 0,
                    last_error: String::new(),
                    claim_head: None,
                    bound_head: s.head.clone(),
                },
            );
        }
    }
    Ok(())
}
/// Called in the existing online owner WriteBatch. History failures are isolated
/// status; a fresh epoch still invalidates claims when the gap budget is full.
pub(crate) fn stage_snapshot(db: &Store, snapshot: &Snapshot, kind: &str, batch: &mut WriteBatch) {
    let Some(reg) = db.history_registration.as_ref() else {
        return;
    };
    let previous = match read(db) {
        Ok(state) => state,
        Err(_) => {
            batch.put(KEY, b"null");
            return;
        }
    };
    let dataset = HistoryDataset {
        chain_key: reg.chain_key.clone(),
        genesis_hash: reg.genesis_hash.clone(),
        scope_hash: hash(&(
            &snapshot.reference.scope_hash,
            snapshot
                .pools
                .iter()
                .map(|p| &p.pool_id)
                .collect::<Vec<_>>(),
        ))
        .unwrap_or_default(),
        decoder_hash: hash(&(
            "solana-history-transactions-json-v1",
            &snapshot.reference.adapter_set_hash,
            snapshot.reference.schema_version,
        ))
        .unwrap_or_default(),
        source_fingerprint: reg.source_fingerprint.clone(),
    };
    let head = HistoryHead {
        slot: snapshot.reference.block.number_or_slot,
        hash: snapshot.reference.block.hash.clone(),
        parent_hash: snapshot.reference.block.parent_hash.clone(),
        timestamp_ms: snapshot.reference.block.timestamp_ms,
    };
    let revoked = kind == "ONLINE_SCOPE_REVOKED";
    let same = previous.as_ref().is_some_and(|s| {
        s.dataset == dataset
            && s.owner_canonical_epoch == snapshot.reference.canonical_epoch
            && head.slot >= s.head.slot
            && (head.slot != s.head.slot || head.hash == s.head.hash)
    });
    let source: Option<serde_json::Value> = serde_json::from_str(&snapshot.source).ok();
    let online_source_epoch = hash(&(
        source.as_ref().and_then(|v| v.get("session_id")),
        source.as_ref().and_then(|v| v.get("wal_generation")),
    ))
    .unwrap_or_default();
    let baseline_replaced = previous.as_ref().is_some_and(|old| {
        snapshot
            .history_gaps
            .iter()
            .any(|g| g.reason.contains("HEAD_FIRST") && g.to >= old.head.slot && g.from < head.slot)
    });
    let canonical_revision = if same {
        previous.as_ref().unwrap().canonical_revision
    } else {
        match previous.as_ref() {
            Some(s) => match s.canonical_revision.checked_add(1) {
                Some(revision) => revision,
                None => {
                    batch.put(KEY, b"null");
                    return;
                }
            },
            None => snapshot.reference.canonical_epoch,
        }
    };
    let epoch = if same
        && previous.as_ref().is_some_and(|s| {
            s.session == db.scope_session
                && s.revoked == revoked
                && s.online_source_epoch == online_source_epoch
        })
        && !baseline_replaced
    {
        previous.as_ref().unwrap().epoch.clone()
    } else {
        hash(&(
            &dataset,
            snapshot.reference.canonical_epoch,
            snapshot.reference.online_generation,
            snapshot.reference.journal_seq,
            db.scope_session,
            &online_source_epoch,
            revoked,
        ))
        .unwrap_or_default()
    };
    let epoch_changed = previous.as_ref().is_none_or(|s| s.epoch != epoch);
    let mut state = State {
        version: 1,
        dataset,
        canonical_revision,
        owner_canonical_epoch: snapshot.reference.canonical_epoch,
        online_source_epoch,
        epoch,
        session: db.scope_session,
        head,
        generation: snapshot.reference.online_generation,
        paused: previous.as_ref().is_some_and(|s| s.paused),
        revoked,
        enabled: reg.enabled,
        jobs: if same {
            previous.as_ref().unwrap().jobs.clone()
        } else {
            BTreeMap::new()
        },
        covered: if same {
            previous.as_ref().unwrap().covered.clone()
        } else {
            Vec::new()
        },
        tail_from: previous
            .as_ref()
            .map_or(snapshot.reference.block.number_or_slot, |s| {
                s.tail_from.min(snapshot.reference.block.number_or_slot)
            }),
        gap_sync_pending: false,
        records: previous.as_ref().map_or(0, |s| s.records),
        bytes: previous.as_ref().map_or(0, |s| s.bytes),
        last_error: if same {
            previous.as_ref().unwrap().last_error.clone()
        } else {
            String::new()
        },
        source_status: previous
            .as_ref()
            .map_or_else(|| "UNKNOWN".into(), |s| s.source_status.clone()),
    };
    if epoch_changed {
        for j in state.jobs.values_mut() {
            if j.phase == HistoryPhase::Running {
                j.phase = HistoryPhase::Pending;
                j.lease_until_ms = 0;
                j.claim_head = None;
            }
        }
    }
    if !revoked {
        let mut ranges = snapshot.history_gaps.clone();
        if let Some(old) = &previous {
            if state.head.slot > state.tail_from {
                ranges.push(Gap {
                    from: state.tail_from,
                    to: state.head.slot - 1,
                    reason: "FINALIZED_HISTORY_TAIL".into(),
                });
            }
            if !same {
                for range in old
                    .covered
                    .iter()
                    .map(|r| (r.from, r.to))
                    .chain(old.jobs.values().map(|j| (j.from, j.to)))
                {
                    if range.0 < state.head.slot {
                        ranges.push(Gap {
                            from: range.0,
                            to: range.1.min(state.head.slot - 1),
                            reason: "CANONICAL_OR_DATASET_REPAIR".into(),
                        });
                    }
                }
            }
        }
        if let Err(e) = add_ranges(&mut state, &ranges, &reg.limits) {
            state.gap_sync_pending = true;
            if e.0 == "HISTORY_QUEUE_BUDGET" {
                if let Some(from) = ranges
                    .iter()
                    .filter(|r| r.from <= r.to && r.to < state.head.slot)
                    .map(|r| r.from)
                    .min()
                {
                    state.tail_from = state.tail_from.min(from);
                }
            }
            state.last_error = e.0;
        }
    }
    if !revoked && !state.gap_sync_pending {
        state.tail_from = state.head.slot;
        if state.last_error == "HISTORY_QUEUE_BUDGET" {
            state.last_error.clear();
        }
    }
    prune_completed(&mut state, &reg.limits);
    let bytes = match encode(&state, &reg.limits) {
        Ok(b) => b,
        Err(_) => {
            if let Some(from) = state.jobs.values().filter_map(|j| j.next).min() {
                state.tail_from = state.tail_from.min(from);
            }
            state.jobs.clear();
            state.gap_sync_pending = true;
            state.last_error = "HISTORY_QUEUE_BUDGET".into();
            match encode(&state, &reg.limits) {
                Ok(b) => b,
                Err(_) => b"null".to_vec(),
            }
        }
    };
    batch.put(KEY, bytes);
}
#[derive(Clone)]
pub struct HistoryQueue {
    db: Arc<Mutex<Store>>,
    registration: HistoryRegistration,
}
impl HistoryQueue {
    pub fn open(db: Arc<Mutex<Store>>, registration: HistoryRegistration) -> Result<Self> {
        registration.limits.validate()?;
        {
            let mut owner = db.lock().map_err(|_| err("STATE_LOCK_POISONED"))?;
            owner.history_registration = Some(registration.clone());
        }
        Ok(Self { db, registration })
    }
    fn lock(&self) -> Result<MutexGuard<'_, Store>> {
        let db = self.db.try_lock().map_err(|_| err("HISTORY_OWNER_BUSY"))?;
        if db.is_uncertain() || db.history_uncertain {
            return Err(err("HISTORY_REOPEN_REQUIRED"));
        }
        if db.history_registration.as_ref().is_none_or(|r| {
            r.chain_key != self.registration.chain_key
                || r.genesis_hash != self.registration.genesis_hash
                || r.source_fingerprint != self.registration.source_fingerprint
                || r.enabled != self.registration.enabled
                || r.limits != self.registration.limits
        }) {
            return Err(err("HISTORY_DATASET_CHANGED"));
        }
        Ok(db)
    }
    fn state(&self, db: &Store) -> Result<State> {
        let s = read(db)?.ok_or_else(|| err("HISTORY_WAITING_BASELINE"))?;
        if s.version != 1 || s.dataset.chain_key != self.registration.chain_key {
            return Err(err("HISTORY_IDENTITY"));
        }
        Ok(s)
    }
    pub fn status(&self) -> Result<HistoryStatus> {
        let db = self.lock()?;
        let s = read(&db)?;
        Ok(HistoryStatus {
            dataset: s.as_ref().map(|s| s.dataset.clone()),
            canonical_revision: s.as_ref().map_or(0, |s| s.canonical_revision),
            owner_canonical_epoch: s.as_ref().map_or(0, |s| s.owner_canonical_epoch),
            online_generation: s.as_ref().map_or(0, |s| s.generation),
            head: s.as_ref().map(|s| s.head.clone()),
            revoked: s.as_ref().is_some_and(|s| s.revoked),
            epoch: s.as_ref().map_or_else(String::new, |s| s.epoch.clone()),
            jobs: s
                .as_ref()
                .map_or_else(Vec::new, |s| s.jobs.values().cloned().collect()),
            covered_ranges: s.as_ref().map_or_else(Vec::new, |s| s.covered.clone()),
            missing_ranges: s.as_ref().map_or_else(Vec::new, |s| {
                s.jobs
                    .values()
                    .filter_map(|j| {
                        j.next.map(|from| Gap {
                            from,
                            to: j.to,
                            reason: j.last_error.clone(),
                        })
                    })
                    .collect()
            }),
            coverage_policy: "DECLARED_GAPS_AND_FINALIZED_TAIL".into(),
            next_tail_to_enqueue: s.as_ref().map_or(0, |s| s.tail_from),
            gap_sync_pending: s.as_ref().is_some_and(|s| s.gap_sync_pending),
            records: s.as_ref().map_or(0, |s| s.records),
            bytes: s.as_ref().map_or(0, |s| s.bytes),
            last_error: s
                .as_ref()
                .map_or_else(String::new, |s| s.last_error.clone()),
            source_status: s
                .as_ref()
                .map_or_else(|| "WAITING_BASELINE".into(), |s| s.source_status.clone()),
            enabled: self.registration.enabled,
            paused: s.as_ref().is_some_and(|s| s.paused),
            history_independent: true,
            historical_accounts_complete: false,
            limits: self.registration.limits.clone(),
        })
    }
    pub fn pages(&self, limit: usize) -> Result<Vec<HistoryPage>> {
        if limit == 0 || limit > 16 {
            return Err(err("HISTORY_READ_BUDGET"));
        }
        let db = self.lock()?;
        let state = self.state(&db)?;
        let prefix = format!(
            "history_replay/v1/{}/{:020}/",
            hash(&state.dataset)?,
            state.canonical_revision
        );
        let mut out = Vec::new();
        let mut bytes = 0usize;
        for row in db.db.iterator(rocksdb::IteratorMode::From(
            prefix.as_bytes(),
            rocksdb::Direction::Forward,
        )) {
            let (key, value) = row.map_err(|_| err("HISTORY_READ"))?;
            if !key.starts_with(prefix.as_bytes()) || out.len() >= limit {
                break;
            }
            bytes = bytes
                .checked_add(value.len())
                .ok_or_else(|| err("HISTORY_READ_BUDGET"))?;
            if bytes > self.registration.limits.max_batch_bytes {
                return Err(err("HISTORY_READ_BUDGET"));
            }
            out.push(serde_json::from_slice(&value).map_err(|_| err("HISTORY_CORRUPT"))?);
        }
        Ok(out)
    }
    pub fn set_paused(&self, paused: bool) -> Result<()> {
        let mut db = self.lock()?;
        let mut s = self.state(&db)?;
        s.paused = paused;
        for j in s.jobs.values_mut() {
            if j.phase == HistoryPhase::Running {
                j.phase = HistoryPhase::Pending;
                j.lease_until_ms = 0;
                j.claim_head = None;
            }
        }
        save(&mut db, &s, &self.registration.limits)
    }
    pub fn claim(&self, now: u64) -> Result<Option<HistoryClaim>> {
        let mut db = self.lock()?;
        let mut s = self.state(&db)?;
        if !self.registration.enabled
            || !s.enabled
            || s.paused
            || s.revoked
            || s.last_error == "HISTORY_FACT_BUDGET"
        {
            return Ok(None);
        }
        if s.gap_sync_pending || s.tail_from < s.head.slot {
            if let Some(snapshot) = db.persisted_snapshot()? {
                let mut batch = WriteBatch::default();
                stage_snapshot(&db, &snapshot, "HISTORY_QUEUE_RESYNC", &mut batch);
                write(&mut db, batch)?;
                s = self.state(&db)?;
            }
        }
        if s.records >= self.registration.limits.max_records
            || s.bytes >= self.registration.limits.max_history_bytes
        {
            s.last_error = "HISTORY_FACT_BUDGET".into();
            s.source_status = "QUOTA_PAUSED".into();
            save(&mut db, &s, &self.registration.limits)?;
            return Ok(None);
        }
        if s.dataset.genesis_hash != self.registration.genesis_hash
            || s.dataset.source_fingerprint != self.registration.source_fingerprint
        {
            return Err(err("HISTORY_DATASET_CHANGED"));
        }
        if s.session != db.scope_session {
            s.epoch = hash(&(&s.epoch, db.scope_session))?;
            s.session = db.scope_session;
            for j in s.jobs.values_mut() {
                if j.phase == HistoryPhase::Running {
                    j.phase = HistoryPhase::Pending;
                    j.lease_until_ms = 0;
                    j.claim_head = None;
                }
            }
        }
        let id = s
            .jobs
            .values()
            .filter(|j| {
                j.next.is_some()
                    && j.due_ms <= now
                    && (j.phase != HistoryPhase::Running || j.lease_until_ms <= now)
            })
            .min_by_key(|j| (j.next, j.from))
            .map(|j| j.id.clone());
        let Some(id) = id else {
            return Ok(None);
        };
        let j = s.jobs.get_mut(&id).unwrap();
        j.attempt = j
            .attempt
            .checked_add(1)
            .ok_or_else(|| err("HISTORY_ATTEMPT_OVERFLOW"))?;
        j.phase = HistoryPhase::Running;
        j.lease_until_ms = now
            .checked_add(self.registration.limits.lease_ms)
            .ok_or_else(|| err("HISTORY_LEASE"))?;
        j.claim_head = Some(j.bound_head.clone());
        let from = j.next.unwrap();
        let claim = HistoryClaim {
            id,
            epoch: s.epoch.clone(),
            session: db.scope_session,
            attempt: j.attempt,
            lease_until_ms: j.lease_until_ms,
            dataset: s.dataset.clone(),
            canonical_revision: s.canonical_revision,
            from,
            to: j
                .to
                .min(from.saturating_add(self.registration.limits.page_slots - 1)),
            head: j.bound_head.clone(),
        };
        save(&mut db, &s, &self.registration.limits)?;
        Ok(Some(claim))
    }
    fn valid<'a>(
        &self,
        db: &Store,
        s: &'a mut State,
        c: &HistoryClaim,
        now: u64,
    ) -> Result<&'a mut HistoryJob> {
        if s.epoch != c.epoch
            || s.session != c.session
            || c.session != db.scope_session
            || s.dataset != c.dataset
            || s.canonical_revision != c.canonical_revision
            || s.revoked
            || s.paused
        {
            return Err(err("HISTORY_STALE_CLAIM"));
        }
        let j = s
            .jobs
            .get_mut(&c.id)
            .ok_or_else(|| err("HISTORY_STALE_CLAIM"))?;
        if j.phase != HistoryPhase::Running
            || j.attempt != c.attempt
            || j.lease_until_ms != c.lease_until_ms
            || now.max(xone_observability::now_ms()) >= j.lease_until_ms
            || j.next != Some(c.from)
            || c.to
                != j.to.min(
                    c.from
                        .saturating_add(self.registration.limits.page_slots - 1),
                )
            || j.claim_head.as_ref() != Some(&c.head)
        {
            return Err(err("HISTORY_STALE_CLAIM"));
        }
        Ok(j)
    }
    pub fn renew(&self, c: &HistoryClaim, now: u64) -> Result<HistoryClaim> {
        let mut db = self.lock()?;
        let mut state = self.state(&db)?;
        let job = self.valid(&db, &mut state, c, now)?;
        job.lease_until_ms = now
            .max(xone_observability::now_ms())
            .checked_add(self.registration.limits.lease_ms)
            .ok_or_else(|| err("HISTORY_LEASE"))?;
        let mut renewed = c.clone();
        renewed.lease_until_ms = job.lease_until_ms;
        save(&mut db, &state, &self.registration.limits)?;
        Ok(renewed)
    }
    pub fn retry(&self, c: &HistoryClaim, now: u64, reason: &str) -> Result<()> {
        let mut db = self.lock()?;
        let mut s = self.state(&db)?;
        let j = self.valid(&db, &mut s, c, now)?;
        j.phase = HistoryPhase::Backoff;
        j.lease_until_ms = 0;
        j.claim_head = None;
        j.due_ms = now.saturating_add(
            self.registration
                .limits
                .retry_min_ms
                .saturating_mul(1u64 << j.attempt.min(16))
                .min(self.registration.limits.retry_max_ms),
        );
        j.last_error = reason.chars().take(128).collect();
        s.last_error = j.last_error.clone();
        s.source_status = "UNKNOWN_OR_UNAVAILABLE".into();
        save(&mut db, &s, &self.registration.limits)
    }
    pub fn cancel(&self, c: &HistoryClaim, now: u64) -> Result<()> {
        let mut db = self.lock()?;
        let mut s = self.state(&db)?;
        let j = self.valid(&db, &mut s, c, now)?;
        j.phase = HistoryPhase::Pending;
        j.lease_until_ms = 0;
        j.claim_head = None;
        save(&mut db, &s, &self.registration.limits)
    }
    pub fn complete(&self, c: &HistoryClaim, page: HistoryPage, now: u64) -> Result<()> {
        crate::history_runtime::validate_page(c, &page, &self.registration.limits, now)?;
        let mut db = self.lock()?;
        let mut s = self.state(&db)?;
        self.valid(&db, &mut s, c, now)?;
        let bytes = serde_json::to_vec(&page).map_err(|_| err("HISTORY_ENCODING"))?;
        if bytes.len() > self.registration.limits.max_batch_bytes
            || s.records >= self.registration.limits.max_records
            || s.bytes
                .checked_add(bytes.len() as u64)
                .is_none_or(|n| n > self.registration.limits.max_history_bytes)
        {
            return Err(err("HISTORY_FACT_BUDGET"));
        }
        let key = format!(
            "history_replay/v1/{}/{:020}/{:020}-{:020}",
            hash(&c.dataset)?,
            c.canonical_revision,
            c.from,
            page.to
        );
        if db
            .db
            .get(key.as_bytes())
            .map_err(|_| err("HISTORY_READ"))?
            .is_some()
        {
            return Err(err("HISTORY_DUPLICATE"));
        }
        let j = s.jobs.get_mut(&c.id).unwrap();
        j.next = if page.to >= j.to {
            None
        } else {
            Some(page.to + 1)
        };
        j.phase = if j.next.is_none() {
            HistoryPhase::Complete
        } else {
            HistoryPhase::Pending
        };
        j.lease_until_ms = 0;
        j.claim_head = None;
        j.due_ms = 0;
        j.last_error.clear();
        merge_covered(&mut s.covered, c.from, page.to);
        if s.covered.len() > self.registration.limits.max_records as usize {
            return Err(err("HISTORY_QUEUE_BUDGET"));
        }
        s.records += 1;
        s.bytes += bytes.len() as u64;
        s.last_error.clear();
        s.source_status = "PUBLIC_FINALIZED_TRANSACTIONS_ONLY".into();
        prune_completed(&mut s, &self.registration.limits);
        let mut batch = WriteBatch::default();
        batch.put(key, bytes);
        batch.put(KEY, encode(&s, &self.registration.limits)?);
        if xone_observability::now_ms() >= c.lease_until_ms {
            return Err(err("HISTORY_STALE_CLAIM"));
        }
        write(&mut db, batch)
    }
}
