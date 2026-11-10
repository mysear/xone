//! Normal Parser-owned public Yellowstone ingestion. All owner publication goes
//! through Parser::commit_snapshot; raw receipts and independent proofs are separate.
use super::*;
use crate::{
    async_frame_wal::StreamWalWriter,
    bank_manifest::{AdmittedCatalog, BankManifest, VerifiedManifest},
    stream::*,
    yellowstone::{Subscription, TransportConfig},
};
use serde::{Deserialize, Serialize};
use xone_config::YellowstoneConfig;

#[derive(Clone, Debug, Default, Serialize)]
pub struct StreamStatus {
    pub phase: String,
    pub source_id: String,
    pub session_id: String,
    pub wal_generation: u64,
    pub captured_cursor: u64,
    pub committed_cursor: u64,
    pub source_sequence: u64,
    pub replay_records: u64,
    pub replay_sufficient: bool,
    pub session_committed: bool,
    pub requested_from_slot: Option<u64>,
    pub catalog_identity: String,
    pub last_error: String,
}
/// Stored inside Snapshot.source so the existing prepared Unit, local owner DB,
/// Kafka JournalUnit and bootstrap all bind the same immutable evidence hashes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamEvidence {
    pub version: u32,
    pub kind: String,
    pub source_id: String,
    pub session_id: String,
    pub proto_commit: String,
    pub bank_id: u64,
    pub wal_generation: u64,
    pub wal_cursor: u64,
    pub wal_record_sha256: String,
    pub manifest_sha256: String,
    #[serde(default)]
    pub manifest_bytes: u64,
    pub catalog_revision: u64,
    pub catalog_identity: String,
}
struct SourceRegistration {
    path: std::path::PathBuf,
    body: serde_json::Value,
    ttl: u64,
}
impl SourceRegistration {
    fn open(
        parser: &Parser,
        chain: &Chain,
        cfg: &YellowstoneConfig,
        session: &str,
    ) -> Result<Self> {
        let catalog = AdmittedCatalog::load(&parser.config.resolve(&cfg.catalog_file), chain, cfg)?;
        let directory = parser.config.resolve(&cfg.manifest_directory);
        std::fs::create_dir_all(&directory).map_err(|_| err("NATIVE_SOURCE_REQUEST_IO"))?;
        let target = parser.scope_target(chain, &catalog.chain(chain))?;
        let body = json!({"version":1,"chain_key":chain.key,"genesis_hash":chain.genesis_hash,"source_id":cfg.source_id,"session_id":session,"catalog_identity":catalog.identity()?,"catalog_revision":catalog.revision,"scope_accounts":crate::rpc::required_scope(&target)?,"expires_ms":0});
        let mut result = Self {
            path: directory.join(format!("{session}.request")),
            body,
            ttl: parser.config.config.control.lease_ttl_ms.min(300_000),
        };
        result.refresh()?;
        Ok(result)
    }
    fn refresh(&mut self) -> Result<()> {
        let now = now_ms();
        if self.body["expires_ms"].as_u64().unwrap_or(0) > now.saturating_add(self.ttl / 2) {
            return Ok(());
        }
        self.body["expires_ms"] = json!(now.saturating_add(self.ttl));
        let bytes = serde_json::to_vec(&self.body).map_err(|_| err("NATIVE_SOURCE_REQUEST_IO"))?;
        let tmp = self.path.with_extension("request.tmp");
        std::fs::write(&tmp, bytes).map_err(|_| err("NATIVE_SOURCE_REQUEST_IO"))?;
        std::fs::rename(tmp, &self.path).map_err(|_| err("NATIVE_SOURCE_REQUEST_IO"))
    }
}
impl Drop for SourceRegistration {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
impl Parser {
    pub fn stream_status(&self, chain: &str) -> Result<StreamStatus> {
        self.stream_statuses
            .lock()
            .map_err(|_| err("STREAM_STATUS_LOCK"))?
            .get(chain)
            .cloned()
            .ok_or_else(|| err("STREAM_NOT_CONFIGURED"))
    }
    fn stream_update(&self, chain: &str, update: impl FnOnce(&mut StreamStatus)) {
        if let Ok(mut statuses) = self.stream_statuses.lock() {
            update(statuses.entry(chain.into()).or_default());
        }
    }
    pub(super) fn admitted_chain(&self, chain: &Chain) -> Result<(Chain, Option<String>)> {
        let Some(cfg) = self.config.config.yellowstone.get(&chain.key) else {
            return Ok((chain.clone(), None));
        };
        let catalog = AdmittedCatalog::load(&self.config.resolve(&cfg.catalog_file), chain, cfg)?;
        let chain = catalog.chain(chain);
        let mut validated = self.config.config.clone();
        let target = validated
            .chains
            .iter_mut()
            .find(|c| c.key == chain.key)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
        *target = chain.clone();
        validated.validate("chain-parser-rust")?;
        Ok((chain, Some(catalog.identity()?)))
    }
    pub(super) fn validate_stream_publication(
        &self,
        chain: &Chain,
        snapshot: &Snapshot,
    ) -> Result<()> {
        if self.source.is_some() || !self.config.config.yellowstone.contains_key(&chain.key) {
            return Ok(());
        }
        let evidence: StreamEvidence = serde_json::from_str(&snapshot.source)
            .map_err(|_| err("STREAM_OWNER_EVIDENCE_REQUIRED"))?;
        let (_, identity) = self.admitted_chain(chain)?;
        if evidence.version != 1
            || evidence.proto_commit != PROJECTION_SCHEMA_COMMIT
            || evidence.wal_cursor == 0
            || identity.as_deref() != Some(&evidence.catalog_identity)
        {
            return Err(err("STREAM_OWNER_EVIDENCE_CHANGED"));
        }
        require_fresh(self, snapshot.reference.block.timestamp_ms)
    }
    pub(super) async fn run_stream(
        self,
        chain: Chain,
        cfg: YellowstoneConfig,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut try_replay = false;
        loop {
            if *stop.borrow() {
                break;
            }
            self.stream_update(&chain.key, |s| {
                s.phase = "CONNECTING".into();
                s.session_committed = false;
                s.source_id = cfg.source_id.clone();
            });
            let result = self
                .stream_connection(&chain, &cfg, &mut stop, try_replay)
                .await;
            let was_live = self
                .stream_status(&chain.key)
                .is_ok_and(|s| s.session_committed);
            // This gate immediately suppresses current()/ready before awaiting the
            // owner actor. Kafka publication, if started, must settle under it.
            self.stream_update(&chain.key, |s| {
                s.phase = "HEAD_FIRST_REQUIRED".into();
                s.last_error = result.as_ref().err().map_or(String::new(), |e| e.0.clone());
            });
            {
                let _actor = self.recovery_lock.lock().await;
                if let Err(error) = self.revoke_scope(&chain).await {
                    self.logger.event(
                        "error",
                        "stream_scope_revocation_failed",
                        json!({"chain_key":chain.key,"error_code":error.0}),
                    );
                }
            }
            if let Some(pg) = self.control.get() {
                let mut writer = self.writers[&chain.key].lock().await;
                if let Some(active) = writer.as_mut() {
                    active.close(pg, &self.config).await;
                }
                *writer = None;
            }
            if let Err(e) = &result {
                self.logger.event(
                    "warn",
                    "yellowstone_reanchor_required",
                    json!({"chain_key":chain.key,"error_code":e.0}),
                );
            }
            if *stop.borrow() {
                break;
            }
            // A rejected replay request falls back to HEAD_FIRST on the next
            // connection. Source write versions never cross session boundaries.
            try_replay = was_live;
            tokio::select! { _=stop.changed()=>{}, _=tokio::time::sleep(Duration::from_millis(cfg.reconnect_delay_ms))=>{} }
        }
        self.stream_update(&chain.key, |s| s.phase = "STOPPED".into());
    }
    async fn stream_connection(
        &self,
        chain: &Chain,
        cfg: &YellowstoneConfig,
        stop: &mut tokio::sync::watch::Receiver<bool>,
        try_replay: bool,
    ) -> Result<()> {
        let path = self
            .config
            .resolve(&self.config.config.state.path)
            .join("yellowstone")
            .join(chain.key.replace(':', "-"));
        let limits = StreamLimits {
            max_bytes: cfg.max_stream_bytes,
            max_slots: cfg.max_stream_slots,
            max_updates: cfg.max_stream_updates,
        };
        let wal = FrameWal::open(
            &path,
            &chain.key,
            FrameWalLimits {
                max_frame_bytes: cfg.max_frame_bytes,
                max_total_bytes: cfg.max_wal_bytes,
                max_records: cfg.max_wal_records,
            },
        )?;
        let recovered = wal.recovery_cursor();
        let pending = wal.len().saturating_sub(recovered.sequence);
        let mut replay_records = 0;
        // No old receipt is admitted as LIVE. Without durable parent/manifest
        // continuity, even a complete raw replay cannot restore online authority.
        if pending <= cfg.max_replay_records as u64 {
            let mut projection_failed = false;
            wal.replay_from(&recovered, |record| {
                replay_records += 1;
                projection_failed |= project_yellowstone(&record, limits).is_err();
                Ok(())
            })?;
            if projection_failed {
                self.logger.event(
                    "warn",
                    "stream_replay_requires_head_first",
                    json!({"chain_key":chain.key}),
                );
            }
        }
        let generation = wal.generation();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| err("YELLOWSTONE_SESSION_CLOCK"))?
            .as_nanos();
        let session_id = format!(
            "wal-{generation:016x}-{nonce:032x}-{:x}",
            std::process::id()
        );
        let last = self.stores[&chain.key]
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .previous();
        let from = if try_replay && pending <= cfg.max_replay_records as u64 {
            last.as_ref()
                .filter(|r| {
                    now_ms().saturating_sub(r.block.timestamp_ms)
                        <= self.config.config.startup.max_baseline_age_ms
                })
                .and_then(|r| r.block.number_or_slot.checked_add(1))
        } else {
            None
        };
        self.stream_update(&chain.key, |s| {
            s.session_id = session_id.clone();
            s.wal_generation = generation;
            s.committed_cursor = recovered.sequence;
            s.replay_records = replay_records;
            s.replay_sufficient = false;
            s.requested_from_slot = from;
        });
        let mut retained_estimate = wal.retained_bytes() as u64;
        let writer = StreamWalWriter::open(wal, cfg.wal_ack_timeout_ms)?;
        let mut rotation = None;
        let result = async {
            let mut native_request=SourceRegistration::open(self,chain,cfg,&session_id)?;
            let token = read_optional_token(self, cfg)?;
            let transport = TransportConfig {
                endpoint: cfg.endpoint.clone(),
                source_id: cfg.source_id.clone(),
                connect_timeout_ms: cfg.connect_timeout_ms,
                idle_timeout_ms: cfg.idle_timeout_ms,
                max_frame_bytes: cfg.max_frame_bytes,
                allow_loopback_plaintext: cfg.allow_loopback_plaintext,
            };
            let mut subscription = tokio::select! {
                biased;
                _ = stop.changed() => { return Ok(()); }
                result = Subscription::connect_finalized(transport, session_id, token.as_deref(), from) => { result? }
            };
            let mut assembler: Option<StreamAssembler> = None;
            let mut assembly_scope = std::collections::BTreeSet::new();
            let mut proofs = BTreeMap::<BankId, VerifiedManifest>::new();
            let mut catalog_identity = None;
            let mut published_in_session = false;
            loop {
                if *stop.borrow() {
                    return Ok(());
                }
                native_request.refresh()?;
                let record = tokio::select! {
                    biased;
                    _ = stop.changed() => { return Ok(()); }
                    result = subscription.capture_next(&writer) => { result? }
                };
                retained_estimate=retained_estimate.saturating_add(record.frame.protobuf.len() as u64+4096);
                self.stream_update(&chain.key, |s| {
                    s.captured_cursor = record.cursor.sequence;
                    s.source_sequence = record.frame.sequence;
                });
                // The only projector input comes from a durable live ACK.
                let event = project_yellowstone(&record, limits)?;
                subscription.respond_to_ping(&record)?;
                let catalog = AdmittedCatalog::load(&self.config.resolve(&cfg.catalog_file), chain, cfg)?;
                let identity = catalog.identity()?;
                if catalog_identity
                    .as_ref()
                    .is_some_and(|old| *old != identity)
                {
                    return Err(err("STREAM_ADMITTED_CATALOG_CHANGED"));
                }
                let (admitted, validated_identity) = self.admitted_chain(chain)?;
                if validated_identity.as_ref() != Some(&identity) {
                    return Err(err("STREAM_ADMITTED_CATALOG_CHANGED"));
                }
                catalog_identity = Some(identity.clone());
                let target = self.scope_target(chain, &admitted)?;
                let requested_scope = crate::rpc::required_scope(&target)?.iter()
                    .map(|id| crate::bank_manifest::key(id)).collect::<Result<std::collections::BTreeSet<_>>>()?;
                if assembler.is_some() && !requested_scope.is_subset(&assembly_scope) {
                    // Scope expansion requires a new native request/session and
                    // a newly completed bank. Never reuse a smaller proof.
                    return Err(err("STREAM_SCOPE_CHANGED"));
                }
                let requested_accounts=serde_json::to_value(crate::rpc::required_scope(&target)?)
                    .map_err(|_|err("NATIVE_SOURCE_REQUEST_IO"))?;
                if native_request.body["scope_accounts"] != requested_accounts {
                    native_request.body["scope_accounts"]=requested_accounts;
                    native_request.body["expires_ms"]=json!(0);
                    native_request.refresh()?;
                }
                let bank = event_bank(&event);
                if let Some(bank) = bank {
                    if !published_in_session
                        && from.is_some_and(|from| bank.slot.saturating_sub(from) >= cfg.max_replay_slots)
                    {
                        return Err(err("STREAM_REPLAY_WINDOW_INSUFFICIENT"));
                    }
                    if !proofs.contains_key(&bank) {
                        if proofs.len() >= cfg.max_stream_slots {
                            return Err(err("BANK_MANIFEST_WINDOW_LIMIT"));
                        }
                        let filename = format!(
                            "{}-{}-{}.json",
                            record.frame.source.session_id, bank.slot, bank.bank_id
                        );
                        let proof_result = BankManifest::load(
                            &self.config.resolve(&cfg.manifest_directory).join(filename),
                            cfg,
                            &target,
                            &catalog,
                            &record.frame.source,
                            &bank,
                        );
                        let mut proof=match proof_result {
                            Err(error) if assembler.is_none() && error.0=="BANK_AUTHORITY_UNAVAILABLE"=>{
                                self.stream_update(&chain.key,|s|{s.phase="AWAITING_NATIVE_MANIFEST".into();s.last_error=error.0;});continue;
                            }
                            result=>result?,
                        };
                        if proofs
                            .values()
                            .map(|p| p.retained_bytes)
                            .sum::<usize>()
                            .checked_add(proof.retained_bytes)
                            .is_none_or(|n| n > cfg.max_stream_bytes)
                        {
                            return Err(err("BANK_MANIFEST_WINDOW_LIMIT"));
                        }
                        require_fresh(self, proof.manifest.timestamp_ms)?;
                        if assembler.is_none() {
                            let mut a = StreamAssembler::new(limits)?;
                            a.anchor_accounts(
                                proof.parent.clone(),
                                proof.parent_proof.clone(),
                                record.frame.sequence,
                                proof.scope.clone(),
                                record.wal_generation,
                            )?;
                            assembly_scope = proof.scope.clone();
                            assembler = Some(a);
                        }
                        let a = assembler.as_mut().ok_or_else(|| err("STREAM_ANCHOR"))?;
                        if !proof.manifest.skipped_slots.is_empty() {
                            a.prove_skipped(SkippedRangeProof {
                                source: record.frame.source.clone(),
                                parent: proof.manifest.parent.anchor(),
                                target: proof.manifest.bank.anchor(),
                                skipped_slots: proof.manifest.skipped_slots.clone(),
                            })?;
                        }
                        a.seal_native_versions(&record.frame.source,std::mem::take(&mut proof.native_writes))?;
                        a.seal_accounts(&record.frame.source, proof.completion.clone())?;
                        proofs.insert(bank, proof);
                        self.ingest_refs.lock().map_err(|_|err("SOURCE_REFS_LOCK"))?.insert(chain.key.clone(),proofs.values().flat_map(|p|[p.manifest.bank.blockhash.clone(),p.manifest.parent.blockhash.clone()]).collect());
                    }
                }
                let Some(a) = assembler.as_mut() else {
                    continue;
                };
                a.admit_captured(&record, event)?;
                if proofs.values().map(|p|p.retained_bytes).sum::<usize>().checked_add(a.usage().bytes).is_none_or(|n|n>cfg.max_stream_bytes){return Err(err("STREAM_COMBINED_BUFFER_LIMIT"));}
                if published_in_session && !a.ready() {
                    self.stream_update(&chain.key, |s| s.phase = "ASSEMBLING_UNCERTAIN".into());
                }
                while let Some(committed) = a.commit_next()? {
                    let proof = proofs
                        .remove(&committed.bank)
                        .ok_or_else(|| err("BANK_MANIFEST_REQUIRED"))?;
                    // A complete intermediate bank cannot reopen owner scope
                    // while a higher finalized boundary still lacks evidence.
                    if !a.ready() { continue; }
                    let full=Arc::new(committed);
                    let material=ScopeMaterial{chain:admitted.clone(),bank:full,receipt:record.clone(),timestamp_ms:proof.manifest.timestamp_ms,source:String::new()};
                    let subset=Self::bank_subset(&material,&target)?;
                    let mut snapshot = crate::manifest_recovery::restore_snapshot(
                        &target,
                        &subset,
                        proof.manifest.timestamp_ms,
                        &record,
                        cfg,
                    )?;
                    let evidence = StreamEvidence {
                        version: 1,
                        kind: "YELLOWSTONE_INDEPENDENT_VERSIONED_BANK".into(),
                        source_id: record.frame.source.source_id.clone(),
                        session_id: record.frame.source.session_id.clone(),
                        proto_commit: PROJECTION_SCHEMA_COMMIT.into(),
                        bank_id: material.bank.bank.bank_id,
                        wal_generation: record.wal_generation,
                        wal_cursor: record.cursor.sequence,
                        wal_record_sha256: record.cursor.record_sha256.clone(),
                        manifest_sha256: xone_domain::hash(&proof.manifest)?,
                        manifest_bytes:proof.source_bytes as u64,
                        catalog_revision: catalog.revision,
                        catalog_identity: identity.clone(),
                    };
                    snapshot.source =
                        serde_json::to_string(&evidence).map_err(|_| err("STREAM_EVIDENCE_ENCODING"))?;
                    let _actor = self.recovery_lock.lock().await;
                    if *stop.borrow() {
                        return Ok(());
                    }
                    let (_, current_identity) = self.admitted_chain(chain)?;
                    if current_identity != Some(identity.clone()) {
                        return Err(err("STREAM_ADMITTED_CATALOG_CHANGED"));
                    }
                    require_fresh(self, snapshot.reference.block.timestamp_ms)?;
                    snapshot.validate(&admitted.owner_service, admitted.assignment_version)?;
                    {
                        let store = self.stores[&chain.key]
                            .lock()
                            .map_err(|_| err("STATE_LOCK_POISONED"))?;
                        if store.previous().is_some_and(|r|proof.manifest.parent.slot==r.block.number_or_slot && proof.manifest.parent.blockhash!=r.block.hash){return Err(err("STREAM_FINALIZED_PARENT_CONFLICT"));}
                        if store.previous().is_some_and(|r| {
                            r.block.number_or_slot > snapshot.reference.block.number_or_slot
                                || (r.block.number_or_slot == snapshot.reference.block.number_or_slot
                                    && r.block.hash != snapshot.reference.block.hash)
                        }) {
                            return Err(err("STREAM_FINALIZED_BANK_REGRESSION"));
                        }
                        if let Some(previous) = store.persisted_snapshot()? {
                            if let Ok(old) = serde_json::from_str::<StreamEvidence>(&previous.source) {
                                if catalog.revision < old.catalog_revision
                                    || (catalog.revision == old.catalog_revision
                                        && identity != old.catalog_identity)
                                {
                                    return Err(err("STREAM_CATALOG_REVISION_REGRESSION"));
                                }
                            }
                            snapshot.history_gaps = previous.history_gaps;
                        }
                        if snapshot.history_gaps.len() >= cfg.max_wal_records {
                            return Err(err("STREAM_HISTORY_GAP_LIMIT"));
                        }
                        if !published_in_session {
                            if let Some(old) = store.previous() {
                                let from = old.block.number_or_slot.saturating_add(1);
                                let to = snapshot.reference.block.number_or_slot.saturating_sub(1);
                                if from <= to {
                                    snapshot.history_gaps.push(Gap {
                                        from,
                                        to,
                                        reason: "HEAD_FIRST_SOURCE_SESSION_CONTINUITY_UNPROVEN".into(),
                                    });
                                }
                            } else if snapshot.reference.block.number_or_slot > 1 {
                                snapshot.history_gaps.push(Gap {
                                    from: 1,
                                    to: snapshot.reference.block.number_or_slot - 1,
                                    reason: "HEAD_FIRST_CURRENT_BANK_BASELINE".into(),
                                });
                            }
                        }
                    }
                    let mut material=material;material.source=snapshot.source.clone();
                    self.commit_snapshot(&target,snapshot.clone(),false,Some(material.clone())).await?;
                    self.certified.lock().map_err(|_|err("SCOPE_CACHE_LOCK"))?.insert(chain.key.clone(),material);
                    self.ingest_refs.lock().map_err(|_|err("SOURCE_REFS_LOCK"))?.insert(chain.key.clone(),proofs.values().flat_map(|p|[p.manifest.bank.blockhash.clone(),p.manifest.parent.blockhash.clone()]).collect());
                    // Checkpoint belongs to the committed owner state. Failed or
                    // ambiguous checkpoints force revocation and a new session.
                    let committed_wal=self.stores[&chain.key].lock().map_err(|_|err("STATE_LOCK_POISONED"))?.committed_wal()?;
                    writer.checkpoint_committed(committed_wal.clone()).await?;
                    published_in_session = true;
                    self.stream_update(&chain.key, |s| {
                        s.phase = "LIVE".into();
                        s.session_committed = true;
                        s.last_error.clear();
                        s.committed_cursor = record.cursor.sequence;
                        s.catalog_identity = identity.clone();
                    });
                    let native_refs=self.stores[&chain.key].lock().map_err(|_|err("STATE_LOCK_POISONED"))?.active_manifest_refs()?;
                    let native_bytes=native_refs.iter().map(|r|r.bytes).sum::<u64>();
                    if proofs.is_empty() && (record.cursor.sequence>=cfg.max_wal_records as u64 || native_refs.len()>=cfg.max_stream_slots || native_bytes.saturating_add(retained_estimate).saturating_add(65536)>=cfg.max_wal_bytes as u64/2) {
                        rotation=Some(committed_wal);return Err(err("STREAM_WAL_SEGMENT_ROTATED"));
                    }
                }
                if published_in_session && !a.ready() {
                    let _actor = self.recovery_lock.lock().await;
                    self.revoke_scope(&admitted).await?;
                }
            }
        }.await;
        // Seal live visibility before any bounded drain wait: an unacknowledged
        // write may still finish durably, but can no longer sustain online scope.
        self.stream_update(&chain.key, |s| s.phase = "DRAIN_AND_MARK_UNCERTAIN".into());
        // Never open a replacement while the previous worker still owns RocksDB.
        writer.close().await?;
        self.ingest_refs
            .lock()
            .map_err(|_| err("SOURCE_REFS_LOCK"))?
            .remove(&chain.key);
        if let Some(proof) = rotation {
            let _actor = self.recovery_lock.lock().await;
            let protected = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .protected_wal_sessions()?;
            let native_refs = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .active_manifest_refs()?;
            let completed_refs = native_refs.clone();
            let native_directory = self.config.resolve(&cfg.manifest_directory);
            let archive_path = path.clone();
            let max_segments = cfg
                .archive_retention
                .as_ref()
                .map_or(cfg.max_stream_slots, |a| a.max_segments);
            let max_bytes = cfg
                .archive_retention
                .as_ref()
                .map_or(cfg.max_wal_bytes as u64, |a| a.max_bytes);
            tokio::task::spawn_blocking(move || {
                crate::wal_segments::rotate(
                    &archive_path,
                    proof,
                    &protected,
                    max_segments,
                    max_bytes,
                    &native_directory,
                    &native_refs,
                )
            })
            .await
            .map_err(|_| err("WAL_SEGMENT_WORKER"))??;
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .release_manifest_refs(&completed_refs)?;
        }
        result
    }
}
fn event_bank(event: &YellowstoneEvent) -> Option<BankId> {
    match event {
        YellowstoneEvent::Account(a) => Some(a.bank.clone()),
        YellowstoneEvent::Transaction(t) => Some(t.bank.clone()),
        YellowstoneEvent::Metadata(m) => Some(m.bank.clone()),
        YellowstoneEvent::ProviderBlock(b) => Some(b.metadata.bank.clone()),
        YellowstoneEvent::Slot(s) => Some(s.bank.clone()),
        _ => None,
    }
}
fn require_fresh(parser: &Parser, timestamp: u64) -> Result<()> {
    let now = now_ms();
    if timestamp > now.saturating_add(30000)
        || now.saturating_sub(timestamp) > parser.config.config.startup.max_baseline_age_ms
    {
        return Err(err("BANK_MANIFEST_STALE"));
    }
    Ok(())
}
fn read_optional_token(parser: &Parser, cfg: &YellowstoneConfig) -> Result<Option<String>> {
    use std::io::Read;
    let Some(path) = &cfg.token_file else {
        return Ok(None);
    };
    let file = std::fs::File::open(parser.config.resolve(path))
        .map_err(|_| err("YELLOWSTONE_TOKEN_FILE"))?;
    let meta = file.metadata().map_err(|_| err("YELLOWSTONE_TOKEN_FILE"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(err("YELLOWSTONE_TOKEN_PERMISSIONS"));
        }
    }
    if !meta.is_file() || meta.len() > 4096 {
        return Err(err("YELLOWSTONE_TOKEN_FILE"));
    }
    let mut value = String::new();
    file.take(4097)
        .read_to_string(&mut value)
        .map_err(|_| err("YELLOWSTONE_TOKEN_FILE"))?;
    let value = value.trim();
    if value.is_empty() || value.len() > 4096 {
        return Err(err("YELLOWSTONE_TOKEN_FILE"));
    }
    Ok(Some(value.into()))
}
