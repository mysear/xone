//! Durable local recovery snapshots. SCOPED_READY means recovery data persisted,
//! never service/router quote readiness. Online DB/prepared publication is a
//! separate owner transaction; two stores cannot pretend to commit atomically.
use super::*;
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

fn io_error(e: std::io::Error) -> ScopeError {
    ScopeError::Storage(e.to_string())
}
fn corrupt<T>() -> ScopeResult<T> {
    Err(ScopeError::StoreCorrupt)
}
fn lock<T>(mutex: &Mutex<T>) -> ScopeResult<MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| ScopeError::StoreClosed)
}
#[derive(Clone, Debug)]
pub struct FileScopeLimits {
    pub max_jobs: u64,
    pub max_account_bytes: u64,
    pub max_retained_jobs: u64,
    pub max_snapshot_bytes: u64,
    pub attempt_lease_ms: u64,
    pub max_authority_lease_ms: u64,
}
#[derive(Clone)]
struct StoredJob {
    job: ScopeJob,
    session: u64,
    deadline_ms: u64,
}
#[derive(Clone)]
struct Disk {
    session: u64,
    fence: Fence,
    jobs: BTreeMap<String, StoredJob>,
    data: BTreeMap<String, Publication>,
}
struct ScopeState {
    file: AtomicFile,
    disk: Disk,
    authority_until: u64,
    fence_floor: Fence, // handle-local accepted fence high-water, including failed writes
}
/// Private local filesystem only. Advisory OS file lock + file fsync + rename +
/// directory fsync. A post-rename I/O error poisons the handle until reopen.
/// No authority or READY is restored from disk. The external verified lease must
/// be projected again through authorize() on EVERY open.
pub struct FileScopeStore {
    state: Mutex<ScopeState>,
    limits: FileScopeLimits,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
}
struct AtomicFile {
    root: PathBuf,
    format: &'static str,
    max: u64,
    lock: Option<File>,
    dir: Option<File>,
    uncertain: bool,
}
fn safe_entry(path: &Path) -> ScopeResult<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ScopeError::StoreUnsafe);
    };
    #[cfg(unix)]
    if metadata.mode() & 0o077 != 0 || metadata.nlink() != 1 {
        return Err(ScopeError::StoreUnsafe);
    };
    Ok(metadata)
}
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
}
impl AtomicFile {
    fn open(root: &Path, format: &'static str, max: u64) -> ScopeResult<(Self, Option<Vec<u8>>)> {
        if !root.is_absolute() || !(256..=64 << 20).contains(&max) {
            return Err(ScopeError::StoreUnsafe);
        }
        let root_created = !root.exists();
        match fs::symlink_metadata(root) {
            Ok(m) => {
                if !m.is_dir() || m.file_type().is_symlink() {
                    return Err(ScopeError::StoreUnsafe);
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(io_error(e));
                }
            }
        }
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(root).map_err(io_error)?;
        let root = fs::canonicalize(root).map_err(io_error)?;
        #[cfg(unix)]
        if fs::metadata(&root).map_err(io_error)?.mode() & 0o077 != 0 {
            return Err(ScopeError::StoreUnsafe);
        }
        #[cfg(not(unix))]
        return Err(ScopeError::StoreUnsafe);
        // Persist creation of recursive directory entries, not only state.json.
        if root_created {
            for parent in root.ancestors().skip(1) {
                File::open(parent)
                    .map_err(io_error)?
                    .sync_all()
                    .map_err(io_error)?;
            }
        }
        let lock_path = root.join("lock");
        match fs::symlink_metadata(&lock_path) {
            Ok(_) => {
                safe_entry(&lock_path)?;
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(io_error(e));
                }
            }
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options.mode(0o600);
        let lock = options.open(&lock_path).map_err(io_error)?;
        if !same_file(
            &safe_entry(&lock_path)?,
            &lock.metadata().map_err(io_error)?,
        ) {
            return Err(ScopeError::StoreUnsafe);
        };
        lock.try_lock().map_err(|_| ScopeError::StoreLocked)?;
        let dir = File::open(&root).map_err(io_error)?;
        let mut file = Self {
            root,
            format,
            max,
            lock: Some(lock),
            dir: Some(dir),
            uncertain: false,
        };
        let pending = file.root.join("pending");
        match fs::symlink_metadata(&pending) {
            Ok(_) => {
                safe_entry(&pending)?;
                fs::remove_file(&pending).map_err(io_error)?;
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(io_error(e));
                }
            }
        }
        let path = file.root.join("state.json");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(_) => safe_entry(&path)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((file, None)),
            Err(e) => return Err(io_error(e)),
        };
        if metadata.len() > max {
            return corrupt();
        }
        let input = File::open(&path).map_err(io_error)?;
        if !same_file(&metadata, &input.metadata().map_err(io_error)?) {
            return Err(ScopeError::StoreUnsafe);
        };
        let mut bytes = vec![];
        input
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() as u64 > max {
            return corrupt();
        };
        let envelope: Value =
            serde_json::from_slice(&bytes).map_err(|_| ScopeError::StoreCorrupt)?;
        let payload = string(&envelope, "payload")?;
        if string(&envelope, "format")? != format
            || string(&envelope, "checksum")? != account_hash(payload.as_bytes())
        {
            return corrupt();
        };
        file.uncertain = false;
        Ok((file, Some(payload.into_bytes())))
    }
    fn write(&mut self, value: &Value, gate: impl Fn() -> ScopeResult<()>) -> ScopeResult<()> {
        if self.lock.is_none() || self.uncertain {
            return Err(ScopeError::StoreClosed);
        };
        let payload = serde_json::to_string(value).map_err(|_| ScopeError::StoreCorrupt)?;
        let envelope = json!({"format":self.format,"checksum":account_hash(payload.as_bytes()),"payload":payload});
        let bytes = serde_json::to_vec(&envelope).map_err(|_| ScopeError::StoreCorrupt)?;
        if bytes.len() as u64 > self.max {
            return Err(ScopeError::Budget);
        }
        let pending = self.root.join("pending");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut out = options.open(&pending).map_err(io_error)?;
        let result = (|| {
            out.write_all(&bytes).map_err(io_error)?;
            out.sync_all().map_err(io_error)?;
            drop(out);
            gate()?;
            fs::rename(&pending, self.root.join("state.json")).map_err(io_error)?;
            if let Err(e) = self.dir.as_ref().ok_or(ScopeError::StoreClosed)?.sync_all() {
                self.uncertain = true;
                return Err(io_error(e));
            };
            Ok(())
        })();
        let _ = fs::remove_file(pending);
        result
    }
    fn close(&mut self) {
        self.lock.take();
        self.dir.take();
    }
    fn available(&self) -> ScopeResult<()> {
        if self.lock.is_none() || self.uncertain {
            Err(ScopeError::StoreClosed)
        } else {
            Ok(())
        }
    }
}
pub(crate) fn fence_advance(old: &Fence, next: &Fence) -> bool {
    // Versions are opaque identities, not ordered strings. Replacing either
    // requires a strictly newer numeric fence; no counter may move backwards.
    let same_versions =
        old.adapter_version == next.adapter_version && old.schema_version == next.schema_version;
    let numeric_advance = next.assignment > old.assignment
        || next.generation > old.generation
        || next.canonical_epoch > old.canonical_epoch
        || next.writer_epoch > old.writer_epoch;
    old.chain_key == next.chain_key
        && old.chain_identity == next.chain_identity
        && next.assignment >= old.assignment
        && next.generation >= old.generation
        && next.canonical_epoch >= old.canonical_epoch
        && next.writer_epoch >= old.writer_epoch
        && (old.owner == next.owner || next.assignment > old.assignment)
        && (same_versions || numeric_advance)
}
fn live(s: &ScopeState, now: u64, f: &Fence) -> ScopeResult<()> {
    s.file.available()?;
    if s.disk.fence != *f || s.authority_until <= now {
        Err(ScopeError::FenceChanged)
    } else {
        Ok(())
    }
}
fn lease_gate(
    now: &Arc<dyn Fn() -> u64 + Send + Sync>,
    until: u64,
    attempt: Option<u64>,
    cancel: Option<&Cancellation>,
) -> ScopeResult<()> {
    if let Some(c) = cancel {
        c.check()?
    };
    let time = now();
    if until <= time {
        return Err(ScopeError::FenceChanged);
    };
    if attempt.is_some_and(|a| a <= time) {
        return Err(ScopeError::StoreConflict);
    };
    Ok(())
}

