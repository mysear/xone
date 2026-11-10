use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use xone_solana_ingest::{owner_scope::*, scope::*, store::Store};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "xone-owner-scope-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fence() -> Fence {
    Fence {
        chain_key: "solana:localnet".into(),
        chain_identity: "localnet".into(),
        owner: "chain-parser-rust".into(),
        assignment: 1,
        generation: 1,
        canonical_epoch: 1,
        writer_epoch: 1,
        adapter_version: "adapter".into(),
        schema_version: "1".into(),
    }
}
fn limits() -> FileScopeLimits {
    FileScopeLimits {
        max_jobs: 2,
        max_account_bytes: 10000,
        max_retained_jobs: 8,
        max_snapshot_bytes: 1000000,
        attempt_lease_ms: 1000,
        max_authority_lease_ms: 5000,
    }
}
#[test]
fn real_owner_ledger_admits_claims_and_reopens_without_cached_ready_or_authority() {
    let dir = Temp::new();
    let db = Arc::new(Mutex::new(
        Store::open(&dir.0, "chain-parser-rust", 1).unwrap(),
    ));
    let clock = Arc::new(AtomicU64::new(1000));
    let now = clock.clone();
    let live = Arc::new(AtomicU64::new(1));
    let authority = live.clone();
    let backend = OwnerScopeStore::new(
        db.clone(),
        limits(),
        Arc::new(move || now.load(Ordering::SeqCst)),
        Arc::new(move |_| {
            if authority.load(Ordering::SeqCst) == 1 {
                Ok(())
            } else {
                Err(ScopeError::FenceChanged)
            }
        }),
        None,
    )
    .unwrap();
    backend
        .begin(fence(), 5000)
        .expect("same owner RocksDB must persist an authorized scope ledger");
    let policy = ScopeLimits {
        max_jobs: 2,
        max_account_bytes: 10000,
        max_tail_events: 10,
        max_tail_bytes: 10000,
        max_tail_age_ms: 5000,
        max_baseline_age_ms: 5000,
        shard_accounts: 2,
    };
    let controller = ScopeController::new(
        ScopeConfig {
            fence: fence(),
            limits: policy.clone(),
            catalog: std::collections::BTreeMap::from([(
                "pool".into(),
                vec![ScopeDependency {
                    id: "account".into(),
                    max_bytes: 100,
                }],
            )]),
            trusted_sources: vec!["public".into()],
        },
        Arc::new(backend.clone()),
        || 1000,
    )
    .unwrap();
    let job = controller
        .ensure_pool_scope(
            &Cancellation::new(),
            EnsurePoolScopeRequest {
                fence: fence(),
                anchor: ScopeAnchor {
                    height: 10,
                    hash: "bank".into(),
                    timestamp_ms: 1000,
                    cursor: 1,
                },
                pool_ids: vec!["pool".into()],
                budget: policy,
                history_after: 0,
            },
        )
        .unwrap();
    let claimed = backend.claim(&job.id, &fence()).unwrap();
    assert_eq!(claimed.attempt, 1);
    let mut mutated = claimed.clone();
    mutated.phase = ScopePhase::LiveBuffering;
    mutated.budget.max_account_bytes *= 100;
    assert!(
        backend.checkpoint(&mutated).is_err(),
        "attempt CAS must also bind the admitted immutable budget"
    );
    live.store(0, Ordering::SeqCst);
    assert_eq!(
        backend.current("solana:localnet"),
        Err(ScopeError::FenceChanged)
    );
    drop(controller);
    drop(backend);
    drop(db);
    let db = Arc::new(Mutex::new(
        Store::open(&dir.0, "chain-parser-rust", 1).unwrap(),
    ));
    let backend =
        OwnerScopeStore::new(db, limits(), Arc::new(|| 1000), Arc::new(|_| Ok(())), None).unwrap();
    assert!(
        backend.current("solana:localnet").is_err(),
        "reopen never inherits an old live fence"
    );
    assert!(
        backend.begin(fence(), 5000).is_err(),
        "reopen must reconcile and advance generation before accepting authority"
    );
}

