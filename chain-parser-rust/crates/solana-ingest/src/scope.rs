//! R04 scope-control foundation. Authentication and durable owner-store/source
//! adapters belong to the service. No in-process store pretends to survive restart.
#[path = "scope_store.rs"]
pub(crate) mod scope_store;
pub use scope_store::{
    FileHistoryLimits, FileHistoryStore, FileScopeLimits, FileScopeStore, OnlineScopeCommit,
    ProductionStore, ProductionTransactions,
};

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    Budget,
    UnknownPool,
    FenceChanged,
    Busy,
    NoBaseline,
    HeadFirst,
    Data,
    Stale,
    Cancelled,
    HistoryCapability,
    HistoryNamespace,
    StoreLocked,
    StoreCorrupt,
    StoreUnsafe,
    StoreConflict,
    StoreClosed,
    Storage(String),
}
pub type ScopeResult<T> = Result<T, ScopeError>;
impl std::fmt::Display for ScopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Budget => "SCOPE_BUDGET",
            Self::UnknownPool => "SCOPE_UNKNOWN_POOL",
            Self::FenceChanged => "SCOPE_FENCE_CHANGED",
            Self::Busy => "SCOPE_BUSY",
            Self::NoBaseline => "SCOPE_NO_TRUSTED_BASELINE",
            Self::HeadFirst => "SCOPE_HEAD_FIRST_REQUIRED",
            Self::Data => "SCOPE_DATA_INVALID",
            Self::Stale => "SCOPE_ANCHOR_STALE",
            Self::Cancelled => "SCOPE_CANCELLED",
            Self::HistoryCapability => "HISTORY_CAPABILITY",
            Self::HistoryNamespace => "HISTORY_NAMESPACE",
            Self::StoreLocked => "SCOPE_STORE_LOCKED",
            Self::StoreCorrupt => "SCOPE_STORE_CORRUPT",
            Self::StoreUnsafe => "SCOPE_STORE_UNSAFE_PATH",
            Self::StoreConflict => "SCOPE_STORE_CONFLICT",
            Self::StoreClosed => "SCOPE_STORE_CLOSED_OR_UNCERTAIN",
            Self::Storage(s) => s,
        })
    }
}
impl std::error::Error for ScopeError {}
/// The service must cancel the token when the request/worker is cancelled. Run
/// blocking source adapters with spawn_blocking; never block a Tokio reactor.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release)
    }
    pub fn check(&self) -> ScopeResult<()> {
        if self.0.load(Ordering::Acquire) {
            Err(ScopeError::Cancelled)
        } else {
            Ok(())
        }
    }
}
/// Current owner-authorized namespace, obtained from the active lease/config.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Fence {
    pub chain_key: String,
    pub chain_identity: String,
    pub owner: String,
    pub assignment: u64,
    pub generation: u64,
    pub canonical_epoch: u64,
    pub writer_epoch: u64,
    pub adapter_version: String,
    pub schema_version: String,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// Cursor is a durable recovery-journal sequence for this Fence, not a raw
