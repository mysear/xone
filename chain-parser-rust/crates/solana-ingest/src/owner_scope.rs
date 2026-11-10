//! R04 ledger in the EXISTING owner RocksDB. Scope ready, snapshot, journal and
//! commit proof share the owner apply WriteBatch. No file store participates.
use crate::{
    scope::{scope_store::*, *},
    store::Store,
};
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};
use xone_domain::{err, hash, Snapshot, SnapshotRef};
use xone_kafka_transport::{Cursor, Unit};
const KEY: &[u8] = b"scope/ledger-v1";
pub type CheckAuthority = Arc<dyn Fn(&Fence) -> ScopeResult<()> + Send + Sync>;
pub type PublishBridge =
    Arc<dyn Fn(&Cancellation, &ScopeJob, Publication) -> ScopeResult<()> + Send + Sync>;
type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    job: ScopeJob,
    session: u64,
    deadline: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    job_id: String,
    attempt: u64,
    session: u64,
    unit_hash: String,
    publication: Publication,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Commit {
    proof: OnlineScopeCommit,
    attempt: u64,
    session: u64,
    unit_hash: String,
    reference: SnapshotRef,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    version: u32,
    max_bytes: u64,
    session: u64,
    fence: Fence,
    until: u64,
    jobs: BTreeMap<String, Entry>,
    data: BTreeMap<String, Publication>,
    commits: BTreeMap<String, Commit>,
    intent: Option<Intent>,
    #[serde(default)]
    demands: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    manifests: BTreeMap<String, NativeManifestRef>,
}
fn io(e: xone_domain::Error) -> ScopeError {
    ScopeError::Storage(e.0)
}
fn db_lock(db: &Mutex<Store>) -> ScopeResult<MutexGuard<'_, Store>> {
    db.lock().map_err(|_| ScopeError::StoreClosed)
}
fn load(db: &Store) -> ScopeResult<Option<Ledger>> {
    if db.uncertain {
        return Err(ScopeError::StoreClosed);
    }
    let value = db
        .db
        .get(KEY)
        .map_err(|_| ScopeError::StoreCorrupt)?
        .map(|raw| serde_json::from_slice::<Ledger>(&raw).map_err(|_| ScopeError::StoreCorrupt))
        .transpose()?;
    if value.as_ref().is_some_and(|v| {
        v.version != 1
            || !crate::scope::valid_fence(&v.fence)
            || v.jobs
                .iter()
                .any(|(id, e)| *id != e.job.id || !valid_job_shape(&e.job))
    }) {
        return Err(ScopeError::StoreCorrupt);
    }
    Ok(value)
}
fn encoded(l: &Ledger, max: u64) -> ScopeResult<Vec<u8>> {
    let b = serde_json::to_vec(l).map_err(|_| ScopeError::StoreCorrupt)?;
    if b.len() as u64 > max {
        return Err(ScopeError::Budget);
    }
    Ok(b)
}
// Charge future Publication bytes before admitting work. JSON Vec<u8> needs
// at most four encoded bytes per input byte; no account payload is allocated.
// Exact prepared/committed shapes are checked again with the actual owner Unit.
fn reserved_ledger_bytes(l: &Ledger) -> ScopeResult<u64> {
    let mut total = serde_json::to_vec(l)
        .map_err(|_| ScopeError::StoreCorrupt)?
        .len() as u64;
    for (id, entry) in &l.jobs {
        let j = &entry.job;
        if j.phase.terminal() || l.intent.as_ref().is_some_and(|i| i.job_id == *id) {
            continue;
        }
        let mut future = entry.clone();
        future.job.phase = ScopePhase::TailAligning;
        future.job.attempt = u64::MAX;
        future.deadline = u64::MAX;
        future.job.cursor = j.anchor.cursor;
        future.job.restored_accounts = j.dependencies.len() as u64;
        if future.job.baseline_hash.is_empty() {
            future.job.baseline_hash = "0".repeat(64);
        }
        if future.job.buffer_id.is_empty() {
            future.job.buffer_id = "0".repeat(128);
        }
        if future.job.history_gap.is_none() {
            future.job.history_gap = Some(HistoryGap {
                after: u64::MAX,
                through: u64::MAX,
                reason: "HEAD_FIRST_BASELINE".into(),
            });
        }
        let publication = Publication {
            fence: j.fence.clone(),
            anchor: j.anchor.clone(),
            pool_ids: j.pool_ids.clone(),
            dependencies: j.dependencies.clone(),
            accounts: j
                .dependencies
                .iter()
                .map(|d| ScopeAccount {
                    id: d.id.clone(),
                    data: vec![],
                    anchor: j.anchor.clone(),
                })
                .collect(),
            baseline_hash: future.job.baseline_hash.clone(),
            buffer_id: future.job.buffer_id.clone(),
            cursor: j.anchor.cursor,
            history_gap: future.job.history_gap.clone(),
        };
        let reference = SnapshotRef {
            block: xone_domain::BlockRef {
                chain_key: j.fence.chain_key.clone(),
                number_or_slot: j.anchor.height,
                hash: j.anchor.hash.clone(),
                parent_hash: "0".repeat(j.anchor.hash.len().max(64)),
                timestamp_ms: j.anchor.timestamp_ms,
            },
            canonical_epoch: j.fence.canonical_epoch,
            journal_seq: u64::MAX,
            schema_version: u32::MAX,
            adapter_set_hash: j.fence.adapter_version.clone(),
            online_generation: j.fence.generation,
            scope_hash: "0".repeat(64),
            writer_epoch: j.fence.writer_epoch,
            owner_service: j.fence.owner.clone(),
            owner_assignment_version: j.fence.assignment,
        };
        let commit = Commit {
            proof: OnlineScopeCommit {
                job_id: id.clone(),
                fence: j.fence.clone(),
                cursor: j.anchor.cursor,
                commit_id: "0".repeat(64),
            },
            attempt: u64::MAX,
            session: entry.session,
            unit_hash: "0".repeat(64),
            reference,
        };
        let intent = Intent {
            job_id: id.clone(),
            attempt: u64::MAX,
            session: entry.session,
            unit_hash: "0".repeat(64),
            publication: publication.clone(),
        };
        let bytes = |v| {
            serde_json::to_vec(v)
                .map(|b| b.len() as u64)
                .map_err(|_| ScopeError::StoreCorrupt)
        };
        let publication_bytes = serde_json::to_vec(&publication)
            .map_err(|_| ScopeError::StoreCorrupt)?
            .len() as u64;
        let intent_bytes = serde_json::to_vec(&intent)
            .map_err(|_| ScopeError::StoreCorrupt)?
            .len() as u64;
        let commit_bytes = serde_json::to_vec(&commit)
            .map_err(|_| ScopeError::StoreCorrupt)?
            .len() as u64;
        let growth = bytes(&future)?.saturating_sub(bytes(entry)?);
        let extra = intent_bytes
            .max(
                publication_bytes
                    .checked_add(commit_bytes)
                    .ok_or(ScopeError::Budget)?,
            )
            .checked_add(j.reserved_bytes.checked_mul(4).ok_or(ScopeError::Budget)?)
            .ok_or(ScopeError::Budget)?
            .checked_add(growth)
            .and_then(|n| n.checked_add((id.len() as u64).saturating_mul(2) + 32))
            .ok_or(ScopeError::Budget)?;
        total = total.checked_add(extra).ok_or(ScopeError::Budget)?;
    }
    Ok(total)
}

