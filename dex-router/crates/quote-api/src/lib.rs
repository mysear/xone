mod approval_sequence;
mod cache_gc;
mod candidate_costs;
mod cost_fallback;
mod evm_approval;
mod evm_preparation;
mod evm_receipt;
mod exact_out;
mod execution_receipts;
mod failure_fees;
mod native_economics;
mod preparation;
pub mod price_publication;
mod remote_checkpoint;
mod solana_simulation;
mod transaction_economics;
mod transaction_lifecycle;
mod v4_execution;
mod wrapped_funding;
pub use exact_out::ExactOutRequest;
mod local_simulation;
mod peer_checkpoint;
mod quote_verification;
mod recovery_metrics;
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use tower_http::cors::CorsLayer;
use xone_config::ConfigSnapshot;
use xone_domain::{err, hash, Amount, Record, Result, Snapshot};
use xone_execution_plan::{Context, Plan};
use xone_kafka_transport::{Bootstrap, Cursor, Reader, Unit};
use xone_observability::{now_ms, Logger};
use xone_state_replica::{QuoteView, Replica};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Constraints {
    pub max_hops: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuoteRequest {
    pub chain_key: String,
    pub kind: String,
    pub sell_token: String,
    pub buy_token: String,
    pub sell_amount: Amount,
    pub caller: String,
    pub recipient: String,
    pub slippage_bps: u16,
    pub verification: String,
    pub constraints: Option<Constraints>,
}

impl QuoteRequest {
    pub fn validate(&self) -> Result<()> {
        if self.kind != "EXACT_IN" {
            return Err(err("QUOTE_KIND_UNSUPPORTED"));
        }
        if self.sell_amount.0.is_zero()
            || self.sell_token.is_empty()
            || self.buy_token.is_empty()
            || self.sell_token == self.buy_token
            || self.caller.is_empty()
            || self.recipient.is_empty()
            || self.caller.len() > 128
            || self.recipient.len() > 128
            || self.sell_token.len() > 128
            || self.buy_token.len() > 128
            || self.slippage_bps > 10000
        {
            return Err(err("INVALID_REQUEST"));
        }
        if !["FAST", "VERIFIED"].contains(&self.verification.as_str()) {
            return Err(err("VERIFICATION_UNSUPPORTED"));
        }
        Ok(())
    }
}
// Reader::poll returns None both for an idle consumer and for a consumed
// BEGIN/CHUNK whose atomic unit is still incomplete. Preserve that distinction
// while draining a bounded number of frames; only complete units carry an
// applied checkpoint into Replica.
struct KafkaPollStep {
    unit: Option<Unit>,
    cursor: Cursor,
    progressed: bool,
}
fn drain_kafka_units(
    max_frames: usize,
    mut poll: impl FnMut() -> Result<KafkaPollStep>,
) -> Result<Vec<(Unit, Cursor)>> {
    let mut complete = Vec::new();
    for _ in 0..max_frames {
        let step = poll()?;
        if let Some(unit) = step.unit {
            complete.push((unit, step.cursor));
        } else if !step.progressed {
            break;
        }
    }
    Ok(complete)
}

fn apply_kafka_unit(replica: &Replica, chain: &str, unit: Unit) -> Result<()> {
    if unit.snapshot.reference.block.chain_key != chain {
        return Err(err("JOURNAL_CHAIN_MISMATCH"));
    }
    if unit.kind == "ONLINE_BASELINE_COMMIT" {
        return replica.install_baseline(unit.snapshot);
    }
    replica.apply(Record {
        reference: unit.snapshot.reference,
        kind: unit.kind,
        normalized_facts: unit.normalized_facts,
        record_id: String::new(),
        payload_hash: hash(&unit.snapshot.pools)?,
        pools: unit.snapshot.pools,
    })
}

fn align_kafka_checkpoint(
    cfg: &ConfigSnapshot,
    chain: &str,
    boot: Bootstrap,
) -> Result<(Bootstrap, Reader)> {
    let kafka = &cfg.config.journal.kafka;
    let timeout = Duration::from_millis(cfg.config.journal.request_timeout_ms);
    let deadline = Instant::now() + timeout;
    let reader = Reader::open(
        &kafka.brokers,
        &format!("quote.{}.{}", chain, kafka.replica_id),
        &boot.cursor,
        cfg.config.journal.max_payload_bytes,
        kafka.max_tail_records,
        timeout,
    )?;
    align_kafka_reader(cfg, chain, boot, reader, deadline, kafka.max_tail_records)
}

fn align_kafka_reader(
    cfg: &ConfigSnapshot,
    chain: &str,
    boot: Bootstrap,
    mut reader: Reader,
    deadline: Instant,
    max_frames: usize,
) -> Result<(Bootstrap, Reader)> {
    reader.last_sequence = boot.snapshot.reference.journal_seq;
    if reader.reached_startup_watermark()? {
        return Ok((boot, reader));
    }

    let staged = Replica::new(
        cfg.config
            .chains
            .iter()
            .map(|c| (c.key.clone(), c.owner_service.clone(), c.assignment_version))
            .collect(),
    );
    staged.restore_owner_checkpoint(boot.snapshot)?;
    let mut frames = 0;
    while !reader.reached_startup_watermark()? || !reader.at_committed_boundary() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| err("KAFKA_STARTUP_REPLAY_DEADLINE"))?;
        let before = reader.cursor();
        let unit = reader.poll(remaining.min(Duration::from_millis(50)))?;
        if reader.cursor() != before {
            frames += 1;
            if frames > max_frames {
                return Err(err("KAFKA_STARTUP_REPLAY_FRAME_LIMIT"));
            }
        }
        if let Some(unit) = unit {
            apply_kafka_unit(&staged, chain, unit)?;
        }
    }
    let view = staged.view(chain)?;
    let snapshot = Snapshot {
        reference: view.reference.clone(),
        pools: view.pools.clone(),
        source: view.source.clone(),
        history_gaps: view.history_gaps.clone(),
    };
    Ok((
        Bootstrap {
            cursor: reader.cursor(),
            snapshot,
        },
        reader,
    ))
}