struct Source;
impl RecoverySource for Source {
    fn start_buffer(&mut self, _: &Cancellation, j: &ScopeJob) -> ScopeResult<StreamBuffer> {
        Ok(StreamBuffer {
            id: "durable-source".into(),
            first_cursor: j.anchor.cursor,
            durable: true,
        })
    }
    fn fix_anchor(
        &mut self,
        _: &Cancellation,
        j: &ScopeJob,
        _: &StreamBuffer,
    ) -> ScopeResult<ScopeAnchor> {
        Ok(j.anchor.clone())
    }
    fn candidates(
        &mut self,
        _: &Cancellation,
        j: &ScopeJob,
        _: &ScopeAnchor,
    ) -> ScopeResult<Vec<BaselineCandidate>> {
        let mut b = BaselineCandidate {
            fence: j.fence.clone(),
            anchor: j.anchor.clone(),
            source: "public".into(),
            kind: BaselineKind::CurrentChainState,
            pool_ids: j.pool_ids.clone(),
            dependencies: j.dependencies.clone(),
            account_hashes: std::collections::BTreeMap::from([(
                "account".into(),
                account_hash(b"account bytes"),
            )]),
            content_hash: String::new(),
        };
        b.content_hash = baseline_hash(&b);
        Ok(vec![b])
    }
    fn restore_shard(
        &mut self,
        _: &Cancellation,
        j: &ScopeJob,
        _: &BaselineCandidate,
        _: &[String],
    ) -> ScopeResult<Vec<ScopeAccount>> {
        Ok(vec![ScopeAccount {
            id: "account".into(),
            data: b"account bytes".to_vec(),
            anchor: j.anchor.clone(),
        }])
    }
    fn align_tail(
        &mut self,
        _: &Cancellation,
        j: &ScopeJob,
        _: &StreamBuffer,
        b: &BaselineCandidate,
        a: &[ScopeAccount],
    ) -> ScopeResult<ScopeTail> {
        Ok(ScopeTail {
            from: b.anchor.clone(),
            to: j.anchor.clone(),
            complete: true,
            events: 0,
            bytes: 13,
            accounts: a.to_vec(),
        })
    }
    fn release_buffer(&mut self, _: &StreamBuffer) -> ScopeResult<()> {
        Ok(())
    }
}
#[test]
fn prepared_owner_apply_atomically_commits_scope_snapshot_journal_and_proof() {
    use xone_domain::Snapshot;
    use xone_kafka_transport::{Cursor, Unit};
    let dir = Temp::new();
    let db = Arc::new(Mutex::new(
        Store::open(&dir.0, "chain-parser-rust", 1).unwrap(),
    ));
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    snapshot.reference.block.timestamp_ms = 1000;
    let mut f = fence();
    f.adapter_version = snapshot.reference.adapter_set_hash.clone();
    let target = snapshot.clone();
    let owner = db.clone();
    let ticks = Arc::new(AtomicU64::new(1000));
    let tick = ticks.clone();
    let clock: Arc<dyn Fn() -> u64 + Send + Sync> = Arc::new(move || tick.load(Ordering::SeqCst));
    let late = Arc::new(AtomicU64::new(0));
    let armed = late.clone();
    let io_clock = ticks.clone();
    let mut l = limits();
    l.max_authority_lease_ms = 10000;
    let lim = l.clone();
    let c = clock.clone();
    let check: CheckAuthority = Arc::new(move |_| {
        if armed.load(Ordering::SeqCst) == 1 {
            io_clock.store(7000, Ordering::SeqCst);
        }
        Ok(())
    });
    let checker = check.clone();
    let bridge: PublishBridge = Arc::new(move |cancel, job, p| {
        let backend =
            OwnerScopeStore::new(owner.clone(), lim.clone(), c.clone(), checker.clone(), None)?;
        let unit = Unit {
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: target.clone(),
            normalized_facts: None,
        };
        backend.prepare(cancel, job, p, &unit)?;
        assert!(owner.lock().unwrap().prepared().unwrap().is_some());
        assert!(
            backend.committed_job(&job.id).is_err(),
            "prepared never means ready"
        );
        owner
            .lock()
            .unwrap()
            .apply_prepared(Cursor {
                topic: "local-owner/solana:localnet".into(),
                partition: 0,
                offset: 1,
            })
            .unwrap();
        Ok(())
    });
    let backend = OwnerScopeStore::new(db.clone(), l, clock, check, Some(bridge)).unwrap();
    backend.begin(f.clone(), 9000).unwrap();
    let production = Arc::new(ProductionStore::new(Arc::new(backend.clone())));
    let b = ScopeLimits {
        max_jobs: 2,
        max_account_bytes: 10000,
        max_tail_events: 10,
        max_tail_bytes: 10000,
        max_tail_age_ms: 5000,
        max_baseline_age_ms: 5000,
        shard_accounts: 1,
    };
    let ctl = ScopeController::new(
        ScopeConfig {
            fence: f.clone(),
            limits: b.clone(),
            catalog: snapshot
                .pools
                .iter()
                .map(|p| {
                    (
                        p.pool_id.clone(),
                        vec![ScopeDependency {
                            id: "account".into(),
                            max_bytes: 100,
                        }],
                    )
                })
                .collect(),
            trusted_sources: vec!["public".into()],
        },
        production.clone(),
        || 1000,
    )
    .unwrap();
    let j = ctl
        .ensure_pool_scope(
            &Cancellation::new(),
            EnsurePoolScopeRequest {
                fence: f,
                anchor: ScopeAnchor {
                    height: snapshot.reference.block.number_or_slot,
                    hash: snapshot.reference.block.hash.clone(),
                    timestamp_ms: 1000,
                    cursor: 1,
                },
                pool_ids: snapshot.pools.iter().map(|p| p.pool_id.clone()).collect(),
                budget: b,
                history_after: 0,
            },
        )
        .unwrap();
    ctl.recover(&Cancellation::new(), &j.id, &mut Source)
        .unwrap();
    let proof = production
        .online_commit(&Cancellation::new(), &j.id)
        .unwrap();
    assert_eq!(proof.cursor, 1);
    let store = db.lock().unwrap();
    assert!(store.prepared().unwrap().is_none());
    assert_eq!(store.current().unwrap().reference, snapshot.reference);
    assert_eq!(store.journal(0, 100).unwrap().len(), 1);
    drop(store);
    assert_eq!(
        backend.committed_job(&j.id).unwrap().phase,
        ScopePhase::ScopedReady
    );
    late.store(1, Ordering::SeqCst);
    assert!(
        matches!(backend.committed_job(&j.id), Err(ScopeError::Stale)),
        "READY must recheck original freshness after the last authority I/O"
    );
    assert!(production
        .online_commit(&Cancellation::new(), &j.id)
        .is_err());
}