// Retire only audit records with no current/prepared/active/intent authority.
// The caller persists deletions together with its new ledger/owner transaction.
fn trim_ledger(
    db: &Store,
    l: &mut Ledger,
    limits: &FileScopeLimits,
    prepared: Option<&Unit>,
) -> ScopeResult<()> {
    let current = db.persisted_snapshot().map_err(io)?.map(|s| s.reference);
    let retained_prepared = db.prepared().map_err(io)?;
    let prepared_ref = retained_prepared.as_ref().map(|u| &u.snapshot.reference);
    let protected = l
        .jobs
        .iter()
        .filter(|(id, e)| {
            !e.job.phase.terminal()
                || l.intent.as_ref().is_some_and(|i| i.job_id == **id)
                || l.commits.get(*id).is_some_and(|c| {
                    current.as_ref() == Some(&c.reference) || prepared_ref == Some(&c.reference)
                })
        })
        .map(|(id, _)| id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    loop {
        let fits = l.jobs.len() as u64 <= limits.max_retained_jobs
            && reserved_ledger_bytes(l)? <= limits.max_snapshot_bytes
            && match prepared {
                Some(u) if l.intent.is_some() => {
                    reserved_ledger_bytes(&committed_projection(l, u)?)?
                        <= limits.max_snapshot_bytes
                }
                _ => true,
            };
        if fits {
            return Ok(());
        }
        let retired = l
            .jobs
            .iter()
            .filter(|(id, e)| e.job.phase.terminal() && !protected.contains(*id))
            .min_by_key(|(_, e)| (e.job.fence.generation, e.job.anchor.cursor))
            .map(|(id, _)| id.clone())
            .ok_or(ScopeError::Budget)?;
        l.jobs.remove(&retired);
        l.data.remove(&retired);
        l.commits.remove(&retired);
    }
}

fn active(db: &Store, l: &Ledger, f: &Fence, now: u64) -> ScopeResult<()> {
    if db.uncertain {
        return Err(ScopeError::StoreClosed);
    }
    if l.session != db.scope_session || l.fence != *f || now >= l.until {
        return Err(ScopeError::FenceChanged);
    }
    Ok(())
}
fn attempt<'a>(db: &Store, l: &'a Ledger, j: &ScopeJob, now: u64) -> ScopeResult<&'a Entry> {
    active(db, l, &j.fence, now)?;
    let e = l.jobs.get(&j.id).ok_or(ScopeError::StoreConflict)?;
    if e.session != db.scope_session
        || e.job.attempt != j.attempt
        || j.attempt == 0
        || now >= e.deadline
        || !immutable_job(&e.job, j)
        || e.job.phase.terminal()
    {
        return Err(ScopeError::StoreConflict);
    }
    Ok(e)
}
fn capacity(l: &Ledger, except: &str, j: &ScopeJob, limits: &ScopeLimits) -> ScopeResult<()> {
    let mut jobs = 1u64;
    let mut bytes = j.reserved_bytes;
    for (id, e) in &l.jobs {
        if id != except && !e.job.phase.terminal() {
            jobs = jobs.checked_add(1).ok_or(ScopeError::Budget)?;
            bytes = bytes
                .checked_add(e.job.reserved_bytes)
                .ok_or(ScopeError::Budget)?;
        }
    }
    if jobs > limits.max_jobs || bytes > limits.max_account_bytes {
        return Err(ScopeError::Budget);
    }
    Ok(())
}
#[derive(Clone)]
pub struct OwnerScopeStore {
    db: Arc<Mutex<Store>>,
    limits: FileScopeLimits,
    now: Clock,
    check: CheckAuthority,
    bridge: Option<PublishBridge>,
}
impl OwnerScopeStore {
    pub fn new(
        db: Arc<Mutex<Store>>,
        limits: FileScopeLimits,
        now: Clock,
        check: CheckAuthority,
        bridge: Option<PublishBridge>,
    ) -> ScopeResult<Self> {
        if limits.max_jobs == 0
            || limits.max_retained_jobs < limits.max_jobs
            || limits.max_snapshot_bytes == 0
            || limits.attempt_lease_ms == 0
            || limits.max_authority_lease_ms == 0
        {
            return Err(ScopeError::Data);
        }
        db_lock(&db)?.scope_now = now.clone();
        Ok(Self {
            db,
            limits,
            now,
            check,
            bridge,
        })
    }
    pub fn begin(&self, fence: Fence, until: u64) -> ScopeResult<()> {
        (self.check)(&fence)?;
        let now = (self.now)();
        if !crate::scope::valid_fence(&fence)
            || until <= now
            || until - now > self.limits.max_authority_lease_ms
        {
            return Err(ScopeError::Data);
        }
        let mut db = db_lock(&self.db)?;
        if db.prepared().map_err(io)?.is_some() {
            return Err(ScopeError::Storage(
                "OWNER_PREPARED_RECONCILE_REQUIRED".into(),
            ));
        }
        let mut l = match load(&db)? {
            Some(mut old) => {
                if old.intent.is_some()
                    || !fence_advance(&old.fence, &fence)
                    || (old.session != db.scope_session && fence.generation <= old.fence.generation)
                {
                    return Err(ScopeError::FenceChanged);
                }
                for e in old.jobs.values_mut() {
                    if (e.job.fence != fence || e.session != db.scope_session)
                        && !e.job.phase.terminal()
                    {
                        e.job.phase = ScopePhase::Failed;
                        e.job.failure = "OWNER_FENCE_CHANGED".into();
                        e.deadline = 0;
                    }
                }
                old.max_bytes = self.limits.max_snapshot_bytes;
                old.session = db.scope_session;
                old.fence = fence.clone();
                old.until = until;
                old
            }
            None => Ledger {
                version: 1,
                max_bytes: self.limits.max_snapshot_bytes,
                session: db.scope_session,
                fence: fence.clone(),
                until,
                jobs: BTreeMap::new(),
                data: BTreeMap::new(),
                commits: BTreeMap::new(),
                intent: None,
                demands: BTreeMap::new(),
                manifests: BTreeMap::new(),
            },
        };
        self.write(&mut db, &mut l)?;
        drop(db);
        (self.check)(&fence)
    }
    fn write(&self, db: &mut Store, l: &mut Ledger) -> ScopeResult<()> {
        let prepared = db.prepared().map_err(io)?;
        trim_ledger(db, l, &self.limits, prepared.as_ref())?;
        active(db, l, &l.fence, (self.now)())?;
        let mut batch = WriteBatch::default();
        batch.put(KEY, encoded(l, self.limits.max_snapshot_bytes)?);
        db.owner_write(batch).map_err(io)?;
        active(db, l, &l.fence, (self.now)())
    }
    pub fn job(&self, id: &str) -> ScopeResult<ScopeJob> {
        let (f, j) = {
            let db = db_lock(&self.db)?;
            let l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
            active(&db, &l, &l.fence, (self.now)())?;
            let j = l.jobs.get(id).ok_or(ScopeError::StoreConflict)?.job.clone();
            (l.fence, j)
        };
        (self.check)(&f)?;
        if j.phase == ScopePhase::ScopedReady {
            self.ready_job(id)?;
        }
        Ok(j)
    }
    pub fn prepare(
        &self,
        c: &Cancellation,
        j: &ScopeJob,
        p: Publication,
        u: &Unit,
    ) -> ScopeResult<()> {
        c.check()?;
        (self.check)(&j.fence)?;
        let mut db = db_lock(&self.db)?;
        let mut l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
        let e = attempt(&db, &l, j, (self.now)())?;
        crate::scope::require_scope_fresh(j, (self.now)())?;
        if u.normalized_facts.is_some()
            || !persisted_stage(&e.job, j)
            || j.phase != ScopePhase::TailAligning
            || j.cursor != j.anchor.cursor
            || j.restored_accounts != j.dependencies.len() as u64
            || !valid_publication(j, &p)
            || u.kind != "ONLINE_BASELINE_COMMIT"
            || !unit_fence(u, &j.fence)
            || u.snapshot.reference.block.hash != j.anchor.hash
            || u.snapshot.reference.block.number_or_slot != j.anchor.height
            || u.snapshot.reference.block.timestamp_ms != j.anchor.timestamp_ms
            || u.snapshot
                .pools
                .iter()
                .map(|p| p.pool_id.clone())
                .collect::<std::collections::BTreeSet<_>>()
                != j.pool_ids.iter().cloned().collect()
        {
            return Err(ScopeError::StoreConflict);
        }
        db.validate_unit(&u.snapshot, &u.kind).map_err(io)?;
        let unit_hash = hash(u).map_err(io)?;
        if let Some(old) = &l.intent {
            if old.job_id != j.id || old.attempt != j.attempt || old.unit_hash != unit_hash {
                return Err(ScopeError::StoreConflict);
            }
        }
        if let Some(old) = db.prepared().map_err(io)? {
            if hash(&old).map_err(io)? != unit_hash {
                return Err(ScopeError::StoreConflict);
            }
        }
        l.intent = Some(Intent {
            job_id: j.id.clone(),
            attempt: j.attempt,
            session: db.scope_session,
            unit_hash,
            publication: p,
        });
        // Reserve the exact post-commit ledger shape BEFORE external publish.
        // A quota error must not first appear after Kafka has committed.
        trim_ledger(&db, &mut l, &self.limits, Some(u))?;
        let mut batch = WriteBatch::default();
        batch.put(KEY, encoded(&l, self.limits.max_snapshot_bytes)?);
        batch.put(
            b"online/prepared",
            serde_json::to_vec(u).map_err(|_| ScopeError::Data)?,
        );
        c.check()?;
        crate::scope::require_scope_fresh(j, (self.now)())?;
        db.owner_write(batch).map_err(io)?;
        drop(db);
        (self.check)(&j.fence)?;
        crate::scope::require_scope_fresh(j, (self.now)())?;
        c.check()
    }
    pub fn committed_job(&self, id: &str) -> ScopeResult<ScopeJob> {
        self.ready_job(id)
    }
    pub fn database(&self) -> Arc<Mutex<Store>> {
        self.db.clone()
    }
}
fn committed_projection(l: &Ledger, u: &Unit) -> ScopeResult<Ledger> {
    let mut projected = l.clone();
    let j = &l
        .jobs
        .get(&l.intent.as_ref().ok_or(ScopeError::StoreConflict)?.job_id)
        .ok_or(ScopeError::StoreConflict)?
        .job;
    let intent = projected.intent.take().ok_or(ScopeError::StoreConflict)?;
    let entry = projected
        .jobs
        .get_mut(&j.id)
        .ok_or(ScopeError::StoreConflict)?;
    entry.job.phase = ScopePhase::ScopedReady;
    entry.job.cursor = intent.publication.cursor;
    entry.deadline = 0;
    projected.commits.insert(
        j.id.clone(),
        Commit {
            proof: OnlineScopeCommit {
                job_id: j.id.clone(),
                fence: j.fence.clone(),
                cursor: intent.publication.cursor,
                commit_id: "0".repeat(64),
            },
            attempt: j.attempt,
            session: intent.session,
            unit_hash: intent.unit_hash,
            reference: u.snapshot.reference.clone(),
        },
    );
    projected.data.insert(j.id.clone(), intent.publication);
    if let Ok(e) = serde_json::from_str::<crate::service::StreamEvidence>(&u.snapshot.source) {
        let filename = format!(
            "{}-{}-{}.json",
            e.session_id, u.snapshot.reference.block.number_or_slot, e.bank_id
        );
        projected.manifests.insert(
            filename.clone(),
            NativeManifestRef {
                filename,
                blockhash: u.snapshot.reference.block.hash.clone(),
                sha256: e.manifest_sha256,
                bytes: e.manifest_bytes,
            },
        );
    }
    Ok(projected)
}
fn unit_fence(u: &Unit, f: &Fence) -> bool {
    let r = &u.snapshot.reference;
    r.block.chain_key == f.chain_key
        && r.owner_service == f.owner
        && r.owner_assignment_version == f.assignment
        && r.online_generation == f.generation
        && r.canonical_epoch == f.canonical_epoch
        && r.writer_epoch == f.writer_epoch
        && r.adapter_set_hash == f.adapter_version
        && r.schema_version.to_string() == f.schema_version
}
impl ScopeStore for OwnerScopeStore {
    fn current(&self, chain: &str) -> ScopeResult<Fence> {
        let f = {
            let db = db_lock(&self.db)?;
            let l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
            active(&db, &l, &l.fence, (self.now)())?;
            if l.fence.chain_key != chain {
                return Err(ScopeError::FenceChanged);
            }
            l.fence
        };
        (self.check)(&f)?;
        {
            let db = db_lock(&self.db)?;
            let l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
            active(&db, &l, &f, (self.now)())?;
        }
        Ok(f)
    }
    fn admit(&self, a: Admission) -> ScopeResult<ScopeJob> {
        self.current(&a.job.fence.chain_key)?;
        let j = a.job;
        if !valid_job_shape(&j)
            || j.phase != ScopePhase::Admitted
            || j.attempt != 0
            || a.limits != j.budget
        {
            return Err(ScopeError::StoreConflict);
        }
        let mut db = db_lock(&self.db)?;
        let mut l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
        active(&db, &l, &j.fence, (self.now)())?;
        if let Some(e) = l.jobs.get(&j.id) {
            if !same_identity(&e.job, &j) {
                return Err(ScopeError::StoreConflict);
            }
            let result = e.job.clone();
            drop(db);
            if result.phase == ScopePhase::ScopedReady {
                self.ready_job(&j.id)?;
            }
            return Ok(result);
        }
        if j.budget.max_jobs > self.limits.max_jobs
            || j.budget.max_account_bytes > self.limits.max_account_bytes
        {
            return Err(ScopeError::Budget);
        }
        capacity(&l, "", &j, &j.budget)?;
        l.jobs.insert(
            j.id.clone(),
            Entry {
                job: j.clone(),
                session: db.scope_session,
                deadline: 0,
            },
        );
        self.write(&mut db, &mut l)?;
        drop(db);
        self.current(&j.fence.chain_key)?;
        Ok(j)
    }
    fn claim(&self, id: &str, f: &Fence) -> ScopeResult<ScopeJob> {
        self.current(&f.chain_key)?;
        let mut db = db_lock(&self.db)?;
        let mut l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
        active(&db, &l, f, (self.now)())?;
        let mut e = l.jobs.get(id).ok_or(ScopeError::StoreConflict)?.clone();
        if e.job.budget.max_jobs > self.limits.max_jobs
            || e.job.budget.max_account_bytes > self.limits.max_account_bytes
        {
            return Err(ScopeError::Budget);
        }
        if e.job.fence != *f {
            return Err(ScopeError::FenceChanged);
        }
        if e.job.phase == ScopePhase::ScopedReady {
            drop(db);
            return self.ready_job(id);
        }
        if !e.job.phase.terminal()
            && e.job.attempt > 0
            && e.session == db.scope_session
            && e.deadline > (self.now)()
        {
            return Err(ScopeError::Busy);
        }
        capacity(&l, id, &e.job, &e.job.budget)?;
        e.job.attempt = e
            .job
            .attempt
            .checked_add(1)
            .ok_or(ScopeError::StoreConflict)?;
        e.job.phase = ScopePhase::Admitted;
        e.session = db.scope_session;
        e.deadline = (self.now)()
            .saturating_add(self.limits.attempt_lease_ms)
            .min(l.until);
        l.jobs.insert(id.into(), e.clone());
        l.data.remove(id);
        l.commits.remove(id);
        self.write(&mut db, &mut l)?;
        Ok(e.job)
    }
    fn checkpoint(&self, j: &ScopeJob) -> ScopeResult<()> {
        self.current(&j.fence.chain_key)?;
        let mut db = db_lock(&self.db)?;
        let mut l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
        let e = attempt(&db, &l, j, (self.now)())?;
        if !phase_step(e.job.phase, j.phase) || j.restored_accounts > j.dependencies.len() as u64 {
            return Err(ScopeError::StoreConflict);
        }
        let deadline = e.deadline;
        l.jobs.insert(
            j.id.clone(),
            Entry {
                job: j.clone(),
                session: db.scope_session,
                deadline,
            },
        );
        self.write(&mut db, &mut l)
    }
    fn fail(&self, j: &ScopeJob) -> ScopeResult<()> {
        // A cancelled waiter cannot erase a prepared/possibly committed owner intent.
        let mut db = db_lock(&self.db)?;
        let mut l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
        attempt(&db, &l, j, (self.now)())?;
        if l.intent.as_ref().is_some_and(|i| i.job_id == j.id) {
            return Err(ScopeError::Storage(
                "OWNER_PREPARED_RECONCILE_REQUIRED".into(),
            ));
        }
        if !matches!(j.phase, ScopePhase::Failed | ScopePhase::HeadFirstRequired) {
            return Err(ScopeError::StoreConflict);
        }
        l.jobs.insert(
            j.id.clone(),
            Entry {
                job: j.clone(),
                session: db.scope_session,
                deadline: 0,
            },
        );
        l.data.remove(&j.id);
        self.write(&mut db, &mut l)
    }
    fn publish(&self, c: &Cancellation, j: &ScopeJob, p: Publication) -> ScopeResult<()> {
        c.check()?;
        self.current(&j.fence.chain_key)?;
        crate::scope::require_scope_fresh(j, (self.now)())?;
        self.bridge.as_ref().ok_or(ScopeError::StoreConflict)?(c, j, p)?;
        self.online_commit(c, &j.id)?;
        crate::scope::require_scope_fresh(j, (self.now)())?;
        c.check()
    }
}
impl ProductionTransactions for OwnerScopeStore {
    fn online_commit(&self, c: &Cancellation, id: &str) -> ScopeResult<OnlineScopeCommit> {
        c.check()?;
        let job = self.ready_job(id)?;
        (self.check)(&job.fence)?;
        let proof = {
            let db = db_lock(&self.db)?;
            let l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
            let commit = l.commits.get(id).ok_or(ScopeError::StoreConflict)?;
            active(&db, &l, &job.fence, (self.now)())?;
            let actual = db
                .persisted_snapshot()
                .map_err(io)?
                .ok_or(ScopeError::StoreConflict)?;
            if commit.session != db.scope_session
                || commit.attempt != job.attempt
                || actual.reference != commit.reference
                || commit.proof.cursor != job.cursor
            {
                return Err(ScopeError::StoreConflict);
            }
            commit.proof.clone()
        };
        (self.check)(&job.fence)?;
        crate::scope::require_scope_fresh(&job, (self.now)())?;
        c.check()?;
        Ok(proof)
    }
    fn check_authority(&self, f: &Fence) -> ScopeResult<()> {
        (self.check)(f)?;
        self.current(&f.chain_key).and_then(|current| {
            if current == *f {
                Ok(())
            } else {
                Err(ScopeError::FenceChanged)
            }
        })
    }
    fn ready_job(&self, id: &str) -> ScopeResult<ScopeJob> {
        let job = {
            let db = db_lock(&self.db)?;
            let l = load(&db)?.ok_or(ScopeError::StoreConflict)?;
            let e = l.jobs.get(id).ok_or(ScopeError::StoreConflict)?;
            active(&db, &l, &e.job.fence, (self.now)())?;
            if e.session != db.scope_session
                || e.job.phase != ScopePhase::ScopedReady
                || !l.commits.contains_key(id)
            {
                return Err(ScopeError::StoreConflict);
            }
            crate::scope::require_scope_fresh(&e.job, (self.now)())?;
            e.job.clone()
        };
        (self.check)(&job.fence)?;
        crate::scope::require_scope_fresh(&job, (self.now)())?;
        Ok(job)
    }
    fn now_ms(&self) -> u64 {
        (self.now)()
    }
}
// Called by Store::apply, before its ONE synchronous owner WriteBatch.
pub(crate) fn apply_scope_batch(
    db: &Store,
    s: &Snapshot,
    c: Option<&Cursor>,
    kind: &str,
    batch: &mut WriteBatch,
) -> xone_domain::Result<()> {
    let Some(mut l) = load(db).map_err(|e| err(&e.to_string()))? else {
        return Ok(());
    };
    if kind == "ONLINE_SCOPE_REVOKED" {
        for e in l.jobs.values_mut() {
            if e.job.phase == ScopePhase::ScopedReady {
                e.job.phase = ScopePhase::Failed;
                e.job.failure = "ONLINE_SCOPE_REVOKED".into();
            }
        }
        batch.put(
            KEY,
            encoded(&l, l.max_bytes).map_err(|e| err(&e.to_string()))?,
        );
        return Ok(());
    }
    let Some(intent) = l.intent.take() else {
        return Ok(());
    };
    if c.is_none() {
        return Err(err("SCOPE_OWNER_CURSOR_REQUIRED"));
    }
    let u = Unit {
        kind: kind.into(),
        snapshot: s.clone(),
        normalized_facts: None,
    };
    if hash(&u)? != intent.unit_hash {
        return Err(err("SCOPE_PREPARED_UNIT_MISMATCH"));
    }
    let e = l
        .jobs
        .get_mut(&intent.job_id)
        .ok_or_else(|| err("SCOPE_JOB_MISSING"))?;
    if e.job.attempt != intent.attempt
        || e.session != intent.session
        || !unit_fence(&u, &e.job.fence)
    {
        return Err(err("SCOPE_PREPARED_ATTEMPT_MISMATCH"));
    }
    // Reconciliation after reopen records the observed outcome but grants no live
    // authority; same-session normal apply rechecks deadline and original TTL.
    if intent.session == db.scope_session {
        if (db.scope_now)() >= e.deadline {
            return Err(err("SCOPE_ATTEMPT_EXPIRED"));
        }
        active(db, &l, &intent.publication.fence, (db.scope_now)())
            .map_err(|e| err(&e.to_string()))?;
        crate::scope::require_scope_fresh(&l.jobs[&intent.job_id].job, (db.scope_now)())
            .map_err(|e| err(&e.to_string()))?;
    }
    let e = l
        .jobs
        .get_mut(&intent.job_id)
        .ok_or_else(|| err("SCOPE_JOB_MISSING"))?;
    e.job.cursor = intent.publication.cursor;
    e.job.phase = ScopePhase::ScopedReady;
    e.deadline = 0;
    let proof = OnlineScopeCommit {
        job_id: intent.job_id.clone(),
        fence: e.job.fence.clone(),
        cursor: intent.publication.cursor,
        commit_id: hash(&(intent.unit_hash.clone(), c, intent.attempt, intent.session))?,
    };
    let published = s
        .pools
        .iter()
        .map(|p| p.pool_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    l.demands
        .retain(|_, ids| !ids.iter().all(|id| published.contains(id.as_str())));
    if let Ok(e) = serde_json::from_str::<crate::service::StreamEvidence>(&s.source) {
        let filename = format!(
            "{}-{}-{}.json",
            e.session_id, s.reference.block.number_or_slot, e.bank_id
        );
        l.manifests.insert(
            filename.clone(),
            NativeManifestRef {
                filename,
                blockhash: s.reference.block.hash.clone(),
                sha256: e.manifest_sha256,
                bytes: e.manifest_bytes,
            },
        );
    }
    l.commits.insert(
        intent.job_id.clone(),
        Commit {
            proof,
            attempt: intent.attempt,
            session: intent.session,
            unit_hash: intent.unit_hash,
            reference: s.reference.clone(),
        },
    );
    l.data.insert(intent.job_id, intent.publication);
    batch.put(
        KEY,
        encoded(&l, l.max_bytes).map_err(|e| err(&e.to_string()))?,
    );
    Ok(())
}
pub(crate) fn after_apply(db: &Store, s: &Snapshot, kind: &str) -> xone_domain::Result<bool> {
    if kind == "ONLINE_SCOPE_REVOKED" {
        return Ok(false);
    }
    if let Some(l) = load(db).map_err(|e| err(&e.to_string()))? {
        if let Some(e) = l.jobs.values().find(|e| {
            e.job.phase == ScopePhase::ScopedReady
                && e.job.fence.generation == s.reference.online_generation
        }) {
            if e.session != db.scope_session {
                return Ok(false);
            }
            active(db, &l, &e.job.fence, (db.scope_now)()).map_err(|e| err(&e.to_string()))?;
            crate::scope::require_scope_fresh(&e.job, (db.scope_now)())
                .map_err(|e| err(&e.to_string()))?;
        }
    }
    Ok(true)
}
pub(crate) fn discard_scope_intent(db: &mut Store) -> xone_domain::Result<()> {
    if let Some(mut l) = load(db).map_err(|e| err(&e.to_string()))? {
        if let Some(i) = l.intent.take() {
            if let Some(e) = l.jobs.get_mut(&i.job_id) {
                e.job.phase = ScopePhase::Failed;
                e.job.failure = "OWNER_RECONCILED_NOT_COMMITTED".into();
                e.deadline = 0;
            }
            let mut b = WriteBatch::default();
            b.put(
                KEY,
                encoded(&l, l.max_bytes).map_err(|e| err(&e.to_string()))?,
            );
            b.delete(b"online/prepared");
            db.owner_write(b)?;
        }
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct EmbeddedAuthority {
    session: u64,
    fence: Fence,
    until: u64,
}
impl Store {
    pub(crate) fn issue_embedded_scope(
        &mut self,
        f: &Fence,
        until: u64,
    ) -> xone_domain::Result<()> {
        if self.uncertain || self.prepared()?.is_some() {
            return Err(err("OWNER_REOPEN_RECONCILE_REQUIRED"));
        }
        if self
            .persisted_snapshot()?
            .is_some_and(|s| s.reference.online_generation >= f.generation)
        {
            return Err(err("SCOPE_AUTHORITY_GENERATION"));
        }
        let mut b = WriteBatch::default();
        b.put(
            b"scope/embedded-authority",
            serde_json::to_vec(&EmbeddedAuthority {
                session: self.scope_session,
                fence: f.clone(),
                until,
            })
            .map_err(|_| err("STATE_ENCODING"))?,
        );
        self.owner_write(b)
    }
    pub(crate) fn check_embedded_scope(&self, expected: Option<&Fence>) -> xone_domain::Result<()> {
        if self.uncertain {
            return Err(err("OWNER_REOPEN_RECONCILE_REQUIRED"));
        }
        let raw = self
            .db
            .get(b"scope/embedded-authority")
            .map_err(|_| err("SCOPE_AUTHORITY_READ"))?
            .ok_or_else(|| err("SCOPE_AUTHORITY_MISSING"))?;
        let actual: EmbeddedAuthority =
            serde_json::from_slice(&raw).map_err(|_| err("SCOPE_AUTHORITY_CORRUPT"))?;
        if actual.session != self.scope_session
            || (self.scope_now)() >= actual.until
            || expected.is_some_and(|f| *f != actual.fence)
        {
            return Err(err("SCOPE_AUTHORITY_CHANGED"));
        }
        Ok(())
    }
}
impl Store {
    pub(crate) fn check_scope_snapshot(&self, reference: &SnapshotRef) -> xone_domain::Result<()> {
        let l = load(self)
            .map_err(|e| err(&e.to_string()))?
            .ok_or_else(|| err("SCOPE_LEDGER_REQUIRED"))?;
        active(self, &l, &l.fence, (self.scope_now)()).map_err(|e| err(&e.to_string()))?;
        let commit = l
            .commits
            .values()
            .find(|p| p.reference == *reference && p.session == self.scope_session)
            .ok_or_else(|| err("SCOPE_COMMIT_PROOF_MISSING"))?;
        let job = &l
            .jobs
            .get(&commit.proof.job_id)
            .ok_or_else(|| err("SCOPE_JOB_MISSING"))?
            .job;
        if job.fence != l.fence
            || job.phase != ScopePhase::ScopedReady
            || job.attempt != commit.attempt
        {
            return Err(err("SCOPE_COMMIT_ATTEMPT_CHANGED"));
        }
        crate::scope::require_scope_fresh(job, (self.scope_now)()).map_err(|e| err(&e.to_string()))
    }
}
/// Sealed capability minted from the actual committed owner batch, not a cursor
/// supplied by a transport. Fields cannot be constructed outside this crate.
#[derive(Clone, Debug, Serialize)]
pub struct CommittedWal {
    pub(crate) chain: String,
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) wal_generation: u64,
    pub(crate) cursor: crate::stream::WalCursor,
    pub(crate) bank: crate::stream::BankId,
    pub(crate) blockhash: String,
    pub(crate) commit_id: String,
}
impl Store {
    pub fn committed_wal(&self) -> xone_domain::Result<CommittedWal> {
        let snapshot = self
            .persisted_snapshot()?
            .ok_or_else(|| err("SCOPE_COMMIT_PROOF_MISSING"))?;
        self.check_scope_snapshot(&snapshot.reference)?;
        let evidence: crate::service::StreamEvidence = serde_json::from_str(&snapshot.source)
            .map_err(|_| err("SCOPE_RAW_PROVENANCE_REQUIRED"))?;
        let l = load(self)
            .map_err(|e| err(&e.to_string()))?
            .ok_or_else(|| err("SCOPE_LEDGER_REQUIRED"))?;
        let proof = l
            .commits
            .values()
            .find(|c| c.reference == snapshot.reference && c.session == self.scope_session)
            .ok_or_else(|| err("SCOPE_COMMIT_PROOF_MISSING"))?;
        if proof.proof.cursor != evidence.wal_cursor {
            return Err(err("SCOPE_WAL_CURSOR_MISMATCH"));
        }
        Ok(CommittedWal {
            chain: snapshot.reference.block.chain_key,
            source_id: evidence.source_id,
            session_id: evidence.session_id,
            wal_generation: evidence.wal_generation,
            cursor: crate::stream::WalCursor {
                sequence: evidence.wal_cursor,
                record_sha256: evidence.wal_record_sha256,
            },
            bank: crate::stream::BankId {
                slot: snapshot.reference.block.number_or_slot,
                bank_id: evidence.bank_id,
            },
            blockhash: snapshot.reference.block.hash,
            commit_id: proof.proof.commit_id.clone(),
        })
    }
    pub(crate) fn protected_wal_sessions(
        &self,
    ) -> xone_domain::Result<std::collections::BTreeSet<String>> {
        if self.uncertain {
            return Err(err("OWNER_REOPEN_RECONCILE_REQUIRED"));
        }
        let mut sessions = std::collections::BTreeSet::new();
        if let Some(s) = self.persisted_snapshot()? {
            if let Ok(e) = serde_json::from_str::<crate::service::StreamEvidence>(&s.source) {
                sessions.insert(e.session_id);
            }
        }
        if let Some(u) = self.prepared()? {
            if let Ok(e) =
                serde_json::from_str::<crate::service::StreamEvidence>(&u.snapshot.source)
            {
                sessions.insert(e.session_id);
            }
        }
        if let Some(l) = load(self).map_err(|e| err(&e.to_string()))? {
            for entry in l.jobs.values() {
                // Terminal audit rows cannot resume recovery. Current committed
                // and prepared snapshots above independently retain their WAL.
                // An unresolved publish intent remains protected until reconcile.
                if entry.job.phase.terminal()
                    && l.intent.as_ref().is_none_or(|i| i.job_id != entry.job.id)
                {
                    continue;
                }
                if let Some(session) = entry.job.buffer_id.split('/').next() {
                    if !session.is_empty() {
                        sessions.insert(session.into());
                    }
                }
            }
        }
        Ok(sessions)
    }
}

impl Store {
    pub(crate) fn scope_demands(&self) -> xone_domain::Result<Vec<String>> {
        Ok(load(self)
            .map_err(|e| err(&e.to_string()))?
            .map(|l| l.demands.into_values().flatten().collect())
            .unwrap_or_default())
    }
    pub(crate) fn request_scope(
        &mut self,
        ids: &[String],
        expected: u64,
        max_jobs: usize,
    ) -> xone_domain::Result<String> {
        let mut l = load(self)
            .map_err(|e| err(&e.to_string()))?
            .ok_or_else(|| err("SCOPE_LEDGER_REQUIRED"))?;
        if l.fence.generation != expected
            || self
                .previous()
                .is_none_or(|r| r.online_generation != expected)
        {
            return Err(err("POOL_SCOPE_GENERATION_CHANGED"));
        }
        let mut ids = ids.to_vec();
        ids.sort();
        ids.dedup();
        let id = hash(&(expected, &ids))?;
        if l.demands.len() >= max_jobs && !l.demands.contains_key(&id) {
            return Err(err("SCOPE_DEMAND_BUDGET"));
        }
        l.demands.insert(id.clone(), ids);
        let mut batch = WriteBatch::default();
        batch.put(
            KEY,
            encoded(&l, l.max_bytes).map_err(|e| err(&e.to_string()))?,
        );
        self.owner_write(batch)?;
        Ok(id)
    }
}

impl Store {
    pub(crate) fn protected_manifest_hashes(
        &self,
    ) -> xone_domain::Result<std::collections::BTreeSet<String>> {
        if self.uncertain {
            return Err(err("OWNER_REOPEN_RECONCILE_REQUIRED"));
        }
        let mut refs = std::collections::BTreeSet::new();
        if let Some(s) = self.persisted_snapshot()? {
            refs.insert(s.reference.block.hash);
        }
        if let Some(u) = self.prepared()? {
            refs.insert(u.snapshot.reference.block.hash);
        }
        if let Some(l) = load(self).map_err(|e| err(&e.to_string()))? {
            refs.extend(l.jobs.values().map(|e| e.job.anchor.hash.clone()));
            refs.extend(l.manifests.values().map(|m| m.blockhash.clone()));
        }
        Ok(refs)
    }
}
impl Store {
    pub(crate) fn scope_generation_floor(&self) -> xone_domain::Result<u64> {
        let mut floor = load(self)
            .map_err(|e| err(&e.to_string()))?
            .map_or(0, |l| l.fence.generation);
        if let Some(raw) = self
            .db
            .get(b"scope/embedded-authority")
            .map_err(|_| err("SCOPE_AUTHORITY_READ"))?
        {
            let a: EmbeddedAuthority =
                serde_json::from_slice(&raw).map_err(|_| err("SCOPE_AUTHORITY_CORRUPT"))?;
            floor = floor.max(a.fence.generation);
        }
        Ok(floor)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NativeManifestRef {
    pub filename: String,
    pub blockhash: String,
    pub sha256: String,
    pub bytes: u64,
}
impl Store {
    pub(crate) fn active_manifest_refs(&self) -> xone_domain::Result<Vec<NativeManifestRef>> {
        Ok(load(self)
            .map_err(|e| err(&e.to_string()))?
            .map(|l| l.manifests.into_values().collect())
            .unwrap_or_default())
    }
    pub(crate) fn release_manifest_refs(
        &mut self,
        archived: &[NativeManifestRef],
    ) -> xone_domain::Result<()> {
        let Some(mut l) = load(self).map_err(|e| err(&e.to_string()))? else {
            return Ok(());
        };
        for item in archived {
            if l.manifests
                .get(&item.filename)
                .is_some_and(|current| current.sha256 == item.sha256)
            {
                l.manifests.remove(&item.filename);
            }
        }
        let mut batch = WriteBatch::default();
        batch.put(
            KEY,
            encoded(&l, l.max_bytes).map_err(|e| err(&e.to_string()))?,
        );
        self.owner_write(batch)
    }
}
impl Store {
    pub(crate) fn native_committed_slot(&self) -> xone_domain::Result<u64> {
        Ok(load(self)
            .map_err(|e| err(&e.to_string()))?
            .map(|l| {
                l.commits
                    .values()
                    .map(|c| c.reference.block.number_or_slot)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0))
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn byte_and_row_gc_preserve_current_prepared_active_and_intent() {
        let path = std::env::temp_dir().join(format!(
            "xone-ledger-gc-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut db = Store::open(&path, "chain-parser-rust", 1).unwrap();
        let mut snapshot: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json"))
                .unwrap();
        snapshot.reference.block.timestamp_ms = 1000;
        db.publish_baseline(snapshot.clone()).unwrap();
        let mut next = snapshot.clone();
        next.reference.online_generation += 1;
        next.reference.journal_seq += 1;
        let prepared = Unit {
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: next.clone(),
            normalized_facts: None,
        };
        db.prepare(&prepared).unwrap();
        let r = &snapshot.reference;
        let fence = Fence {
            chain_key: r.block.chain_key.clone(),
            chain_identity: "localnet".into(),
            owner: r.owner_service.clone(),
            assignment: r.owner_assignment_version,
            generation: r.online_generation,
            canonical_epoch: r.canonical_epoch,
            writer_epoch: r.writer_epoch,
            adapter_version: r.adapter_set_hash.clone(),
            schema_version: r.schema_version.to_string(),
        };
        let policy = ScopeLimits {
            max_jobs: 10,
            max_account_bytes: 10000,
            max_tail_events: 10,
            max_tail_bytes: 10000,
            max_tail_age_ms: 5000,
            max_baseline_age_ms: 5000,
            shard_accounts: 1,
        };
        let mut l = Ledger {
            version: 1,
            max_bytes: 1_000_000,
            session: db.scope_session,
            fence: fence.clone(),
            until: 5000,
            jobs: BTreeMap::new(),
            data: BTreeMap::new(),
            commits: BTreeMap::new(),
            intent: None,
            demands: BTreeMap::new(),
            manifests: BTreeMap::new(),
        };
        for (name, phase, reference) in [
            (
                "current",
                ScopePhase::ScopedReady,
                snapshot.reference.clone(),
            ),
            ("prepared", ScopePhase::ScopedReady, next.reference.clone()),
            ("active", ScopePhase::Restoring, r.clone()),
            ("intent", ScopePhase::Failed, r.clone()),
            ("retired", ScopePhase::ScopedReady, r.clone()),
        ] {
            let job = ScopeJob {
                id: name.into(),
                fence: fence.clone(),
                anchor: ScopeAnchor {
                    height: r.block.number_or_slot,
                    hash: r.block.hash.clone(),
                    timestamp_ms: 1000,
                    cursor: 1,
                },
                pool_ids: vec!["pool".into()],
                budget: policy.clone(),
                history_after: 0,
                dependencies: vec![ScopeDependency {
                    id: "account".into(),
                    max_bytes: 100,
                }],
                reserved_bytes: 100,
                phase,
                attempt: 1,
                buffer_id: "buffer".into(),
                baseline_hash: "0".repeat(64),
                cursor: 1,
                restored_accounts: 1,
                failure: String::new(),
                history_gap: None,
            };
            let publication = Publication {
                fence: fence.clone(),
                anchor: job.anchor.clone(),
                pool_ids: job.pool_ids.clone(),
                dependencies: job.dependencies.clone(),
                accounts: vec![],
                baseline_hash: job.baseline_hash.clone(),
                buffer_id: job.buffer_id.clone(),
                cursor: 1,
                history_gap: None,
            };
            l.jobs.insert(
                name.into(),
                Entry {
                    job,
                    session: db.scope_session,
                    deadline: 5000,
                },
            );
            l.data.insert(name.into(), publication.clone());
            if name == "current" || name == "prepared" {
                l.commits.insert(
                    name.into(),
                    Commit {
                        proof: OnlineScopeCommit {
                            job_id: name.into(),
                            fence: fence.clone(),
                            cursor: 1,
                            commit_id: "0".repeat(64),
                        },
                        attempt: 1,
                        session: db.scope_session,
                        unit_hash: "0".repeat(64),
                        reference,
                    },
                );
            }
            if name == "intent" {
                l.intent = Some(Intent {
                    job_id: name.into(),
                    attempt: 1,
                    session: db.scope_session,
                    unit_hash: "0".repeat(64),
                    publication,
                });
            }
        }
        let mut limits = FileScopeLimits {
            max_jobs: 4,
            max_account_bytes: 10000,
            max_retained_jobs: 4,
            max_snapshot_bytes: 1_000_000,
            attempt_lease_ms: 1000,
            max_authority_lease_ms: 5000,
        };
        trim_ledger(&db, &mut l, &limits, None).unwrap();
        assert_eq!(l.jobs.len(), 4);
        for id in ["current", "prepared", "active", "intent"] {
            assert!(l.jobs.contains_key(id));
            assert!(l.data.contains_key(id));
        }
        assert!(!l.data.contains_key("retired"));
        assert!(l.commits.contains_key("current") && l.commits.contains_key("prepared"));
        limits.max_retained_jobs = 3;
        assert!(
            matches!(
                trim_ledger(&db, &mut l, &limits, None),
                Err(ScopeError::Budget)
            ),
            "protected authority cannot be removed to make a quota pass"
        );
        assert_eq!(l.jobs.len(), 4);
        drop(db);
        std::fs::remove_dir_all(path).unwrap();
    }
}
