//! Owned async bridge. ScopeController runs on a blocking worker; its publish
//! request is serviced by the already-held chain actor. Kafka is never dropped.
use super::*;
use crate::{
    manifest_recovery::ManifestRecoverySource,
    owner_scope::*,
    scope::*,
    stream::{CapturedFrame, CommittedBank},
};
use xone_control::Lease;
#[derive(Clone)]
pub(super) struct ScopeMaterial {
    pub chain: Chain,
    pub bank: Arc<CommittedBank>,
    pub receipt: CapturedFrame,
    pub timestamp_ms: u64,
    pub source: String,
}
struct PublishRequest {
    cancel: Cancellation,
    job: ScopeJob,
    publication: Publication,
    reply: tokio::sync::oneshot::Sender<ScopeResult<()>>,
}
impl Parser {
    pub(super) fn scope_target(&self, boot: &Chain, admitted: &Chain) -> Result<Chain> {
        let db = self.stores[&boot.key]
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?;
        let previous = db.persisted_snapshot()?;
        let mut ids = previous
            .as_ref()
            .map(|s| {
                s.pools
                    .iter()
                    .map(|p| p.pool_id.clone())
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_else(|| boot.pools.iter().map(|p| p.id.clone()).collect());
        ids.extend(db.scope_demands()?);
        let mut target = admitted.clone();
        target.pools.retain(|p| ids.contains(&p.id));
        if target.pools.is_empty() {
            if let Some(first) = admitted.pools.first() {
                target.pools.push(first.clone());
            }
        }
        Ok(target)
    }
    pub(super) fn bank_subset(material: &ScopeMaterial, chain: &Chain) -> Result<CommittedBank> {
        let keys = crate::rpc::required_scope(chain)?
            .iter()
            .map(|id| crate::bank_manifest::key(id))
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let bank = &material.bank;
        let accounts = bank
            .accounts
            .iter()
            .filter(|(id, _)| keys.contains(*id))
            .map(|(id, a)| (*id, a.clone()))
            .collect::<BTreeMap<_, _>>();
        if accounts.len() != keys.len() {
            return Err(err("SCOPE_DEPENDENCIES_INCOMPLETE"));
        }
        Ok(CommittedBank {
            bank: bank.bank.clone(),
            blockhash: bank.blockhash.clone(),
            parent: bank.parent.clone(),
            accounts,
            account_versions: bank
                .account_versions
                .iter()
                .filter(|((id, _), _)| keys.contains(id))
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
            transactions: bank.transactions.clone(),
        })
    }
    pub(super) async fn restore_cached_scope(
        &self,
        original: &Chain,
        ids: &[String],
    ) -> Result<()> {
        let material = self
            .certified
            .lock()
            .map_err(|_| err("SCOPE_CACHE_LOCK"))?
            .get(&original.key)
            .cloned()
            .ok_or_else(|| err("POOL_SCOPE_STREAM_RECOVERY_PENDING"))?;
        let status = self.stream_status(&original.key)?;
        if status.phase != "LIVE" || status.session_id != material.receipt.frame.source.session_id {
            return Err(err("POOL_SCOPE_STREAM_RECOVERY_PENDING"));
        }
        let (admitted, identity) = self.admitted_chain(original)?;
        let evidence: StreamEvidence =
            serde_json::from_str(&material.source).map_err(|_| err("SCOPE_CACHE_PROVENANCE"))?;
        if identity.as_deref() != Some(&evidence.catalog_identity) {
            return Err(err("POOL_SCOPE_STREAM_RECOVERY_PENDING"));
        }
        let mut target = self.scope_target(original, &admitted)?;
        for id in ids {
            if !target.pools.iter().any(|p| p.id == *id) {
                target.pools.push(
                    admitted
                        .pools
                        .iter()
                        .find(|p| p.id == *id)
                        .cloned()
                        .ok_or_else(|| err("POOL_NOT_IN_ADMITTED_CATALOG"))?,
                );
            }
        }
        let bank = Self::bank_subset(&material, &target)?;
        let mut snapshot = crate::rpc::snapshot_from_bank(&target, &bank, material.timestamp_ms)?;
        snapshot.source = material.source.clone();
        snapshot.history_gaps = self.stores[&original.key]
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .persisted_snapshot()?
            .map(|s| s.history_gaps)
            .unwrap_or_default();
        self.commit_snapshot(&target, snapshot, false, Some(material))
            .await
    }
    pub(super) async fn checked_current(&self, key: &str) -> Result<Snapshot> {
        if self.source.is_some() {
            return self.current(key);
        }
        let versioned = self.config.config.yellowstone.contains_key(key);
        let chain = self
            .config
            .config
            .chains
            .iter()
            .find(|c| c.key == key)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
        let lease = self.writers[key]
            .lock()
            .await
            .as_ref()
            .map(|w| w.lease.clone());
        self.check_owner_authority(chain, lease.as_ref()).await?;
        let snapshot = self.current(key)?;
        if let (Some(pg), Some(lease)) = (self.control.get(), lease.as_ref()) {
            tokio::time::timeout(
                Duration::from_millis(self.config.config.control.renewal_ms),
                pg.check_committed_reference(lease, &snapshot.reference),
            )
            .await
            .map_err(|_| err("CONTROL_DEADLINE"))??;
        }
        {
            let db = self.stores[key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            if versioned {
                db.check_scope_snapshot(&snapshot.reference)?;
            } else if db
                .persisted_snapshot()?
                .is_none_or(|actual| actual.reference != snapshot.reference)
            {
                return Err(err("SCOPE_READY_REFERENCE_CHANGED"));
            }
        }
        self.check_owner_authority(chain, lease.as_ref()).await?;
        if let (Some(pg), Some(lease)) = (self.control.get(), lease.as_ref()) {
            tokio::time::timeout(
                Duration::from_millis(self.config.config.control.renewal_ms),
                pg.check_committed_reference(lease, &snapshot.reference),
            )
            .await
            .map_err(|_| err("CONTROL_DEADLINE"))??;
        }
        let current = self.current(key)?;
        if current.reference != snapshot.reference {
            return Err(err("SCOPE_READY_REFERENCE_CHANGED"));
        }
        {
            let db = self.stores[key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            if versioned {
                db.check_scope_snapshot(&snapshot.reference)?;
            } else if db
                .persisted_snapshot()?
                .is_none_or(|actual| actual.reference != snapshot.reference)
            {
                return Err(err("SCOPE_READY_REFERENCE_CHANGED"));
            }
        }
        // Last authority I/O is after the last owner state I/O. The original
        // bank timestamp is checked once more after this operation as well.
        if let (Some(pg), Some(lease)) = (self.control.get(), lease.as_ref()) {
            tokio::time::timeout(
                Duration::from_millis(self.config.config.control.renewal_ms),
                pg.check_committed_reference(lease, &snapshot.reference),
            )
            .await
            .map_err(|_| err("CONTROL_DEADLINE"))??;
        } else if versioned {
            let expected = Fence {
                chain_key: chain.key.clone(),
                chain_identity: chain.genesis_hash.clone(),
                owner: chain.owner_service.clone(),
                assignment: chain.assignment_version,
                generation: snapshot.reference.online_generation,
                canonical_epoch: snapshot.reference.canonical_epoch,
                writer_epoch: snapshot.reference.writer_epoch,
                adapter_version: snapshot.reference.adapter_set_hash.clone(),
                schema_version: snapshot.reference.schema_version.to_string(),
            };
            self.stores[key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .check_embedded_scope(Some(&expected))?;
        }
        // No await follows this local read barrier. A finalized incomplete bank
        // or a new receipt session may have revoked visibility during PG I/O.
        let current = self.current(key)?;
        if current.reference != snapshot.reference {
            return Err(err("SCOPE_READY_REFERENCE_CHANGED"));
        }
        if versioned {
            let evidence: StreamEvidence = serde_json::from_str(&snapshot.source)
                .map_err(|_| err("SCOPE_CACHE_PROVENANCE"))?;
            let statuses = self
                .stream_statuses
                .lock()
                .map_err(|_| err("STREAM_STATUS_LOCK"))?;
            let status = statuses
                .get(key)
                .ok_or_else(|| err("STREAM_NOT_CONFIGURED"))?;
            let db = self.stores[key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            if status.phase != "LIVE"
                || status.session_id != evidence.session_id
                || status.wal_generation != evidence.wal_generation
                || status.catalog_identity != evidence.catalog_identity
                || db
                    .current()
                    .is_none_or(|s| s.reference != snapshot.reference)
            {
                return Err(err("CHAIN_NOT_READY"));
            }
        }
        let now = now_ms();
        if snapshot.reference.block.timestamp_ms > now.saturating_add(30000)
            || now.saturating_sub(snapshot.reference.block.timestamp_ms)
                > self.config.config.startup.max_baseline_age_ms
        {
            return Err(err("BASELINE_TOO_OLD"));
        }
        Ok(snapshot)
    }
    pub(super) async fn publish_scope_unit(
        &self,
        chain: &Chain,
        snapshot: Snapshot,
        material: ScopeMaterial,
        mut writer: Option<&mut Writer>,
    ) -> Result<()> {
        if material.chain.key != chain.key || material.chain.genesis_hash != chain.genesis_hash {
            return Err(err("SCOPE_SOURCE_CHAIN_CHANGED"));
        }
        let cfg = self
            .config
            .config
            .yellowstone
            .get(&chain.key)
            .ok_or_else(|| err("SCOPE_STREAM_CONFIG"))?;
        let now = now_ms();
        let ttl = self.config.config.control.lease_ttl_ms;
        let fence = Fence {
            chain_key: chain.key.clone(),
            chain_identity: chain.genesis_hash.clone(),
            owner: chain.owner_service.clone(),
            assignment: chain.assignment_version,
            generation: snapshot.reference.online_generation,
            canonical_epoch: snapshot.reference.canonical_epoch,
            writer_epoch: snapshot.reference.writer_epoch,
            adapter_version: snapshot.reference.adapter_set_hash.clone(),
            schema_version: snapshot.reference.schema_version.to_string(),
        };
        let lease = writer.as_ref().map(|w| w.lease.clone());
        let authority = self.scope_authority(lease.clone(), fence.clone());
        let until = lease
            .as_ref()
            .map_or(now.saturating_add(ttl), |l| l.expires_ms);
        let limits = FileScopeLimits {
            max_jobs: cfg.max_catalog_pools.min(cfg.max_wal_records) as u64,
            max_account_bytes: cfg.max_stream_bytes as u64,
            max_retained_jobs: cfg.max_catalog_pools.min(cfg.max_wal_records) as u64,
            max_snapshot_bytes: cfg.max_stream_bytes as u64,
            attempt_lease_ms: self
                .config
                .config
                .journal
                .kafka
                .transaction_timeout_ms
                .saturating_add(2 * self.config.config.control.renewal_ms)
                .saturating_add(self.config.config.journal.request_timeout_ms),
            max_authority_lease_ms: ttl,
        };
        let (send, mut receive) = tokio::sync::mpsc::channel::<PublishRequest>(1);
        let bridge: PublishBridge = Arc::new(move |cancel, job, publication| {
            let (reply, wait) = tokio::sync::oneshot::channel();
            send.blocking_send(PublishRequest {
                cancel: cancel.clone(),
                job: job.clone(),
                publication,
                reply,
            })
            .map_err(|_| ScopeError::StoreClosed)?;
            wait.blocking_recv().map_err(|_| ScopeError::StoreClosed)?
        });
        let db = self
            .stores
            .get(&chain.key)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?
            .clone();
        let backend = OwnerScopeStore::new(db, limits, Arc::new(now_ms), authority, Some(bridge))
            .map_err(scope_error)?;
        if lease.is_none() {
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .issue_embedded_scope(&fence, until)?;
        }
        let begin = backend.clone();
        let begin_fence = fence.clone();
        tokio::task::spawn_blocking(move || begin.begin(begin_fence, until))
            .await
            .map_err(|_| err("SCOPE_WORKER_JOIN"))?
            .map_err(scope_error)?;
        let subset = Self::bank_subset(&material, chain)?;
        let mut source = ManifestRecoverySource::from_committed(
            chain,
            &subset,
            material.timestamp_ms,
            &material.receipt,
            cfg.max_stream_bytes as u64,
            100,
        )
        .map_err(scope_error)?;
        source.verify_complete(chain).map_err(scope_error)?;
        if let Some(base) = self
            .certified
            .lock()
            .map_err(|_| err("SCOPE_CACHE_LOCK"))?
            .get(&chain.key)
            .cloned()
        {
            if let Ok(bank) = Self::bank_subset(&base, chain) {
                let _ = source.set_tail(
                    chain,
                    crate::manifest_recovery::TailBaseline {
                        bank: &bank,
                        receipt: &base.receipt,
                        timestamp_ms: base.timestamp_ms,
                    },
                    cfg.max_frame_bytes,
                );
            }
        }
        let account_budgets = source.dependencies();
        let catalog = chain
            .pools
            .iter()
            .map(|pool| {
                let mut one = chain.clone();
                one.pools = vec![pool.clone()];
                Ok((
                    pool.id.clone(),
                    crate::rpc::required_scope(&one)?
                        .into_iter()
                        .map(|id| {
                            account_budgets
                                .get(&id)
                                .cloned()
                                .ok_or_else(|| err("SCOPE_DEPENDENCY_MISSING"))
                        })
                        .collect::<Result<Vec<_>>>()?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let budget = ScopeLimits {
            max_jobs: cfg.max_catalog_pools.min(cfg.max_wal_records) as u64,
            max_account_bytes: cfg.max_stream_bytes as u64,
            max_tail_events: cfg.max_replay_records as u64,
            max_tail_bytes: cfg.max_stream_bytes as u64,
            max_tail_age_ms: self.config.config.startup.max_baseline_age_ms,
            max_baseline_age_ms: self.config.config.startup.max_baseline_age_ms,
            shard_accounts: 100,
        };
        let anchor = source.anchor();
        let controller = ScopeController::new(
            ScopeConfig {
                fence: fence.clone(),
                limits: budget.clone(),
                catalog,
                trusted_sources: vec![material.receipt.frame.source.source_id.clone()],
            },
            Arc::new(ProductionStore::new(Arc::new(backend.clone()))),
            now_ms,
        )
        .map_err(scope_error)?;
        let request = EnsurePoolScopeRequest {
            fence,
            anchor: anchor.clone(),
            pool_ids: chain.pools.iter().map(|p| p.id.clone()).collect(),
            budget,
            history_after: anchor.cursor,
        };
        let unit = Unit {
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: snapshot.clone(),
            normalized_facts: None,
        };
        if xone_kafka_transport::encode_unit(&unit)?.len()
            > self.config.config.journal.max_payload_bytes
        {
            return Err(err("SCOPE_UNIT_BUDGET"));
        }
        let task = tokio::task::spawn_blocking(move || {
            let cancel = Cancellation::new();
            let job = controller.ensure_pool_scope(&cancel, request)?;
            controller.recover(&cancel, &job.id, &mut source)?;
            Ok::<_, ScopeError>(job.id)
        });
        tokio::pin!(task);
        loop {
            tokio::select! {
             Some(request)=receive.recv()=>{
              let result=async {
               self.check_owner_authority(chain,lease.as_ref()).await?;
               self.validate_stream_publication(chain,&unit.snapshot)?;
               let prepare=backend.clone();let cancel=request.cancel.clone();let job=request.job.clone();let prepared_unit=unit.clone();
               tokio::task::spawn_blocking(move||prepare.prepare(&cancel,&job,request.publication,&prepared_unit)).await.map_err(|_|err("SCOPE_WORKER_JOIN"))?.map_err(scope_error)?;
               let cursor=if let Some(active)=writer.as_deref_mut(){active.publish(self.control.get().ok_or_else(||err("CONTROL_REQUIRED"))?,&self.config,unit.clone()).await?}else{xone_kafka_transport::Cursor{topic:format!("local-owner/{}",chain.key),partition:0,offset:i64::try_from(unit.snapshot.reference.journal_seq).map_err(|_|err("SEQUENCE_OVERFLOW"))?}};
               self.check_owner_authority(chain,lease.as_ref()).await?;
               self.validate_stream_publication(chain,&unit.snapshot)?;
               self.stores[&chain.key].lock().map_err(|_|err("STATE_LOCK_POISONED"))?.apply_prepared(cursor)?;
               // Final I/O is followed by both real authority and original bank freshness.
               self.check_owner_authority(chain,lease.as_ref()).await?;
               self.validate_stream_publication(chain,&unit.snapshot)?;
               Ok::<_,xone_domain::Error>(())
              }.await;
              if result.is_err(){if let Ok(mut store)=self.stores[&chain.key].lock(){store.mark_uncertain();}}
              let _=request.reply.send(result.map_err(|e|ScopeError::Storage(e.0)));
             }
             result=&mut task=>{
              let result=match result {Ok(result)=>result.map_err(scope_error),Err(_)=>Err(err("SCOPE_WORKER_JOIN"))};
              if let Err(error)=result {
               // The actor remains held while reopening and reconciling the exact intent.
               if let Some(active)=writer.as_deref_mut(){self.reconcile_scope_writer(chain,active).await?;}
               else {self.reconcile_local_scope(chain)?;}
               return Err(error);
              }
              return Ok(());
             }
            }
        }
    }
    fn scope_authority(&self, lease: Option<Lease>, fence: Fence) -> CheckAuthority {
        let pg = self.control.get().cloned();
        let handle = tokio::runtime::Handle::current();
        let timeout = self.config.config.control.renewal_ms;
        let store = self.stores[&fence.chain_key].clone();
        let owner_session = store.lock().map(|s| s.scope_session).unwrap_or(0);
        Arc::new(move |requested| {
            store
                .lock()
                .map_err(|_| ScopeError::StoreClosed)?
                .check_open_session(owner_session)
                .map_err(|_| ScopeError::FenceChanged)?;
            if *requested != fence {
                return Err(ScopeError::FenceChanged);
            }
            if let (Some(pg), Some(lease)) = (&pg, &lease) {
                let result = handle.block_on(async {
                    tokio::time::timeout(
                        Duration::from_millis(timeout),
                        pg.check_scope_authority(
                            lease,
                            xone_control::OwnerFence {
                                chain_key: &requested.chain_key,
                                owner_service: &requested.owner,
                                assignment: requested.assignment,
                                generation: requested.generation,
                                canonical_epoch: requested.canonical_epoch,
                                journal_seq: None,
                            },
                            0,
                        ),
                    )
                    .await
                    .map_err(|_| ScopeError::FenceChanged)?
                    .map_err(|_| ScopeError::FenceChanged)
                });
                result?;
                store
                    .lock()
                    .map_err(|_| ScopeError::StoreClosed)?
                    .check_open_session(owner_session)
                    .map_err(|_| ScopeError::FenceChanged)
            } else {
                let db = store.lock().map_err(|_| ScopeError::StoreClosed)?;
                db.check_embedded_scope(Some(requested))
                    .map_err(|_| ScopeError::FenceChanged)
            }
        })
    }
    pub(super) async fn check_owner_authority(
        &self,
        chain: &Chain,
        lease: Option<&Lease>,
    ) -> Result<()> {
        if let Some(pg) = self.control.get() {
            let l = lease.ok_or_else(|| err("LEASE_NOT_OWNED"))?;
            tokio::time::timeout(
                Duration::from_millis(self.config.config.control.renewal_ms),
                pg.check(l, 0),
            )
            .await
            .map_err(|_| err("CONTROL_DEADLINE"))??;
        } else {
            let db = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            if self.config.config.yellowstone.contains_key(&chain.key) {
                db.check_embedded_scope(None)?;
            } else {
                db.check_open_session(db.scope_session)?;
            }
        }
        Ok(())
    }
    pub(super) fn reconcile_local_scope(&self, chain: &Chain) -> Result<()> {
        let mut db = self.stores[&chain.key]
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?;
        if db.is_uncertain() {
            db.reopen()?;
        }
        if let Some(unit) = db.prepared()? {
            // Local owner has one atomic DB commit; an unapplied prepared record is
            // definitively uncommitted, unlike a broker transaction with unknown outcome.
            let actual = db.persisted_snapshot()?;
            if actual
                .as_ref()
                .is_some_and(|s| s.reference == unit.snapshot.reference)
            {
                return Err(err("LOCAL_PREPARED_COMMIT_CONFLICT"));
            }
            db.discard_prepared()?;
        }
        Ok(())
    }
    async fn reconcile_scope_writer(&self, chain: &Chain, active: &mut Writer) -> Result<()> {
        let pg = self.control.get().ok_or_else(|| err("CONTROL_REQUIRED"))?;
        active.close(pg, &self.config).await;
        let (previous, pending) = {
            let mut db = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            if db.is_uncertain() {
                db.reopen()?;
            }
            (db.previous(), db.prepared()?)
        };
        let (opened, recovery) = Writer::open(pg, &self.config, chain, previous, pending).await?;
        {
            let mut db = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            if let Some(cursor) = recovery {
                db.apply_prepared(cursor)?;
            } else {
                db.discard_prepared()?;
            }
            db.suspend();
        }
        *active = opened;
        Ok(())
    }
}
fn scope_error(error: ScopeError) -> xone_domain::Error {
    err(&error.to_string())
}
