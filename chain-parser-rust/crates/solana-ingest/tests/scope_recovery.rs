use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use xone_solana_ingest::scope::*;

// Transactional test double only; disk/crash durability is a service-store contract.
#[derive(Default)]
struct State {
    current: Option<Fence>,
    jobs: BTreeMap<String, ScopeJob>,
    published: Option<Publication>,
    events: Vec<String>,
    fail_publish: bool,
    fail_at: Option<ScopePhase>,
}
#[derive(Default)]
struct Store(Mutex<State>);
impl ScopeStore for Store {
    fn current(&self, _: &str) -> ScopeResult<Fence> {
        Ok(self.0.lock().unwrap().current.clone().unwrap())
    }
    fn admit(&self, a: Admission) -> ScopeResult<ScopeJob> {
        let mut s = self.0.lock().unwrap();
        if let Some(j) = s.jobs.get(&a.job.id) {
            return Ok(j.clone());
        };
        let active: Vec<_> = s.jobs.values().filter(|j| !j.phase.terminal()).collect();
        let bytes: u64 = active.iter().map(|j| j.reserved_bytes).sum();
        if active.len() as u64 >= a.limits.max_jobs
            || a.job.reserved_bytes > a.limits.max_account_bytes - bytes
        {
            return Err(ScopeError::Budget);
        };
        s.jobs.insert(a.job.id.clone(), a.job.clone());
        Ok(a.job)
    }
    fn claim(&self, id: &str, _: &Fence) -> ScopeResult<ScopeJob> {
        let mut s = self.0.lock().unwrap();
        let j = s.jobs.get_mut(id).unwrap();
        if j.phase == ScopePhase::ScopedReady {
            return Ok(j.clone());
        };
        if j.attempt > 0 && !j.phase.terminal() {
            return Err(ScopeError::Busy);
        };
        j.attempt += 1;
        j.phase = ScopePhase::Admitted;
        Ok(j.clone())
    }
    fn checkpoint(&self, j: &ScopeJob) -> ScopeResult<()> {
        let mut s = self.0.lock().unwrap();
        s.events.push(format!("{:?}", j.phase));
        if s.fail_at == Some(j.phase) {
            return Err(ScopeError::Storage("disk".into()));
        }
        s.jobs.insert(j.id.clone(), j.clone());
        Ok(())
    }
    fn fail(&self, j: &ScopeJob) -> ScopeResult<()> {
        self.0.lock().unwrap().jobs.insert(j.id.clone(), j.clone());
        Ok(())
    }
    fn publish(&self, c: &Cancellation, j: &ScopeJob, p: Publication) -> ScopeResult<()> {
        let mut s = self.0.lock().unwrap();
        c.check()?;
        if s.current.as_ref() != Some(&j.fence) {
            return Err(ScopeError::FenceChanged);
        };
        if s.fail_publish {
            return Err(ScopeError::Storage("disk".into()));
        };
        let mut ready = j.clone();
        ready.phase = ScopePhase::ScopedReady;
        s.jobs.insert(j.id.clone(), ready);
        s.published = Some(p);
        Ok(())
    }
}
struct Source {
    store: Arc<Store>,
    candidates: Vec<BaselineCandidate>,
    data: BTreeMap<String, Vec<u8>>,
    order: Vec<String>,
    bad_shard: bool,
    gap: bool,
    cancel: bool,
    epoch: bool,
    tail_events: Option<u64>,
    first_cursor: u64,
    durable: bool,
    tail_kind: Option<&'static str>,
}
impl RecoverySource for Source {
    fn start_buffer(&mut self, _: &Cancellation, _: &ScopeJob) -> ScopeResult<StreamBuffer> {
        self.order.push("buffer".into());
        Ok(StreamBuffer {
            id: "wal-session".into(),
            first_cursor: self.first_cursor,
            durable: self.durable,
        })
    }
    fn fix_anchor(
        &mut self,
        _: &Cancellation,
        j: &ScopeJob,
        _: &StreamBuffer,
    ) -> ScopeResult<ScopeAnchor> {
        self.order.push("anchor".into());
        Ok(j.anchor.clone())
    }
    fn candidates(
        &mut self,
        _: &Cancellation,
        _: &ScopeJob,
        _: &ScopeAnchor,
    ) -> ScopeResult<Vec<BaselineCandidate>> {
        Ok(self.candidates.clone())
    }
    fn restore_shard(
        &mut self,
        _: &Cancellation,
        _: &ScopeJob,
        b: &BaselineCandidate,
        ids: &[String],
    ) -> ScopeResult<Vec<ScopeAccount>> {
        self.order.push("shard".into());
        let mut out: Vec<_> = ids
            .iter()
            .map(|id| ScopeAccount {
                id: id.clone(),
                data: self.data[id].clone(),
                anchor: b.anchor.clone(),
            })
            .collect();
        if self.bad_shard {
            out[0].id = "alien".into()
        };
        Ok(out)
    }
    fn align_tail(
        &mut self,
        c: &Cancellation,
        j: &ScopeJob,
        _: &StreamBuffer,
        b: &BaselineCandidate,
        _: &[ScopeAccount],
    ) -> ScopeResult<ScopeTail> {
        self.order.push("tail".into());
        if self.cancel {
            c.cancel()
        };
        if self.epoch {
            self.store
                .0
                .lock()
                .unwrap()
                .current
                .as_mut()
                .unwrap()
                .canonical_epoch += 1
        };
        let mut tail = ScopeTail {
            from: b.anchor.clone(),
            to: j.anchor.clone(),
            complete: !self.gap,
            events: self.tail_events.unwrap_or(1),
            bytes: 2,
            accounts: j
                .dependencies
                .iter()
                .map(|d| ScopeAccount {
                    id: d.id.clone(),
                    data: self.data[&d.id].clone(),
                    anchor: j.anchor.clone(),
                })
                .collect(),
        };
        match self.tail_kind {
            Some("bytes") => tail.bytes = 9,
            Some("anchor") => tail.to.hash = "fork".into(),
            Some("account_bytes") => tail.accounts[0].data = vec![1, 2, 3],
            Some("zero_substitute") => tail.accounts[0].data = vec![99],
            _ => (),
        }
        Ok(tail)
    }
    fn release_buffer(&mut self, _: &StreamBuffer) -> ScopeResult<()> {
        self.order.push("release".into());
        Ok(())
    }
}
fn now() -> u64 {
    1000
}
fn fixture() -> (
    ScopeController,
    Arc<Store>,
    Source,
    EnsurePoolScopeRequest,
    ScopeConfig,
) {
    let fence = Fence {
        chain_key: "sol".into(),
        chain_identity: "genesis".into(),
        owner: "owner".into(),
        assignment: 1,
        generation: 2,
        canonical_epoch: 3,
        writer_epoch: 4,
        adapter_version: "a1".into(),
        schema_version: "s1".into(),
    };
    let limits = ScopeLimits {
        max_jobs: 2,
        max_account_bytes: 8,
        max_tail_events: 2,
        max_tail_bytes: 8,
        max_tail_age_ms: 50,
        max_baseline_age_ms: 100,
        shard_accounts: 1,
    };
    let deps = vec![
        ScopeDependency {
            id: "a".into(),
            max_bytes: 2,
        },
        ScopeDependency {
            id: "b".into(),
            max_bytes: 2,
        },
    ];
    let config = ScopeConfig {
        fence: fence.clone(),
        limits: limits.clone(),
        catalog: BTreeMap::from([
            ("p".into(), deps.clone()),
            (
                "q".into(),
                vec![ScopeDependency {
                    id: "c".into(),
                    max_bytes: 2,
                }],
            ),
        ]),
        trusted_sources: vec!["trusted".into()],
    };
    let store = Arc::new(Store::default());
    store.0.lock().unwrap().current = Some(fence.clone());
    let controller = ScopeController::new(config.clone(), store.clone(), now).unwrap();
    let req = EnsurePoolScopeRequest {
        fence: fence.clone(),
        anchor: ScopeAnchor {
            height: 11,
            hash: "h11".into(),
            timestamp_ms: 990,
            cursor: 11,
        },
        pool_ids: vec!["p".into()],
        budget: limits,
        history_after: 1,
    };
    let data = BTreeMap::from([("a".into(), vec![1]), ("b".into(), vec![2])]);
    let mut b = BaselineCandidate {
        fence,
        anchor: ScopeAnchor {
            height: 10,
            hash: "h10".into(),
            timestamp_ms: 980,
            cursor: 10,
        },
        source: "trusted".into(),
        kind: BaselineKind::HealthyPeer,
        pool_ids: vec!["p".into()],
        dependencies: deps,
        account_hashes: BTreeMap::from([
            ("a".into(), account_hash(&[1])),
            ("b".into(), account_hash(&[2])),
        ]),
        content_hash: String::new(),
    };
    b.content_hash = baseline_hash(&b);
    let source = Source {
        store: store.clone(),
        candidates: vec![b],
        data,
        order: vec![],
        bad_shard: false,
        gap: false,
        cancel: false,
        epoch: false,
        tail_events: None,
        first_cursor: 11,
        durable: true,
        tail_kind: None,
    };
    (controller, store, source, req, config)
}
#[test]
fn admission_is_idempotent_bounded_and_current() {
    let (c, s, _, mut r, _) = fixture();
    let token = Cancellation::new();
    let a = c.ensure_pool_scope(&token, r.clone()).unwrap();
    r.pool_ids.push("p".into());
    assert_eq!(a.id, c.ensure_pool_scope(&token, r.clone()).unwrap().id);
    assert_eq!(s.0.lock().unwrap().jobs.len(), 1);
    r.pool_ids = vec!["unknown".into()];
    assert_eq!(
        c.ensure_pool_scope(&token, r.clone()).unwrap_err(),
        ScopeError::UnknownPool
    );
    r.pool_ids = vec!["p".into()];
    r.budget.max_account_bytes = 3;
    assert_eq!(
        c.ensure_pool_scope(&token, r.clone()).unwrap_err(),
        ScopeError::Budget
    );
    r.budget.max_account_bytes = 8;
    r.fence.generation += 1;
    assert_eq!(
        c.ensure_pool_scope(&token, r).unwrap_err(),
        ScopeError::FenceChanged
    )
}
#[test]
fn recovery_buffers_restores_and_publishes_the_entire_scope() {
    let (c, s, mut source, r, _) = fixture();
    let token = Cancellation::new();
    let job = c.ensure_pool_scope(&token, r.clone()).unwrap();
    c.recover(&token, &job.id, &mut source).unwrap();
    assert_eq!(
        source.order,
        vec!["buffer", "anchor", "shard", "shard", "tail", "release"]
    );
    let state = s.0.lock().unwrap();
    assert_eq!(state.jobs[&job.id].phase, ScopePhase::ScopedReady);
    let p = state.published.as_ref().unwrap();
    assert_eq!(p.fence, r.fence);
    assert_eq!(p.accounts.len(), 2);
    assert_eq!(p.history_gap.as_ref().unwrap().after, 1);
    drop(state);
    c.recover(&token, &job.id, &mut source).unwrap();
    assert_eq!(source.order.len(), 6)
}
#[test]
fn failures_cancellation_and_epoch_changes_never_publish() {
    for kind in ["shard", "hash", "gap", "cancel", "epoch", "publish"] {
        let (c, s, mut src, r, _) = fixture();
        let token = Cancellation::new();
        match kind {
            "shard" => src.bad_shard = true,
            "hash" => {
                src.data.insert("a".into(), vec![9]);
            }
            "gap" => src.gap = true,
            "cancel" => src.cancel = true,
            "epoch" => src.epoch = true,
            "publish" => s.0.lock().unwrap().fail_publish = true,
            _ => unreachable!(),
        };
        let j = c.ensure_pool_scope(&token, r).unwrap();
        assert!(c.recover(&token, &j.id, &mut src).is_err(), "{kind}");
        let state = s.0.lock().unwrap();
        assert!(state.published.is_none(), "{kind}");
        assert_ne!(state.jobs[&j.id].phase, ScopePhase::ScopedReady);
        if kind == "gap" {
            assert_eq!(state.jobs[&j.id].phase, ScopePhase::HeadFirstRequired);
            assert!(state.jobs[&j.id].history_gap.is_some())
        }
    }
}
#[test]
fn baseline_requires_complete_identity_version_scope_age_and_hash() {
    let (c, _, src, r, cfg) = fixture();
    let token = Cancellation::new();
    let j = c.ensure_pool_scope(&token, r).unwrap();
    for kind in [
        "identity",
        "version",
        "scope",
        "age",
        "hash",
        "untrusted",
        "dependency",
    ] {
        let mut b = src.candidates[0].clone();
        match kind {
            "identity" => b.fence.chain_identity = "fork".into(),
            "version" => b.fence.adapter_version = "old".into(),
            "scope" => b.pool_ids = vec!["q".into()],
            "age" => b.anchor.timestamp_ms = 100,
            "hash" => b.content_hash = "bad".into(),
            "untrusted" => b.source = "unknown".into(),
            "dependency" => {
                b.dependencies.pop();
            }
            _ => unreachable!(),
        };
        if kind != "hash" {
            b.content_hash = baseline_hash(&b)
        };
        assert_eq!(
            select_scope_baseline(&j, &[b], &cfg, 1000).unwrap_err(),
            ScopeError::NoBaseline,
            "{kind}"
        )
    }
}
#[derive(Default)]
struct History(Mutex<Vec<HistoryRecord>>);
impl HistoryStore for History {
    fn append_history(&self, r: HistoryRecord, _: &HistoryLimits) -> ScopeResult<()> {
        self.0.lock().unwrap().push(r);
        Ok(())
    }
}
#[test]
fn history_has_its_own_capability_namespace_and_budget() {
    let sink = Arc::new(History::default());
    let writer = HistoryWriter::new(
        sink.clone(),
        "sol".into(),
        "cap-secret".into(),
        HistoryLimits {
            max_jobs: 1,
            max_bytes: 3,
        },
    )
    .unwrap();
    let token = Cancellation::new();
    let mut r = HistoryRecord {
        chain_key: "sol".into(),
        namespace: "history_replay".into(),
        cursor: 2,
        payload: vec![1],
    };
    assert_eq!(
        writer.write(&token, "wrong", r.clone()),
        Err(ScopeError::HistoryCapability)
    );
    r.namespace = "online".into();
    assert_eq!(
        writer.write(&token, "cap-secret", r.clone()),
        Err(ScopeError::HistoryNamespace)
    );
    r.namespace = "history_replay".into();
    r.payload = vec![1, 2, 3, 4];
    assert_eq!(
        writer.write(&token, "cap-secret", r.clone()),
        Err(ScopeError::Budget)
    );
    r.payload = vec![1];
    writer.write(&token, "cap-secret", r).unwrap();
    assert_eq!(sink.0.lock().unwrap().len(), 1);
}

