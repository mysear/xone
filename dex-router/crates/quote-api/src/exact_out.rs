//! ExactOut has a separate request, commitment, cache and engine identity.
use super::*;
use xone_execution_plan::exact_out::{self as plan, ExactOutPlan, ENGINE};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExactOutRequest {
    pub chain_key: String,
    pub kind: String,
    pub sell_token: String,
    pub buy_token: String,
    pub buy_amount: Amount,
    pub max_sell_amount: Amount,
    pub caller: String,
    pub recipient: String,
    pub slippage_bps: u16,
    pub verification: String,
    pub constraints: Option<Constraints>,
}
impl ExactOutRequest {
    pub fn validate(&self) -> Result<()> {
        if self.kind != "EXACT_OUT" {
            return Err(err("QUOTE_KIND_UNSUPPORTED"));
        }
        if self.buy_amount.0.is_zero()
            || self.max_sell_amount.0.is_zero()
            || !(self.chain_key.starts_with("evm:") || self.chain_key.starts_with("solana:"))
            || self.sell_token == self.buy_token
            || self.slippage_bps > 10000
        {
            return Err(err("INVALID_REQUEST"));
        }
        if self.chain_key.starts_with("solana:")
            && (self.buy_amount.0.bits() > 64 || self.max_sell_amount.0.bits() > 64)
        {
            return Err(err("EXACT_OUT_SOLANA_AMOUNT_WIDTH"));
        }
        if self.chain_key.starts_with("solana:")
            && [&self.caller, &self.recipient]
                .into_iter()
                .any(|a| xone_domain::raydium::validate_pubkey(a).is_err())
        {
            return Err(err("INVALID_REQUEST"));
        }
        for address in [
            &self.sell_token,
            &self.buy_token,
            &self.caller,
            &self.recipient,
        ] {
            plan::validate_chain_address(&self.chain_key, address)?;
            if address == xone_domain::singleton::ZERO_ADDRESS {
                return Err(err("INVALID_REQUEST"));
            }
        }
        if !["FAST", "VERIFIED"].contains(&self.verification.as_str()) {
            return Err(err("VERIFICATION_UNSUPPORTED"));
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(super) struct ExactOutCached {
    pub(super) view: Arc<QuoteView>,
    pub(super) plan: ExactOutPlan,
    response: Value,
    pub(super) expires: u64,
    request_hash: String,
}
impl Api {
    pub async fn quote_exact_out(&self, request: ExactOutRequest) -> Result<Value> {
        request.validate()?;
        if request.verification == "VERIFIED" {
            return Err(err("INDEPENDENT_SIMULATION_UNAVAILABLE"));
        }
        let view = self.replica.view(&request.chain_key)?;
        self.check_view(&view)?;
        let request_hash = hash(&json!({"request": request, "view_id": view.view_id,
            "policy": self.config.economic_hash, "engine": ENGINE}))?;
        let shard = u64::from_str_radix(&request_hash[..16], 16)
            .map_err(|_| err("HASH_ENCODING"))? as usize
            % self.flights.len();
        let started = Instant::now();
        let timeout = Duration::from_millis(self.config.config.api.quote_timeout_ms);
        let _flight = tokio::time::timeout(timeout, self.flights[shard].lock())
            .await
            .map_err(|_| err("BUDGET_EXHAUSTED"))?;
        {
            let mut cache = self.exact_out_cache.lock().map_err(|_| err("CACHE_LOCK"))?;
            cache.retain(|_, c| c.expires > now_ms());
            if let Some(c) = cache.values().find(|c| c.request_hash == request_hash) {
                self.check_view(&c.view)?;
                self.observe_successful_quote(&c.view);
                return Ok(c.response.clone());
            }
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
        let deadline = started + timeout;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
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
            let candidates = xone_routing::exact_out::search(
                &worker_view.graph,
                &worker_view.pools,
                &req.sell_token,
                &req.buy_token,
                &req.buy_amount,
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
            let optimized = xone_optimizer::exact_out::optimize(
                &worker_view.pools,
                &candidates.candidates,
                &req.buy_amount,
                &req.max_sell_amount,
                xone_optimizer::OptimizationLimits {
                    parts: routing.split_parts,
                    max_splits: routing.max_splits,
                    max_evaluations: routing.max_expansions,
                },
                funds,
                Some((&cancelled, deadline)),
            )?;
            let expires = now_ms()
                .checked_add(routing.quote_ttl_ms)
                .ok_or_else(|| err("TIMESTAMP_OVERFLOW"))?;
            let context = Context {
                chain_key: req.chain_key,
                caller: req.caller,
                recipient: req.recipient,
                sell_token: req.sell_token,
                buy_token: req.buy_token,
                slippage_bps: req.slippage_bps,
                deadline_ms: expires,
            };
            let commitment = plan::build(
                &context,
                &optimized.allocation,
                &worker_view.reference,
                &policy,
            )?;
            Ok((commitment, candidates, optimized, expires))
        });
        let (plan, candidates, optimized, expires) =
            tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), task)
                .await
                .map_err(|_| err("BUDGET_EXHAUSTED"))?
                .map_err(|_| err("COMPUTE_TASK_FAILED"))??;
        self.check_view(&view)?;
        let quote_id = format!(
            "xo-{}",
            hash(&json!({"request_hash": request_hash,
            "plan_hash": plan.plan_hash, "expires": expires}))?
        );
        let quality = json!({"budgetExhausted": candidates.budget_exhausted || optimized.budget_exhausted,
                "candidateScope": view.pools.iter().filter(|p| !candidates.excluded_pools.contains_key(&p.pool_id))
                    .map(|p| p.pool_id.clone()).collect::<Vec<_>>(),
                "excludedPools": candidates.excluded_pools, "source": view.source, "historyIndependent": true,
                "optimalityClaim": "MINIMUM_RAW_INPUT_WITHIN_EVALUATED_ORDERED_GRID",
                "pathPolicy": {"uniswap-v2-complete": "SIMPLE_PATH_SINGLE_FACTORY",
                    "pancake-v3-bsc-lm-v4": "SINGLE_POOL_EXTERNAL_CALLBACK_FUNDING",
                    "pancake-v3-lm-context": "SINGLE_POOL_EXTERNAL_CALLBACK_FUNDING",
                    "aerodrome-slipstream-context": "SINGLE_POOL_ORIGIN_BOUND_EXTERNAL_CALLBACK_FUNDING",
                    "raydium-cpmm-creator": "SINGLE_POOL_CLASSIC_SPL_CREATOR_FEES",
                    "raydium-clmm-static":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT_SELECTED_FEE_TOKEN",
                    "raydium-clmm-rewards":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT_SELECTED_FEE_TOKEN",
                    "raydium-clmm-token-fee":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT_SELECTED_FEE_TOKEN",
                    "raydium-clmm-bitmap":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT_SELECTED_FEE_TOKEN",
                    "raydium-clmm-custom-seed":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT_SELECTED_FEE_TOKEN",
                    "raydium-clmm-dynamic":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT_SELECTED_FEE_TOKEN",
                    "orca-whirlpools-static":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT",
                    "orca-whirlpools-rewards":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT",
                    "orca-whirlpools-adaptive":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT",
                    "orca-whirlpools-adaptive-rewards":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT",
                    "orca-whirlpools-adaptive-dynamic":"SINGLE_POOL_CLASSIC_SPL_EXACT_OUTPUT",
                    "lfj-lb-v21": "SINGLE_POOL_MINIMUM_TARGET_EXPLICIT_SURPLUS"},
                "candidateSearch": {
                    "maxHops": request.constraints.as_ref().map_or(self.config.config.routing.max_hops, |c| c.max_hops),
                    "maxCandidates": self.config.config.routing.max_candidates,
                    "maxExpansions": self.config.config.routing.max_expansions,
                    "probeParts": if self.config.config.routing.max_splits > 1 { self.config.config.routing.split_parts } else { 1 }
                },
                "allocationSearch": {"evaluations": optimized.evaluations, "exhaustiveWithinGrid": optimized.grid_exhaustive,
                    "gridParts": self.config.config.routing.split_parts, "maxSplits": self.config.config.routing.max_splits,
                    "eachCandidateAtMostOnce": true},
                "expansions": candidates.expansions, "pathEvaluations": candidates.evaluations});
        let protocol_view_ids = view.protocol_bindings(
            plan.allocation
                .routes
                .iter()
                .flat_map(|r| r.edges.iter().map(|e| e.pool_id.as_str())),
        )?;
        let response = json!({
            "protocolViewIds":protocol_view_ids,
            "quoteId": quote_id, "requestHash": request_hash, "kind": "EXACT_OUT",
            "chainKey": request.chain_key, "snapshotRef": view.reference,
            "ownerService": view.reference.owner_service, "onlineGeneration": view.reference.online_generation,
            "scopeHash": view.reference.scope_hash, "quoteViewId": view.view_id, "graphVersion": view.graph_version,
            "engineVersion": ENGINE, "economicPolicyHash": self.config.economic_hash,
            "configRelease": self.config.release, "buyAmount": plan.allocation.target_output,
            "grossBuyAmount": plan.allocation.gross_output(), "minBuyAmount": plan.min_buy_amount,
            "outputSurplus": Amount(&plan.allocation.gross_output().0 - &plan.allocation.target_output.0),
            "requiredSellAmount": plan.allocation.required_input, "consumedIn": plan.allocation.required_input,
            "inputBudget": plan.allocation.input_budget, "maxSellAmount": plan.max_sell_amount,
            "unusedInputBudget": plan.allocation.unused_input_budget, "refund": "0",
            "netSellAmountEstimate": Value::Null, "costs": {"unavailableReason": "CHAIN_FEE_MODEL_NOT_IMPLEMENTED"},
            "routes": plan.allocation.routes, "planHash": plan.plan_hash,
            "verification": {"level": "INDICATIVE", "success": false, "reason": "INDEPENDENT_SIMULATION_UNAVAILABLE"},
            "validity": {"expiresAtMs": expires, "canonicalEpoch": view.reference.canonical_epoch},
            "quality": quality
        });
        {
            let mut cache = self.exact_out_cache.lock().map_err(|_| err("CACHE_LOCK"))?;
            let capacity = (self.config.config.api.requests_per_minute as usize).clamp(16, 10000);
            if cache.len() >= capacity {
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }
            cache.insert(
                quote_id.clone(),
                ExactOutCached {
                    view: view.clone(),
                    plan,
                    response: response.clone(),
                    expires,
                    request_hash,
                },
            );
        }
        self.logger.event("info", "exact_out_quote_completed", json!({"quote_id": quote_id,
            "chain_key": request.chain_key, "quote_view_id": view.view_id, "verification": "INDICATIVE"}));
        self.observe_successful_quote(&view);
        Ok(response)
    }
    pub(super) fn exact_out_execution(&self, id: &str) -> Result<bool> {
        let cached = self
            .exact_out_cache
            .lock()
            .map_err(|_| err("CACHE_LOCK"))?
            .get(id)
            .cloned();
        if let Some(cached) = cached {
            if cached.expires <= now_ms() {
                return Err(err("QUOTE_EXPIRED"));
            }
            self.check_view(&cached.view)?;
            plan::verify(&cached.plan)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn exact_out_evidence(&self, id: &str) -> Result<Value> {
        let cache = self.exact_out_cache.lock().map_err(|_| err("CACHE_LOCK"))?;
        let c = cache.get(id).ok_or_else(|| err("UNKNOWN_QUOTE"))?;
        Ok(
            json!({"quote": c.response, "execution_plan": c.plan, "expired": c.expires <= now_ms(),
            "snapshot_valid": c.view.is_valid(), "simulation_evidence": Value::Null}),
        )
    }
}

pub(super) async fn quote(
    State(api): State<Api>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Json<Value>, HttpError> {
    api.authorize(&headers).map_err(failure)?;
    api.rate_limit().map_err(failure)?;
    api.metrics.requests.fetch_add(1, Ordering::Relaxed);
    let request: ExactOutRequest = serde_json::from_slice(&body)
        .map_err(|_| failure(err("INVALID_REQUEST_AMOUNT_MUST_BE_STRING")))?;
    match api.quote_exact_out(request).await {
        Ok(value) => {
            api.metrics.succeeded.fetch_add(1, Ordering::Relaxed);
            Ok(Json(value))
        }
        Err(error) => {
            api.metrics.failed.fetch_add(1, Ordering::Relaxed);
            Err(failure(error))
        }
    }
}
