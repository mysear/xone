use crate::session::Writer;
use crate::source_wal::{Recorder, SourceWal, WalLimits};
use crate::{rpc::Rpc, store::Store};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};
use xone_config::{Chain, ConfigSnapshot};
use xone_control::Postgres;
use xone_domain::{err, Gap, Result, Snapshot};
use xone_kafka_transport::Unit;
use xone_observability::{now_ms, Logger};
#[path = "normal_stream.rs"]
mod normal_stream;
pub use normal_stream::{StreamEvidence, StreamStatus};
#[path = "service_scope_runtime.rs"]
mod scope_runtime;
use scope_runtime::ScopeMaterial;

pub type SnapshotSource = Arc<dyn Fn(&Chain) -> Result<Snapshot> + Send + Sync>;
#[derive(Clone)]
pub struct Parser {
    ingest_refs: Arc<Mutex<BTreeMap<String, std::collections::BTreeSet<String>>>>,
    certified: Arc<Mutex<BTreeMap<String, ScopeMaterial>>>,
    stream_statuses: Arc<Mutex<BTreeMap<String, StreamStatus>>>,
    stream_stop: tokio::sync::watch::Sender<bool>,
    stream_tasks: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    recovery_lock: Arc<tokio::sync::Mutex<()>>,
    started: Arc<std::sync::atomic::AtomicBool>,
    control: Arc<tokio::sync::OnceCell<Postgres>>,
    writers: Arc<BTreeMap<String, tokio::sync::Mutex<Option<Writer>>>>,
    pub config: Arc<ConfigSnapshot>,
    pub logger: Logger,
    stores: Arc<BTreeMap<String, Arc<Mutex<Store>>>>,
    rpc: Rpc,
    history: Arc<crate::history_runtime::HistoryRuntime>,
    source_wals: Arc<BTreeMap<String, Recorder>>,
    source: Option<SnapshotSource>,
}
impl Parser {
    pub fn new(config: ConfigSnapshot) -> Result<Self> {
        Self::with_source(config, None)
    }
    pub fn with_source(config: ConfigSnapshot, source: Option<SnapshotSource>) -> Result<Self> {
        config.config.validate("chain-parser-rust")?;
        if config.config.service.role != "online" {
            return Err(err("ANALYTICS_ROLE_NOT_ENABLED"));
        }
        let mut stores = BTreeMap::new();
        for chain in &config.config.chains {
            stores.insert(
                chain.key.clone(),
                Arc::new(Mutex::new(Store::open(
                    &config
                        .resolve(&config.config.state.path)
                        .join(chain.key.replace(':', "-")),
                    &chain.owner_service,
                    chain.assignment_version,
                )?)),
            );
        }
        let mut source_wals = BTreeMap::new();
        if source.is_none() {
            for chain in &config.config.chains {
                if config.config.yellowstone.contains_key(&chain.key) {
                    continue;
                }
                let path = config
                    .resolve(&config.config.state.path)
                    .join("source-evidence")
                    .join(chain.key.replace(':', "-"));
                let wal = SourceWal::open(
                    &path,
                    &chain.key,
                    WalLimits {
                        max_response_bytes: config.config.journal.max_payload_bytes,
                        max_records: config.config.journal.kafka.max_tail_records,
                    },
                )?;
                source_wals.insert(chain.key.clone(), Recorder::new(wal));
            }
        }
        let logger = Logger::new(&config);
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(
                config.config.journal.request_timeout_ms,
            ))
            .build()
            .map_err(|_| err("RPC_CLIENT_CONFIG"))?;
        let max_bytes = config.config.journal.max_payload_bytes;
        let writers = config
            .config
            .chains
            .iter()
            .map(|c| (c.key.clone(), tokio::sync::Mutex::new(None)))
            .collect();
        let (stream_stop, _) = tokio::sync::watch::channel(false);
        let stores = Arc::new(stores);
        let rpc = Rpc { http, max_bytes };
        let history = crate::history_runtime::HistoryRuntime::new(&config, &stores, rpc.clone())?;
        Ok(Self {
            ingest_refs: Arc::new(Mutex::new(BTreeMap::new())),
            certified: Arc::new(Mutex::new(BTreeMap::new())),
            stream_statuses: Arc::new(Mutex::new(
                config
                    .config
                    .yellowstone
                    .keys()
                    .map(|key| (key.clone(), StreamStatus::default()))
                    .collect(),
            )),
            stream_stop,
            stream_tasks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            recovery_lock: Arc::new(tokio::sync::Mutex::new(())),
            started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            control: Arc::new(tokio::sync::OnceCell::new()),
            writers: Arc::new(writers),
            config: Arc::new(config),
            logger,
            stores,
            rpc,
            history,
            source_wals: Arc::new(source_wals),
            source,
        })
    }
    pub async fn start(&self) -> Result<()> {
        let _startup = self.recovery_lock.lock().await;
        if self.started.load(Ordering::Acquire) || *self.stream_stop.borrow() {
            return Err(err("PARSER_ALREADY_STARTED_OR_CLOSED"));
        }
        if self.config.config.journal.transport == "kafka" {
            let pg = Postgres::open(&self.config.control_dsn).await?;
            self.control
                .set(pg)
                .map_err(|_| err("CONTROL_ALREADY_INITIALIZED"))?;
        }
        for chain in &self.config.config.chains {
            if self.source.is_none() {
                if let Some(stream) = self.config.config.yellowstone.get(&chain.key) {
                    self.recover_revocation(chain).await?;
                    let task = tokio::spawn(self.clone().run_stream(
                        chain.clone(),
                        stream.clone(),
                        self.stream_stop.subscribe(),
                    ));
                    self.stream_tasks.lock().await.push(task);
                    continue;
                }
            }
            if let Err(e) = self.reanchor(chain, true).await {
                if e.0 != "LEASE_NOT_OWNED" {
                    return Err(e);
                }
                self.logger
                    .event("info", "writer_standby", json!({"chain_key":chain.key}));
            }
        }
        self.started.store(true, Ordering::Release);
        self.history.start(self.stream_stop.subscribe()).await;
        Ok(())
    }
    async fn recover_revocation(&self, chain: &Chain) -> Result<()> {
        let (previous, pending) = {
            let mut store = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            let pending = store.prepared()?;
            if pending
                .as_ref()
                .is_none_or(|u| u.kind != "ONLINE_SCOPE_REVOKED")
            {
                return Ok(());
            }
            store.suspend();
            (store.previous(), pending)
        };
        let Some(pg) = self.control.get() else {
            return Err(err("REVOCATION_CONTROL_REQUIRED"));
        };
        let mut writer = self.writers[&chain.key].lock().await;
        if let Some(old) = writer.as_mut() {
            old.close(pg, &self.config).await;
        }
        *writer = None;
        let (mut opened, recovery) =
            Writer::open(pg, &self.config, chain, previous, pending).await?;
        let applied = {
            let mut store = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            let result = if let Some(cursor) = recovery {
                store.apply_prepared(cursor)
            } else {
                store.discard_prepared()
            };
            store.suspend();
            result
        };
        if let Err(e) = applied {
            opened.close(pg, &self.config).await;
            return Err(e);
        }
        *writer = Some(opened);
        self.logger.event(
            "warn",
            "scope_revocation_reconciled",
            json!({"chain_key":chain.key}),
        );
        Ok(())
    }
    pub async fn reanchor(&self, chain: &Chain, startup: bool) -> Result<()> {
        if self.source.is_none() && self.config.config.yellowstone.contains_key(&chain.key) {
            return Err(err("STREAM_RECOVERY_OWNS_CHAIN"));
        }
        self.recover_revocation(chain).await?;
        let snapshot: Snapshot = if let Some(source) = &self.source {
            source(chain)?
        } else {
            tokio::time::timeout(
                Duration::from_millis(self.config.config.journal.request_timeout_ms),
                self.rpc
                    .current_baseline_recorded(chain, self.source_wals.get(&chain.key)),
            )
            .await
            .map_err(|_| err("BASELINE_DEADLINE"))??
        };
        snapshot.validate(&chain.owner_service, chain.assignment_version)?;
        if snapshot.reference.block.chain_key != chain.key {
            return Err(err("BASELINE_WRONG_CHAIN"));
        }
        if snapshot.pools.is_empty() {
            return Err(err("EMPTY_POOL_SCOPE"));
        }
        let timestamp = snapshot.reference.block.timestamp_ms;
        let now = now_ms();
        if timestamp > now + 30000
            || now.saturating_sub(timestamp) > self.config.config.startup.max_baseline_age_ms
        {
            return Err(err("BASELINE_TOO_OLD"));
        }
        self.commit_snapshot(chain, snapshot, startup, None).await?;
        Ok(())
    }
    async fn commit_snapshot(
        &self,
        chain: &Chain,
        mut snapshot: Snapshot,
        startup: bool,
        scope: Option<ScopeMaterial>,
    ) -> Result<()> {
        if scope.is_some() && self.control.get().is_none() {
            self.reconcile_local_scope(chain)?;
        }
        self.validate_stream_publication(chain, &snapshot)?;
        if let Some(pg) = self.control.get() {
            let writer = self
                .writers
                .get(&chain.key)
                .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
            let mut writer = writer.lock().await;
            if writer.is_none() {
                let (previous, pending) = {
                    let store = self
                        .stores
                        .get(&chain.key)
                        .ok_or_else(|| err("UNKNOWN_CHAIN"))?
                        .lock()
                        .map_err(|_| err("STATE_LOCK_POISONED"))?;
                    (
                        store.previous().or_else(|| {
                            let mut r = snapshot.reference.clone();
                            r.writer_epoch = 0;
                            r.online_generation = 0;
                            r.journal_seq = 0;
                            Some(r)
                        }),
                        store.prepared()?,
                    )
                };
                let (opened, recovery) =
                    Writer::open(pg, &self.config, chain, previous, pending).await?;
                {
                    let mut store = self.stores[&chain.key]
                        .lock()
                        .map_err(|_| err("STATE_LOCK_POISONED"))?;
                    if let Some(cursor) = recovery {
                        store.apply_prepared(cursor)?;
                    } else {
                        store.discard_prepared()?;
                    }
                    store.suspend();
                }
                self.stores[&chain.key]
                    .lock()
                    .map_err(|_| err("STATE_LOCK_POISONED"))?
                    .set_authority_deadline(opened.deadline(&self.config));
                *writer = Some(opened);
            }
            let active = writer.as_mut().ok_or_else(|| err("LEASE_NOT_OWNED"))?;
            active.heartbeat(pg, &self.config).await?;
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .set_authority_deadline(active.deadline(&self.config));
            {
                let store = self.stores[&chain.key]
                    .lock()
                    .map_err(|_| err("STATE_LOCK_POISONED"))?;
                if !startup
                    && store.current().is_some_and(|c| {
                        c.reference.block == snapshot.reference.block && c.pools == snapshot.pools
                    })
                {
                    return Ok(());
                }
            }
            active.baseline(pg, &self.config, &mut snapshot).await?;
            if let Some(material) = scope {
                return self
                    .publish_scope_unit(chain, snapshot, material, Some(active))
                    .await;
            }
            let unit = Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: snapshot.clone(),
            };
            self.validate_stream_publication(chain, &snapshot)?;
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .prepare(&unit)?;
            let cursor = active.publish(pg, &self.config, unit).await?;
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .apply_prepared(cursor)?;
            self.logger.event("info","baseline_committed",json!({"chain_key":chain.key,"online_generation":snapshot.reference.online_generation,"journal_seq":snapshot.reference.journal_seq,"writer_epoch":snapshot.reference.writer_epoch,"transport":"kafka"}));
            return Ok(());
        }
        {
            let store = self
                .stores
                .get(&chain.key)
                .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
            let mut store = store.lock().map_err(|_| err("STATE_LOCK_POISONED"))?;
            if !startup {
                if let Some(current) = store.current() {
                    if current.reference.block.hash == snapshot.reference.block.hash
                        && current.pools == snapshot.pools
                    {
                        return Ok(());
                    }
                }
            }
            let previous = store.previous();
            snapshot.reference.online_generation = previous
                .as_ref()
                .map_or(0, |r| r.online_generation)
                .max(store.scope_generation_floor()?)
                .checked_add(1)
                .ok_or_else(|| err("GENERATION_OVERFLOW"))?;
            snapshot.reference.journal_seq = previous.as_ref().map_or(1, |r| r.journal_seq + 1);
            snapshot.reference.writer_epoch = previous
                .as_ref()
                .map_or(1, |r| r.writer_epoch + u64::from(startup));
            snapshot.reference.canonical_epoch = previous.as_ref().map_or(1, |r| r.canonical_epoch);
            if let Some(old) = &previous {
                if snapshot.reference.block.number_or_slot == old.block.number_or_slot
                    && snapshot.reference.block.hash != old.block.hash
                {
                    snapshot.reference.canonical_epoch += 1;
                }
                if snapshot.reference.block.number_or_slot > old.block.number_or_slot + 1 {
                    snapshot.history_gaps.push(Gap {
                        from: old.block.number_or_slot + 1,
                        to: snapshot.reference.block.number_or_slot - 1,
                        reason: "HEAD_FIRST_HISTORY_DECOUPLED".into(),
                    });
                }
            }
            self.validate_stream_publication(chain, &snapshot)?;
            if scope.is_none() {
                store.publish_baseline(snapshot.clone())?;
            }
        }
        if let Some(material) = scope {
            return self
                .publish_scope_unit(chain, snapshot, material, None)
                .await;
        }
        self.logger.event("info","baseline_committed",json!({"chain_key":chain.key,"online_generation":snapshot.reference.online_generation,"journal_seq":snapshot.reference.journal_seq,"source":snapshot.source}));
        Ok(())
    }
    async fn revoke_scope(&self, chain: &Chain) -> Result<()> {
        let old = {
            let mut store = self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?;
            let old = store.current();
            store.suspend();
            old
        };
        let Some(old) = old else { return Ok(()) };
        if let Some(pg) = self.control.get() {
            let mut writer = self.writers[&chain.key].lock().await;
            let active = writer.as_mut().ok_or_else(|| err("LEASE_NOT_OWNED"))?;
            let unit = active.revocation(&old)?;
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .prepare(&unit)?;
            let cursor = active.publish(pg, &self.config, unit).await?;
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .apply_prepared(cursor)?;
        } else {
            self.stores[&chain.key]
                .lock()
                .map_err(|_| err("STATE_LOCK_POISONED"))?
                .revoke_local(old)?;
        }
        self.logger
            .event("warn", "scope_revoked", json!({"chain_key":chain.key}));
        Ok(())
    }
    pub async fn tick(&self) {
        let _actor = self.recovery_lock.lock().await;
        if let Some(pg) = self.control.get() {
            for chain in &self.config.config.chains {
                let mut writer = self.writers[&chain.key].lock().await;
                if let Some(active) = writer.as_mut() {
                    if active.heartbeat(pg, &self.config).await.is_err() {
                        active.close(pg, &self.config).await;
                        *writer = None;
                        self.stores[&chain.key].lock().map(|mut s| s.suspend()).ok();
                    } else {
                        self.stores[&chain.key]
                            .lock()
                            .map(|mut s| s.set_authority_deadline(active.deadline(&self.config)))
                            .ok();
                    }
                }
            }
        }
        if self.source.is_some() && self.control.get().is_none() {
            return;
        }
        for chain in &self.config.config.chains {
            if self.source.is_none() && self.config.config.yellowstone.contains_key(&chain.key) {
                let stale = self.current(&chain.key).is_err();
                if stale {
                    if let Err(error) = self.revoke_scope(chain).await {
                        self.logger.event(
                            "error",
                            "stream_scope_revocation_failed",
                            json!({"chain_key":chain.key,"error_code":error.0}),
                        );
                    }
                }
                continue;
            }
            if let Err(e) = self.reanchor(chain, false).await {
                if e.0 == "LEASE_NOT_OWNED" {
                    continue;
                }
                if let Err(error) = self.revoke_scope(chain).await {
                    self.logger.event(
                        "error",
                        "scope_revocation_failed",
                        json!({"chain_key":chain.key,"error_code":error.0}),
                    );
                }
                if let Some(store) = self.stores.get(&chain.key) {
                    if let Ok(mut s) = store.lock() {
                        s.suspend();
                    }
                }
                if let Some(pg) = self.control.get() {
                    let mut writer = self.writers[&chain.key].lock().await;
                    if let Some(w) = writer.as_mut() {
                        w.close(pg, &self.config).await;
                    }
                    *writer = None;
                }
                self.logger.event(
                    "error",
                    "chain_sync_failed",
                    json!({"chain_key":chain.key,"error_code":e.0}),
                );
            }
        }
    }
    pub fn handler(&self) -> Router {
        Router::new()
            .route("/health/live", get(|| async { Json(json!({"live":true})) }))
            .route("/health/startup", get(startup))
            .route("/health/ready", get(ready))
            .route("/internal/config", get(config))
            .route("/internal/chains/{chain}/bootstrap", get(bootstrap))
            .route("/internal/chains/{chain}/stream-status", get(stream_status))
            .route(
                "/internal/chains/{chain}/history-status",
                get(history_status).post(history_pause),
            )
            .route("/internal/chains/{chain}/source-refs", get(source_refs))
            .route("/internal/chains/{chain}/baseline", get(baseline))
            .route("/internal/chains/{chain}/status", get(status))
            .route(
                "/internal/chains/{chain}/ensure-pool-scope",
                post(ensure_pool_scope),
            )
            .route("/internal/chains/{chain}/journal", get(journal))
            .route(
                "/internal/chains/{chain}/source-evidence",
                get(source_evidence),
            )
            .route("/metrics", get(metrics))
            .with_state(self.clone())
    }
    pub async fn close(&self) {
        self.started.store(false, Ordering::Release);
        self.stream_stop.send_replace(true);
        self.history.close().await;
        for task in self.stream_tasks.lock().await.drain(..) {
            let _ = task.await;
        }
        let _drain_actor = self.recovery_lock.lock().await;
        if let Some(pg) = self.control.get() {
            for writer in self.writers.values() {
                let mut writer = writer.lock().await;
                if let Some(w) = writer.as_mut() {
                    w.close(pg, &self.config).await;
                }
                *writer = None;
            }
        }
    }
    fn authorize(&self, headers: &HeaderMap) -> Result<()> {
        if !self.config.token.is_empty()
            && headers.get("authorization").and_then(|v| v.to_str().ok())
                != Some(&format!("Bearer {}", self.config.token))
        {
            return Err(err("UNAUTHORIZED"));
        }
        Ok(())
    }
    fn current(&self, chain: &str) -> Result<Snapshot> {
        if self.source.is_none() && self.config.config.yellowstone.contains_key(chain) {
            let status = self.stream_status(chain)?;
            if status.phase != "LIVE" {
                return Err(err("CHAIN_NOT_READY"));
            }
            let original = self
                .config
                .config
                .chains
                .iter()
                .find(|c| c.key == chain)
                .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
            let (_, identity) = self.admitted_chain(original)?;
            if identity.as_deref() != Some(&status.catalog_identity) {
                return Err(err("STREAM_ADMITTED_CATALOG_CHANGED"));
            }
        }
        self.stores
            .get(chain)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .current()
            .ok_or_else(|| err("CHAIN_NOT_READY"))
            .and_then(|snapshot| {
                if self.source.is_none()
                    && self.config.config.yellowstone.contains_key(chain)
                    && now_ms().saturating_sub(snapshot.reference.block.timestamp_ms)
                        > self.config.config.startup.max_baseline_age_ms
                {
                    return Err(err("BASELINE_TOO_OLD"));
                }
                Ok(snapshot)
            })
    }
}
fn failure(e: xone_domain::Error) -> (StatusCode, Json<Value>) {
    let status = match e.0.as_str() {
        "UNAUTHORIZED" => StatusCode::UNAUTHORIZED,
        "UNKNOWN_CHAIN" => StatusCode::NOT_FOUND,
        "INVALID_CURSOR" => StatusCode::BAD_REQUEST,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (status, Json(json!({"error":e.0})))
}
async fn startup(State(s): State<Parser>) -> impl IntoResponse {
    let started = s.started.load(Ordering::Acquire);
    (
        if started {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({"started":started})),
    )
}
async fn ready(State(s): State<Parser>) -> impl IntoResponse {
    let mut ready = false;
    for c in &s.config.config.chains {
        if s.checked_current(&c.key).await.is_ok_and(|v| {
            now_ms().saturating_sub(v.reference.block.timestamp_ms)
                <= s.config.config.startup.max_baseline_age_ms
        }) {
            ready = true;
            break;
        }
    }
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({"ready":ready,"scope":"PARSER_STATE_SCOPE"})),
    )
}
async fn config(
    State(s): State<Parser>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    Ok(Json(
        json!({"release":s.config.release,"public_hash":s.config.public_hash,"economic_hash":s.config.economic_hash}),
    ))
}
async fn bootstrap(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<([(&'static str, &'static str); 1], Vec<u8>), (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    let before = s.checked_current(&chain).await.map_err(failure)?;
    let bytes = {
        let store = s
            .stores
            .get(&chain)
            .ok_or_else(|| failure(err("UNKNOWN_CHAIN")))?
            .lock()
            .map_err(|_| failure(err("STATE_LOCK_POISONED")))?;
        let boot = store.bootstrap().map_err(failure)?;
        if boot.snapshot.reference != before.reference {
            return Err(failure(err("SCOPE_READY_REFERENCE_CHANGED")));
        }
        xone_kafka_transport::encode_bootstrap(&boot).map_err(failure)?
    };
    let after = s.checked_current(&chain).await.map_err(failure)?;
    if before.reference != after.reference {
        return Err(failure(err("SCOPE_READY_REFERENCE_CHANGED")));
    }
    Ok(([("content-type", "application/x-protobuf")], bytes))
}
async fn baseline(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<Snapshot>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    Ok(Json(s.checked_current(&chain).await.map_err(failure)?))
}
async fn status(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    let snapshot = s.checked_current(&chain).await.map_err(failure)?;
    Ok(Json(
        json!({"ready":true,"snapshot_ref":snapshot.reference,"history_gaps":snapshot.history_gaps,"readiness_scope":"PARSER_STATE_SCOPE"}),
    ))
}
async fn journal(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
    Query(q): Query<BTreeMap<String, String>>,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    let after = q
        .get("after")
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| failure(err("INVALID_CURSOR")))?;
    let store = s
        .stores
        .get(&chain)
        .ok_or_else(|| failure(err("UNKNOWN_CHAIN")))?
        .lock()
        .map_err(|_| failure(err("STATE_LOCK_POISONED")))?;
    Ok(Json(json!(store.journal(after, 100).map_err(failure)?)))
}
async fn stream_status(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<StreamStatus>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    Ok(Json(s.stream_status(&chain).map_err(failure)?))
}
async fn source_refs(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    // The ingress-ref lock spans the owner DB read. A staged ref is added before
    // prepare and removed only after commit/reconciliation, so this union is coherent.
    let ingress = s
        .ingest_refs
        .lock()
        .map_err(|_| failure(err("SOURCE_REFS_LOCK")))?;
    let db = s
        .stores
        .get(&chain)
        .ok_or_else(|| failure(err("UNKNOWN_CHAIN")))?
        .lock()
        .map_err(|_| failure(err("STATE_LOCK_POISONED")))?;
    let mut refs = db.protected_manifest_hashes().map_err(failure)?;
    refs.extend(ingress.get(&chain).into_iter().flatten().cloned());
    Ok(Json(
        json!({"chain_key":chain,"protected_blockhashes":refs,"committed_slot":db.native_committed_slot().map_err(failure)?,"gc_permitted":true,"ready":false}),
    ))
}
async fn source_evidence(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    if !s.stores.contains_key(&chain) {
        return Err(failure(err("UNKNOWN_CHAIN")));
    }
    let recorder = s
        .source_wals
        .get(&chain)
        .ok_or_else(|| failure(err("SOURCE_EVIDENCE_DEBUG_SOURCE")))?;
    Ok(Json(recorder.status().map_err(failure)?))
}