#[test]
fn corrupted_persisted_job_cannot_shrink_catalog_dependencies() {
    let (c, s, mut src, r, _) = fixture();
    let cancel = Cancellation::new();
    let mut j = c.ensure_pool_scope(&cancel, r).unwrap();
    j.dependencies.truncate(1);
    j.reserved_bytes = 2;
    s.0.lock().unwrap().jobs.insert(j.id.clone(), j.clone());
    src.candidates[0].dependencies = j.dependencies.clone();
    src.candidates[0].account_hashes.remove("b");
    src.candidates[0].content_hash = baseline_hash(&src.candidates[0]);
    assert_eq!(c.recover(&cancel, &j.id, &mut src), Err(ScopeError::Data));
    assert!(s.0.lock().unwrap().published.is_none());
}
#[test]
fn tail_must_cover_cursor_interval() {
    let (c, s, mut src, r, _) = fixture();
    let cancel = Cancellation::new();
    src.tail_events = Some(0);
    let j = c.ensure_pool_scope(&cancel, r).unwrap();
    assert_eq!(
        c.recover(&cancel, &j.id, &mut src),
        Err(ScopeError::HeadFirst)
    );
    assert!(s.0.lock().unwrap().published.is_none());
}

#[test]
fn buffer_shard_tail_and_transaction_bounds_fail_closed() {
    for kind in [
        "durable",
        "retention",
        "events",
        "bytes",
        "anchor",
        "account_bytes",
        "checkpoint",
    ] {
        let (c, s, mut src, r, _) = fixture();
        let cancel = Cancellation::new();
        match kind {
            "durable" => src.durable = false,
            "retention" => src.first_cursor = 30,
            "events" => src.tail_events = Some(3),
            "checkpoint" => s.0.lock().unwrap().fail_at = Some(ScopePhase::Restoring),
            _ => src.tail_kind = Some(kind),
        }
        let j = c.ensure_pool_scope(&cancel, r).unwrap();
        assert!(c.recover(&cancel, &j.id, &mut src).is_err(), "{kind}");
        assert!(s.0.lock().unwrap().published.is_none(), "{kind}");
    }
}
#[test]
fn admission_concurrently_enforces_aggregate_quota() {
    let (c, s, _, r, _) = fixture();
    let controller = Arc::new(c);
    let mut threads = vec![];
    for i in 0..16 {
        let c = controller.clone();
        let mut req = r.clone();
        req.anchor.hash = format!("h{i}");
        threads.push(std::thread::spawn(move || {
            c.ensure_pool_scope(&Cancellation::new(), req)
        }))
    }
    let mut accepted = 0;
    for worker in threads {
        match worker.join().unwrap() {
            Ok(_) => accepted += 1,
            Err(e) => assert_eq!(e, ScopeError::Budget),
        }
    }
    assert_eq!(accepted, 2);
    assert_eq!(s.0.lock().unwrap().jobs.len(), 2);
}
#[test]
fn reclaimed_attempt_restarts_buffer_and_all_shards() {
    let (c, s, mut src, r, _) = fixture();
    let cancel = Cancellation::new();
    let mut j = c.ensure_pool_scope(&cancel, r).unwrap();
    // Store-authorized expired lease reclamation; not evidence of disk durability.
    j.attempt = 1;
    j.phase = ScopePhase::Failed;
    j.cursor = 999;
    j.restored_accounts = 2;
    j.buffer_id = "expired".into();
    s.0.lock().unwrap().jobs.insert(j.id.clone(), j.clone());
    c.recover(&cancel, &j.id, &mut src).unwrap();
    let state = s.0.lock().unwrap();
    assert_eq!(state.jobs[&j.id].attempt, 2);
    assert_eq!(src.order[0], "buffer");
    assert_ne!(state.published.as_ref().unwrap().buffer_id, "expired");
}
#[test]
fn cross_language_manifest_golden() {
    let (c, _, src, r, _) = fixture();
    let j = c.ensure_pool_scope(&Cancellation::new(), r).unwrap();
    assert_eq!(
        j.id,
        "cf093763d6a226c410f7676a3635fe8dfac2df9f436d1658773250dbb2f61b48"
    );
    assert_eq!(
        src.candidates[0].content_hash,
        "464b2026484bfc692874127d6b8631d71bf432fbcd9abf04ac6bcfa8e13691ce"
    );
}
#[test]
fn baseline_prefers_healthy_peer_then_newest_and_shared_dependencies_count_once() {
    let (c, _, src, r, mut cfg) = fixture();
    let j = c
        .ensure_pool_scope(&Cancellation::new(), r.clone())
        .unwrap();
    let peer = src.candidates[0].clone();
    let mut current = peer.clone();
    current.kind = BaselineKind::CurrentChainState;
    current.anchor = r.anchor.clone();
    current.content_hash = baseline_hash(&current);
    assert_eq!(
        select_scope_baseline(&j, &[current, peer.clone()], &cfg, 1000)
            .unwrap()
            .kind,
        BaselineKind::HealthyPeer
    );
    let mut newer = peer.clone();
    newer.anchor = r.anchor.clone();
    newer.content_hash = baseline_hash(&newer);
    assert_eq!(
        select_scope_baseline(&j, &[peer, newer], &cfg, 1000)
            .unwrap()
            .anchor,
        r.anchor
    );
    cfg.catalog.insert(
        "q".into(),
        vec![ScopeDependency {
            id: "b".into(),
            max_bytes: 2,
        }],
    );
    let store = Arc::new(Store::default());
    store.0.lock().unwrap().current = Some(cfg.fence.clone());
    let controller = ScopeController::new(cfg, store, now).unwrap();
    let mut req = r;
    req.pool_ids = vec!["q".into(), "p".into()];
    let job = controller
        .ensure_pool_scope(&Cancellation::new(), req)
        .unwrap();
    assert_eq!(job.reserved_bytes, 4);
    assert_eq!(job.dependencies.len(), 2);
}