#[test]
fn unknown_publish_outcome_requires_reopen_and_reconcile_before_new_generation() {
    use xone_domain::Snapshot;
    use xone_kafka_transport::{Cursor, Unit};
    let dir = Temp::new();
    let db = Arc::new(Mutex::new(
        Store::open(&dir.0, "chain-parser-rust", 1).unwrap(),
    ));
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    snapshot.reference.block.timestamp_ms = 1000;
    let mut f = fence();
    f.adapter_version = snapshot.reference.adapter_set_hash.clone();
    let target = snapshot.clone();
    let owner = db.clone();
    let bridge: PublishBridge = Arc::new(move |cancel, job, p| {
        let backend = OwnerScopeStore::new(
            owner.clone(),
            limits(),
            Arc::new(|| 1000),
            Arc::new(|_| Ok(())),
            None,
        )?;
        backend.prepare(
            cancel,
            job,
            p,
            &Unit {
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: target.clone(),
                normalized_facts: None,
            },
        )?;
        owner.lock().unwrap().mark_uncertain();
        Err(ScopeError::Storage("KAFKA_OUTCOME_UNKNOWN".into()))
    });
    let backend = OwnerScopeStore::new(
        db.clone(),
        limits(),
        Arc::new(|| 1000),
        Arc::new(|_| Ok(())),
        Some(bridge),
    )
    .unwrap();
    backend.begin(f.clone(), 5000).unwrap();
    let budget = ScopeLimits {
        max_jobs: 2,
        max_account_bytes: 10000,
        max_tail_events: 10,
        max_tail_bytes: 10000,
        max_tail_age_ms: 5000,
        max_baseline_age_ms: 5000,
        shard_accounts: 1,
    };
    let controller = ScopeController::new(
        ScopeConfig {
            fence: f.clone(),
            limits: budget.clone(),
            catalog: snapshot
                .pools
                .iter()
                .map(|p| {
                    (
                        p.pool_id.clone(),
                        vec![ScopeDependency {
                            id: "account".into(),
                            max_bytes: 100,
                        }],
                    )
                })
                .collect(),
            trusted_sources: vec!["public".into()],
        },
        Arc::new(ProductionStore::new(Arc::new(backend.clone()))),
        || 1000,
    )
    .unwrap();
    let j = controller
        .ensure_pool_scope(
            &Cancellation::new(),
            EnsurePoolScopeRequest {
                fence: f.clone(),
                anchor: ScopeAnchor {
                    height: snapshot.reference.block.number_or_slot,
                    hash: snapshot.reference.block.hash.clone(),
                    timestamp_ms: 1000,
                    cursor: 1,
                },
                pool_ids: snapshot.pools.iter().map(|p| p.pool_id.clone()).collect(),
                budget,
                history_after: 0,
            },
        )
        .unwrap();
    assert!(controller
        .recover(&Cancellation::new(), &j.id, &mut Source)
        .is_err());
    assert!(db.lock().unwrap().prepared().is_err());
    assert!(backend.current("solana:localnet").is_err());
    db.lock().unwrap().reopen().unwrap();
    assert!(db.lock().unwrap().prepared().unwrap().is_some());
    let mut next = f.clone();
    next.generation += 1;
    assert!(
        backend.begin(next.clone(), 5000).is_err(),
        "new namespace cannot hide an unresolved prepared outcome"
    );
    db.lock()
        .unwrap()
        .apply_prepared(Cursor {
            topic: "controlled-committed-journal".into(),
            partition: 0,
            offset: 1,
        })
        .unwrap();
    assert!(
        db.lock().unwrap().current().is_none(),
        "historical reconcile is not fresh owner authority"
    );
    assert!(backend.online_commit(&Cancellation::new(), &j.id).is_err());
    backend.begin(next, 5000).unwrap();
    assert!(backend.committed_job(&j.id).is_err());
}