#[derive(Clone)]
struct Cached {
    view: Arc<QuoteView>,
    plan: Plan,
    response: Value,
    expires: u64,
    request_hash: String,
    simulation_evidence: Option<Value>,
    quote_verification_evidence: Option<Value>,
    evm_preparation: Option<evm_preparation::Prepared>,
}
#[derive(Default)]
pub struct Metrics {
    pub(crate) recovery: recovery_metrics::RecoveryTimings,
    pub requests: AtomicU64,
    pub succeeded: AtomicU64,
    pub failed: AtomicU64,
    pub rate_limited: AtomicU64,
    pub simulation_started: AtomicU64,
    pub simulation_success: AtomicU64,
    pub simulation_failed: AtomicU64,
    pub simulation_busy: AtomicU64,
}
type AppliedCursors = Arc<Mutex<BTreeMap<String, (xone_domain::SnapshotRef, Cursor)>>>;
#[derive(Clone)]
pub struct Api {
    constructed_at: Instant,
    readers: Arc<Mutex<BTreeMap<String, Reader>>>,
    applied_cursors: AppliedCursors,
    pub config: Arc<ConfigSnapshot>,
    pub replica: Arc<Replica>,
    pub logger: Logger,
    cache: Arc<Mutex<BTreeMap<String, Cached>>>,
    exact_out_cache: Arc<Mutex<BTreeMap<String, exact_out::ExactOutCached>>>,
    permits: Arc<Semaphore>,
    local_simulation: Option<Arc<local_simulation::LocalSimulation>>,
    solana_sbf: Option<Arc<xone_simulation::solana_sbf::Engine>>,
    preparations: Arc<Mutex<BTreeMap<String, solana_simulation::Prepared>>>,
    evm_preparation_budget: Arc<Semaphore>,
    receipts: Option<Arc<Mutex<execution_receipts::Journal>>>,
    flights: Arc<Vec<AsyncMutex<()>>>,
    rate: Arc<Mutex<(u64, u64)>>,
    pub metrics: Arc<Metrics>,
}
impl Api {
    pub fn new(config: ConfigSnapshot) -> Result<Self> {
        config.config.validate("dex-router")?;
        let local_simulation = local_simulation::LocalSimulation::load(&config)?;
        let solana_sbf = if config.config.api.solana_runtime_manifest.is_empty() {
            None
        } else {
            Some(Arc::new(xone_simulation::solana_sbf::Engine::load(
                &config.resolve(&config.config.api.solana_runtime_manifest),
            )?))
        };
        let receipts = if config.config.api.transaction_receipt_journal.is_empty() {
            None
        } else {
            Some(Arc::new(Mutex::new(execution_receipts::Journal::open(
                &config.resolve(&config.config.api.transaction_receipt_journal),
                usize::from(config.config.api.transaction_max_pending.max(1)),
            )?)))
        };
        let owners = config
            .config
            .chains
            .iter()
            .map(|c| (c.key.clone(), c.owner_service.clone(), c.assignment_version))
            .collect();
        let concurrency = config.config.api.quote_concurrency;
        let logger = Logger::new(&config);
        Ok(Self {
            constructed_at: Instant::now(),
            readers: Arc::new(Mutex::new(BTreeMap::new())),
            applied_cursors: Arc::new(Mutex::new(BTreeMap::new())),
            config: Arc::new(config),
            replica: Arc::new(Replica::new(owners)),
            logger,
            cache: Arc::new(Mutex::new(BTreeMap::new())),
            exact_out_cache: Arc::new(Mutex::new(BTreeMap::new())),
            permits: Arc::new(Semaphore::new(concurrency)),
            local_simulation,
            solana_sbf,
            preparations: Arc::new(Mutex::new(BTreeMap::new())),
            evm_preparation_budget: Arc::new(Semaphore::new(32 * 1024 * 1024)),
            receipts,
            flights: Arc::new((0..concurrency).map(|_| AsyncMutex::new(())).collect()),
            rate: Arc::new(Mutex::new((0, 0))),
            metrics: Arc::new(Metrics::default()),
        })
    }
    fn authorize(&self, h: &HeaderMap) -> Result<()> {
        if !self.config.token.is_empty()
            && h.get("authorization").and_then(|h| h.to_str().ok())
                != Some(&format!("Bearer {}", self.config.token))
        {
            return Err(err("UNAUTHORIZED"));
        }
        Ok(())
    }
    fn rate_limit(&self) -> Result<()> {
        let mut rate = self.rate.lock().map_err(|_| err("RATE_LOCK"))?;
        let window = now_ms() / 60000;
        if rate.0 != window {
            *rate = (window, 0);
        }
        if rate.1 >= self.config.config.api.requests_per_minute {
            self.metrics.rate_limited.fetch_add(1, Ordering::Relaxed);
            return Err(err("RATE_LIMITED"));
        }
        rate.1 += 1;
        Ok(())
    }
    fn check_view(&self, v: &QuoteView) -> Result<()> {
        if !v.is_valid() {
            return Err(err("SNAPSHOT_REORGED"));
        }
        let current = self.replica.view(&v.reference.block.chain_key)?;
        if current.reference.online_generation != v.reference.online_generation
            || current.reference.canonical_epoch != v.reference.canonical_epoch
            || current.reference.scope_hash != v.reference.scope_hash
        {
            return Err(err("SNAPSHOT_REORGED"));
        }
        let age = now_ms().saturating_sub(v.reference.block.timestamp_ms);
        if age > self.config.config.routing.max_state_age_ms {
            return Err(err("STATE_TOO_OLD"));
        }
        self.observe_qualified_view(v);
        Ok(())
    }
    fn cors_layer(&self) -> CorsLayer {
        let origins = if self.config.config.api.cors_allowed_origins.is_empty() {
            vec![
                "http://127.0.0.1:5173".to_string(),
                "http://localhost:5173".to_string(),
            ]
        } else {
            self.config.config.api.cors_allowed_origins.clone()
        };
        CorsLayer::new()
            .allow_origin(
                origins
                    .iter()
                    .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
                    .collect::<Vec<_>>(),
            )
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ])
            .max_age(Duration::from_secs(600))
    }
    pub fn handler(&self) -> Router {
        Router::new()
            .route("/health/live", get(|| async { Json(json!({"live":true})) }))
            .route("/health/startup", get(ready))
            .route("/health/ready", get(ready))
            .route("/metrics", get(metrics))
            .route("/v1/quote", post(quote))
            .route("/v1/quote-exact-out", post(exact_out::quote))
            .route("/v1/quote-options", get(quote_options))
            .route("/v1/quotes/{id}/build", post(build_transaction))
            .route("/v1/quotes/{id}/prepare", post(preparation::prepare))
            .route(
                "/v1/chains/{chain}/pools/{pool}/prepare-orca-reward",
                post(preparation::prepare_orca_reward),
            )
            .route(
                "/v1/chains/{chain}/pools/{pool}/prepare-rayclmm-reward",
                post(preparation::prepare_rayclmm_reward),
            )
            .route(
                "/v1/quotes/{id}/prepare-evm",
                post(evm_preparation::prepare),
            )
            .route(
                "/v1/quotes/{id}/preparations/{preparation}/simulate-independent",
                post(solana_simulation::simulate),
            )
            .route(
                "/v1/quotes/{id}/preparations/{preparation}/verify-signature",
                post(transaction_lifecycle::verify_wallet),
            )
            .route(
                "/v1/quotes/{id}/preparations/{preparation}/submit",
                post(transaction_lifecycle::submit),
            )
            .route(
                "/v1/transactions/{signature}",
                get(transaction_lifecycle::receipt),
            )
            .route(
                "/v1/quotes/{id}/preparations/{hash}/simulate-evm",
                post(evm_preparation::simulate),
            )
            .route("/v1/quotes/{id}/simulate", post(simulate_transaction))
            .route("/internal/quotes/{id}/evidence", get(evidence))
            .route("/internal/chains/{chain}/status", get(chain_status))
            .route(
                "/internal/chains/{chain}/peer-checkpoint",
                get(peer_checkpoint::checkpoint),
            )
            .route("/internal/config", get(config))
            .route(
                "/internal/native-valuation/reload",
                post(reload_native_valuation),
            )
            .route(
                "/internal/native-valuation/publication",
                post(native_publication),
            )
            .layer(DefaultBodyLimit::max(self.config.config.api.max_body_bytes))
            .layer(self.cors_layer())
            .with_state(self.clone())
    }
    pub async fn quote(&self, request: QuoteRequest) -> Result<Value> {
        request.validate()?;
        let verified_backend = self.verified_backend(
            &request.verification,
            &request.chain_key,
            &request.sell_token,
            &request.buy_token,
        )?;
        let view = self.replica.view(&request.chain_key)?;
        self.check_view(&view)?;
        let cost_backend = if self.config.config.simulation.candidate_cost_max_plans > 0 {
            self.local_simulation
                .as_ref()
                .map(|b| b.pin_prices())
                .transpose()?
        } else {
            None
        };
        let price_identity = cost_backend.as_ref().and_then(|b| b.price_identity());
        let mut identity = json!({"request":request,"view_id":view.view_id,"policy":self.config.economic_hash,"engine":"xone-router/0.1"});
        if let Some(price) = price_identity {
            identity["candidate_price_book"] = json!(price);
        }
        let request_hash = hash(&identity)?;
        let shard = u64::from_str_radix(&request_hash[..16], 16)
            .map_err(|_| err("HASH_ENCODING"))? as usize
            % self.flights.len();
        let started = Instant::now();
        let timeout = Duration::from_millis(self.config.config.api.quote_timeout_ms);
        let deadline = started + timeout;
        let _flight = tokio::time::timeout(timeout, self.flights[shard].lock())
            .await
            .map_err(|_| err("BUDGET_EXHAUSTED"))?;
        let existing = {
            let mut cache = self.cache.lock().map_err(|_| err("CACHE_LOCK"))?;
            cache.retain(|_, c| c.expires > now_ms());
            cache
                .values()
                .find(|c| c.request_hash == request_hash)
                .map(|c| (c.view.clone(), c.expires, c.response.clone()))
        };
        if let Some((cached_view, expires, response)) = existing {
            if let Some(backend) = &verified_backend {
                self.recheck_verified_quote(backend, &cached_view, expires, deadline)
                    .await?;
            }
            self.check_view(&cached_view)?;
            if let Some(backend) = &cost_backend {
                backend.check_current_prices()?;
            }
            self.observe_successful_quote(&cached_view, &response);
            return Ok(response);
        }
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| err("RATE_LIMITED"))?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel = CancelOnDrop(cancelled.clone());
        let routing = self.config.config.routing.clone();
        let policy = self.config.economic_hash.clone();
        let worker_view = view.clone();
        let req = request.clone();
        let mut cost_model =
            (self.config.config.simulation.candidate_cost_max_plans > 0).then(|| {
                candidate_costs::Model::new(
                    self,
                    cost_backend.clone(),
                    view.clone(),
                    cancelled.clone(),
                    deadline,
                )
            });
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if cancelled.load(Ordering::Relaxed) {
                return Err(err("CANCELLED"));
            }
            let max_hops = req
                .constraints
                .as_ref()
                .map_or(routing.max_hops, |c| c.max_hops);
            if max_hops == 0 || max_hops > routing.max_hops {
                return Err(err("INVALID_HOP_LIMIT"));
            }
            let funds = xone_routing::Funds {
                sender: &req.caller,
                recipient: &req.recipient,
            };
            let candidates = xone_routing::search_bounded_with_funds(
                &worker_view.graph,
                &worker_view.pools,
                &req.sell_token,
                &req.buy_token,
                &req.sell_amount,
                xone_routing::Budget {
                    split_parts: if routing.max_splits > 1 {
                        routing.split_parts
                    } else {
                        1
                    },
                    max_hops,
                    max_candidates: routing.max_candidates,
                    max_expansions: routing.max_expansions,
                },
                xone_routing::SearchExecution {
                    control: Some(xone_routing::SearchControl {
                        cancel: &cancelled,
                        deadline,
                    }),
                    funds: Some(funds),
                },
            )?;
            let expires = now_ms()
                .checked_add(routing.quote_ttl_ms)
                .ok_or_else(|| err("TIMESTAMP_OVERFLOW"))?;
            let context = Context {
                chain_key: req.chain_key.clone(),
                caller: req.caller.clone(),
                recipient: req.recipient.clone(),
                sell_token: req.sell_token.clone(),
                buy_token: req.buy_token.clone(),
                slippage_bps: req.slippage_bps,
                deadline_ms: expires,
            };
            let (optimized, cost_ranking) = if let Some(mut model) = cost_model.take() {
                let scored = xone_optimizer::optimize_with_costs(
                    &worker_view.pools,
                    &candidates.candidates,
                    &req.sell_amount,
                    xone_optimizer::OptimizationLimits {
                        parts: routing.split_parts,
                        max_splits: routing.max_splits,
                        max_evaluations: routing.max_expansions,
                    },
                    xone_optimizer::OptimizationExecution {
                        budget: Some((&cancelled, deadline)),
                        funds: Some(funds),
                    },
                    &mut |allocation| {
                        model.exact_in(xone_execution_plan::build(
                            &context,
                            allocation,
                            &worker_view.reference,
                            &policy,
                        )?)
                    },
                )?;
                let selected = xone_execution_plan::build(
                    &context,
                    &scored.optimization.allocation,
                    &worker_view.reference,
                    &policy,
                )?;
                let ranking = model.finish(&scored.ranking, &selected.plan_hash);
                (scored.optimization, Some(ranking))
            } else {
                let optimized = xone_optimizer::optimize_with_limits(
                    &worker_view.pools,
                    &candidates.candidates,
                    &req.sell_amount,
                    &Amount::zero(),
                    xone_optimizer::OptimizationLimits {
                        parts: routing.split_parts,
                        max_splits: routing.max_splits,
                        max_evaluations: routing.max_expansions,
                    },
                    xone_optimizer::OptimizationExecution {
                        budget: Some((&cancelled, deadline)),
                        funds: Some(funds),
                    },
                )?;
                (optimized, None)
            };
            let plan = xone_execution_plan::build(
                &context,
                &optimized.allocation,
                &worker_view.reference,
                &policy,
            )?;
            Ok((
                plan,
                candidates.budget_exhausted || optimized.budget_exhausted,
                candidates.expansions,
                expires,
                optimized.evaluations,
                optimized.grid_exhaustive,
                cost_ranking,
            ))
        });
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (
            plan,
            exhausted,
            expansions,
            expires,
            optimizer_evaluations,
            grid_exhaustive,
            cost_ranking,
        ) = tokio::time::timeout(remaining, task)
            .await
            .map_err(|_| err("BUDGET_EXHAUSTED"))?
            .map_err(|_| err("COMPUTE_TASK_FAILED"))??;
        self.check_view(&view)?;
        let quote_id = hash(
            &json!({"request_hash":request_hash,"plan_hash":plan.plan_hash,"expires":expires}),
        )?;
        let protocol_view_ids = view.protocol_bindings(
            plan.allocation
                .routes
                .iter()
                .flat_map(|r| r.path.edges.iter().map(|e| e.pool_id.as_str())),
        )?;
        let mut response = json!({"kind":"EXACT_IN","protocolViewIds":protocol_view_ids,"quoteId":quote_id,"requestHash":request_hash,"chainKey":request.chain_key,"snapshotRef":view.reference,"ownerService":view.reference.owner_service,"onlineGeneration":view.reference.online_generation,"scopeHash":view.reference.scope_hash,"quoteViewId":view.view_id,"graphVersion":view.graph_version,"engineVersion":"xone-router/0.1","economicPolicyHash":self.config.economic_hash,"configRelease":self.config.release,"sellAmount":request.sell_amount,"consumedIn":plan.allocation.sell_amount,"refund":plan.allocation.refund,"grossBuyAmount":plan.allocation.gross_buy_amount,"minBuyAmount":plan.min_buy_amount,"netBuyAmountEstimate":Value::Null,"costs":cost_fallback::costs(&self.config.config.routing.route_activation_cost),"routes":plan.allocation.routes,"planHash":plan.plan_hash,"verification":{"level":"INDICATIVE","success":false,"reason":"INDEPENDENT_SIMULATION_UNAVAILABLE"},"validity":{"expiresAtMs":expires,"canonicalEpoch":view.reference.canonical_epoch},"quality":{"costRanking":cost_fallback::ranking(false),"budgetExhausted":exhausted,"candidateScope":view.pools.iter().filter(|p|!view.graph.excluded_pools.contains_key(&p.pool_id)).map(|p|p.pool_id.clone()).collect::<Vec<_>>(),"excludedPools":view.graph.excluded_pools,"source":view.source,"historyIndependent":true,"historyGaps":view.history_gaps,"optimalityClaim":"HEURISTIC_WITHIN_SCOPE","allocationSearch":{"evaluations":optimizer_evaluations,"exhaustiveWithinGrid":grid_exhaustive,"gridParts":self.config.config.routing.split_parts,"maxSplits":self.config.config.routing.max_splits},"expansions":expansions}});
        if let Some(ranking) = cost_ranking {
            if expires <= now_ms() {
                return Err(err("QUOTE_EXPIRED"));
            }
            if ranking["simulationBudgetExhausted"] == true {
                response["quality"]["budgetExhausted"] = true.into();
            }
            candidate_costs::disclose(
                &mut response,
                &ranking,
                &plan.context,
                &plan.plan_hash,
                &plan.allocation.sell_amount,
                &plan.allocation.gross_buy_amount,
            )?;
            response["quality"]["costRanking"] = ranking;
            response["quality"]["optimalityClaim"] = "LOCAL_EXECUTION_FEE_OBJECTIVE_WITHIN_EVALUATED_ORDERED_GRID_OR_UNIFORM_RAW_FALLBACK".into();
        }
        let mut cached = Cached {
            view: view.clone(),
            plan,
            response,
            expires,
            request_hash,
            simulation_evidence: None,
            quote_verification_evidence: None,
            evm_preparation: None,
        };
        if let Some(backend) = verified_backend {
            let (verification, evidence) = self
                .verify_quote(backend, cached.clone().into(), deadline)
                .await?;
            cached.response["verification"] = verification;
            cached.quote_verification_evidence = Some(evidence);
        }
        if let Some(backend) = &cost_backend {
            backend.check_current_prices()?;
        }
        let mut cache = self.cache.lock().map_err(|_| err("CACHE_LOCK"))?;
        self.check_view(&view)?;
        if expires <= now_ms() {
            return Err(err("QUOTE_EXPIRED"));
        }
        let capacity = (self.config.config.api.requests_per_minute as usize).clamp(16, 10000);
        if cache.len() >= capacity {
            if let Some(k) = cache.keys().next().cloned() {
                cache.remove(&k);
            }
        }
        let response = cached.response.clone();
        cache.insert(quote_id.clone(), cached);
        drop(cache);
        self.logger.event("info","quote_completed",json!({"quote_id":quote_id,"chain_key":request.chain_key,"quote_view_id":view.view_id,"graph_version":view.graph_version,"canonical_epoch":view.reference.canonical_epoch,"online_generation":view.reference.online_generation,"economic_policy_hash":self.config.economic_hash,"verification":response["verification"]["level"]}));
        self.observe_successful_quote(&view, &response);
        Ok(response)
    }
    pub async fn reload_native_prices(&self) -> Result<Value> {
        self.native_publication(transaction_economics::PublicationCommand::Reload)
            .await
    }
    async fn native_publication(
        &self,
        command: transaction_economics::PublicationCommand,
    ) -> Result<Value> {
        // Explicit mutation is unavailable on an anonymously configured server.
        if self.config.token.is_empty() {
            return Err(err("NATIVE_VALUATION_RELOAD_AUTH_REQUIRED"));
        }
        let backend = self
            .local_simulation
            .clone()
            .ok_or_else(|| err("NATIVE_VALUATION_NOT_CONFIGURED"))?;
        let permit = backend
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| err("NATIVE_VALUATION_RELOAD_BUSY"))?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel = CancelOnDrop(cancelled.clone());
        let config = self.config.clone();
        let deadline = Instant::now()
            + Duration::from_millis(
                config
                    .config
                    .api
                    .quote_timeout_ms
                    .min(config.config.simulation.timeout_ms),
            );
        let mutates = command.mutates();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            backend.reload_prices(&config, command, &cancelled, deadline)
        });
        let result = tokio::time::timeout_at(deadline.into(), task)
            .await
            .map_err(|_| {
                if mutates
                    && self
                        .config
                        .config
                        .simulation
                        .native_valuation
                        .as_ref()
                        .is_some_and(|v| v.publication_journal.is_some())
                {
                    err("NATIVE_VALUATION_DURABLE_ACK_UNCERTAIN")
                } else {
                    err("NATIVE_VALUATION_RELOAD_TIMEOUT")
                }
            })?
            .map_err(|_| err("NATIVE_VALUATION_RELOAD_FAILED"))??;
        self.logger
            .event("info", "native_valuation_publication", result.clone());
        Ok(result)
    }
    fn check_quote_prices(&self, quote: &Value) -> Result<()> {
        if let Some(expected) = quote["quality"]["costRanking"]["priceBookHash"].as_str() {
            let backend = self
                .local_simulation
                .as_ref()
                .ok_or_else(|| err("NATIVE_VALUATION_NOT_CONFIGURED"))?
                .pin_prices()?;
            if backend.price_identity().map(|p| p.0) != Some(expected) {
                return Err(err("NATIVE_VALUATION_QUOTE_PRICE_CHANGED"));
            }
        }
        Ok(())
    }
    pub async fn sync_sources(&self) {
        for chain in &self.config.config.chains {
            self.sync_chain(&chain.key).await;
        }
    }
    async fn sync_chain(&self, chain: &str) {
        if self.config.config.journal.transport == "kafka" {
            self.sync_kafka(chain).await;
        } else {
            self.sync_http(chain).await;
        }
        self.observe_ready_view(chain);
    }
    async fn sync_http(&self, chain: &str) {
        for source in &self.config.config.journal.source_urls {
            match self.sync_source(source, chain).await {
                Ok(()) => return,
                Err(e) => {
                    self.logger.event(
                        "debug",
                        "state_source_unavailable",
                        json!({"chain_key":chain,"error_code":e.0}),
                    );
                }
            }
        }
        self.replica.suspend(chain);
    }
    async fn sync_kafka(&self, chain: &str) {
        let key = chain.to_owned();
        let readers = self.readers.clone();
        let max = self.config.config.journal.kafka.max_tail_records;
        let timeout = Duration::from_millis(self.config.config.journal.request_timeout_ms);
        let poll = tokio::task::spawn_blocking(
            move || -> Result<Option<Vec<(xone_kafka_transport::Unit, Cursor)>>> {
                let reader = readers.lock().map_err(|_| err("READER_LOCK"))?.remove(&key);
                let Some(mut reader) = reader else {
                    return Ok(None);
                };
                reader.check_lag(timeout)?;
                let units = drain_kafka_units(max, || {
                    let before = reader.cursor();
                    let unit = reader.poll(Duration::ZERO)?;
                    let cursor = reader.cursor();
                    Ok(KafkaPollStep {
                        unit,
                        progressed: cursor != before,
                        cursor,
                    })
                })?;
                readers
                    .lock()
                    .map_err(|_| err("READER_LOCK"))?
                    .insert(key, reader);
                Ok(Some(units))
            },
        )
        .await;
        let mut restored = false;
        match poll {
            Ok(Ok(Some(units))) => {
                let mut ok = true;
                for (u, cursor) in units {
                    let reference = u.snapshot.reference.clone();
                    if let Err(e) = apply_kafka_unit(&self.replica, chain, u) {
                        ok = false;
                        self.logger.event(
                            "error",
                            "journal_apply_failed",
                            json!({"chain_key":chain,"error_code":e.0}),
                        );
                        break;
                    }
                    if let Ok(mut applied) = self.applied_cursors.lock() {
                        applied.insert(chain.to_owned(), (reference, cursor));
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    restored = true;
                } else {
                    self.replica.suspend(chain);
                    if let Ok(mut readers) = self.readers.lock() {
                        readers.remove(chain);
                    }
                    if let Ok(mut applied) = self.applied_cursors.lock() {
                        applied.remove(chain);
                    }
                }
            }
            Ok(Ok(None)) => {}
            _ => self.replica.suspend(chain),
        }
        if restored {
            return;
        }
        if self.resume_applied_kafka(chain).await.is_ok() {
            return;
        }
        if self.restore_peer_checkpoint(chain).await.is_ok() {
            return;
        }
        for source in &self.config.config.journal.source_urls {
            match self.restore_kafka(source, chain).await {
                Ok(()) => return,
                Err(e) => self.logger.event(
                    "debug",
                    "kafka_restore_source_unavailable",
                    json!({"chain_key":chain,"error_code":e.0}),
                ),
            }
        }
    }
    async fn resume_applied_kafka(&self, chain: &str) -> Result<()> {
        let (reference, cursor) = self
            .applied_cursors
            .lock()
            .map_err(|_| err("CURSOR_LOCK"))?
            .get(chain)
            .cloned()
            .ok_or_else(|| err("APPLIED_CURSOR_REQUIRED"))?;
        if self.replica.last_reference(chain)?.as_ref() != Some(&reference)
            || self.config.config.journal.kafka.topics.get(chain) != Some(&cursor.topic)
        {
            return Err(err("APPLIED_CURSOR_MISMATCH"));
        }
        if !self.replica.reorg_pending(chain)? {
            let view = self.replica.diagnostic_view(chain)?;
            if view.reference != reference {
                return Err(err("APPLIED_CURSOR_MISMATCH"));
            }
            // Replay through the broker's current watermark before restoring readiness.
            let boot = Bootstrap {
                cursor,
                snapshot: Snapshot {
                    reference,
                    pools: view.pools.clone(),
                    source: view.source.clone(),
                    history_gaps: view.history_gaps.clone(),
                },
            };
            return self
                .restore_kafka_checkpoint(chain, boot, "APPLIED_CHECKPOINT_AND_KAFKA")
                .await;
        }
        let cfg = self.config.clone();
        let chain_key = chain.to_owned();
        let reader = tokio::task::spawn_blocking(move || {
            let k = &cfg.config.journal.kafka;
            let mut reader = Reader::open(
                &k.brokers,
                &format!("quote.{}.{}", chain_key, k.replica_id),
                &cursor,
                cfg.config.journal.max_payload_bytes,
                k.max_tail_records,
                Duration::from_millis(cfg.config.journal.request_timeout_ms),
            )?;
            reader.last_sequence = reference.journal_seq;
            Ok::<_, xone_domain::Error>(reader)
        })
        .await
        .map_err(|_| err("KAFKA_READER_TASK"))??;
        self.readers
            .lock()
            .map_err(|_| err("READER_LOCK"))?
            .insert(chain.into(), reader);
        Ok(())
    }
    async fn restore_kafka(&self, source: &str, chain: &str) -> Result<()> {
        if self.config.config.journal.checkpoint_store.is_some() {
            let replay =
                self.replay_remote_checkpoint(source, chain)
                    .await
                    .and_then(|(boot, reader)| {
                        self.activate_recovery(
                            chain,
                            boot,
                            reader,
                            "REMOTE_CHECKPOINT_BOUNDED_REPLAY",
                        )
                    });
            match replay {
                Ok(()) => return Ok(()),
                Err(error) => self.logger.event(
                    "debug",
                    "remote_checkpoint_replay_unavailable",
                    json!({"chain_key":chain,"error_code":error.0}),
                ),
            }
            let result = match self.remote_bootstrap(source, chain).await {
                Ok(boot) => {
                    self.restore_kafka_checkpoint(chain, boot, "REMOTE_CHECKPOINT_AND_KAFKA")
                        .await
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(()) => return Ok(()),
                Err(error) => self.logger.event(
                    "debug",
                    "remote_checkpoint_unavailable",
                    json!({"chain_key":chain,"error_code":error.0}),
                ),
            }
        }
        let boot = self.owner_bootstrap(source, chain).await?;
        self.restore_kafka_checkpoint(chain, boot, "OWNER_CHECKPOINT_AND_KAFKA")
            .await
    }
    async fn restore_kafka_checkpoint(
        &self,
        chain: &str,
        boot: Bootstrap,
        recovery_source: &str,
    ) -> Result<()> {
        if boot.snapshot.reference.block.chain_key != chain
            || self.config.config.journal.kafka.topics.get(chain) != Some(&boot.cursor.topic)
            || boot.cursor.partition != 0
            || boot.cursor.offset < 1
        {
            return Err(err("BOOTSTRAP_CURSOR_IDENTITY"));
        }
        let cfg = self.config.clone();
        let chain_key = chain.to_owned();
        let (boot, reader) =
            tokio::task::spawn_blocking(move || align_kafka_checkpoint(&cfg, &chain_key, boot))
                .await
                .map_err(|_| err("KAFKA_READER_TASK"))??;
        self.activate_recovery(chain, boot, reader, recovery_source)
    }
    fn activate_recovery(
        &self,
        chain: &str,
        boot: Bootstrap,
        reader: Reader,
        recovery_source: &str,
    ) -> Result<()> {
        let now = now_ms();
        let timestamp = boot.snapshot.reference.block.timestamp_ms;
        if timestamp > now.saturating_add(30_000)
            || now.saturating_sub(timestamp) > self.config.config.startup.max_baseline_age_ms
        {
            return Err(err("BASELINE_TOO_OLD"));
        }
        let mut applied = self
            .applied_cursors
            .lock()
            .map_err(|_| err("CURSOR_LOCK"))?;
        let mut readers = self.readers.lock().map_err(|_| err("READER_LOCK"))?;
        let applied_reference = boot.snapshot.reference.clone();
        self.replica.restore_owner_checkpoint(boot.snapshot)?;
        applied.insert(chain.into(), (applied_reference, boot.cursor));
        readers.insert(chain.into(), reader);
        self.logger.event(
            "info",
            "quote_scope_restored",
            json!({"chain_key":chain,"source":recovery_source,"history_independent":true}),
        );
        Ok(())
    }
    async fn fetch(&self, path: &str) -> Result<Value> {
        serde_json::from_slice(&self.fetch_raw(path).await?).map_err(|_| err("STATE_SOURCE_JSON"))
    }
    async fn fetch_raw(&self, path: &str) -> Result<Vec<u8>> {
        self.fetch_raw_limited(path, self.config.config.journal.max_payload_bytes)
            .await
    }
    async fn fetch_raw_limited(&self, path: &str, limit: usize) -> Result<Vec<u8>> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(
                self.config.config.journal.request_timeout_ms,
            ))
            .build()
            .map_err(|_| err("STATE_CLIENT_CONFIG"))?;
        let token = self
            .config
            .config
            .journal
            .source_auth_tokens
            .iter()
            .filter(|(source, _)| {
                path.strip_prefix(source.trim_end_matches('/'))
                    .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
            })
            .max_by_key(|(source, _)| source.trim_end_matches('/').len())
            .map(|(_, token)| token);
        let request = client.get(path);
        let request = match token {
            Some(token) if !token.is_empty() => request.bearer_auth(token),
            _ => request,
        };
        let mut response = request
            .send()
            .await
            .map_err(|_| err("STATE_SOURCE_NETWORK"))?;
        if !response.status().is_success() {
            return Err(err("STATE_SOURCE_STATUS"));
        }
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(err("STATE_SOURCE_LIMIT"));
        }
        let mut b = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| err("STATE_SOURCE_NETWORK"))?
        {
            if b.len().saturating_add(chunk.len()) > limit {
                return Err(err("STATE_SOURCE_LIMIT"));
            }
            b.extend(chunk);
        }
        Ok(b)
    }
    async fn sync_source(&self, source: &str, chain: &str) -> Result<()> {
        let snapshot: Snapshot = serde_json::from_value(
            self.fetch(&format!("{source}/internal/chains/{chain}/baseline"))
                .await?,
        )
        .map_err(|_| err("BASELINE_SCHEMA"))?;
        if snapshot.reference.block.chain_key != chain {
            return Err(err("STATE_SOURCE_WRONG_CHAIN"));
        }
        if snapshot.reference.block.timestamp_ms > now_ms() + 30000
            || now_ms().saturating_sub(snapshot.reference.block.timestamp_ms)
                > self.config.config.startup.max_baseline_age_ms
        {
            return Err(err("BASELINE_TOO_OLD"));
        }
        match self.replica.last_reference(chain)? {
            None => self.replica.install_baseline(snapshot),
            Some(reference) => {
                if snapshot.reference.online_generation > reference.online_generation {
                    return self.replica.install_baseline(snapshot);
                }
                if snapshot.reference.online_generation < reference.online_generation
                    || snapshot.reference.journal_seq < reference.journal_seq
                {
                    return Err(err("STALE_BASELINE"));
                }
                if snapshot.reference.journal_seq == reference.journal_seq {
                    return self.replica.restore_owner_checkpoint(snapshot);
                }
                let records: Vec<Record> = serde_json::from_value(
                    self.fetch(&format!(
                        "{source}/internal/chains/{chain}/journal?after={}",
                        reference.journal_seq
                    ))
                    .await?,
                )
                .map_err(|_| err("JOURNAL_SCHEMA"))?;
                for record in records {
                    if record.reference.block.chain_key != chain {
                        return Err(err("STATE_SOURCE_WRONG_CHAIN"));
                    }
                    if let Err(e) = self.replica.apply(record) {
                        self.replica.suspend(chain);
                        // A complete owner checkpoint can recover a local-only
                        // suspension. Replica still rejects checkpoints while
                        // a canonical reorg barrier is pending.
                        if e.0 == "REANCHOR_REQUIRED" {
                            return self.replica.restore_owner_checkpoint(snapshot);
                        }
                        return Err(e);
                    }
                }
                if self.replica.last_reference(chain)?.as_ref() != Some(&snapshot.reference) {
                    return Err(err("JOURNAL_INCOMPLETE_TAIL"));
                }
                self.replica.restore_owner_checkpoint(snapshot)
            }
        }
    }
}
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
type HttpError = (StatusCode, Json<Value>);
fn failure(e: xone_domain::Error) -> HttpError {
    let status = match e.0.as_str() {
        "UNAUTHORIZED" => StatusCode::UNAUTHORIZED,
        "NATIVE_VALUATION_PUBLICATION_PRECONDITION" => StatusCode::CONFLICT,
        "RATE_LIMITED" => StatusCode::TOO_MANY_REQUESTS,
        "CHAIN_NOT_READY"
        | "STATE_TOO_OLD"
        | "CHECKPOINT_PEER_UNAVAILABLE"
        | "NATIVE_VALUATION_DURABLE_UNAVAILABLE"
        | "NATIVE_VALUATION_DURABLE_ACK_UNCERTAIN"
        | "PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"
        | "PRICE_PUBLICATION_JOURNAL_REPLACED" => StatusCode::SERVICE_UNAVAILABLE,
        "BUDGET_EXHAUSTED" => StatusCode::GATEWAY_TIMEOUT,
        "SNAPSHOT_REORGED"
        | "QUOTE_EXPIRED"
        | "QUOTE_STATE_CHANGED"
        | "QUOTE_CLOCK_CHANGED"
        | "NATIVE_VALUATION_QUOTE_PRICE_CHANGED"
        | "NATIVE_VALUATION_REVISION_ROLLBACK_OR_CONFLICT"
        | "PREPARATION_PROGRAM_CHANGED" => StatusCode::CONFLICT,
        "INDEPENDENT_SIMULATION_UNAVAILABLE"
        | "SIMULATION_FAILED"
        | "EXECUTION_ADAPTER_UNSUPPORTED"
        | "PREPARATION_RPC_NOT_CONFIGURED"
        | "PREPARATION_SINGLE_POOL_ONLY"
        | "PREPARATION_ACCOUNT_INVALID"
        | "RAYCLMM_POSITION_INVALID"
        | "RAYCLMM_POSITION_PAYER_INVALID"
        | "RAYCLMM_POSITION_POOL_IDENTITY"
        | "RAYCLMM_POSITION_TICKS"
        | "RAYCLMM_POSITION_NOT_APPROVED"
        | "RAYCLMM_POSITION_FEE_OVERFLOW"
        | "RAYCLMM_POSITION_OWED_OVERFLOW"
        | "RAYCLMM_POSITION_REWARD_LEDGER"
        | "RAYCLMM_POSITION_SETTLEMENT_BALANCE"
        | "RAYCLMM_REWARD_ACCOUNT_COUNT"
        | "RAYCLMM_REWARD_ACCOUNT_INVALID"
        | "RAYCLMM_REWARD_MINT_UNSUPPORTED"
        | "RAYCLMM_REWARD_VAULT_INVALID"
        | "PREPARATION_TRANSACTION_TOO_LARGE"
        | "PREPARATION_CREATE_REQUIRES_ATA"
        | "PREPARATION_CREATE_TOKEN2022_UNSUPPORTED"
        | "PREPARATION_WRAP_REQUIRES_WSOL"
        | "PREPARATION_REFUND_REQUIRES_TEMP_SOURCE"
        | "PREPARATION_UNWRAP_CONTEXT_UNSUPPORTED"
        | "PREPARATION_TEMP_ACCOUNT_OCCUPIED"
        | "PREPARATION_TEMP_ACCOUNT_NOT_CLOSED"
        | "PREPARATION_SIMULATION_FAILED"
        | "SBF_BANK_NOT_COVERED"
        | "SBF_BANK_AMBIGUOUS"
        | "SBF_RUNTIME_NOT_CONFIGURED"
        | "SBF_PREPARATION_INVALID"
        | "SBF_SIMULATION_TIMEOUT"
        | "SBF_TRANSACTION_FAILED"
        | "SBF_AMOUNT_MISMATCH"
        | "PREPARATION_QUOTE_MISMATCH"
        | "PREPARATION_OPTIONS_UNSUPPORTED"
        | "PREPARATION_QUOTE_INVALID"
        | "PREPARATION_SIMULATION_FEE_MISMATCH"
        | "PREPARATION_SIMULATION_AMOUNT_MISMATCH" => StatusCode::UNPROCESSABLE_ENTITY,
        code if code.starts_with("EVM_RPC_")
            || code.starts_with("PREPARATION_EVM_")
            || code.starts_with("EVM_PREPARATION_") =>
        {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        "PREPARATION_NOT_FOUND" | "RECEIPT_NOT_FOUND" => StatusCode::NOT_FOUND,
        "SUBMISSION_BUSY" | "SBF_SIMULATION_BUSY" => StatusCode::TOO_MANY_REQUESTS,
        code if code.starts_with("SIGNED_")
            || code.starts_with("WALLET_")
            || code.starts_with("TRANSACTION_")
            || code.starts_with("RECEIPT_")
            || code == "INDEPENDENT_SIMULATION_REQUIRED"
            || code.starts_with("SBF_") =>
        {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        "NO_ROUTE" => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, Json(json!({"error":e.0})))
}
async fn quote(
    State(api): State<Api>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    api.metrics.requests.fetch_add(1, Ordering::Relaxed);
    let request: QuoteRequest = serde_json::from_slice(&body)
        .map_err(|_| failure(err("INVALID_REQUEST_AMOUNT_MUST_BE_STRING")))?;
    match api.quote(request).await {
        Ok(v) => {
            api.metrics.succeeded.fetch_add(1, Ordering::Relaxed);
            Ok(Json(v))
        }
        Err(e) => {
            api.metrics.failed.fetch_add(1, Ordering::Relaxed);
            Err(failure(e))
        }
    }
}
async fn build_transaction(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    if let Some(response) = v4_execution::build(&api, &id).await.map_err(failure)? {
        return Ok(Json(response));
    }
    if let Some(backend) = api.local_simulation.clone().filter(|b| b.has_op_source()) {
        let selected = if api.exact_out_execution(&id).map_err(failure)? {
            let cache = api
                .exact_out_cache
                .lock()
                .map_err(|_| failure(err("CACHE_LOCK")))?;
            local_simulation::Selection::ExactOut(
                cache
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?,
            )
        } else {
            let cache = api.cache.lock().map_err(|_| failure(err("CACHE_LOCK")))?;
            local_simulation::Selection::ExactIn(
                cache
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?,
            )
        };
        if selected.expires() <= now_ms() {
            return Err(failure(err("QUOTE_EXPIRED")));
        }
        api.check_view(selected.view()).map_err(failure)?;
        api.check_quote_prices(selected.response())
            .map_err(failure)?;
        let deadline = Instant::now()
            + Duration::from_millis(api.config.config.simulation.timeout_ms.min(60_000));
        let mut response = backend
            .build_op_template(api.config.clone(), selected.clone(), deadline)
            .await
            .map_err(simulation_failure)?;
        if selected.expires() <= now_ms() {
            return Err(failure(err("QUOTE_EXPIRED")));
        }
        api.check_view(selected.view()).map_err(failure)?;
        api.check_quote_prices(selected.response())
            .map_err(failure)?;
        response["quoteId"] = id.clone().into();
        response["quoteViewId"] = selected.view().view_id.clone().into();
        let current_hash = match &selected {
            local_simulation::Selection::ExactIn(c) => {
                let cache = api.cache.lock().map_err(|_| failure(err("CACHE_LOCK")))?;
                let current = cache
                    .get(&id)
                    .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?;
                if current.expires <= now_ms() {
                    return Err(failure(err("QUOTE_EXPIRED")));
                }
                api.check_view(&current.view).map_err(failure)?;
                if current.plan.plan_hash != c.plan.plan_hash {
                    return Err(failure(err("SNAPSHOT_REORGED")));
                }
                c.plan.plan_hash.clone()
            }
            local_simulation::Selection::ExactOut(c) => {
                let cache = api
                    .exact_out_cache
                    .lock()
                    .map_err(|_| failure(err("CACHE_LOCK")))?;
                let current = cache
                    .get(&id)
                    .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?;
                if current.expires <= now_ms() {
                    return Err(failure(err("QUOTE_EXPIRED")));
                }
                api.check_view(&current.view).map_err(failure)?;
                if current.plan.plan_hash != c.plan.plan_hash {
                    return Err(failure(err("SNAPSHOT_REORGED")));
                }
                c.plan.plan_hash.clone()
            }
        };
        response["planHash"] = current_hash.into();
        response["buildHash"] = xone_domain::hash(&response).map_err(failure)?.into();
        return Ok(Json(response));
    }
    if api.exact_out_execution(&id).map_err(failure)? {
        return Err(failure(err("EXECUTION_ADAPTER_UNSUPPORTED")));
    }
    let cached = {
        let cache = api.cache.lock().map_err(|_| failure(err("CACHE_LOCK")))?;
        cache
            .get(&id)
            .cloned()
            .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?
    };
    if cached.expires <= now_ms() {
        return Err(failure(err("QUOTE_EXPIRED")));
    }
    api.check_view(&cached.view).map_err(failure)?;
    api.check_quote_prices(&cached.response).map_err(failure)?;
    xone_simulation::verify(&cached.plan).map_err(failure)?;
    Err(failure(err("EXECUTION_ADAPTER_UNSUPPORTED")))
}
fn simulation_failure(error: local_simulation::Failure) -> HttpError {
    let status = if error.code == "SIMULATION_BUSY" {
        StatusCode::TOO_MANY_REQUESTS
    } else if error.code == "NATIVE_VALUATION_QUOTE_PRICE_CHANGED" {
        StatusCode::CONFLICT
    } else if error.code == "NATIVE_VALUATION_DURABLE_UNAVAILABLE" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    (
        status,
        Json(json!({"error":error.code,"phase":error.phase,"usage":error.usage})),
    )
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulationRequest {
    #[serde(default, rename = "signedOpTransaction")]
    signed_op_transaction: Option<local_simulation::SignedOpRequest>,
    #[serde(default, rename = "signedEvmTransaction")]
    signed_evm_transaction: Option<xone_simulation::worker::SignedEvmTransaction>,
    #[serde(default, rename = "checkEvmReceipt")]
    check_evm_receipt: bool,
    #[serde(default, rename = "checkEvmSimulation")]
    check_evm_simulation: bool,
}
async fn simulate_transaction(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> std::result::Result<Json<Value>, HttpError> {
    let started = Instant::now();
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    let deadline =
        started + Duration::from_millis(api.config.config.simulation.timeout_ms.min(60_000));
    let body = tokio::time::timeout_at(
        deadline.into(),
        axum::body::to_bytes(request.into_body(), api.config.config.api.max_body_bytes),
    )
    .await
    .map_err(|_| {
        simulation_failure(local_simulation::Failure::new(
            "SIMULATION_TIMEOUT",
            "request",
        ))
    })?
    .map_err(|_| failure(err("INVALID_SIMULATION_REQUEST")))?;
    let simulation_request = if body.is_empty() {
        SimulationRequest::default()
    } else {
        if body.iter().find(|byte| !byte.is_ascii_whitespace()) != Some(&b'{') {
            return Err(failure(err("INVALID_SIMULATION_REQUEST")));
        }
        serde_json::from_slice::<SimulationRequest>(&body)
            .map_err(|_| failure(err("INVALID_SIMULATION_REQUEST")))?
    };
    if simulation_request.signed_evm_transaction.is_none() && simulation_request.check_evm_receipt
        || simulation_request.signed_evm_transaction.is_some()
            && simulation_request.signed_op_transaction.is_some()
        || simulation_request.check_evm_simulation
            && (simulation_request.check_evm_receipt
                || simulation_request.signed_evm_transaction.is_some()
                || simulation_request.signed_op_transaction.is_some())
    {
        return Err(failure(err("INVALID_SIMULATION_REQUEST")));
    }
    let is_exact_out = api.exact_out_execution(&id).map_err(failure)?;
    let backend = api
        .local_simulation
        .clone()
        .ok_or_else(|| failure(err("INDEPENDENT_SIMULATION_UNAVAILABLE")))?;
    let cached = if is_exact_out {
        let cache = api
            .exact_out_cache
            .lock()
            .map_err(|_| failure(err("CACHE_LOCK")))?;
        local_simulation::Selection::ExactOut(
            cache
                .get(&id)
                .cloned()
                .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?,
        )
    } else {
        let cache = api.cache.lock().map_err(|_| failure(err("CACHE_LOCK")))?;
        local_simulation::Selection::ExactIn(
            cache
                .get(&id)
                .cloned()
                .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?,
        )
    };
    if cached.expires() <= now_ms() {
        return Err(failure(err("QUOTE_EXPIRED")));
    }
    api.check_view(cached.view()).map_err(failure)?;
    api.metrics
        .simulation_started
        .fetch_add(1, Ordering::Relaxed);
    let receipt_backend = simulation_request
        .check_evm_receipt
        .then(|| backend.clone());
    let native_backend = simulation_request
        .check_evm_simulation
        .then(|| backend.clone());
    let executed = if let Some(signed) = simulation_request.signed_evm_transaction {
        backend
            .execute_signed_evm(api.config.clone(), cached.clone(), deadline, signed)
            .await
    } else if let Some(signed) = simulation_request.signed_op_transaction {
        backend
            .execute_signed_op(api.config.clone(), cached.clone(), deadline, signed)
            .await
    } else {
        backend
            .execute(api.config.clone(), cached.clone(), deadline)
            .await
    };
    let executed = match (executed, receipt_backend) {
        (Ok(mut response), Some(backend)) => evm_receipt::compare(
            &api,
            backend.fixture(),
            cached.context(),
            &response,
            deadline,
        )
        .await
        .map(|observed| {
            response["nodeReceipt"] = observed;
            response
        }),
        (executed, _) => executed,
    };
    let executed = match (executed, native_backend) {
        (Ok(mut response), Some(backend)) => evm_receipt::compare_simulation(
            &api,
            backend.fixture(),
            cached.context(),
            &response,
            deadline,
        )
        .await
        .map(|observed| {
            response["nodeSimulation"] = observed;
            response
        }),
        (executed, _) => executed,
    };
    let response = match executed {
        Ok(response) => response,
        Err(error) => {
            if error.code == "SIMULATION_BUSY" {
                api.metrics.simulation_busy.fetch_add(1, Ordering::Relaxed);
            } else {
                api.metrics
                    .simulation_failed
                    .fetch_add(1, Ordering::Relaxed);
            }
            return Err(simulation_failure(error));
        }
    };
    match cached {
        local_simulation::Selection::ExactIn(cached) => {
            publish_simulation(&api, &id, &cached, response)
        }
        local_simulation::Selection::ExactOut(cached) => {
            publish_exact_out_simulation(&api, &id, &cached, response)
        }
    }
}
fn publish_simulation(
    api: &Api,
    id: &str,
    cached: &Cached,
    mut response: Value,
) -> std::result::Result<Json<Value>, HttpError> {
    if cached.expires <= now_ms() {
        return Err(failure(err("QUOTE_EXPIRED")));
    }
    api.check_view(&cached.view).map_err(failure)?;
    api.check_quote_prices(&cached.response).map_err(failure)?;
    response["quoteId"] = id.into();
    response["quoteViewId"] = cached.view.view_id.clone().into();
    response["planHash"] = cached.plan.plan_hash.clone().into();
    response["snapshotRef"] =
        serde_json::to_value(&cached.plan.snapshot).map_err(|_| failure(err("HASH_ENCODING")))?;
    response["verification"]["verified"] = false.into();
    {
        let mut cache = api.cache.lock().map_err(|_| failure(err("CACHE_LOCK")))?;
        let current = cache
            .get_mut(id)
            .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?;
        if current.expires <= now_ms() {
            return Err(failure(err("QUOTE_EXPIRED")));
        }
        api.check_view(&current.view).map_err(failure)?;
        if current.plan.plan_hash != cached.plan.plan_hash {
            return Err(failure(err("SNAPSHOT_REORGED")));
        }
        if let Some(expected) = response.get("apiPreparationHash") {
            if current.evm_preparation.as_ref().map(|p| p.hash.as_str()) != expected.as_str() {
                return Err(failure(err("EVM_PREPARATION_REPLACED")));
            }
        }
        current.simulation_evidence = Some(response.clone());
    }
    if response["verification"]["success"] == true {
        api.metrics
            .simulation_success
            .fetch_add(1, Ordering::Relaxed);
    } else {
        api.metrics
            .simulation_failed
            .fetch_add(1, Ordering::Relaxed);
    }
    Ok(Json(response))
}
fn publish_exact_out_simulation(
    api: &Api,
    id: &str,
    cached: &exact_out::ExactOutCached,
    mut response: Value,
) -> std::result::Result<Json<Value>, HttpError> {
    if cached.expires <= now_ms() {
        return Err(failure(err("QUOTE_EXPIRED")));
    }
    api.check_view(&cached.view).map_err(failure)?;
    api.check_quote_prices(&cached.response).map_err(failure)?;
    response["quoteId"] = id.into();
    response["quoteViewId"] = cached.view.view_id.clone().into();
    response["planHash"] = cached.plan.plan_hash.clone().into();
    response["snapshotRef"] =
        serde_json::to_value(&cached.plan.snapshot).map_err(|_| failure(err("HASH_ENCODING")))?;
    response["verification"]["verified"] = false.into();
    {
        let mut cache = api
            .exact_out_cache
            .lock()
            .map_err(|_| failure(err("CACHE_LOCK")))?;
        let current = cache
            .get_mut(id)
            .ok_or_else(|| failure(err("UNKNOWN_QUOTE")))?;
        if current.expires <= now_ms() {
            return Err(failure(err("QUOTE_EXPIRED")));
        }
        api.check_view(&current.view).map_err(failure)?;
        if current.plan.plan_hash != cached.plan.plan_hash {
            return Err(failure(err("SNAPSHOT_REORGED")));
        }
        if let Some(expected) = response.get("apiPreparationHash") {
            if current.evm_preparation.as_ref().map(|p| p.hash.as_str()) != expected.as_str() {
                return Err(failure(err("EVM_PREPARATION_REPLACED")));
            }
        }
        current.simulation_evidence = Some(response.clone());
    }
    if response["verification"]["success"] == true {
        api.metrics
            .simulation_success
            .fetch_add(1, Ordering::Relaxed);
    } else {
        api.metrics
            .simulation_failed
            .fetch_add(1, Ordering::Relaxed);
    }
    Ok(Json(response))
}

async fn evidence(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    {
        let cache = api.cache.lock().map_err(|_| failure(err("CACHE_LOCK")))?;
        if let Some(cached) = cache.get(&id) {
            return Ok(Json(
                json!({"quote":cached.response,"execution_plan":cached.plan,"expired":cached.expires<=now_ms(),"snapshot_valid":cached.view.is_valid(),"price_current":api.check_quote_prices(&cached.response).is_ok(),"simulation_evidence":cached.simulation_evidence,"quote_verification_evidence":cached.quote_verification_evidence}),
            ));
        }
    }
    api.exact_out_evidence(&id).map(Json).map_err(failure)
}

async fn quote_options(
    State(api): State<Api>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    let mut chains = Vec::new();
    for configured in &api.config.config.chains {
        let view = api.replica.diagnostic_view(&configured.key).ok();
        let mut pools = Vec::new();
        let mut tokens = std::collections::BTreeSet::new();
        let ready = view
            .as_ref()
            .is_some_and(|v| !v.graph.adjacency.is_empty() && api.check_view(v).is_ok());
        if let Some(ref v) = view {
            let mut by_pool = BTreeMap::<&str, Vec<(&String, &xone_routing::Edge)>>::new();
            if ready {
                for (from, edges) in &v.graph.adjacency {
                    for edge in edges.iter() {
                        by_pool.entry(&edge.pool_id).or_default().push((from, edge));
                    }
                }
            }
            for pool in &v.pools {
                let mut directions = Vec::new();
                if let Some(edges) = by_pool.get(pool.pool_id.as_str()) {
                    for (from, edge) in edges {
                        let mut modes = vec!["EXACT_IN"];
                        if xone_protocol_math::exact_out::supports_pair(pool, from, &edge.to) {
                            modes.push("EXACT_OUT");
                        }
                        tokens.insert((*from).clone());
                        tokens.insert(edge.to.clone());
                        directions
                            .push(json!({"sellToken":from,"buyToken":edge.to,"quoteModes":modes}));
                    }
                }
                let quote_modes = directions
                    .iter()
                    .flat_map(|d| d["quoteModes"].as_array().into_iter().flatten())
                    .filter_map(Value::as_str)
                    .collect::<std::collections::BTreeSet<_>>();
                let available = !directions.is_empty();
                let reason = if !ready {
                    Some("CHAIN_NOT_READY")
                } else {
                    v.graph
                        .excluded_pools
                        .get(&pool.pool_id)
                        .map(String::as_str)
                        .or(if available {
                            None
                        } else {
                            Some("NO_SEARCHABLE_EDGE")
                        })
                };
                let protocol_view = v
                    .protocol_views
                    .get(&format!("{}@{}", pool.protocol, pool.adapter_version))
                    .ok_or_else(|| failure(err("PROTOCOL_VIEW_MISSING")))?;
                let capability = &protocol_view.capabilities;
                let local_sbf = available
                    && api
                        .solana_sbf
                        .as_ref()
                        .is_some_and(|engine| engine.available(pool, &configured.genesis_hash));
                let wallet_submission = capability.wallet_submission
                    && local_sbf
                    && api.config.config.api.transaction_submission_enabled
                    && api.receipts.is_some()
                    && api
                        .config
                        .config
                        .api
                        .transaction_submission_chains
                        .iter()
                        .any(|key| key == &configured.key);
                let evm_preparation = available
                    && pool.protocol == xone_domain::v2_swap::PROFILE
                    && configured.rpc_urls.len() == 1
                    && api
                        .config
                        .config
                        .api
                        .evm_preparation
                        .get(&configured.key)
                        .is_some_and(|p| {
                            p.token_code_sha256.contains_key(&pool.token0)
                                && p.token_code_sha256.contains_key(&pool.token1)
                                && pool.v2.as_ref().is_some_and(|v| v.factory == p.factory)
                        });
                let evm_local = evm_preparation
                    && api.local_simulation.as_ref().is_some_and(|backend| {
                        backend.matches_snapshot(&v.reference, &configured.genesis_hash)
                    });
                let v4_template = available && v4_execution::configured(&api, pool);
                pools.push(json!({
                    "protocolViewId":protocol_view.view_id,
                    "capabilities":capability,
                    "poolId": pool.pool_id,
                    "protocol": pool.protocol,
                    "adapterVersion": pool.adapter_version,
                    "token0": pool.token0,
                    "token1": pool.token1,
                    "quoteModes": quote_modes,
                    "directions": directions,
                    "available": available,
                    "unavailableReason": reason,
                    "transactionBuildEndpoint":v4_template.then_some("/v1/quotes/{id}/build"),
                    "execution": {"evmIndependentLocalSimulation":evm_local,"evmPreparation":evm_preparation,"evmPreparationEndpoint":if evm_preparation{Some("/v1/quotes/{id}/prepare-evm")}else{None},"quote": available, "preparation": available && capability.execution_preparation && !configured.rpc_urls.is_empty(), "unsignedPlan": available && capability.unsigned_transaction && !configured.rpc_urls.is_empty(), "rpcSimulation": available && capability.rpc_simulation && !configured.rpc_urls.is_empty(), "createDestinationAta":available && capability.account_creation && !configured.rpc_urls.is_empty(), "wrapSourceSol": available && capability.native_funding && !configured.rpc_urls.is_empty() && (pool.token0 == xone_domain::pump_amm::WSOL || pool.token1 == xone_domain::pump_amm::WSOL), "unwrapDestinationSol": available && capability.native_funding && !configured.rpc_urls.is_empty() && (pool.token0 == xone_domain::pump_amm::WSOL || pool.token1 == xone_domain::pump_amm::WSOL), "walletSubmission":wallet_submission,"receiptTracking":api.receipts.is_some(),"independentLocalSimulation":local_sbf,"simulation": false},
                }));
            }
        }
        let protocols = pools
            .iter()
            .filter(|pool| pool["available"] == true)
            .filter_map(|pool| pool.get("protocol").and_then(Value::as_str))
            .collect::<std::collections::BTreeSet<_>>();
        let (view_id, graph_version, snapshot_ref, pool_count) =
            view.as_ref()
                .map_or((Value::Null, Value::Null, Value::Null, 0usize), |v| {
                    (
                        json!(v.view_id),
                        json!(v.graph_version),
                        json!(v.reference),
                        v.pools.len(),
                    )
                });
        let protocol_views = view.as_ref().map_or_else(Vec::new, |v| {
            v.protocol_views
                .values()
                .map(|p| {
                    let mut data = serde_json::to_value(p).expect("protocol view serialization");
                    data["valid"] = json!(ready && p.is_valid());
                    data["ready"] =
                        json!(ready && p.is_valid() && !p.available_pool_ids.is_empty());
                    data
                })
                .collect::<Vec<_>>()
        });
        chains.push(json!({
            "protocolViews":protocol_views,
            "chainKey": configured.key,
            "ownerService": configured.owner_service,
            "ready": ready,
            "viewId": view_id,
            "graphVersion": graph_version,
            "snapshotRef": snapshot_ref,
            "poolCount": pool_count,
            "protocols": protocols.into_iter().collect::<Vec<_>>(),
            "tokens": tokens.into_iter().collect::<Vec<_>>(),
            "pools": pools,
        }));
    }
    Ok(Json(
        json!({"protocolCatalog":xone_protocol_math::capabilities::catalog(),"chains": chains, "engineVersion": "xone-router/0.1", "limits":{"maxHops":api.config.config.routing.max_hops,"maxSplits":api.config.config.routing.max_splits}}),
    ))
}

async fn chain_status(
    State(api): State<Api>,
    headers: HeaderMap,
    Path(chain): Path<String>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    let view = api.replica.view(&chain).map_err(failure)?;
    let ready = api.check_view(&view).is_ok();
    Ok(Json(
        json!({"chain_key":chain,"ready":ready,"readiness_scope":"LOCAL_QUOTE_SCOPE","snapshot_ref":view.reference,"quote_view_id":view.view_id,"graph_version":view.graph_version,"scope_pool_count":view.pools.len(),"recovery":api.recovery_status(&chain)}),
    ))
}
async fn config(
    State(api): State<Api>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    Ok(Json(
        json!({"release":api.config.release,"public_hash":api.config.public_hash,"economic_hash":api.config.economic_hash}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReloadNativeValuation {}
async fn native_publication(
    State(api): State<Api>,
    headers: HeaderMap,
    Json(command): Json<transaction_economics::PublicationCommand>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    api.native_publication(command)
        .await
        .map(Json)
        .map_err(failure)
}
async fn reload_native_valuation(
    State(api): State<Api>,
    headers: HeaderMap,
    Json(_request): Json<ReloadNativeValuation>,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    api.reload_native_prices().await.map(Json).map_err(failure)
}
async fn ready(State(api): State<Api>) -> impl IntoResponse {
    let ready = api.replica.chains().iter().any(|k| {
        api.replica
            .view(k)
            .is_ok_and(|v| api.check_view(&v).is_ok())
    });
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({"ready":ready,"scope":"LOCAL_QUOTE_SCOPE"})),
    )
}
async fn metrics(State(api): State<Api>) -> String {
    api.observe_ready_views();
    let simulation = format!("xone_simulation_started_total {}\nxone_simulation_success_total {}\nxone_simulation_failed_total {}\nxone_simulation_busy_total {}\n", api.metrics.simulation_started.load(Ordering::Relaxed),api.metrics.simulation_success.load(Ordering::Relaxed),api.metrics.simulation_failed.load(Ordering::Relaxed),api.metrics.simulation_busy.load(Ordering::Relaxed));
    api.recovery_prometheus() + &simulation + &format!("xone_quote_requests_total {}\nxone_quote_success_total {}\nxone_quote_failed_total {}\nxone_quote_rate_limited_total {}\nxone_log_dropped_total {}\n",api.metrics.requests.load(Ordering::Relaxed),api.metrics.succeeded.load(Ordering::Relaxed),api.metrics.failed.load(Ordering::Relaxed),api.metrics.rate_limited.load(Ordering::Relaxed),api.logger.counters.dropped.load(Ordering::Relaxed))
}
pub async fn run(config: ConfigSnapshot) -> Result<()> {
    let api = Api::new(config)?;
    let listener = tokio::net::TcpListener::bind(&api.config.config.api.listen)
        .await
        .map_err(|_| err("HTTP_LISTENER_FAILED"))?;
    serve(api, listener, shutdown()).await
}

pub(crate) async fn serve(
    api: Api,
    listener: tokio::net::TcpListener,
    signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let mut tasks = cache_gc::BackgroundTasks(Vec::new());
    let poll_interval = Duration::from_millis(api.config.config.journal.poll_interval_ms);
    for chain in &api.config.config.chains {
        let sync = api.clone();
        let key = chain.key.clone();
        tasks.0.push(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(poll_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                sync.sync_chain(&key).await;
            }
        }));
    }
    let maintenance = api.clone();
    tasks.0.push(tokio::spawn(async move {
        maintenance.maintain_cached_views().await
    }));
    let result = axum::serve(listener, api.handler())
        .with_graceful_shutdown(signal)
        .await;
    tasks.stop().await;
    api.logger.flush();
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
#[path = "../tests/support/evm_snapshot.rs"]
mod evm_snapshot;

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::evm_snapshot::snapshot;
    use xone_kafka_transport::{KafkaProducer, Unit};

    #[test]
    fn buffered_kafka_frames_drain_through_revocation_without_publishing_partial_units() {
        use std::collections::VecDeque;
        use xone_kafka_transport::{encode, Assembler};
        let mut snapshot = snapshot(1);
        let mut frames = VecDeque::new();
        for kind in ["BLOCK_COMMIT", "BLOCK_COMMIT", "ONLINE_SCOPE_REVOKED"] {
            snapshot.reference.journal_seq += 1;
            frames.extend(
                encode(
                    &Unit {
                        kind: kind.into(),
                        snapshot: snapshot.clone(),
                        normalized_facts: None,
                    },
                    1024,
                )
                .unwrap(),
            );
        }
        let total = frames.len();
        let mut consumed = 0;
        let mut assembler = Assembler::new(8 << 20);
        let units = drain_kafka_units(total + 1, || {
            let frame = frames.pop_front();
            let progressed = frame.is_some();
            let unit = if let Some(frame) = frame {
                consumed += 1;
                assembler.push(frame)?
            } else {
                None
            };
            Ok(KafkaPollStep {
                unit,
                cursor: Cursor {
                    topic: "test.frames".into(),
                    partition: 0,
                    offset: consumed as i64,
                },
                progressed,
            })
        })
        .unwrap();
        assert_eq!(units.len(), 3, "BEGIN/CHUNK are progress, not queue idle");
        assert_eq!(units.last().unwrap().0.kind, "ONLINE_SCOPE_REVOKED");
        assert_eq!(units.last().unwrap().1.offset, total as i64);
        let mut first = encode(
            &Unit {
                kind: "ONLINE_SCOPE_REVOKED".into(),
                snapshot,
                normalized_facts: None,
            },
            1024,
        )
        .unwrap()
        .into_iter();
        let mut assembler = Assembler::new(8 << 20);
        let mut offset = 0;
        let partial = drain_kafka_units(1, || {
            offset += 1;
            Ok(KafkaPollStep {
                unit: assembler.push(first.next().unwrap())?,
                cursor: Cursor {
                    topic: "test.frames".into(),
                    partition: 0,
                    offset,
                },
                progressed: true,
            })
        })
        .unwrap();
        assert!(
            partial.is_empty(),
            "incomplete unit must not become an applied cursor"
        );
        assert!(assembler.in_progress());
    }

    #[tokio::test]
    async fn kafka_poll_failure_between_barrier_and_commit_resumes_exact_applied_cursor() {
        let Ok(broker) = std::env::var("XONE_TEST_KAFKA_BROKER") else {
            return;
        };
        let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
        config.config.journal.kafka.brokers = vec![broker.clone()];
        config
            .config
            .journal
            .kafka
            .topics
            .insert("evm:31337".into(), "test.router-reorg".into());
        config.config.journal.kafka.topics.insert(
            "solana:localnet".into(),
            "canonical.solana-localnet.v1".into(),
        );
        config.config.journal.source_urls.clear();
        config.config.journal.transport = "kafka".into();
        let api = Api::new(config).unwrap();
        let mut s = snapshot(1);
        let mut producer = KafkaProducer::open(
            std::slice::from_ref(&broker),
            "test.router-reorg",
            "test.router-reorg-writer",
            1024,
            8 << 20,
            Duration::from_secs(5),
        )
        .unwrap();
        let mut publish = |kind: &str, snapshot: &Snapshot| {
            producer
                .publish(
                    &Unit {
                        normalized_facts: None,
                        kind: kind.into(),
                        snapshot: snapshot.clone(),
                    },
                    || Ok(()),
                )
                .unwrap()
        };
        let cursor = publish("ONLINE_BASELINE_COMMIT", &s);
        api.replica.install_baseline(s.clone()).unwrap();
        let old = api.replica.view("evm:31337").unwrap();
        api.applied_cursors
            .lock()
            .unwrap()
            .insert("evm:31337".into(), (s.reference.clone(), cursor));
        api.replica.suspend("evm:31337");
        api.sync_kafka("evm:31337").await;
        assert_eq!(
            api.replica.view("evm:31337").unwrap().reference,
            s.reference
        );
        assert!(!old.is_valid());

        let old = api.replica.view("evm:31337").unwrap();
        api.replica.suspend("evm:31337");
        api.readers.lock().unwrap().remove("evm:31337");
        s.reference.journal_seq += 1;
        s.reference.block.number_or_slot += 1;
        s.reference.block.parent_hash = s.reference.block.hash;
        s.reference.block.hash = format!("0x{:064x}", s.reference.block.number_or_slot);
        publish("BLOCK_COMMIT", &s);
        api.sync_kafka("evm:31337").await;
        assert_eq!(
            api.replica.view("evm:31337").unwrap().reference,
            s.reference
        );
        assert!(!old.is_valid());

        let old = api.replica.view("evm:31337").unwrap();
        s.reference.journal_seq += 1;
        s.reference.canonical_epoch += 1;
        publish("REORG_BEGIN", &s);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while api.replica.last_reference("evm:31337").unwrap().as_ref() != Some(&s.reference)
            && tokio::time::Instant::now() < deadline
        {
            api.sync_kafka("evm:31337").await;
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(!old.is_valid());
        let applied = api
            .applied_cursors
            .lock()
            .unwrap()
            .get("evm:31337")
            .unwrap()
            .0
            .clone();
        assert_eq!(applied, s.reference);
        // The same reset used after poll/check_lag failure. No checkpoint or
        // generation allocation is allowed to clear the pending barrier.
        api.replica.suspend("evm:31337");
        api.readers.lock().unwrap().remove("evm:31337");
        s.reference.journal_seq += 1;
        s.reference.block.hash =
            "0x3333333333333333333333333333333333333333333333333333333333333333".into();
        publish("REORG_COMMIT", &s);
        assert!(api.replica.restore_owner_checkpoint(s.clone()).is_err());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while api.replica.view("evm:31337").is_err() && tokio::time::Instant::now() < deadline {
            api.sync_kafka("evm:31337").await;
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(
            api.replica.view("evm:31337").unwrap().reference,
            s.reference
        );
        assert!(!old.is_valid());
    }
}

#[cfg(test)]
mod source_auth_tests {
    use super::*;
    #[tokio::test]
    async fn upstream_auth_is_independent_of_inbound_api_auth() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let routes = Router::new().route(
                "/a/internal/test",
                get(|headers: HeaderMap| async move {
                    if headers.get("authorization").and_then(|v| v.to_str().ok())
                        == Some("Bearer upstream-only-token-1234567890")
                    {
                        (StatusCode::OK, "source-ok")
                    } else {
                        (StatusCode::UNAUTHORIZED, "missing upstream token")
                    }
                }),
            );
            axum::serve(listener, routes).await.unwrap();
        });
        let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
        assert!(config.token.is_empty());
        config.config.journal.source_urls = vec![format!("{source}/a")];
        config.config.journal.source_auth_tokens.insert(
            format!("{source}/a"),
            "upstream-only-token-1234567890".into(),
        );
        config.config.journal.source_auth_tokens.insert(
            format!("{source}/////"),
            "other-upstream-root-token-123456".into(),
        );
        let api = Api::new(config).unwrap();
        let result = api.fetch_raw(&format!("{source}/a/internal/test")).await;
        server.abort();
        assert_eq!(result.unwrap(), b"source-ok");
    }
}