async fn metrics(State(s): State<Parser>) -> String {
    format!(
        "xone_log_dropped_total {}\nxone_log_write_errors_total {}\n",
        s.logger.counters.dropped.load(Ordering::Relaxed),
        s.logger.counters.write_errors.load(Ordering::Relaxed)
    )
}
pub async fn run(config: ConfigSnapshot) -> Result<()> {
    serve(Parser::new(config)?).await
}
pub async fn serve(parser: Parser) -> Result<()> {
    if let Err(error) = parser.start().await {
        parser.close().await;
        return Err(error);
    }
    let listener = match tokio::net::TcpListener::bind(&parser.config.config.api.listen).await {
        Ok(listener) => listener,
        Err(_) => {
            parser.close().await;
            return Err(err("HTTP_LISTENER_FAILED"));
        }
    };
    let background = parser.clone();
    let task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(
            background.config.config.journal.poll_interval_ms,
        ));
        loop {
            ticker.tick().await;
            background.tick().await;
        }
    });
    let result = axum::serve(listener, parser.handler())
        .with_graceful_shutdown(shutdown())
        .await;
    task.abort();
    let _ = task.await;
    parser.close().await;
    parser.logger.flush();
    result.map_err(|_| err("HTTP_SERVER_FAILED"))
}
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_ = tokio::signal::ctrl_c()=>{},_ = term.recv()=>{}}
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    #[tokio::test]
    async fn parser_readiness_uses_parser_freshness_without_router_config() {
        let mut cfg =
            xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
        let directory = std::env::temp_dir().join(format!(
            "xone-parser-ready-{}-{}",
            std::process::id(),
            now_ms()
        ));
        cfg.config.state.path = directory.to_string_lossy().into_owned();
        cfg.config.routing.max_state_age_ms = 0;
        let snapshot: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json"))
                .unwrap();
        let source = Arc::new(move |_: &Chain| Ok(snapshot.clone()));
        let parser = Parser::with_source(cfg, Some(source)).unwrap();
        parser.start().await.unwrap();
        let response = ready(State(parser.clone())).await.into_response();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "parser readiness depends on omitted router configuration"
        );
        drop(parser);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