pub(crate) fn valid_job_shape(j: &ScopeJob) -> bool {
    if !valid_fence(&j.fence)
        || !valid_limits(&j.budget)
        || j.anchor.hash.is_empty()
        || j.pool_ids.is_empty()
        || j.dependencies.is_empty()
        || canonical_pools(&j.pool_ids) != j.pool_ids
        || scope_job_id(j) != j.id
    {
        return false;
    }
    let mut sum = 0u64;
    for (i, d) in j.dependencies.iter().enumerate() {
        if d.id.is_empty() || d.max_bytes == 0 || (i > 0 && j.dependencies[i - 1].id >= d.id) {
            return false;
        };
        let Some(next) = sum.checked_add(d.max_bytes) else {
            return false;
        };
        sum = next;
        if sum > j.budget.max_account_bytes {
            return false;
        }
    }
    sum == j.reserved_bytes
}
pub(crate) fn persisted_stage(a: &ScopeJob, b: &ScopeJob) -> bool {
    a.buffer_id == b.buffer_id
        && a.baseline_hash == b.baseline_hash
        && a.restored_accounts == b.restored_accounts
        && a.history_gap == b.history_gap
        && a.failure == b.failure
        && b.failure.is_empty()
        && a.cursor <= b.cursor
}

pub(crate) fn same_identity(a: &ScopeJob, b: &ScopeJob) -> bool {
    a.fence == b.fence
        && a.anchor == b.anchor
        && a.pool_ids == b.pool_ids
        && a.dependencies == b.dependencies
        && a.reserved_bytes == b.reserved_bytes
}
pub(crate) fn immutable_job(a: &ScopeJob, b: &ScopeJob) -> bool {
    same_identity(a, b)
        && a.id == b.id
        && a.budget == b.budget
        && a.history_after == b.history_after
}
pub(crate) fn phase_step(old: ScopePhase, next: ScopePhase) -> bool {
    matches!(
        (old, next),
        (ScopePhase::Admitted, ScopePhase::LiveBuffering)
            | (ScopePhase::LiveBuffering, ScopePhase::BaselineAcquiring)
            | (ScopePhase::BaselineAcquiring, ScopePhase::Restoring)
            | (
                ScopePhase::Restoring,
                ScopePhase::Restoring | ScopePhase::TailAligning
            )
    )
}
pub(crate) fn valid_publication(j: &ScopeJob, p: &Publication) -> bool {
    p.fence == j.fence
        && p.anchor == j.anchor
        && p.pool_ids == j.pool_ids
        && p.dependencies == j.dependencies
        && p.cursor == p.anchor.cursor
        && p.baseline_hash == j.baseline_hash
        && p.buffer_id == j.buffer_id
        && p.baseline_hash.len() == 64
        && !p.buffer_id.is_empty()
        && p.history_gap == j.history_gap
        && validate_accounts(&p.accounts, &j.dependencies, &p.anchor, None)
            .is_ok_and(|n| n <= j.budget.max_account_bytes)
}
impl FileScopeStore {
    pub fn open(
        root: &Path,
        fence: Fence,
        limits: FileScopeLimits,
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> ScopeResult<Self> {
        if !valid_fence(&fence)
            || limits.max_jobs == 0
            || limits.max_account_bytes == 0
            || limits.max_retained_jobs < limits.max_jobs
            || limits.max_retained_jobs > 100000
            || limits.attempt_lease_ms == 0
            || limits.max_authority_lease_ms == 0
        {
            return Err(ScopeError::Data);
        }
        let (mut file, payload) =
            AtomicFile::open(root, "xone-online-recovery-v1", limits.max_snapshot_bytes)?;
        let mut disk = match payload {
            None => Disk {
                session: 0,
                fence: fence.clone(),
                jobs: BTreeMap::new(),
                data: BTreeMap::new(),
            },
            Some(bytes) => {
                parse_disk(&serde_json::from_slice(&bytes).map_err(|_| ScopeError::StoreCorrupt)?)?
            }
        };
        if !fence_advance(&disk.fence, &fence) {
            return Err(ScopeError::FenceChanged);
        };
        if disk.jobs.len() as u64 > limits.max_retained_jobs {
            return corrupt();
        };
        for (id, e) in &disk.jobs {
            if id != &e.job.id || !valid_job_shape(&e.job) {
                return corrupt();
            }
        }
        for (id, p) in &disk.data {
            if !disk
                .jobs
                .get(id)
                .is_some_and(|e| valid_publication(&e.job, p))
            {
                return corrupt();
            }
        }
        disk.session = disk
            .session
            .checked_add(1)
            .ok_or(ScopeError::StoreCorrupt)?;
        disk.fence = fence;
        for entry in disk.jobs.values_mut() {
            entry.job.phase = ScopePhase::Failed;
            entry.job.failure = "RESTART_REVALIDATION_REQUIRED".into();
            entry.deadline_ms = 0
        }
        file.write(&disk_value(&disk), || Ok(()))?;
        Ok(Self {
            state: Mutex::new(ScopeState {
                file,
                fence_floor: disk.fence.clone(),
                disk,
                authority_until: 0,
            }),
            limits,
            now,
        })
    }
    /// The caller must project its externally verified PG/Kafka owner authority.
    /// This store does not acquire an external lease. Reopen always clears it.
    pub fn authorize(&self, cancel: &Cancellation, fence: Fence, until: u64) -> ScopeResult<()> {
        let mut s = lock(&self.state)?;
        if !valid_fence(&fence) || !fence_advance(&s.fence_floor, &fence) {
            return Err(ScopeError::FenceChanged);
        };
        let gate = || {
            cancel.check()?;
            let now = (self.now)();
            if until <= now || until - now > self.limits.max_authority_lease_ms {
                Err(ScopeError::FenceChanged)
            } else {
                Ok(())
            }
        };
        gate()?;
        // Accepting a replacement invalidates the old handle BEFORE fallible
        // serialization/fsync/rename. Its floor survives persistence failure.
        if fence != s.disk.fence {
            s.authority_until = 0;
            s.fence_floor = fence.clone();
        }
        let mut next = s.disk.clone();
        if fence != next.fence {
            next.fence = fence;
            next.session = next
                .session
                .checked_add(1)
                .ok_or(ScopeError::StoreConflict)?;
            for entry in next.jobs.values_mut() {
                entry.job.phase = ScopePhase::Failed;
                entry.job.failure = "FENCE_CHANGED".into();
                entry.deadline_ms = 0
            }
        };
        s.file.write(&disk_value(&next), gate)?;
        s.disk = next;
        s.authority_until = until;
        Ok(())
    }
    fn capacity(&self, d: &Disk, except: &str, j: &ScopeJob, b: &ScopeLimits) -> ScopeResult<()> {
        let max_jobs = b.max_jobs.min(self.limits.max_jobs);
        let max_bytes = b.max_account_bytes.min(self.limits.max_account_bytes);
        let mut jobs = 0u64;
        let mut bytes = 0u64;
        for (id, e) in &d.jobs {
            if id == except || e.job.phase.terminal() {
                continue;
            };
            jobs += 1;
            bytes = bytes
                .checked_add(e.job.reserved_bytes)
                .ok_or(ScopeError::Budget)?
        }
        if jobs >= max_jobs || bytes > max_bytes || j.reserved_bytes > max_bytes - bytes {
            return Err(ScopeError::Budget);
        };
        Ok(())
    }
    fn attempt(&self, s: &ScopeState, j: &ScopeJob) -> ScopeResult<StoredJob> {
        live(s, (self.now)(), &j.fence)?;
        let e = s.disk.jobs.get(&j.id).ok_or(ScopeError::StoreConflict)?;
        if e.session != s.disk.session
            || e.job.attempt == 0
            || e.job.attempt != j.attempt
            || e.deadline_ms <= (self.now)()
            || e.job.phase.terminal()
            || !immutable_job(&e.job, j)
        {
            return Err(ScopeError::StoreConflict);
        };
        Ok(e.clone())
    }
    pub fn readiness_scope(&self) -> &'static str {
        "RECOVERY_DATA_ONLY"
    }
    pub fn job(&self, id: &str) -> ScopeResult<ScopeJob> {
        let s = lock(&self.state)?;
        s.file.available()?;
        let result = s
            .disk
            .jobs
            .get(id)
            .ok_or(ScopeError::StoreConflict)?
            .job
            .clone();
        if result.phase == ScopePhase::ScopedReady {
            live(&s, (self.now)(), &result.fence)?;
            require_scope_fresh(&result, (self.now)())?;
        }
        Ok(result)
    }
    /// Recovery output only. Never an online-readiness or quote capability.
    pub fn recovery_data(&self, id: &str) -> ScopeResult<Publication> {
        let s = lock(&self.state)?;
        live(&s, (self.now)(), &s.disk.fence)?;
        let e = s.disk.jobs.get(id).ok_or(ScopeError::StoreConflict)?;
        if e.session != s.disk.session || e.job.phase != ScopePhase::ScopedReady {
            return Err(ScopeError::StoreConflict);
        };
        let result = s
            .disk
            .data
            .get(id)
            .cloned()
            .ok_or(ScopeError::StoreConflict)?;
        live(&s, (self.now)(), &e.job.fence)?;
        require_scope_fresh(&e.job, (self.now)())?;
        Ok(result)
    }
    pub fn close(&self) -> ScopeResult<()> {
        lock(&self.state)?.file.close();
        Ok(())
    }
}
impl ScopeStore for FileScopeStore {
    fn current(&self, chain: &str) -> ScopeResult<Fence> {
        let s = lock(&self.state)?;
        if chain != s.disk.fence.chain_key {
            return Err(ScopeError::FenceChanged);
        };
        live(&s, (self.now)(), &s.disk.fence)?;
        Ok(s.disk.fence.clone())
    }
    fn admit(&self, a: Admission) -> ScopeResult<ScopeJob> {
        let mut s = lock(&self.state)?;
        let j = a.job;
        live(&s, (self.now)(), &j.fence)?;
        if j.phase != ScopePhase::Admitted
            || j.attempt != 0
            || !valid_job_shape(&j)
            || a.limits != j.budget
        {
            return Err(ScopeError::StoreConflict);
        };
        if let Some(e) = s.disk.jobs.get(&j.id) {
            if !same_identity(&e.job, &j) {
                return Err(ScopeError::StoreConflict);
            };
            let result = e.job.clone();
            if result.phase == ScopePhase::ScopedReady {
                live(&s, (self.now)(), &result.fence)?;
                require_scope_fresh(&result, (self.now)())?;
            }
            return Ok(result);
        };
        if s.disk.jobs.len() as u64 >= self.limits.max_retained_jobs {
            return Err(ScopeError::Budget);
        };
        self.capacity(&s.disk, "", &j, &a.limits)?;
        let mut next = s.disk.clone();
        next.jobs.insert(
            j.id.clone(),
            StoredJob {
                job: j.clone(),
                session: next.session,
                deadline_ms: 0,
            },
        );
        let until = s.authority_until;
        s.file.write(&disk_value(&next), || {
            lease_gate(&self.now, until, None, None)
        })?;
        s.disk = next;
        Ok(j)
    }
    fn claim(&self, id: &str, fence: &Fence) -> ScopeResult<ScopeJob> {
        let mut s = lock(&self.state)?;
        live(&s, (self.now)(), fence)?;
        let mut e = s
            .disk
            .jobs
            .get(id)
            .ok_or(ScopeError::StoreConflict)?
            .clone();
        if e.job.fence != *fence {
            return Err(ScopeError::StoreConflict);
        };
        if e.job.phase == ScopePhase::ScopedReady && e.session == s.disk.session {
            live(&s, (self.now)(), &e.job.fence)?;
            require_scope_fresh(&e.job, (self.now)())?;
            return Ok(e.job);
        };
        if !e.job.phase.terminal()
            && e.job.attempt > 0
            && e.session == s.disk.session
            && e.deadline_ms > (self.now)()
        {
            return Err(ScopeError::Busy);
        };
        self.capacity(&s.disk, id, &e.job, &e.job.budget)?;
        e.job.attempt = e
            .job
            .attempt
            .checked_add(1)
            .ok_or(ScopeError::StoreConflict)?;
        e.job.phase = ScopePhase::Admitted;
        e.session = s.disk.session;
        e.deadline_ms = (self.now)()
            .checked_add(self.limits.attempt_lease_ms)
            .ok_or(ScopeError::StoreConflict)?
            .min(s.authority_until);
        let mut next = s.disk.clone();
        next.jobs.insert(id.into(), e.clone());
        next.data.remove(id);
        let until = s.authority_until;
        s.file.write(&disk_value(&next), || {
            lease_gate(&self.now, until, None, None)
        })?;
        s.disk = next;
        Ok(e.job)
    }
    fn checkpoint(&self, j: &ScopeJob) -> ScopeResult<()> {
        let mut s = lock(&self.state)?;
        let mut e = self.attempt(&s, j)?;
        if !phase_step(e.job.phase, j.phase) || j.restored_accounts > j.dependencies.len() as u64 {
            return Err(ScopeError::StoreConflict);
        };
        e.job = j.clone();
        let deadline = e.deadline_ms;
        let mut next = s.disk.clone();
        next.jobs.insert(j.id.clone(), e);
        let until = s.authority_until;
        s.file.write(&disk_value(&next), || {
            lease_gate(&self.now, until, Some(deadline), None)
        })?;
        s.disk = next;
        Ok(())
    }
    fn fail(&self, j: &ScopeJob) -> ScopeResult<()> {
        let mut s = lock(&self.state)?;
        let mut e = self.attempt(&s, j)?;
        if !matches!(j.phase, ScopePhase::Failed | ScopePhase::HeadFirstRequired) {
            return Err(ScopeError::StoreConflict);
        };
        let deadline = e.deadline_ms;
        e.job = j.clone();
        e.deadline_ms = 0;
        let mut next = s.disk.clone();
        next.jobs.insert(j.id.clone(), e);
        next.data.remove(&j.id);
        let until = s.authority_until;
        s.file.write(&disk_value(&next), || {
            lease_gate(&self.now, until, Some(deadline), None)
        })?;
        s.disk = next;
        Ok(())
    }
    fn publish(&self, cancel: &Cancellation, j: &ScopeJob, p: Publication) -> ScopeResult<()> {
        let mut s = lock(&self.state)?;
        cancel.check()?;
        let mut e = self.attempt(&s, j)?;
        require_scope_fresh(j, (self.now)())?;
        if !persisted_stage(&e.job, j)
            || e.job.phase != ScopePhase::TailAligning
            || j.phase != ScopePhase::TailAligning
            || j.cursor != j.anchor.cursor
            || j.restored_accounts != j.dependencies.len() as u64
            || !valid_publication(j, &p)
        {
            return Err(ScopeError::StoreConflict);
        };
        let deadline = e.deadline_ms;
        e.job = j.clone();
        e.job.phase = ScopePhase::ScopedReady;
        e.deadline_ms = 0;
        let mut next = s.disk.clone();
        next.jobs.insert(j.id.clone(), e);
        next.data.insert(j.id.clone(), p);
        let until = s.authority_until;
        s.file.write(&disk_value(&next), || {
            lease_gate(&self.now, until, Some(deadline), Some(cancel))?;
            require_scope_fresh(j, (self.now)())
        })?;
        s.disk = next;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FileHistoryLimits {
    pub max_records: u64,
    pub max_bytes: u64,
    pub max_snapshot_bytes: u64,
}
struct HistoryState {
    file: AtomicFile,
    chain: String,
    records: Vec<HistoryRecord>,
}
/// Separate root, mutex, lock and snapshot from online recovery. No online API.
/// A try-lock bounds this writer to one active history transaction, with no queue.
pub struct FileHistoryStore {
    state: Mutex<HistoryState>,
    limits: FileHistoryLimits,
}
impl FileHistoryStore {
    pub fn open(root: &Path, chain: String, limits: FileHistoryLimits) -> ScopeResult<Self> {
        if chain.is_empty()
            || limits.max_records == 0
            || limits.max_records > 100000
            || limits.max_bytes == 0
        {
            return Err(ScopeError::Data);
        };
        let (mut file, payload) =
            AtomicFile::open(root, "xone-history-replay-v1", limits.max_snapshot_bytes)?;
        let records = match payload {
            None => vec![],
            Some(bytes) => {
                let v: Value =
                    serde_json::from_slice(&bytes).map_err(|_| ScopeError::StoreCorrupt)?;
                if string(&v, "chain")? != chain {
                    return corrupt();
                };
                array(&v, "records")?
                    .iter()
                    .map(parse_history)
                    .collect::<ScopeResult<Vec<_>>>()?
            }
        };
        if records.len() as u64 > limits.max_records {
            return corrupt();
        };
        let mut bytes = 0u64;
        let mut seen = BTreeSet::new();
        for r in &records {
            if r.chain_key != chain
                || r.namespace != "history_replay"
                || r.payload.is_empty()
                || !seen.insert(r.cursor)
            {
                return corrupt();
            };
            bytes = bytes
                .checked_add(r.payload.len() as u64)
                .ok_or(ScopeError::StoreCorrupt)?;
            if bytes > limits.max_bytes {
                return corrupt();
            }
        }
        file.write(&history_disk_value(&chain, &records), || Ok(()))?;
        Ok(Self {
            state: Mutex::new(HistoryState {
                file,
                chain,
                records,
            }),
            limits,
        })
    }
    pub fn records(&self) -> ScopeResult<Vec<HistoryRecord>> {
        let s = lock(&self.state)?;
        s.file.available()?;
        Ok(s.records.clone())
    }
    pub fn close(&self) -> ScopeResult<()> {
        lock(&self.state)?.file.close();
        Ok(())
    }
}
impl HistoryStore for FileHistoryStore {
    fn append_history(&self, r: HistoryRecord, b: &HistoryLimits) -> ScopeResult<()> {
        let mut s = match self.state.try_lock() {
            Ok(s) => s,
            Err(std::sync::TryLockError::WouldBlock) => return Err(ScopeError::Busy),
            Err(_) => return Err(ScopeError::StoreClosed),
        };
        s.file.available()?;
        if r.chain_key != s.chain || r.namespace != "history_replay" {
            return Err(ScopeError::HistoryNamespace);
        };
        if b.max_jobs == 0 || r.payload.is_empty() || r.payload.len() as u64 > b.max_bytes {
            return Err(ScopeError::Budget);
        };
        let mut bytes = 0u64;
        for old in &s.records {
            if old.cursor == r.cursor {
                return if old.payload == r.payload {
                    Ok(())
                } else {
                    Err(ScopeError::StoreConflict)
                };
            };
            bytes = bytes
                .checked_add(old.payload.len() as u64)
                .ok_or(ScopeError::Budget)?
        }
        let limit = b.max_bytes.min(self.limits.max_bytes);
        if s.records.len() as u64 >= self.limits.max_records
            || bytes > limit
            || r.payload.len() as u64 > limit - bytes
        {
            return Err(ScopeError::Budget);
        };
        let mut next = s.records.clone();
        next.push(r);
        let value = history_disk_value(&s.chain, &next);
        s.file.write(&value, || Ok(()))?;
        s.records = next;
        Ok(())
    }
}

// Explicit JSON codecs avoid adding a serde dependency to this crate. Decode is
// typed and bounded by AtomicFile's input cap; missing/malformed fields fail closed.
fn string(v: &Value, key: &str) -> ScopeResult<String> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .ok_or(ScopeError::StoreCorrupt)
}
fn number(v: &Value, key: &str) -> ScopeResult<u64> {
    v[key].as_u64().ok_or(ScopeError::StoreCorrupt)
}
fn array<'a>(v: &'a Value, key: &str) -> ScopeResult<&'a Vec<Value>> {
    v[key].as_array().ok_or(ScopeError::StoreCorrupt)
}
fn strings(v: &Value, key: &str) -> ScopeResult<Vec<String>> {
    array(v, key)?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or(ScopeError::StoreCorrupt)
        })
        .collect()
}
fn fence_value(f: &Fence) -> Value {
    json!({"chain_key":f.chain_key,"chain_identity":f.chain_identity,"owner":f.owner,"assignment":f.assignment,"online_generation":f.generation,"canonical_epoch":f.canonical_epoch,"writer_epoch":f.writer_epoch,"adapter_version":f.adapter_version,"schema_version":f.schema_version})
}
fn parse_fence(v: &Value) -> ScopeResult<Fence> {
    let f = Fence {
        chain_key: string(v, "chain_key")?,
        chain_identity: string(v, "chain_identity")?,
        owner: string(v, "owner")?,
        assignment: number(v, "assignment")?,
        generation: number(v, "online_generation")?,
        canonical_epoch: number(v, "canonical_epoch")?,
        writer_epoch: number(v, "writer_epoch")?,
        adapter_version: string(v, "adapter_version")?,
        schema_version: string(v, "schema_version")?,
    };
    if !valid_fence(&f) {
        return corrupt();
    };
    Ok(f)
}
fn anchor_value(a: &ScopeAnchor) -> Value {
    json!({"height":a.height,"hash":a.hash,"timestamp_ms":a.timestamp_ms,"cursor":a.cursor})
}
fn parse_anchor(v: &Value) -> ScopeResult<ScopeAnchor> {
    Ok(ScopeAnchor {
        height: number(v, "height")?,
        hash: string(v, "hash")?,
        timestamp_ms: number(v, "timestamp_ms")?,
        cursor: number(v, "cursor")?,
    })
}
fn limits_value(b: &ScopeLimits) -> Value {
    json!({"max_jobs":b.max_jobs,"max_account_bytes":b.max_account_bytes,"max_tail_events":b.max_tail_events,"max_tail_bytes":b.max_tail_bytes,"max_tail_age_ms":b.max_tail_age_ms,"max_baseline_age_ms":b.max_baseline_age_ms,"shard_accounts":b.shard_accounts})
}
fn parse_limits(v: &Value) -> ScopeResult<ScopeLimits> {
    let b = ScopeLimits {
        max_jobs: number(v, "max_jobs")?,
        max_account_bytes: number(v, "max_account_bytes")?,
        max_tail_events: number(v, "max_tail_events")?,
        max_tail_bytes: number(v, "max_tail_bytes")?,
        max_tail_age_ms: number(v, "max_tail_age_ms")?,
        max_baseline_age_ms: number(v, "max_baseline_age_ms")?,
        shard_accounts: number(v, "shard_accounts")?,
    };
    if !valid_limits(&b) {
        return corrupt();
    };
    Ok(b)
}
fn deps_value(deps: &[ScopeDependency]) -> Value {
    Value::Array(
        deps.iter()
            .map(|d| json!({"id":d.id,"max_bytes":d.max_bytes}))
            .collect(),
    )
}
fn parse_deps(v: &Value) -> ScopeResult<Vec<ScopeDependency>> {
    v.as_array()
        .ok_or(ScopeError::StoreCorrupt)?
        .iter()
        .map(|d| {
            Ok(ScopeDependency {
                id: string(d, "id")?,
                max_bytes: number(d, "max_bytes")?,
            })
        })
        .collect()
}
fn gap_value(g: &Option<HistoryGap>) -> Value {
    g.as_ref().map_or(
        Value::Null,
        |g| json!({"after":g.after,"through":g.through,"reason":g.reason}),
    )
}
fn parse_gap(v: &Value) -> ScopeResult<Option<HistoryGap>> {
    if v.is_null() {
        return Ok(None);
    };
    Ok(Some(HistoryGap {
        after: number(v, "after")?,
        through: number(v, "through")?,
        reason: string(v, "reason")?,
    }))
}
fn parse_phase(v: &Value) -> ScopeResult<ScopePhase> {
    match v.as_str() {
        Some("ADMITTED") => Ok(ScopePhase::Admitted),
        Some("LIVE_BUFFERING") => Ok(ScopePhase::LiveBuffering),
        Some("BASELINE_ACQUIRING") => Ok(ScopePhase::BaselineAcquiring),
        Some("RESTORING") => Ok(ScopePhase::Restoring),
        Some("TAIL_ALIGNING") => Ok(ScopePhase::TailAligning),
        Some("SCOPED_READY") => Ok(ScopePhase::ScopedReady),
        Some("FAILED") => Ok(ScopePhase::Failed),
        Some("HEAD_FIRST_REQUIRED") => Ok(ScopePhase::HeadFirstRequired),
        _ => corrupt(),
    }
}
fn job_value(j: &ScopeJob) -> Value {
    json!({"id":j.id,"fence":fence_value(&j.fence),"anchor":anchor_value(&j.anchor),"pool_ids":j.pool_ids,"budget":limits_value(&j.budget),"history_after":j.history_after,"dependencies":deps_value(&j.dependencies),"reserved_bytes":j.reserved_bytes,"phase":j.phase.as_str(),"attempt":j.attempt,"buffer_id":j.buffer_id,"baseline_hash":j.baseline_hash,"cursor":j.cursor,"restored_accounts":j.restored_accounts,"failure":j.failure,"history_gap":gap_value(&j.history_gap)})
}
fn parse_job(v: &Value) -> ScopeResult<ScopeJob> {
    Ok(ScopeJob {
        id: string(v, "id")?,
        fence: parse_fence(&v["fence"])?,
        anchor: parse_anchor(&v["anchor"])?,
        pool_ids: strings(v, "pool_ids")?,
        budget: parse_limits(&v["budget"])?,
        history_after: number(v, "history_after")?,
        dependencies: parse_deps(&v["dependencies"])?,
        reserved_bytes: number(v, "reserved_bytes")?,
        phase: parse_phase(&v["phase"])?,
        attempt: number(v, "attempt")?,
        buffer_id: string(v, "buffer_id")?,
        baseline_hash: string(v, "baseline_hash")?,
        cursor: number(v, "cursor")?,
        restored_accounts: number(v, "restored_accounts")?,
        failure: string(v, "failure")?,
        history_gap: parse_gap(&v["history_gap"])?,
    })
}
fn account_value(a: &ScopeAccount) -> Value {
    json!({"id":a.id,"data":a.data,"anchor":anchor_value(&a.anchor)})
}
fn parse_bytes(v: &Value) -> ScopeResult<Vec<u8>> {
    v.as_array()
        .ok_or(ScopeError::StoreCorrupt)?
        .iter()
        .map(|n| {
            n.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or(ScopeError::StoreCorrupt)
        })
        .collect()
}
fn parse_account(v: &Value) -> ScopeResult<ScopeAccount> {
    Ok(ScopeAccount {
        id: string(v, "id")?,
        data: parse_bytes(&v["data"])?,
        anchor: parse_anchor(&v["anchor"])?,
    })
}
fn publication_value(p: &Publication) -> Value {
    json!({"fence":fence_value(&p.fence),"anchor":anchor_value(&p.anchor),"pool_ids":p.pool_ids,"dependencies":deps_value(&p.dependencies),"accounts":p.accounts.iter().map(account_value).collect::<Vec<_>>(),"baseline_hash":p.baseline_hash,"buffer_id":p.buffer_id,"cursor":p.cursor,"history_gap":gap_value(&p.history_gap)})
}
fn parse_publication(v: &Value) -> ScopeResult<Publication> {
    Ok(Publication {
        fence: parse_fence(&v["fence"])?,
        anchor: parse_anchor(&v["anchor"])?,
        pool_ids: strings(v, "pool_ids")?,
        dependencies: parse_deps(&v["dependencies"])?,
        accounts: array(v, "accounts")?
            .iter()
            .map(parse_account)
            .collect::<ScopeResult<_>>()?,
        baseline_hash: string(v, "baseline_hash")?,
        buffer_id: string(v, "buffer_id")?,
        cursor: number(v, "cursor")?,
        history_gap: parse_gap(&v["history_gap"])?,
    })
}
fn disk_value(d: &Disk) -> Value {
    let jobs: serde_json::Map<String, Value> = d
        .jobs
        .iter()
        .map(|(id, e)| {
            (
                id.clone(),
                json!({"job":job_value(&e.job),"session":e.session,"deadline_ms":e.deadline_ms}),
            )
        })
        .collect();
    let data: serde_json::Map<String, Value> = d
        .data
        .iter()
        .map(|(id, p)| (id.clone(), publication_value(p)))
        .collect();
    json!({"session":d.session,"fence":fence_value(&d.fence),"jobs":jobs,"data":data})
}
fn parse_disk(v: &Value) -> ScopeResult<Disk> {
    let jobs = v["jobs"]
        .as_object()
        .ok_or(ScopeError::StoreCorrupt)?
        .iter()
        .map(|(id, e)| {
            Ok((
                id.clone(),
                StoredJob {
                    job: parse_job(&e["job"])?,
                    session: number(e, "session")?,
                    deadline_ms: number(e, "deadline_ms")?,
                },
            ))
        })
        .collect::<ScopeResult<_>>()?;
    let data = v["data"]
        .as_object()
        .ok_or(ScopeError::StoreCorrupt)?
        .iter()
        .map(|(id, p)| Ok((id.clone(), parse_publication(p)?)))
        .collect::<ScopeResult<_>>()?;
    Ok(Disk {
        session: number(v, "session")?,
        fence: parse_fence(&v["fence"])?,
        jobs,
        data,
    })
}
fn history_value(r: &HistoryRecord) -> Value {
    json!({"chain_key":r.chain_key,"namespace":r.namespace,"cursor":r.cursor,"payload":r.payload})
}
fn parse_history(v: &Value) -> ScopeResult<HistoryRecord> {
    Ok(HistoryRecord {
        chain_key: string(v, "chain_key")?,
        namespace: string(v, "namespace")?,
        cursor: number(v, "cursor")?,
        payload: parse_bytes(&v["payload"])?,
    })
}
fn history_disk_value(chain: &str, records: &[HistoryRecord]) -> Value {
    json!({"chain":chain,"records":records.iter().map(history_value).collect::<Vec<_>>()})
}

