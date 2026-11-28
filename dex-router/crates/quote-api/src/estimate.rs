use super::{exact_out::ExactOutRequest, *};
use xone_optimizer::{OptimizationExecution, OptimizationLimits};
use xone_routing::{Budget, SearchControl, SearchExecution};

pub(super) fn validate_addresses(
    caller: &str,
    recipient: &str,
    verification: &str,
) -> Result<bool> {
    let funded = !caller.is_empty();
    if funded == recipient.is_empty() || caller.len() > 128 || recipient.len() > 128 {
        return Err(err("INVALID_REQUEST"));
    }
    if !funded && verification == "VERIFIED" {
        return Err(err("QUOTE_ADDRESSES_REQUIRED"));
    }
    Ok(funded)
}

enum Amounts {
    ExactIn { sell: Amount },
    ExactOut { buy: Amount, max_sell: Amount },
}

pub(super) struct Estimate {
    chain: String,
    sell_token: String,
    buy_token: String,
    constraints: Option<Constraints>,
    amounts: Amounts,
}

impl From<QuoteRequest> for Estimate {
    fn from(request: QuoteRequest) -> Self {
        Self {
            chain: request.chain_key,
            sell_token: request.sell_token,
            buy_token: request.buy_token,
            constraints: request.constraints,
            amounts: Amounts::ExactIn {
                sell: request.sell_amount,
            },
        }
    }
}

impl From<ExactOutRequest> for Estimate {
    fn from(request: ExactOutRequest) -> Self {
        Self {
            chain: request.chain_key,
            sell_token: request.sell_token,
            buy_token: request.buy_token,
            constraints: request.constraints,
            amounts: Amounts::ExactOut {
                buy: request.buy_amount,
                max_sell: request.max_sell_amount,
            },
        }
    }
}

impl Api {
    pub(super) async fn estimate(&self, request: Estimate) -> Result<Value> {
        let view = self.replica.view(&request.chain)?;
        self.check_view(&view)?;
        let routing = &self.config.config.routing;
        let max_hops = request
            .constraints
            .as_ref()
            .map_or(routing.max_hops, |c| c.max_hops);
        if max_hops == 0 || max_hops > routing.max_hops {
            return Err(err("INVALID_HOP_LIMIT"));
        }
        let budget = Budget {
            split_parts: if routing.max_splits > 1 {
                routing.split_parts
            } else {
                1
            },
            max_hops,
            max_candidates: routing.max_candidates,
            max_expansions: routing.max_expansions,
        };
        let limits = OptimizationLimits {
            parts: routing.split_parts,
            max_splits: routing.max_splits,
            max_evaluations: routing.max_expansions,
        };
        let deadline =
            Instant::now() + Duration::from_millis(self.config.config.api.quote_timeout_ms);
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| err("RATE_LIMITED"))?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel = CancelOnDrop(cancelled.clone());
        let worker_view = view.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            calculate(request, &worker_view, budget, limits, &cancelled, deadline)
        });
        let response =
            tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), task)
                .await
                .map_err(|_| err("BUDGET_EXHAUSTED"))?
                .map_err(|_| err("COMPUTE_TASK_FAILED"))??;
        self.check_view(&view)?;
        Ok(response)
    }
}

fn calculate(
    request: Estimate,
    view: &QuoteView,
    budget: Budget,
    limits: OptimizationLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<Value> {
    let search = SearchExecution {
        control: Some(SearchControl {
            cancel: cancelled,
            deadline,
        }),
        funds: None,
    };
    let optimize = OptimizationExecution {
        budget: Some((cancelled, deadline)),
        funds: None,
    };
    let mut response = match request.amounts {
        Amounts::ExactIn { sell } => {
            let candidates = xone_routing::search_bounded_with_funds(
                &view.graph,
                &view.pools,
                &request.sell_token,
                &request.buy_token,
                &sell,
                budget,
                search,
            )?;
            let result = xone_optimizer::optimize_with_limits(
                &view.pools,
                &candidates.candidates,
                &sell,
                &Amount::zero(),
                limits,
                optimize,
            )?;
            let allocation = result.allocation;
            let bindings = view.protocol_bindings(
                allocation
                    .routes
                    .iter()
                    .flat_map(|route| route.path.edges.iter().map(|edge| edge.pool_id.as_str())),
            )?;
            json!({
                "kind": "EXACT_IN", "protocolViewIds": bindings, "sellAmount": sell,
                "consumedIn": allocation.sell_amount, "grossBuyAmount": allocation.gross_buy_amount,
                "refund": allocation.refund, "routes": allocation.routes,
                "quality": { "budgetExhausted": candidates.budget_exhausted || result.budget_exhausted }
            })
        }
        Amounts::ExactOut { buy, max_sell } => {
            let candidates = xone_routing::exact_out::search(
                &view.graph,
                &view.pools,
                &request.sell_token,
                &request.buy_token,
                &buy,
                budget,
                search,
            )?;
            let result = xone_optimizer::exact_out::optimize_with_limits(
                &view.pools,
                &candidates.candidates,
                &buy,
                &max_sell,
                limits,
                optimize,
            )?;
            let allocation = result.allocation;
            let bindings = view.protocol_bindings(
                allocation
                    .routes
                    .iter()
                    .flat_map(|route| route.edges.iter().map(|edge| edge.pool_id.as_str())),
            )?;
            json!({
                "kind": "EXACT_OUT", "protocolViewIds": bindings, "buyAmount": buy,
                "requiredSellAmount": allocation.required_input, "grossBuyAmount": allocation.gross_output(),
                "minBuyAmount": allocation.target_output,
                "outputSurplus": Amount(&allocation.gross_output().0 - &allocation.target_output.0),
                "unusedInputBudget": allocation.unused_input_budget, "routes": allocation.routes,
                "quality": { "budgetExhausted": candidates.budget_exhausted || result.budget_exhausted }
            })
        }
    };
    response["chainKey"] = json!(request.chain);
    response["snapshotRef"] = json!(view.reference);
    response["quoteViewId"] = json!(view.view_id);
    response["graphVersion"] = json!(view.graph_version);
    response["verification"] = json!({
        "level": "INDICATIVE", "success": false, "reason": "AMOUNT_ESTIMATE"
    });
    response["estimateOnly"] = json!(true);
    Ok(response)
}
