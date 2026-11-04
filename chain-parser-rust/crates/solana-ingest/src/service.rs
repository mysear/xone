use crate::session::Writer;
use crate::source_wal::{Recorder, SourceWal, WalLimits};
use crate::{rpc::Rpc, store::Store};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
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
pub type SnapshotSource = Arc<dyn Fn(&Chain) -> Result<Snapshot> + Send + Sync>;
#[derive(Clone)]
pub struct Parser {
    started: Arc<std::sync::atomic::AtomicBool>,
    control: Arc<tokio::sync::OnceCell<Postgres>>,
    writers: Arc<BTreeMap<String, tokio::sync::Mutex<Option<Writer>>>>,
    pub config: Arc<ConfigSnapshot>,
    pub logger: Logger,
    stores: Arc<BTreeMap<String, Mutex<Store>>>,
    rpc: Rpc,
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
                Mutex::new(Store::open(
                    &config
                        .resolve(&config.config.state.path)
                        .join(chain.key.replace(':', "-")),
                    &chain.owner_service,
                    chain.assignment_version,
                )?),
            );
        }
        let mut source_wals = BTreeMap::new();
        if source.is_none() {
            for chain in &config.config.chains {
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
        Ok(Self {
            started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            control: Arc::new(tokio::sync::OnceCell::new()),
            writers: Arc::new(writers),
            config: Arc::new(config),
            logger,
            stores: Arc::new(stores),
            rpc: Rpc { http, max_bytes },
            source_wals: Arc::new(source_wals),
            source,
        })
    }
    pub async fn start(&self) -> Result<()> {
        if self.config.config.journal.transport == "kafka" {
            let pg = Postgres::open(&self.config.control_dsn).await?;
            self.control
                .set(pg)
                .map_err(|_| err("CONTROL_ALREADY_INITIALIZED"))?;
        }
        for chain in &self.config.config.chains {
            if let Err(e) = self.reanchor(chain, true).await {
                if e.0 != "LEASE_NOT_OWNED" {
                    return Err(e);
                }
                self.logger
                    .event("info", "writer_standby", json!({"chain_key":chain.key}));
            }
        }
        self.started.store(true, Ordering::Release);
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
        self.commit_snapshot(chain, snapshot, startup).await?;
        Ok(())
    }
    async fn commit_snapshot(
        &self,
        chain: &Chain,
        mut snapshot: Snapshot,
        startup: bool,
    ) -> Result<()> {
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
            let unit = Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: snapshot.clone(),
            };
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
        snapshot.reference.online_generation =
            previous.as_ref().map_or(1, |r| r.online_generation + 1);
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
        store.publish_baseline(snapshot.clone())?;
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
            .route("/internal/chains/{chain}/baseline", get(baseline))
            .route("/internal/chains/{chain}/status", get(status))
            .route("/internal/chains/{chain}/journal", get(journal))
            .route(
                "/internal/chains/{chain}/source-evidence",
                get(source_evidence),
            )
            .route("/metrics", get(metrics))
            .with_state(self.clone())
    }
    async fn close(&self) {
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
        self.stores
            .get(chain)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?
            .lock()
            .map_err(|_| err("STATE_LOCK_POISONED"))?
            .current()
            .ok_or_else(|| err("CHAIN_NOT_READY"))
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
    let ready = s.config.config.chains.iter().any(|c| {
        s.current(&c.key).is_ok_and(|v| {
            now_ms().saturating_sub(v.reference.block.timestamp_ms)
                <= s.config.config.startup.max_baseline_age_ms
        })
    });
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
    let store = s
        .stores
        .get(&chain)
        .ok_or_else(|| failure(err("UNKNOWN_CHAIN")))?
        .lock()
        .map_err(|_| failure(err("STATE_LOCK_POISONED")))?;
    let bytes = xone_kafka_transport::encode_bootstrap(&store.bootstrap().map_err(failure)?)
        .map_err(failure)?;
    Ok(([("content-type", "application/x-protobuf")], bytes))
}
async fn baseline(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<Snapshot>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    Ok(Json(s.current(&chain).map_err(failure)?))
}
async fn status(
    State(s): State<Parser>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<Value>, (StatusCode, Json<Value>)> {
    s.authorize(&headers).map_err(failure)?;
    let snapshot = s.current(&chain).map_err(failure)?;
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
    parser.start().await?;
    let listener = tokio::net::TcpListener::bind(&parser.config.config.api.listen)
        .await
        .map_err(|_| err("HTTP_LISTENER_FAILED"))?;
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