struct TempRoot(std::path::PathBuf);
impl TempRoot {
    fn new() -> Self {
        static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "xone-scope-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_ROOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        Self(path)
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn file_fixture() -> (
    ScopeController,
    Arc<FileScopeStore>,
    Source,
    EnsurePoolScopeRequest,
    Arc<std::sync::atomic::AtomicU64>,
    TempRoot,
    FileScopeLimits,
) {
    let (_, _, src, req, cfg) = fixture();
    let clock = Arc::new(std::sync::atomic::AtomicU64::new(1000));
    let c = clock.clone();
    let path = TempRoot::new();
    let limits = FileScopeLimits {
        max_jobs: 2,
        max_account_bytes: 8,
        max_retained_jobs: 8,
        max_snapshot_bytes: 1 << 20,
        attempt_lease_ms: 10,
        max_authority_lease_ms: 100,
    };
    let store = Arc::new(
        FileScopeStore::open(
            &path.0,
            req.fence.clone(),
            limits.clone(),
            Arc::new(move || c.load(std::sync::atomic::Ordering::SeqCst)),
        )
        .unwrap(),
    );
    assert_eq!(store.current("sol"), Err(ScopeError::FenceChanged));
    store
        .authorize(&Cancellation::new(), req.fence.clone(), 1100)
        .unwrap();
    let controller = ScopeController::new(cfg, store.clone(), now).unwrap();
    (controller, store, src, req, clock, path, limits)
}
#[test]
fn file_store_restart_requires_fresh_authority_and_complete_recovery() {
    let (controller, store, mut src, req, clock, root, limits) = file_fixture();
    let cancel = Cancellation::new();
    let j = controller.ensure_pool_scope(&cancel, req.clone()).unwrap();
    controller.recover(&cancel, &j.id, &mut src).unwrap();
    assert_eq!(store.job(&j.id).unwrap().phase, ScopePhase::ScopedReady);
    assert_eq!(store.recovery_data(&j.id).unwrap().accounts.len(), 2);
    store.close().unwrap();
    let reopened = Arc::new(
        FileScopeStore::open(
            &root.0,
            req.fence.clone(),
            limits,
            Arc::new(move || clock.load(std::sync::atomic::Ordering::SeqCst)),
        )
        .unwrap(),
    );
    let saved = reopened.job(&j.id).unwrap();
    assert_ne!(saved.phase, ScopePhase::ScopedReady);
    assert_eq!(saved.failure, "RESTART_REVALIDATION_REQUIRED");
    assert!(reopened.recovery_data(&j.id).is_err());
    assert_eq!(reopened.current("sol"), Err(ScopeError::FenceChanged));
    reopened.authorize(&cancel, req.fence, 1100).unwrap();
    let (_, _, _, _, config) = fixture();
    let c = ScopeController::new(config, reopened.clone(), now).unwrap();
    c.recover(&cancel, &j.id, &mut src).unwrap();
    assert_eq!(src.order.len(), 12);
    reopened.close().unwrap();
}
#[test]
fn file_store_lock_checksum_and_symlink_boundaries() {
    let (_, store, _, req, _, root, limits) = file_fixture();
    assert!(matches!(
        FileScopeStore::open(&root.0, req.fence.clone(), limits.clone(), Arc::new(now)),
        Err(ScopeError::StoreLocked)
    ));
    store.close().unwrap();
    let file = root.0.join("state.json");
    let mut bytes = std::fs::read(&file).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 1;
    std::fs::write(&file, bytes).unwrap();
    assert!(matches!(
        FileScopeStore::open(&root.0, req.fence.clone(), limits.clone(), Arc::new(now)),
        Err(ScopeError::StoreCorrupt)
    ));
    #[cfg(unix)]
    {
        let link = TempRoot::new();
        std::os::unix::fs::symlink(&root.0, &link.0).unwrap();
        assert!(matches!(
            FileScopeStore::open(&link.0, req.fence, limits, Arc::new(now)),
            Err(ScopeError::StoreUnsafe)
        ));
        std::fs::remove_file(&link.0).unwrap();
    }
}
#[test]
fn file_store_attempt_cas_and_expiry() {
    let (c, s, _, r, clock, _root, _) = file_fixture();
    let cancel = Cancellation::new();
    let j = c.ensure_pool_scope(&cancel, r.clone()).unwrap();
    let mut first = s.claim(&j.id, &r.fence).unwrap();
    assert_eq!(s.claim(&j.id, &r.fence).unwrap_err(), ScopeError::Busy);
    clock.store(1011, std::sync::atomic::Ordering::SeqCst);
    let mut second = s.claim(&j.id, &r.fence).unwrap();
    assert_eq!(second.attempt, first.attempt + 1);
    first.phase = ScopePhase::Restoring;
    assert_eq!(s.checkpoint(&first), Err(ScopeError::StoreConflict));
    second.phase = ScopePhase::TailAligning;
    assert_eq!(s.checkpoint(&second), Err(ScopeError::StoreConflict));
    s.close().unwrap();
}
#[test]
fn file_history_is_durable_separate_and_bounded() {
    let root = TempRoot::new();
    let limits = FileHistoryLimits {
        max_records: 2,
        max_bytes: 3,
        max_snapshot_bytes: 1 << 20,
    };
    let store = FileHistoryStore::open(&root.0, "sol".into(), limits.clone()).unwrap();
    let budget = HistoryLimits {
        max_jobs: 1,
        max_bytes: 3,
    };
    let mut r = HistoryRecord {
        chain_key: "sol".into(),
        namespace: "history_replay".into(),
        cursor: 2,
        payload: vec![1],
    };
    store.append_history(r.clone(), &budget).unwrap();
    store.append_history(r.clone(), &budget).unwrap();
    r.payload = vec![2];
    assert_eq!(
        store.append_history(r.clone(), &budget),
        Err(ScopeError::StoreConflict)
    );
    store.close().unwrap();
    let reopened = FileHistoryStore::open(&root.0, "sol".into(), limits).unwrap();
    let records = reopened.records().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].payload, vec![1]);
    r.cursor = 3;
    r.payload = vec![2, 3, 4];
    assert_eq!(
        reopened.append_history(r.clone(), &budget),
        Err(ScopeError::Budget)
    );
    r.namespace = "online".into();
    assert_eq!(
        reopened.append_history(r, &budget),
        Err(ScopeError::HistoryNamespace)
    );
    reopened.close().unwrap();
}
#[test]
fn file_store_rejects_undercharged_dependencies() {
    let (c, s, _, r, _, _root, _) = file_fixture();
    let (model, _, _, mut other, _) = fixture();
    other.anchor.hash = "next".into();
    let mut j = model
        .ensure_pool_scope(&Cancellation::new(), other)
        .unwrap();
    let _ = (c, r);
    j.reserved_bytes = 1;
    assert_eq!(
        s.admit(Admission {
            limits: j.budget.clone(),
            job: j
        })
        .unwrap_err(),
        ScopeError::StoreConflict
    );
    s.close().unwrap();
}
#[test]
fn file_store_publish_must_use_persisted_verified_stage() {
    let (c, s, src, r, _, _root, _) = file_fixture();
    let cancel = Cancellation::new();
    let admitted = c.ensure_pool_scope(&cancel, r.clone()).unwrap();
    let mut j = s.claim(&admitted.id, &r.fence).unwrap();
    j.phase = ScopePhase::LiveBuffering;
    s.checkpoint(&j).unwrap();
    j.buffer_id = "session".into();
    j.phase = ScopePhase::BaselineAcquiring;
    s.checkpoint(&j).unwrap();
    j.baseline_hash = src.candidates[0].content_hash.clone();
    j.cursor = 10;
    j.phase = ScopePhase::Restoring;
    s.checkpoint(&j).unwrap();
    j.restored_accounts = 2;
    j.phase = ScopePhase::TailAligning;
    s.checkpoint(&j).unwrap();
    j.cursor = j.anchor.cursor;
    j.baseline_hash = "f".repeat(64);
    let p = Publication {
        fence: j.fence.clone(),
        anchor: j.anchor.clone(),
        pool_ids: j.pool_ids.clone(),
        dependencies: j.dependencies.clone(),
        accounts: j
            .dependencies
            .iter()
            .map(|d| ScopeAccount {
                id: d.id.clone(),
                data: src.data[&d.id].clone(),
                anchor: j.anchor.clone(),
            })
            .collect(),
        baseline_hash: j.baseline_hash.clone(),
        buffer_id: j.buffer_id.clone(),
        cursor: j.cursor,
        history_gap: j.history_gap.clone(),
    };
    assert_eq!(s.publish(&cancel, &j, p), Err(ScopeError::StoreConflict));
    s.close().unwrap();
}
struct OnlineBackend {
    store: Arc<Store>,
    proof: Mutex<Option<OnlineScopeCommit>>,
}
impl ScopeStore for OnlineBackend {
    fn current(&self, c: &str) -> ScopeResult<Fence> {
        self.store.current(c)
    }
    fn admit(&self, a: Admission) -> ScopeResult<ScopeJob> {
        self.store.admit(a)
    }
    fn claim(&self, id: &str, f: &Fence) -> ScopeResult<ScopeJob> {
        self.store.claim(id, f)
    }
    fn checkpoint(&self, j: &ScopeJob) -> ScopeResult<()> {
        self.store.checkpoint(j)
    }
    fn fail(&self, j: &ScopeJob) -> ScopeResult<()> {
        self.store.fail(j)
    }
    fn publish(&self, c: &Cancellation, j: &ScopeJob, p: Publication) -> ScopeResult<()> {
        self.store.publish(c, j, p)?;
        *self.proof.lock().unwrap() = Some(OnlineScopeCommit {
            job_id: j.id.clone(),
            fence: j.fence.clone(),
            cursor: j.cursor,
            commit_id: "test-owner-committed".into(),
        });
        Ok(())
    }
}
impl ProductionTransactions for OnlineBackend {
    fn check_authority(&self, f: &Fence) -> ScopeResult<()> {
        if self.store.current(&f.chain_key)? == *f {
            Ok(())
        } else {
            Err(ScopeError::FenceChanged)
        }
    }
    fn ready_job(&self, id: &str) -> ScopeResult<ScopeJob> {
        self.store
            .0
            .lock()
            .unwrap()
            .jobs
            .get(id)
            .cloned()
            .ok_or(ScopeError::StoreConflict)
    }
    fn now_ms(&self) -> u64 {
        now()
    }
    fn online_commit(&self, _: &Cancellation, _: &str) -> ScopeResult<OnlineScopeCommit> {
        self.proof
            .lock()
            .unwrap()
            .clone()
            .ok_or(ScopeError::StoreConflict)
    }
}
#[test]
fn production_store_delegates_to_one_authoritative_backend() {
    let (_, store, mut src, r, cfg) = fixture();
    let backend = Arc::new(OnlineBackend {
        store: store.clone(),
        proof: Mutex::new(None),
    });
    let production = Arc::new(ProductionStore::new(backend.clone()));
    let ctl = ScopeController::new(cfg, production.clone(), now).unwrap();
    let cancel = Cancellation::new();
    let j = ctl.ensure_pool_scope(&cancel, r.clone()).unwrap();
    ctl.recover(&cancel, &j.id, &mut src).unwrap();
    *backend.proof.lock().unwrap() = Some(OnlineScopeCommit {
        job_id: j.id.clone(),
        fence: r.fence,
        cursor: r.anchor.cursor,
        commit_id: "owner-prepared-commit".into(),
    });
    production.online_commit(&cancel, &j.id).unwrap();
    store.0.lock().unwrap().current.as_mut().unwrap().generation += 1;
    assert_eq!(
        production.online_commit(&cancel, &j.id).unwrap_err(),
        ScopeError::FenceChanged
    );
}