#[test]
fn quality_terminal_byte_budget_progress() {
    use xone_domain::Snapshot;
    use xone_kafka_transport::{Cursor, Unit};
    let dir = Temp::new();
    let db = Arc::new(Mutex::new(
        Store::open(&dir.0, "chain-parser-rust", 1).unwrap(),
    ));
    let mut lim = limits();
    lim.max_jobs = 100;
    lim.max_retained_jobs = 100;
    lim.max_snapshot_bytes = 20_000;
    let policy = ScopeLimits {
        max_jobs: 100,
        max_account_bytes: 10000,
        max_tail_events: 10,
        max_tail_bytes: 10000,
        max_tail_age_ms: 5000,
        max_baseline_age_ms: 5000,
        shard_accounts: 1,
    };
    let base: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    let mut committed = 0;
    for generation in 1..=30u64 {
        let mut snapshot = base.clone();
        snapshot.reference.online_generation = generation;
        snapshot.reference.journal_seq = generation;
        snapshot.reference.block.number_or_slot = 500 + generation;
        snapshot.reference.block.timestamp_ms = 1000;
        let mut f = fence();
        f.generation = generation;
        f.adapter_version = snapshot.reference.adapter_set_hash.clone();
        let target = snapshot.clone();
        let owner = db.clone();
        let lim_bridge = lim.clone();
        let bridge: PublishBridge = Arc::new(move |cancel, job, publication| {
            let backend = OwnerScopeStore::new(
                owner.clone(),
                lim_bridge.clone(),
                Arc::new(|| 1000),
                Arc::new(|_| Ok(())),
                None,
            )?;
            let unit = Unit {
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: target.clone(),
                normalized_facts: None,
            };
            backend.prepare(cancel, job, publication, &unit)?;
            owner
                .lock()
                .unwrap()
                .apply_prepared(Cursor {
                    topic: "local-owner/solana:localnet".into(),
                    partition: 0,
                    offset: generation as i64,
                })
                .map_err(|e| ScopeError::Storage(e.0))?;
            Ok(())
        });
        let backend = OwnerScopeStore::new(
            db.clone(),
            lim.clone(),
            Arc::new(|| 1000),
            Arc::new(|_| Ok(())),
            Some(bridge),
        )
        .unwrap();
        let begin = backend.begin(f.clone(), 5000);
        if let Err(e) = begin {
            println!("BEGIN failed generation={generation} committed={committed} error={e:?}");
            continue;
        }
        let ctl = ScopeController::new(
            ScopeConfig {
                fence: f.clone(),
                limits: policy.clone(),
                catalog: snapshot
                    .pools
                    .iter()
                    .map(|p| {
                        (
                            p.pool_id.clone(),
                            vec![ScopeDependency {
                                id: "account".into(),
                                max_bytes: 100,
                            }],
                        )
                    })
                    .collect(),
                trusted_sources: vec!["public".into()],
            },
            Arc::new(ProductionStore::new(Arc::new(backend))),
            || 1000,
        )
        .unwrap();
        let job = ctl.ensure_pool_scope(
            &Cancellation::new(),
            EnsurePoolScopeRequest {
                fence: f,
                anchor: ScopeAnchor {
                    height: snapshot.reference.block.number_or_slot,
                    hash: snapshot.reference.block.hash.clone(),
                    timestamp_ms: 1000,
                    cursor: generation,
                },
                pool_ids: snapshot.pools.iter().map(|p| p.pool_id.clone()).collect(),
                budget: policy.clone(),
                history_after: 0,
            },
        );
        let result = job.and_then(|j| ctl.recover(&Cancellation::new(), &j.id, &mut Source));
        if let Err(e) = result {
            println!("RECOVER failed generation={generation} committed={committed} error={e:?}; rows below retention=100");
            assert!(db.lock().unwrap().prepared().unwrap().is_none());
            let retry = OwnerScopeStore::new(
                db.clone(),
                lim.clone(),
                Arc::new(|| 1000),
                Arc::new(|_| Ok(())),
                None,
            )
            .unwrap();
            let mut next = fence();
            next.generation = generation + 1;
            next.adapter_version = base.reference.adapter_set_hash.clone();
            println!("later-generation begin={:?}", retry.begin(next, 5000));
            continue;
        }
        committed += 1;
    }
    assert_eq!(
        committed, 30,
        "terminal audit rows exhaust bytes before the row-count GC gate"
    );
}