/// Metadata from the owner DB's committed journal. Router quote readiness still
/// requires applying this commit to the local immutable view.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct OnlineScopeCommit {
    pub job_id: String,
    pub fence: Fence,
    pub cursor: u64,
    pub commit_id: String,
}
/// Implement on the EXISTING online DB/prepared owner adapter. Its publish must
/// include job/attempt/fence/cursor, complete pool state and owner journal in ONE
/// transaction. FileScopeStore intentionally does not implement this capability.
pub trait ProductionTransactions: ScopeStore {
    /// Independent live owner check, never a cached lease deadline.
    fn check_authority(&self, _fence: &Fence) -> ScopeResult<()> {
        Err(ScopeError::FenceChanged)
    }
    fn ready_job(&self, _id: &str) -> ScopeResult<ScopeJob> {
        Err(ScopeError::StoreConflict)
    }
    fn now_ms(&self) -> u64 {
        xone_observability::now_ms()
    }
    fn online_commit(&self, cancel: &Cancellation, id: &str) -> ScopeResult<OnlineScopeCommit>;
}
/// Concrete forwarding adapter: all operations use ONE authoritative backend,
/// with no hidden file-then-DB writes. The service supplies its DB implementation.
pub struct ProductionStore {
    backend: Arc<dyn ProductionTransactions>,
}
impl ProductionStore {
    pub fn new(backend: Arc<dyn ProductionTransactions>) -> Self {
        Self { backend }
    }
    pub fn readiness_scope(&self) -> &'static str {
        "ONLINE_OWNER_COMMIT"
    }
    pub fn online_commit(&self, cancel: &Cancellation, id: &str) -> ScopeResult<OnlineScopeCommit> {
        let before = self.backend.ready_job(id)?;
        require_scope_fresh(&before, self.backend.now_ms())?;
        self.backend.check_authority(&before.fence)?;
        let proof = self.backend.online_commit(cancel, id)?;
        if proof.job_id != id || proof.commit_id.is_empty() || !valid_fence(&proof.fence) {
            return Err(ScopeError::StoreConflict);
        };
        if self.backend.current(&proof.fence.chain_key)? != proof.fence {
            return Err(ScopeError::FenceChanged);
        };
        self.backend.check_authority(&proof.fence)?;
        let after = self.backend.ready_job(id)?;
        if after.fence != proof.fence
            || after.cursor != proof.cursor
            || after.attempt != before.attempt
        {
            return Err(ScopeError::StoreConflict);
        }
        require_scope_fresh(&after, self.backend.now_ms())?;
        cancel.check()?;
        Ok(proof)
    }
}
impl ScopeStore for ProductionStore {
    fn current(&self, c: &str) -> ScopeResult<Fence> {
        self.backend.current(c)
    }
    fn admit(&self, a: Admission) -> ScopeResult<ScopeJob> {
        self.backend.admit(a)
    }
    fn claim(&self, id: &str, f: &Fence) -> ScopeResult<ScopeJob> {
        let j = self.backend.claim(id, f)?;
        if j.phase == ScopePhase::ScopedReady {
            let proof = self.online_commit(&Cancellation::new(), id)?;
            if proof.fence != j.fence || proof.cursor != j.cursor {
                return Err(ScopeError::StoreConflict);
            }
        }
        Ok(j)
    }
    fn checkpoint(&self, j: &ScopeJob) -> ScopeResult<()> {
        self.backend.checkpoint(j)
    }
    fn fail(&self, j: &ScopeJob) -> ScopeResult<()> {
        self.backend.fail(j)
    }
    fn publish(&self, c: &Cancellation, j: &ScopeJob, p: Publication) -> ScopeResult<()> {
        if !valid_publication(j, &p) {
            return Err(ScopeError::StoreConflict);
        };
        self.backend.publish(c, j, p)?;
        self.online_commit(c, &j.id)?;
        require_scope_fresh(j, self.backend.now_ms())
    }
}