#[test]
fn file_store_process_helper() {
    let Some(root) = std::env::var_os("SCOPE_STORE_CHILD_ROOT") else {
        return;
    };
    let (_, _, _, req, cfg) = fixture();
    let limits = FileScopeLimits {
        max_jobs: 2,
        max_account_bytes: 8,
        max_retained_jobs: 8,
        max_snapshot_bytes: 1 << 20,
        attempt_lease_ms: 10,
        max_authority_lease_ms: 100,
    };
    let store = Arc::new(
        FileScopeStore::open(
            std::path::Path::new(&root),
            req.fence.clone(),
            limits,
            Arc::new(now),
        )
        .unwrap(),
    );
    let cancel = Cancellation::new();
    store.authorize(&cancel, req.fence.clone(), 1100).unwrap();
    let controller = ScopeController::new(cfg, store.clone(), now).unwrap();
    let j = controller.ensure_pool_scope(&cancel, req).unwrap();
    let mut j = store.claim(&j.id, &j.fence).unwrap();
    j.phase = ScopePhase::LiveBuffering;
    store.checkpoint(&j).unwrap();
    println!("SCOPE_SAVED {}", j.id);
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    loop {
        std::thread::park()
    }
}
#[test]
fn file_store_killed_process_reopens_without_old_ready() {
    use std::io::BufRead;
    let (controller, seed_store, mut src, seed_req, _clock, root, _limits) = file_fixture();
    let cancel = Cancellation::new();
    let seed = controller.ensure_pool_scope(&cancel, seed_req).unwrap();
    controller.recover(&cancel, &seed.id, &mut src).unwrap();
    seed_store.close().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "file_store_process_helper", "--nocapture"])
        .env("SCOPE_STORE_CHILD_ROOT", &root.0)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let line = line.unwrap();
            if let Some(id) = line.strip_prefix("SCOPE_SAVED ") {
                let _ = tx.send(id.to_string());
                return;
            }
        }
    });
    let result = rx.recv_timeout(std::time::Duration::from_secs(10));
    let _ = child.kill();
    let _ = child.wait();
    reader.join().unwrap();
    let id = result.expect("child did not persist");
    let (_, _, _, req, _) = fixture();
    let limits = FileScopeLimits {
        max_jobs: 2,
        max_account_bytes: 8,
        max_retained_jobs: 8,
        max_snapshot_bytes: 1 << 20,
        attempt_lease_ms: 10,
        max_authority_lease_ms: 100,
    };
    let reopened = FileScopeStore::open(&root.0, req.fence.clone(), limits, Arc::new(now)).unwrap();
    let j = reopened.job(&id).unwrap();
    assert_eq!(j.attempt, 2);
    assert_ne!(j.phase, ScopePhase::ScopedReady);
    assert_eq!(reopened.current("sol"), Err(ScopeError::FenceChanged));
    reopened
        .authorize(&Cancellation::new(), req.fence.clone(), 1100)
        .unwrap();
    assert_eq!(reopened.claim(&id, &req.fence).unwrap().attempt, 3);
    reopened.close().unwrap();
}
#[test]
fn baseline_falls_back_when_peer_exceeds_tail_budget() {
    let (c, _, src, r, cfg) = fixture();
    let j = c
        .ensure_pool_scope(&Cancellation::new(), r.clone())
        .unwrap();
    let mut peer = src.candidates[0].clone();
    peer.anchor.cursor = 1;
    peer.anchor.height = 1;
    peer.content_hash = baseline_hash(&peer);
    let mut current = src.candidates[0].clone();
    current.kind = BaselineKind::CurrentChainState;
    current.anchor = r.anchor;
    current.content_hash = baseline_hash(&current);
    assert_eq!(
        select_scope_baseline(&j, &[peer.clone(), current], &cfg, 1000)
            .unwrap()
            .kind,
        BaselineKind::CurrentChainState
    );
    assert_eq!(
        select_scope_baseline(&j, &[peer], &cfg, 1000).unwrap_err(),
        ScopeError::HeadFirst
    );
}
#[test]
fn file_store_enforces_real_capacity_and_duplicate_admission() {
    let (c, s, _, mut r, _, _root, _) = file_fixture();
    let cancel = Cancellation::new();
    let first = c.ensure_pool_scope(&cancel, r.clone()).unwrap();
    assert_eq!(
        c.ensure_pool_scope(&cancel, r.clone()).unwrap().id,
        first.id
    );
    r.anchor.hash = "two".into();
    c.ensure_pool_scope(&cancel, r.clone()).unwrap();
    r.anchor.hash = "three".into();
    assert_eq!(
        c.ensure_pool_scope(&cancel, r).unwrap_err(),
        ScopeError::Budget
    );
    s.close().unwrap();
}
#[test]
fn file_store_publish_checks_cancel_fence_and_attempt_in_transaction() {
    for gate in ["cancel", "fence", "attempt"] {
        let (c, s, src, r, clock, _root, _) = file_fixture();
        let token = Cancellation::new();
        let admitted = c.ensure_pool_scope(&token, r.clone()).unwrap();
        let mut j = s.claim(&admitted.id, &r.fence).unwrap();
        j.phase = ScopePhase::LiveBuffering;
        s.checkpoint(&j).unwrap();
        j.buffer_id = "session".into();
        j.phase = ScopePhase::BaselineAcquiring;
        s.checkpoint(&j).unwrap();
        j.baseline_hash = src.candidates[0].content_hash.clone();
        j.cursor = 10;
        j.phase = ScopePhase::Restoring;
        s.checkpoint(&j).unwrap();
        j.restored_accounts = 2;
        j.phase = ScopePhase::TailAligning;
        s.checkpoint(&j).unwrap();
        j.cursor = j.anchor.cursor;
        let p = Publication {
            fence: j.fence.clone(),
            anchor: j.anchor.clone(),
            pool_ids: j.pool_ids.clone(),
            dependencies: j.dependencies.clone(),
            accounts: j
                .dependencies
                .iter()
                .map(|d| ScopeAccount {
                    id: d.id.clone(),
                    data: src.data[&d.id].clone(),
                    anchor: j.anchor.clone(),
                })
                .collect(),
            baseline_hash: j.baseline_hash.clone(),
            buffer_id: j.buffer_id.clone(),
            cursor: j.cursor,
            history_gap: j.history_gap.clone(),
        };
        match gate {
            "cancel" => token.cancel(),
            "fence" => {
                let mut f = r.fence.clone();
                f.generation += 1;
                s.authorize(&token, f, 1100).unwrap()
            }
            "attempt" => {
                clock.store(1011, std::sync::atomic::Ordering::SeqCst);
                s.claim(&j.id, &r.fence).unwrap();
            }
            _ => unreachable!(),
        };
        assert!(s.publish(&token, &j, p).is_err(), "{gate}");
        assert!(s.recovery_data(&j.id).is_err(), "{gate}");
        s.close().unwrap();
    }
}