/// height, RPC slot or per-session write_version. The source must prove the
/// candidate and buffer cursors belong to the same timeline.
pub struct ScopeAnchor {
    pub height: u64,
    pub hash: String,
    pub timestamp_ms: u64,
    pub cursor: u64,
}
/// Include ALL pool dependencies, including dynamic accounts, before admission.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeDependency {
    pub id: String,
    pub max_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeLimits {
    pub max_jobs: u64,
    pub max_account_bytes: u64,
    pub max_tail_events: u64,
    pub max_tail_bytes: u64,
    pub max_tail_age_ms: u64,
    pub max_baseline_age_ms: u64,
    pub shard_accounts: u64,
}
#[derive(Clone, Debug)]
pub struct ScopeConfig {
    pub fence: Fence,
    pub limits: ScopeLimits,
    pub catalog: BTreeMap<String, Vec<ScopeDependency>>,
    pub trusted_sources: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct EnsurePoolScopeRequest {
    pub fence: Fence,
    pub anchor: ScopeAnchor,
    pub pool_ids: Vec<String>,
    pub budget: ScopeLimits,
    pub history_after: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScopePhase {
    Admitted,
    LiveBuffering,
    BaselineAcquiring,
    Restoring,
    TailAligning,
    ScopedReady,
    Failed,
    HeadFirstRequired,
}
impl ScopePhase {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::ScopedReady | Self::Failed | Self::HeadFirstRequired
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "ADMITTED",
            Self::LiveBuffering => "LIVE_BUFFERING",
            Self::BaselineAcquiring => "BASELINE_ACQUIRING",
            Self::Restoring => "RESTORING",
            Self::TailAligning => "TAIL_ALIGNING",
            Self::ScopedReady => "SCOPED_READY",
            Self::Failed => "FAILED",
            Self::HeadFirstRequired => "HEAD_FIRST_REQUIRED",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HistoryGap {
    pub after: u64,
    pub through: u64,
    pub reason: String,
}
/// Durable job/attempt/phase/cursor. Reclaimed attempts restore ALL shards from
/// a new buffer; the saved cursor alone can never establish readiness.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScopeJob {
    pub id: String,
    pub fence: Fence,
    pub anchor: ScopeAnchor,
    pub pool_ids: Vec<String>,
    pub budget: ScopeLimits,
    pub history_after: u64,
    pub dependencies: Vec<ScopeDependency>,
    pub reserved_bytes: u64,
    pub phase: ScopePhase,
    pub attempt: u64,
    pub buffer_id: String,
    pub baseline_hash: String,
    pub cursor: u64,
    pub restored_accounts: u64,
    pub failure: String,
    pub history_gap: Option<HistoryGap>,
}
#[derive(Clone, Debug)]
pub struct Admission {
    pub job: ScopeJob,
    pub limits: ScopeLimits,
}
/// REQUIRED durable transaction contract (existing RocksDB/prepared owner store):
/// - admit atomically checks current Fence/lease, deduplicates ID, and reserves
///   aggregate job/account-byte quotas. Terminal jobs release reservations.
/// - claim exclusively leases a new persisted Attempt; concurrent attempts fail.
///   After crash only an expired/fenced attempt may be reclaimed. Ready is returned
///   unchanged. A terminal retry must atomically reacquire the original quotas.
///   Retention/GC of terminal jobs is an explicit store policy.
/// - checkpoint/fail CAS on ID+Attempt under the current lease. fail records the
///   error/gap, discards unpublished staging and is allowed after cancellation.
/// - publish checks cancellation, CURRENT Fence, lease AND Attempt in the SAME
///   transaction as ALL accounts, cursor, history gap and recovery visibility.
///   An online DB adapter must ALSO include the owner prepared journal/generation.
///   Recheck original anchor freshness on READY reads and after staging I/O at
///   the final commit gate; neither reads nor authority renewal extend its TTL.
///
/// FileScopeStore is durable RECOVERY_DATA_ONLY output, never online readiness.
/// ProductionStore delegates all operations directly to an online DB backend.
pub trait ScopeStore: Send + Sync {
    fn current(&self, chain: &str) -> ScopeResult<Fence>;
    fn admit(&self, admission: Admission) -> ScopeResult<ScopeJob>;
    fn claim(&self, id: &str, fence: &Fence) -> ScopeResult<ScopeJob>;
    fn checkpoint(&self, job: &ScopeJob) -> ScopeResult<()>;
    fn fail(&self, job: &ScopeJob) -> ScopeResult<()>;
    fn publish(
        &self,
        cancel: &Cancellation,
        job: &ScopeJob,
        publication: Publication,
    ) -> ScopeResult<()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BaselineKind {
    HealthyPeer,
    RecentSnapshot,
    CurrentChainState,
}
#[derive(Clone, Debug)]
pub struct BaselineCandidate {
    pub fence: Fence,
    pub anchor: ScopeAnchor,
    pub source: String,
    pub kind: BaselineKind,
    pub pool_ids: Vec<String>,
    pub dependencies: Vec<ScopeDependency>,
    pub account_hashes: BTreeMap<String, String>,
    pub content_hash: String,
}
#[derive(Clone, Debug)]
pub struct StreamBuffer {
    pub id: String,
    pub first_cursor: u64,
    pub durable: bool,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScopeAccount {
    pub id: String,
    pub data: Vec<u8>,
    pub anchor: ScopeAnchor,
}
#[derive(Clone, Debug)]
pub struct ScopeTail {
    pub from: ScopeAnchor,
    pub to: ScopeAnchor,
    pub complete: bool,
    pub events: u64,
    pub bytes: u64,
    pub accounts: Vec<ScopeAccount>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Publication {
    pub fence: Fence,
    pub anchor: ScopeAnchor,
    pub pool_ids: Vec<String>,
    pub dependencies: Vec<ScopeDependency>,
    pub accounts: Vec<ScopeAccount>,
    pub baseline_hash: String,
    pub buffer_id: String,
    pub cursor: u64,
    pub history_gap: Option<HistoryGap>,
}
/// Source adapters MUST enforce job/dependency budgets BEFORE network/WAL
/// allocation, honor cancellation, and verify versioned bytes at the anchor.
/// `complete` means a verified continuous source-session boundary, not merely a
/// head notification. Current RPC context is not proof of a historical bank.
/// align_tail returns the complete final scope, not partial deltas. release_buffer
/// releases the attempt handle, never committed generation's durable journal.
pub trait RecoverySource {
    fn start_buffer(&mut self, cancel: &Cancellation, job: &ScopeJob) -> ScopeResult<StreamBuffer>;
    fn fix_anchor(
        &mut self,
        cancel: &Cancellation,
        job: &ScopeJob,
        buffer: &StreamBuffer,
    ) -> ScopeResult<ScopeAnchor>;
    fn candidates(
        &mut self,
        cancel: &Cancellation,
        job: &ScopeJob,
        anchor: &ScopeAnchor,
    ) -> ScopeResult<Vec<BaselineCandidate>>;
    fn restore_shard(
        &mut self,
        cancel: &Cancellation,
        job: &ScopeJob,
        baseline: &BaselineCandidate,
        ids: &[String],
    ) -> ScopeResult<Vec<ScopeAccount>>;
    fn align_tail(
        &mut self,
        cancel: &Cancellation,
        job: &ScopeJob,
        buffer: &StreamBuffer,
        baseline: &BaselineCandidate,
        accounts: &[ScopeAccount],
    ) -> ScopeResult<ScopeTail>;
    fn release_buffer(&mut self, buffer: &StreamBuffer) -> ScopeResult<()>;
}
pub struct ScopeController {
    config: ScopeConfig,
    store: Arc<dyn ScopeStore>,
    now: fn() -> u64,
}
impl ScopeController {
    pub fn new(
        mut config: ScopeConfig,
        store: Arc<dyn ScopeStore>,
        now: fn() -> u64,
    ) -> ScopeResult<Self> {
        if !valid_fence(&config.fence)
            || !valid_limits(&config.limits)
            || config.trusted_sources.is_empty()
        {
            return Err(ScopeError::Data);
        }
        for (id, deps) in &mut config.catalog {
            if id.is_empty() || deps.is_empty() {
                return Err(ScopeError::Data);
            };
            deps.sort_by(|a, b| a.id.cmp(&b.id));
            if deps.iter().any(|d| d.id.is_empty() || d.max_bytes == 0)
                || deps.windows(2).any(|d| d[0].id == d[1].id)
            {
                return Err(ScopeError::Data);
            }
        }
        Ok(Self { config, store, now })
    }
    fn check(&self, cancel: &Cancellation, fence: &Fence) -> ScopeResult<()> {
        cancel.check()?;
        if self.store.current(&fence.chain_key)? != *fence || *fence != self.config.fence {
            Err(ScopeError::FenceChanged)
        } else {
            Ok(())
        }
    }
    /// AFTER authentication/authorization. Admission is not quote readiness.
    pub fn ensure_pool_scope(
        &self,
        cancel: &Cancellation,
        mut r: EnsurePoolScopeRequest,
    ) -> ScopeResult<ScopeJob> {
        self.check(cancel, &r.fence)?;
        if r.anchor.hash.is_empty() || !valid_limits(&r.budget) || r.pool_ids.is_empty() {
            return Err(ScopeError::Data);
        };
        r.pool_ids = canonical_pools(&r.pool_ids);
        r.budget = cap_limits(&r.budget, &self.config.limits);
        let mut deps = BTreeMap::new();
        for pool in &r.pool_ids {
            for d in self
                .config
                .catalog
                .get(pool)
                .ok_or(ScopeError::UnknownPool)?
            {
                if deps
                    .insert(d.id.clone(), d.max_bytes)
                    .is_some_and(|v| v != d.max_bytes)
                {
                    return Err(ScopeError::Data);
                }
            }
        }
        let mut reserved_bytes = 0u64;
        let mut dependencies = vec![];
        for (id, max_bytes) in deps {
            reserved_bytes = reserved_bytes
                .checked_add(max_bytes)
                .ok_or(ScopeError::Budget)?;
            if reserved_bytes > r.budget.max_account_bytes {
                return Err(ScopeError::Budget);
            };
            dependencies.push(ScopeDependency { id, max_bytes })
        }
        let mut job = ScopeJob {
            id: String::new(),
            fence: r.fence,
            anchor: r.anchor,
            pool_ids: r.pool_ids,
            budget: r.budget,
            history_after: r.history_after,
            dependencies,
            reserved_bytes,
            phase: ScopePhase::Admitted,
            attempt: 0,
            buffer_id: String::new(),
            baseline_hash: String::new(),
            cursor: 0,
            restored_accounts: 0,
            failure: String::new(),
            history_gap: None,
        };
        job.id = scope_job_id(&job);
        let limits = job.budget.clone();
        cancel.check()?;
        self.store.admit(Admission { job, limits })
    }

    // Revalidate durable records against current catalog and policy on every
    // reclaimed attempt; neither old config nor damaged records may shrink scope.
    fn validate_job(&self, j: &ScopeJob) -> ScopeResult<()> {
        if !valid_limits(&j.budget) || cap_limits(&j.budget, &self.config.limits) != j.budget {
            return Err(ScopeError::Budget);
        }
        if j.anchor.hash.is_empty()
            || j.pool_ids.is_empty()
            || canonical_pools(&j.pool_ids) != j.pool_ids
        {
            return Err(ScopeError::Data);
        }
        let mut expected = BTreeMap::new();
        for pool in &j.pool_ids {
            for d in self
                .config
                .catalog
                .get(pool)
                .ok_or(ScopeError::UnknownPool)?
            {
                if expected
                    .insert(d.id.clone(), d.max_bytes)
                    .is_some_and(|v| v != d.max_bytes)
                {
                    return Err(ScopeError::Data);
                }
            }
        }
        let mut total = 0u64;
        let mut dependencies = vec![];
        for (id, max_bytes) in expected {
            total = total.checked_add(max_bytes).ok_or(ScopeError::Budget)?;
            if total > j.budget.max_account_bytes {
                return Err(ScopeError::Budget);
            };
            dependencies.push(ScopeDependency { id, max_bytes })
        }
        if dependencies != j.dependencies || total != j.reserved_bytes || scope_job_id(j) != j.id {
            return Err(ScopeError::Data);
        }
        Ok(())
    }

    fn save(
        &self,
        cancel: &Cancellation,
        job: &mut ScopeJob,
        phase: ScopePhase,
    ) -> ScopeResult<()> {
        self.check(cancel, &job.fence)?;
        job.phase = phase;
        self.store.checkpoint(job)
    }
    pub fn recover(
        &self,
        cancel: &Cancellation,
        id: &str,
        source: &mut dyn RecoverySource,
    ) -> ScopeResult<()> {
        self.check(cancel, &self.config.fence)?;
        let mut job = self.store.claim(id, &self.config.fence)?;
        self.check(cancel, &job.fence)?;
        if job.phase == ScopePhase::ScopedReady {
            self.validate_job(&job)?;
            self.check(cancel, &job.fence)?;
            return require_scope_fresh(&job, (self.now)());
        }
        let mut buffer = None;
        let result = self.execute(cancel, &mut job, source, &mut buffer);
        // Cleanup failure cannot undo a committed transaction. Adapters report handle
        // cleanup failures through their observability; durable WAL is retained.
        if let Some(b) = buffer {
            let _ = source.release_buffer(&b);
        }
        if let Err(ref error) = result {
            job.failure = error.to_string();
            job.phase = if *error == ScopeError::HeadFirst {
                job.history_gap = Some(HistoryGap {
                    after: job.cursor,
                    through: job.anchor.cursor,
                    reason: "STREAM_INSUFFICIENT_HEAD_FIRST".into(),
                });
                ScopePhase::HeadFirstRequired
            } else {
                ScopePhase::Failed
            };
            if let Err(persist) = self.store.fail(&job) {
                return Err(ScopeError::Storage(format!(
                    "{error}; scope failure persistence: {persist}"
                )));
            }
        }
        result
    }
    fn execute(
        &self,
        cancel: &Cancellation,
        j: &mut ScopeJob,
        source: &mut dyn RecoverySource,
        held: &mut Option<StreamBuffer>,
    ) -> ScopeResult<()> {
        self.check(cancel, &j.fence)?;
        self.validate_job(j)?;
        j.buffer_id.clear();
        j.baseline_hash.clear();
        j.restored_accounts = 0;
        j.cursor = 0;
        j.failure.clear();
        j.history_gap = None;
        self.save(cancel, j, ScopePhase::LiveBuffering)?;
        let buffer = source.start_buffer(cancel, j)?;
        *held = Some(buffer.clone());
        if !buffer.durable || buffer.id.is_empty() {
            return Err(ScopeError::HeadFirst);
        };
        j.buffer_id = buffer.id.clone();
        self.save(cancel, j, ScopePhase::BaselineAcquiring)?;
        let anchor = source.fix_anchor(cancel, j, &buffer)?;
        if anchor != j.anchor {
            return Err(ScopeError::Data);
        };
        require_scope_fresh(j, (self.now)())?;
        let candidates = source.candidates(cancel, j, &anchor)?;
        let mut baseline = select_scope_baseline(j, &candidates, &self.config, (self.now)())?;
        j.baseline_hash = baseline.content_hash.clone();
        j.cursor = baseline.anchor.cursor;
        if buffer.first_cursor.saturating_sub(baseline.anchor.cursor) > 1 {
            let available: Vec<_> = candidates
                .iter()
                .filter(|candidate| {
                    buffer.first_cursor.saturating_sub(candidate.anchor.cursor) <= 1
                })
                .cloned()
                .collect();
            baseline = select_scope_baseline(j, &available, &self.config, (self.now)())
                .map_err(|_| ScopeError::HeadFirst)?;
            j.baseline_hash = baseline.content_hash.clone();
            j.cursor = baseline.anchor.cursor;
        }
        if j.history_after < baseline.anchor.cursor {
            j.history_gap = Some(HistoryGap {
                after: j.history_after,
                through: baseline.anchor.cursor,
                reason: "HEAD_FIRST_BASELINE".into(),
            })
        }
        let shard_size = j.budget.shard_accounts.min(j.dependencies.len() as u64) as usize;
        if shard_size == 0 {
            return Err(ScopeError::Data);
        };
        let mut accounts = vec![];
        for start in (0..j.dependencies.len()).step_by(shard_size) {
            self.save(cancel, j, ScopePhase::Restoring)?;
            let deps = &j.dependencies[start..(start + shard_size).min(j.dependencies.len())];
            let ids: Vec<_> = deps.iter().map(|d| d.id.clone()).collect();
            let shard = source.restore_shard(cancel, j, &baseline, &ids)?;
            validate_accounts(
                &shard,
                deps,
                &baseline.anchor,
                Some(&baseline.account_hashes),
            )?;
            accounts.extend(shard);
            j.restored_accounts = accounts.len() as u64;
        }
        self.save(cancel, j, ScopePhase::TailAligning)?;
        let tail = source.align_tail(cancel, j, &buffer, &baseline, &accounts)?;
        if !tail.complete || tail.events != anchor.cursor - baseline.anchor.cursor {
            return Err(ScopeError::HeadFirst);
        };
        if tail.from != baseline.anchor || tail.to != anchor {
            return Err(ScopeError::Data);
        };
        if tail.events > j.budget.max_tail_events || tail.bytes > j.budget.max_tail_bytes {
            return Err(ScopeError::HeadFirst);
        };
        // With zero events there is no authenticated account change: retain the
        // selected manifest's per-account digests even when the source returns bytes.
        let hashes = if tail.events == 0 {
            Some(&baseline.account_hashes)
        } else {
            None
        };
        let bytes = validate_accounts(&tail.accounts, &j.dependencies, &anchor, hashes)?;
        if bytes > tail.bytes {
            return Err(ScopeError::Budget);
        };
        require_scope_fresh(j, (self.now)())?;
        self.check(cancel, &j.fence)?;
        j.cursor = anchor.cursor;
        self.store.publish(
            cancel,
            j,
            Publication {
                fence: j.fence.clone(),
                anchor,
                pool_ids: j.pool_ids.clone(),
                dependencies: j.dependencies.clone(),
                accounts: tail.accounts,
                baseline_hash: j.baseline_hash.clone(),
                buffer_id: buffer.id,
                cursor: j.cursor,
                history_gap: j.history_gap.clone(),
            },
        )
    }
}
pub(crate) fn valid_fence(f: &Fence) -> bool {
    !f.chain_key.is_empty()
        && !f.chain_identity.is_empty()
        && !f.owner.is_empty()
        && f.assignment > 0
        && f.generation > 0
        && f.canonical_epoch > 0
        && f.writer_epoch > 0
        && !f.adapter_version.is_empty()
        && !f.schema_version.is_empty()
}
pub(crate) fn valid_limits(b: &ScopeLimits) -> bool {
    b.max_jobs > 0
        && b.max_account_bytes > 0
        && b.max_tail_events > 0
        && b.max_tail_bytes > 0
        && b.max_tail_age_ms > 0
        && b.max_baseline_age_ms > 0
        && b.shard_accounts > 0
}
fn cap_limits(a: &ScopeLimits, b: &ScopeLimits) -> ScopeLimits {
    ScopeLimits {
        max_jobs: a.max_jobs.min(b.max_jobs),
        max_account_bytes: a.max_account_bytes.min(b.max_account_bytes),
        max_tail_events: a.max_tail_events.min(b.max_tail_events),
        max_tail_bytes: a.max_tail_bytes.min(b.max_tail_bytes),
        max_tail_age_ms: a.max_tail_age_ms.min(b.max_tail_age_ms),
        max_baseline_age_ms: a.max_baseline_age_ms.min(b.max_baseline_age_ms),
        shard_accounts: a.shard_accounts.min(b.shard_accounts),
    }
}
fn canonical_pools(p: &[String]) -> Vec<String> {
    p.iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn fresh(ts: u64, now: u64, max: u64) -> bool {
    ts <= now && now - ts <= max
}
// Always evaluate the admitted anchor timestamp/budget, never a renewed TTL.
pub(crate) fn require_scope_fresh(job: &ScopeJob, now: u64) -> ScopeResult<()> {
    if fresh(job.anchor.timestamp_ms, now, job.budget.max_tail_age_ms) {
        Ok(())
    } else {
        Err(ScopeError::Stale)
    }
}

/// Raw SHA-256 lowercase hex. Baseline manifests use the same length-framed
/// strings as Go, independent of JSON ordering and with UTF-8 byte lengths.
pub fn account_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn frame_hash(parts: &[String]) -> String {
    let mut bytes = String::new();
    for part in parts {
        bytes.push_str(&part.len().to_string());
        bytes.push(':');
        bytes.push_str(part)
    }
    account_hash(bytes.as_bytes())
}
fn manifest_parts(
    f: &Fence,
    a: &ScopeAnchor,
    pools: &[String],
    deps: &[ScopeDependency],
) -> Vec<String> {
    let mut p = vec![
        f.chain_key.clone(),
        f.chain_identity.clone(),
        f.owner.clone(),
        f.assignment.to_string(),
        f.generation.to_string(),
        f.canonical_epoch.to_string(),
        f.writer_epoch.to_string(),
        f.adapter_version.clone(),
        f.schema_version.clone(),
        a.height.to_string(),
        a.hash.clone(),
        a.timestamp_ms.to_string(),
        a.cursor.to_string(),
    ];
    let pools = canonical_pools(pools);
    p.push(pools.len().to_string());
    p.extend(pools);
    let mut ds = deps.to_vec();
    ds.sort_by(|a, b| a.id.cmp(&b.id));
    p.push(ds.len().to_string());
    for d in ds {
        p.push(d.id);
        p.push(d.max_bytes.to_string())
    }
    p
}
pub(crate) fn scope_job_id(j: &ScopeJob) -> String {
    let mut p = vec!["scope-job-v1".into()];
    p.extend(manifest_parts(
        &j.fence,
        &j.anchor,
        &j.pool_ids,
        &j.dependencies,
    ));
    frame_hash(&p)
}
pub fn baseline_hash(b: &BaselineCandidate) -> String {
    let mut p = vec!["scope-baseline-v1".into()];
    p.extend(manifest_parts(
        &b.fence,
        &b.anchor,
        &b.pool_ids,
        &b.dependencies,
    ));
    p.push(b.source.clone());
    p.push((b.kind as u8).to_string());
    p.push(b.account_hashes.len().to_string());
    for (id, hash) in &b.account_hashes {
        p.push(id.clone());
        p.push(hash.clone())
    }
    frame_hash(&p)
}
pub fn select_scope_baseline(
    j: &ScopeJob,
    candidates: &[BaselineCandidate],
    config: &ScopeConfig,
    now: u64,
) -> ScopeResult<BaselineCandidate> {
    let mut best: Option<&BaselineCandidate> = None;
    let mut insufficient = false;
    for b in candidates {
        let mut deps = b.dependencies.clone();
        deps.sort_by(|a, b| a.id.cmp(&b.id));
        if !config.trusted_sources.contains(&b.source)
            || b.fence != j.fence
            || b.pool_ids.len() != j.pool_ids.len()
            || canonical_pools(&b.pool_ids) != j.pool_ids
            || deps != j.dependencies
            || b.anchor.hash.is_empty()
            || !fresh(b.anchor.timestamp_ms, now, j.budget.max_baseline_age_ms)
            || b.anchor.height > j.anchor.height
            || b.anchor.cursor > j.anchor.cursor
            || b.anchor.timestamp_ms > j.anchor.timestamp_ms
            || (b.anchor.height == j.anchor.height && b.anchor.hash != j.anchor.hash)
            || b.account_hashes.len() != j.dependencies.len()
            || b.content_hash != baseline_hash(b)
        {
            continue;
        };
        if j.dependencies.iter().any(|d| {
            !b.account_hashes.get(&d.id).is_some_and(|h| {
                h.len() == 64
                    && h.bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            })
        }) {
            continue;
        };
        if j.anchor.cursor - b.anchor.cursor > j.budget.max_tail_events
            || j.anchor.timestamp_ms - b.anchor.timestamp_ms > j.budget.max_tail_age_ms
        {
            insufficient = true;
            continue;
        }
        if best.is_none_or(|old| {
            b.kind < old.kind
                || (b.kind == old.kind
                    && (b.anchor.cursor > old.anchor.cursor
                        || (b.anchor.cursor == old.anchor.cursor
                            && b.anchor.timestamp_ms > old.anchor.timestamp_ms)))
        }) {
            best = Some(b)
        }
    }
    best.cloned().ok_or(if insufficient {
        ScopeError::HeadFirst
    } else {
        ScopeError::NoBaseline
    })
}
fn validate_accounts(
    accounts: &[ScopeAccount],
    deps: &[ScopeDependency],
    anchor: &ScopeAnchor,
    hashes: Option<&BTreeMap<String, String>>,
) -> ScopeResult<u64> {
    if accounts.len() != deps.len() {
        return Err(ScopeError::Data);
    };
    let expected: BTreeMap<_, _> = deps.iter().map(|d| (&d.id, d.max_bytes)).collect();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for a in accounts {
        let budget = expected.get(&a.id).ok_or(ScopeError::Data)?;
        if !seen.insert(&a.id)
            || a.anchor != *anchor
            || a.data.is_empty()
            || a.data.len() as u64 > *budget
        {
            return Err(ScopeError::Data);
        };
        if let Some(h) = hashes {
            if h.get(&a.id) != Some(&account_hash(&a.data)) {
                return Err(ScopeError::Data);
            }
        };
        total = total
            .checked_add(a.data.len() as u64)
            .ok_or(ScopeError::Budget)?
    }
    Ok(total)
}

#[derive(Clone, Debug)]
pub struct HistoryLimits {
    pub max_jobs: u64,
    pub max_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct HistoryRecord {
    pub chain_key: String,
    pub namespace: String,
    pub cursor: u64,
    pub payload: Vec<u8>,
}
/// Separate history_replay keys/queue, its OWN aggregate worker/byte quotas,
/// and atomic cursor/data transaction. This capability has no online methods.
pub trait HistoryStore: Send + Sync {
    fn append_history(&self, record: HistoryRecord, limits: &HistoryLimits) -> ScopeResult<()>;
}
pub struct HistoryWriter {
    store: Arc<dyn HistoryStore>,
    chain: String,
    token: String,
    limits: HistoryLimits,
}
impl HistoryWriter {
    pub fn new(
        store: Arc<dyn HistoryStore>,
        chain: String,
        token: String,
        limits: HistoryLimits,
    ) -> ScopeResult<Self> {
        if chain.is_empty() || token.is_empty() || limits.max_jobs == 0 || limits.max_bytes == 0 {
            return Err(ScopeError::Data);
        };
        Ok(Self {
            store,
            chain,
            token,
            limits,
        })
    }
    pub fn write(
        &self,
        cancel: &Cancellation,
        token: &str,
        record: HistoryRecord,
    ) -> ScopeResult<()> {
        let matches = token.len() == self.token.len()
            && token
                .bytes()
                .zip(self.token.bytes())
                .fold(0u8, |diff, (a, b)| diff | (a ^ b))
                == 0;
        if !matches {
            return Err(ScopeError::HistoryCapability);
        };
        if record.namespace != "history_replay" || record.chain_key != self.chain {
            return Err(ScopeError::HistoryNamespace);
        };
        if record.payload.is_empty() || record.payload.len() as u64 > self.limits.max_bytes {
            return Err(ScopeError::Budget);
        };
        cancel.check()?;
        self.store.append_history(record, &self.limits)
    }
}