fn spawn_owned_scope_recovery<F>(
    actor: tokio::sync::OwnedMutexGuard<()>,
    work: F,
) -> tokio::task::JoinHandle<Result<()>>
where
    F: std::future::Future<Output = Result<()>> + Send + 'static,
{
    tokio::spawn(async move {
        let _actor = actor;
        work.await
    })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EnsureScopeRequest {
    pool_ids: Vec<String>,
    expected_online_generation: u64,
}
async fn ensure_pool_scope(
    State(s): State<Parser>,
    Path(chain): Path<String>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let failure = |code, reason: &str| (code, Json(json!({"error":reason,"ready":false})));
    if s.authorize(&headers).is_err() {
        return failure(StatusCode::UNAUTHORIZED, "UNAUTHORIZED");
    }
    if !s.started.load(Ordering::Acquire) {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "PARSER_NOT_STARTED");
    }
    let body =
        match axum::body::to_bytes(request.into_body(), s.config.config.api.max_body_bytes).await {
            Ok(b) => b,
            Err(_) => return failure(StatusCode::BAD_REQUEST, "INVALID_POOL_SCOPE"),
        };
    let mut requested: EnsureScopeRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return failure(StatusCode::BAD_REQUEST, "INVALID_POOL_SCOPE"),
    };
    if requested.pool_ids.is_empty() || requested.pool_ids.len() > 1024 {
        return failure(StatusCode::BAD_REQUEST, "INVALID_POOL_SCOPE");
    }
    let Some(config) = s
        .config
        .config
        .chains
        .iter()
        .find(|c| c.key == chain)
        .cloned()
    else {
        return failure(StatusCode::NOT_FOUND, "UNKNOWN_CHAIN");
    };
    let config = match s.admitted_chain(&config) {
        Ok((chain, _)) => chain,
        Err(_) => {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "ADMITTED_CATALOG_UNAVAILABLE",
            )
        }
    };
    let mut seen = std::collections::BTreeSet::new();
    for id in &requested.pool_ids {
        if id.is_empty() || !seen.insert(id.clone()) {
            return failure(StatusCode::BAD_REQUEST, "INVALID_POOL_SCOPE");
        }
        if !config.pools.iter().any(|p| p.id == *id) {
            return failure(StatusCode::NOT_FOUND, "POOL_NOT_IN_ADMITTED_CATALOG");
        }
    }
    if s.source.is_none() && !s.config.config.yellowstone.contains_key(&chain) {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "SCOPE_VERSIONED_SOURCE_REQUIRED",
        );
    }
    let Ok(actor) = s.recovery_lock.clone().try_lock_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "CHAIN_ACTOR_BUSY");
    };
    let mut actor = Some(actor);
    let generation = match s
        .stores
        .get(&chain)
        .and_then(|store| store.lock().ok())
        .and_then(|store| store.previous())
    {
        Some(r) => r.online_generation,
        None => 0,
    };
    if requested.expected_online_generation == 0
        || requested.expected_online_generation != generation
    {
        return failure(StatusCode::CONFLICT, "POOL_SCOPE_GENERATION_CHANGED");
    }
    if s.source.is_none() && s.config.config.yellowstone.contains_key(&chain) {
        let lease = s.writers[&chain]
            .lock()
            .await
            .as_ref()
            .map(|w| w.lease.clone());
        if let Err(error) = s.check_owner_authority(&config, lease.as_ref()).await {
            return failure(StatusCode::SERVICE_UNAVAILABLE, &error.0);
        }
        let max_jobs = s.config.config.yellowstone[&chain].max_catalog_pools;
        if let Err(error) = s.stores[&chain]
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))
            .and_then(|mut db| db.request_scope(&requested.pool_ids, generation, max_jobs))
        {
            return failure(StatusCode::SERVICE_UNAVAILABLE, &error.0);
        }
        let missing = s.current(&chain).is_ok_and(|v| {
            requested
                .pool_ids
                .iter()
                .any(|id| !v.pools.iter().any(|p| p.pool_id == *id))
        });
        if missing {
            let recovery = s.clone();
            let recovery_chain = config.clone();
            let ids = requested.pool_ids.clone();
            let task = spawn_owned_scope_recovery(actor.take().expect("scope actor"), async move {
                recovery.restore_cached_scope(&recovery_chain, &ids).await
            });
            if !matches!(task.await, Ok(Ok(()))) {
                return failure(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "POOL_SCOPE_STREAM_RECOVERY_PENDING",
                );
            }
            actor = match s.recovery_lock.clone().try_lock_owned() {
                Ok(g) => Some(g),
                Err(_) => return failure(StatusCode::TOO_MANY_REQUESTS, "CHAIN_ACTOR_BUSY"),
            };
        }
    }
    let fresh = |snapshot: &Snapshot| {
        snapshot.reference.block.timestamp_ms <= now_ms() + 30000
            && now_ms().saturating_sub(snapshot.reference.block.timestamp_ms)
                <= s.config.config.startup.max_baseline_age_ms
    };
    if s.current(&chain).is_err() || s.current(&chain).is_ok_and(|v| !fresh(&v)) {
        if s.source.is_none() && s.config.config.yellowstone.contains_key(&chain) {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "POOL_SCOPE_STREAM_RECOVERY_PENDING",
            );
        }
        // Source reads already have their own budget. Once publication starts,
        // retain the actor until Kafka's bounded transaction settles, including
        // when the HTTP waiter is dropped. Never timeout/drop a publish future.
        let recovery = s.clone();
        let recovery_chain = config.clone();
        let task = spawn_owned_scope_recovery(actor.take().expect("scope actor"), async move {
            let result = recovery.reanchor(&recovery_chain, false).await;
            if result.is_err() {
                if let Err(error) = recovery.revoke_scope(&recovery_chain).await {
                    recovery.logger.event(
                        "error",
                        "scope_revocation_failed",
                        json!({"chain_key":recovery_chain.key,"error_code":error.0}),
                    );
                }
                if let Some(store) = recovery.stores.get(&recovery_chain.key) {
                    if let Ok(mut store) = store.lock() {
                        store.suspend();
                    }
                }
                // Pending prepared data is reconciled through the normal owner
                // path before a later attempt; never forge a rollback result.
            }
            result
        });
        if !matches!(task.await, Ok(Ok(()))) {
            return failure(StatusCode::SERVICE_UNAVAILABLE, "POOL_SCOPE_RESTORE_FAILED");
        }
        actor = match s.recovery_lock.clone().try_lock_owned() {
            Ok(v) => Some(v),
            Err(_) => return failure(StatusCode::TOO_MANY_REQUESTS, "CHAIN_ACTOR_BUSY"),
        };
    }
    let snapshot = match s.checked_current(&chain).await {
        Ok(v) if fresh(&v) => v,
        _ => return failure(StatusCode::SERVICE_UNAVAILABLE, "POOL_SCOPE_NOT_READY"),
    };
    if requested
        .pool_ids
        .iter()
        .any(|id| !snapshot.pools.iter().any(|p| p.pool_id == *id))
    {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "POOL_SCOPE_DEPENDENCIES_INCOMPLETE",
        );
    }
    if !s
        .checked_current(&chain)
        .await
        .is_ok_and(|v| v.reference == snapshot.reference)
    {
        return failure(StatusCode::CONFLICT, "POOL_SCOPE_GENERATION_CHANGED");
    }
    let _actor = actor;
    requested.pool_ids.sort();
    (
        StatusCode::OK,
        Json(
            json!({"ready":true,"scope":"PARSER_STATE_SCOPE","pool_ids":requested.pool_ids,"snapshot_ref":snapshot.reference,"history_blocks_readiness":false,"strict_historical_bank_proof":s.source.is_none() && s.config.config.yellowstone.contains_key(&chain)}),
        ),
    )
}

#[cfg(test)]
#[path = "service_scope_tests.rs"]
mod scope_tests;

#[cfg(test)]
#[path = "service_scope_race_tests.rs"]
mod scope_read_race_tests;

async fn history_status(
    State(s): State<Parser>,
    Path(chain): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = s.authorize(&headers) {
        return failure(e);
    }
    match s.history.status(&chain) {
        Ok(status) => (StatusCode::OK, Json(json!(status))),
        Err(e) => failure(e),
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryPause {
    paused: bool,
}
async fn history_pause(
    State(s): State<Parser>,
    Path(chain): Path<String>,
    headers: HeaderMap,
    Json(body): Json<HistoryPause>,
) -> impl IntoResponse {
    if let Err(e) = s.authorize(&headers) {
        return failure(e).into_response();
    }
    match s.history.pause(&chain, body.paused).await {
        Ok(()) => history_status(State(s), Path(chain), headers)
            .await
            .into_response(),
        Err(e) => failure(e).into_response(),
    }
}