#[test]
fn admission_reserves_publication_bytes_without_evicting_active_jobs() {
    let dir = Temp::new();
    let db = Arc::new(Mutex::new(
        Store::open(&dir.0, "chain-parser-rust", 1).unwrap(),
    ));
    let mut lim = limits();
    lim.max_jobs = 100;
    lim.max_retained_jobs = 100;
    lim.max_snapshot_bytes = 20_000;
    let backend = OwnerScopeStore::new(
        db.clone(),
        lim,
        Arc::new(|| 1000),
        Arc::new(|_| Ok(())),
        None,
    )
    .unwrap();
    backend.begin(fence(), 5000).unwrap();
    let policy = ScopeLimits {
        max_jobs: 100,
        max_account_bytes: 10000,
        max_tail_events: 10,
        max_tail_bytes: 10000,
        max_tail_age_ms: 5000,
        max_baseline_age_ms: 5000,
        shard_accounts: 1,
    };
    let ctl = ScopeController::new(
        ScopeConfig {
            fence: fence(),
            limits: policy.clone(),
            catalog: std::collections::BTreeMap::from([
                (
                    "small".into(),
                    vec![ScopeDependency {
                        id: "first".into(),
                        max_bytes: 1000,
                    }],
                ),
                (
                    "large".into(),
                    vec![ScopeDependency {
                        id: "second".into(),
                        max_bytes: 4000,
                    }],
                ),
            ]),
            trusted_sources: vec!["public".into()],
        },
        Arc::new(backend.clone()),
        || 1000,
    )
    .unwrap();
    let request = |pool: &str, cursor: u64| EnsurePoolScopeRequest {
        fence: fence(),
        anchor: ScopeAnchor {
            height: cursor,
            hash: "bank".into(),
            timestamp_ms: 1000,
            cursor,
        },
        pool_ids: vec![pool.into()],
        budget: policy.clone(),
        history_after: 0,
    };
    let first = ctl
        .ensure_pool_scope(&Cancellation::new(), request("small", 1))
        .unwrap();
    assert!(
        matches!(
            ctl.ensure_pool_scope(&Cancellation::new(), request("large", 2)),
            Err(ScopeError::Budget)
        ),
        "admission must reserve the encoded Publication, not only job metadata"
    );
    assert_eq!(backend.job(&first.id).unwrap().phase, ScopePhase::Admitted);
    assert!(backend.claim(&first.id, &fence()).is_ok());
    assert!(db.lock().unwrap().prepared().unwrap().is_none());
}