#[test]
fn scope_regression_failed_fence_persistence_revokes_handle() {
    let (c, s, mut src, r, _clock, root, _limits) = file_fixture();
    let token = Cancellation::new();
    let j = c.ensure_pool_scope(&token, r.clone()).unwrap();
    c.recover(&token, &j.id, &mut src).unwrap();
    let pending = root.0.join("pending");
    std::fs::write(&pending, b"pre-rename failure").unwrap();
    let mut next = r.fence.clone();
    next.generation += 1;
    assert!(s.authorize(&token, next.clone(), 1100).is_err());
    assert!(
        s.current(&r.fence.chain_key).is_err(),
        "old authority survived failed advance"
    );
    assert!(
        s.recovery_data(&j.id).is_err(),
        "old ready data survived failed advance"
    );
    assert!(s.job(&j.id).is_err(), "old ready job is readable");
    assert!(
        c.recover(&token, &j.id, &mut src).is_err(),
        "old controller still recovered"
    );
    std::fs::remove_file(&pending).unwrap();
    assert!(
        s.authorize(&token, r.fence, 1100).is_err(),
        "old fence can be authorized again"
    );
    s.authorize(&token, next.clone(), 1100).unwrap();
    assert_eq!(s.current(&next.chain_key).unwrap(), next);
    s.close().unwrap();
}
#[test]
fn scope_regression_zero_event_tail_preserves_manifest() {
    for mutate in [false, true] {
        let (c, s, mut src, r, _clock, _root, _limits) = file_fixture();
        let token = Cancellation::new();
        src.candidates[0].anchor = r.anchor.clone();
        src.candidates[0].content_hash = baseline_hash(&src.candidates[0]);
        src.tail_events = Some(0);
        if mutate {
            src.tail_kind = Some("zero_substitute")
        };
        let j = c.ensure_pool_scope(&token, r).unwrap();
        let result = c.recover(&token, &j.id, &mut src);
        if mutate {
            assert!(result.is_err(), "zero-event tail replaced manifest bytes");
            assert!(s.recovery_data(&j.id).is_err())
        } else {
            result.unwrap();
            assert_eq!(s.recovery_data(&j.id).unwrap().accounts[0].data, vec![1])
        };
        s.close().unwrap();
    }
}
#[test]
fn scope_regression_controller_ready_rechecks_age() {
    let (c, memory, mut src, r, cfg) = fixture();
    let token = Cancellation::new();
    let j = c.ensure_pool_scope(&token, r).unwrap();
    c.recover(&token, &j.id, &mut src).unwrap();
    let calls = src.order.len();
    let boundary = ScopeController::new(cfg.clone(), memory.clone(), || 1040).unwrap();
    boundary.recover(&token, &j.id, &mut src).unwrap();
    let later = ScopeController::new(cfg, memory, || 1050).unwrap();
    assert!(
        later.recover(&token, &j.id, &mut src).is_err(),
        "controller returned stale ready"
    );
    assert_eq!(src.order.len(), calls);
}
#[test]
fn scope_regression_file_ready_reads_recheck_age() {
    let (c, s, mut src, r, clock, _root, _limits) = file_fixture();
    let token = Cancellation::new();
    let j = c.ensure_pool_scope(&token, r.clone()).unwrap();
    c.recover(&token, &j.id, &mut src).unwrap();
    clock.store(1040, std::sync::atomic::Ordering::SeqCst);
    s.recovery_data(&j.id).unwrap();
    clock.store(1050, std::sync::atomic::Ordering::SeqCst);
    let calls = src.order.len();
    assert!(
        c.recover(&token, &j.id, &mut src).is_err(),
        "Recover returned stale ready"
    );
    assert!(
        s.claim(&j.id, &r.fence).is_err(),
        "Claim returned stale ready"
    );
    assert!(s.job(&j.id).is_err(), "Job returned stale ready");
    assert!(
        s.recovery_data(&j.id).is_err(),
        "RecoveryData returned stale data"
    );
    assert!(
        c.ensure_pool_scope(&token, r.clone()).is_err(),
        "duplicate admission returned stale ready"
    );
    assert_eq!(src.order.len(), calls);
    s.current(&r.fence.chain_key).unwrap();
    s.close().unwrap();
}
#[test]
fn scope_regression_commit_rechecks_age_after_snapshot_sync() {
    let (_, _, mut src, r, cfg) = fixture();
    let root = TempRoot::new();
    let pending = root.0.join("pending");
    let clock = Arc::new(std::sync::atomic::AtomicU64::new(1000));
    let clock_source = clock.clone();
    let now = Arc::new(move || {
        let data = std::fs::read(&pending).unwrap_or_default();
        if data
            .windows(b"SCOPED_READY".len())
            .any(|bytes| bytes == b"SCOPED_READY")
        {
            clock_source.store(1050, std::sync::atomic::Ordering::SeqCst)
        };
        clock_source.load(std::sync::atomic::Ordering::SeqCst)
    });
    let limits = FileScopeLimits {
        max_jobs: 2,
        max_account_bytes: 8,
        max_retained_jobs: 8,
        max_snapshot_bytes: 1 << 20,
        attempt_lease_ms: 100,
        max_authority_lease_ms: 100,
    };
    let store = Arc::new(FileScopeStore::open(&root.0, r.fence.clone(), limits, now).unwrap());
    let token = Cancellation::new();
    store.authorize(&token, r.fence.clone(), 1100).unwrap();
    let controller = ScopeController::new(cfg, store.clone(), self::now).unwrap();
    let job = controller.ensure_pool_scope(&token, r).unwrap();
    assert_eq!(
        controller.recover(&token, &job.id, &mut src),
        Err(ScopeError::Stale),
        "freshness must reject while original authority/attempt leases remain valid"
    );
    assert_eq!(clock.load(std::sync::atomic::Ordering::SeqCst), 1050);
    let envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.0.join("state.json")).unwrap()).unwrap();
    let disk: serde_json::Value =
        serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    assert_ne!(disk["jobs"][&job.id]["job"]["phase"], "SCOPED_READY");
    assert!(disk["data"].as_object().unwrap().is_empty());
    assert!(store.recovery_data(&job.id).is_err());
    store.close().unwrap();
}

fn assert_version_identity_requires_numeric_fence_advance(field: &str) {
    let (c, s, mut src, r, _clock, root, _limits) = file_fixture();
    let token = Cancellation::new();
    let j = c.ensure_pool_scope(&token, r.clone()).unwrap();
    c.recover(&token, &j.id, &mut src).unwrap();
    let mut next = r.fence.clone();
    if field == "adapter" {
        next.adapter_version = "a2".into()
    } else {
        next.schema_version = "s2".into()
    };
    let pending = root.0.join("pending");
    std::fs::write(&pending, b"blocked").unwrap();
    assert_eq!(
        s.authorize(&token, next, 1100),
        Err(ScopeError::FenceChanged),
        "{field}: reject equal numeric version replacement before persistence"
    );
    assert_eq!(s.current(&r.fence.chain_key).unwrap(), r.fence);
    s.recovery_data(&j.id).unwrap();
    std::fs::remove_file(pending).unwrap();
    s.authorize(&token, r.fence, 1100).unwrap();
    c.recover(&token, &j.id, &mut src).unwrap();
    s.close().unwrap();
}
#[test]
fn scope_version_identity_adapter_requires_numeric_fence_advance() {
    assert_version_identity_requires_numeric_fence_advance("adapter");
}
#[test]
fn scope_version_identity_schema_requires_numeric_fence_advance() {
    assert_version_identity_requires_numeric_fence_advance("schema");
}
#[test]
fn scope_version_identity_advanced_fence_failure_and_retry() {
    for field in ["adapter", "schema"] {
        for axis in ["assignment", "generation", "canonical", "writer"] {
            let (c, s, mut src, r, _clock, root, _limits) = file_fixture();
            let token = Cancellation::new();
            let j = c.ensure_pool_scope(&token, r.clone()).unwrap();
            c.recover(&token, &j.id, &mut src).unwrap();
            let mut next = r.fence.clone();
            // A lexically smaller version is allowed only with a numerical fence advance.
            if field == "adapter" {
                next.adapter_version = "a0".into()
            } else {
                next.schema_version = "s0".into()
            };
            match axis {
                "assignment" => next.assignment += 1,
                "generation" => next.generation += 1,
                "canonical" => next.canonical_epoch += 1,
                "writer" => next.writer_epoch += 1,
                _ => unreachable!(),
            };
            let pending = root.0.join("pending");
            std::fs::write(&pending, b"blocked").unwrap();
            assert!(
                matches!(
                    s.authorize(&token, next.clone(), 1100),
                    Err(ScopeError::Storage(_))
                ),
                "{field}/{axis}: accepted identity must reach pending failure"
            );
            assert_eq!(s.current(&r.fence.chain_key), Err(ScopeError::FenceChanged));
            assert!(s.recovery_data(&j.id).is_err());
            assert!(c.recover(&token, &j.id, &mut src).is_err());
            std::fs::remove_file(pending).unwrap();
            assert_eq!(
                s.authorize(&token, r.fence, 1100),
                Err(ScopeError::FenceChanged)
            );
            s.authorize(&token, next.clone(), 1100).unwrap();
            assert_eq!(s.current(&next.chain_key).unwrap(), next);
            s.close().unwrap();
        }
    }
}
