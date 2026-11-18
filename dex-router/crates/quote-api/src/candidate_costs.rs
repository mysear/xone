//! Optional local execution-cost ranking. Every cost is bound to its own plan;
//! local state and configured oracle authorization confer no production trust.
use crate::{
    exact_out::ExactOutCached,
    local_simulation::{LocalSimulation, Selection},
    Cached,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use xone_config::ConfigSnapshot;
use xone_domain::{err, Amount, Result};
use xone_execution_plan::{exact_out::ExactOutPlan, Context, Plan};
use xone_optimizer::cost_scoring::CostRanking;
use xone_state_replica::QuoteView;

pub(crate) struct Model {
    config: Arc<ConfigSnapshot>,
    backend: Option<Arc<LocalSimulation>>,
    view: Arc<QuoteView>,
    runtime: tokio::runtime::Handle,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    attempts: usize,
    unavailable: BTreeMap<String, usize>,
    evidence: BTreeMap<String, Value>,
}
impl Model {
    pub(crate) fn new(
        api: &crate::Api,
        backend: Option<Arc<LocalSimulation>>,
        view: Arc<QuoteView>,
        cancelled: Arc<AtomicBool>,
        deadline: Instant,
    ) -> Self {
        Self {
            config: api.config.clone(),
            backend,
            view,
            runtime: tokio::runtime::Handle::current(),
            cancelled,
            deadline,
            attempts: 0,
            unavailable: BTreeMap::new(),
            evidence: BTreeMap::new(),
        }
    }
    fn missing(&mut self, reason: &str) -> Result<Option<Amount>> {
        *self.unavailable.entry(reason.into()).or_default() += 1;
        Ok(None)
    }
    pub(crate) fn exact_in(&mut self, plan: Plan) -> Result<Option<Amount>> {
        let expected_in = plan.allocation.sell_amount.clone();
        let expected_out = plan.allocation.gross_buy_amount.clone();
        let selection = Selection::ExactIn(Cached {
            view: self.view.clone(),
            expires: plan.context.deadline_ms,
            plan,
            response: Value::Null,
            request_hash: String::new(),
            simulation_evidence: None,
            evm_preparation: None,
        });
        self.evaluate(selection, &expected_in, &expected_out, false)
    }
    pub(crate) fn exact_out(&mut self, plan: ExactOutPlan) -> Result<Option<Amount>> {
        let expected_in = plan.allocation.required_input.clone();
        let expected_out = plan.allocation.gross_output().clone();
        let selection = Selection::ExactOut(ExactOutCached {
            view: self.view.clone(),
            expires: plan.context.deadline_ms,
            plan,
            response: Value::Null,
            request_hash: String::new(),
            simulation_evidence: None,
            evm_preparation: None,
        });
        self.evaluate(selection, &expected_in, &expected_out, true)
    }
    fn evaluate(
        &mut self,
        selection: Selection,
        input: &Amount,
        output: &Amount,
        exact_out: bool,
    ) -> Result<Option<Amount>> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(err("CANCELLED"));
        }
        if Instant::now() >= self.deadline {
            return Err(err("BUDGET_EXHAUSTED"));
        }
        if !self.view.is_valid() {
            return Err(err("STALE_VIEW"));
        }
        let Some(backend) = self.backend.clone() else {
            return self.missing("LOCAL_BACKEND_UNAVAILABLE");
        };
        if backend.has_op_source() {
            return self.missing("OP_SIGNED_ENVELOPE_REQUIRED");
        }
        let genesis = &self
            .config
            .config
            .chains
            .iter()
            .find(|c| c.key == selection.context().chain_key)
            .ok_or_else(|| err("CHAIN_NOT_CONFIGURED"))?
            .genesis_hash;
        if !backend.matches_snapshot(&self.view.reference, genesis) {
            return self.missing("LOCAL_SOURCE_MISMATCH");
        }
        let binding = match backend.candidate_binding(&self.config, &selection) {
            Ok(binding) => binding,
            Err(e) if e.0 == "EXECUTION_ADAPTER_UNSUPPORTED" => {
                return self.missing("EXECUTION_ADAPTER_UNSUPPORTED")
            }
            Err(e) => return Err(e),
        };
        if self.attempts >= self.config.config.simulation.candidate_cost_max_plans {
            return self.missing("MAX_SIMULATION_PLANS");
        }
        self.attempts += 1;
        let plan_hash = match &selection {
            Selection::ExactIn(c) => c.plan.plan_hash.clone(),
            Selection::ExactOut(c) => c.plan.plan_hash.clone(),
        };
        let context = selection.context().clone();
        let execution_deadline = self.deadline.min(
            Instant::now()
                + Duration::from_millis(self.config.config.simulation.timeout_ms.min(60_000)),
        );
        let config = self.config.clone();
        let cancelled = self.cancelled.clone();
        let response = self.runtime.block_on(await_execution(
            async move {
                backend
                    .execute(config, selection, execution_deadline)
                    .await
                    .map_err(|e| err(&e.code))
            },
            cancelled,
        ))?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(err("CANCELLED"));
        }
        if !self.view.is_valid() {
            return Err(err("STALE_VIEW"));
        }
        let cost = audited_cost(&response, &binding, &context, input, output, exact_out)?;
        if cost.is_none() {
            *self
                .unavailable
                .entry("EXECUTION_OR_AUTHORIZED_VALUATION_UNAVAILABLE".into())
                .or_default() += 1;
        }
        self.evidence.insert(plan_hash, response);
        Ok(cost)
    }
    pub(crate) fn finish(self, ranking: &CostRanking, selected_hash: &str) -> Value {
        let mut value = json!({"version":"xone-local-candidate-execution-costs/1","scope":"LOCAL_EVM_TRANSACTION_EXECUTION_FEES_ONLY",
            "costAdjustmentApplied":ranking.cost_adjustment_applied,
            "objective":ranking.objective,"optimizer":ranking,
            "authority":"PROVIDED_LOCAL_STATE_AND_CONFIGURED_ORACLE_QUORUM",
            "chainFeeCompleteness":if self.evidence.is_empty() {"UNAVAILABLE"} else {"LOCAL_EVM_ONLY"},"completeQuoteCostRanking":false,
            "additionalOperationsIncluded":false,"productionVerified":false,
            "maxSimulationPlans":self.config.config.simulation.candidate_cost_max_plans,
            "simulationAttempts":self.attempts,"simulationBudgetExhausted":self.unavailable.contains_key("MAX_SIMULATION_PLANS"),"unavailableReasons":self.unavailable,
            "selectedPlanHash":selected_hash,"selectedExecution":self.evidence.get(selected_hash),
            "candidateExecutions":self.evidence});
        if let Some((book, revision)) = self.backend.as_ref().and_then(|b| b.price_identity()) {
            value["priceBookHash"] = book.into();
            value["publicationRevision"] = revision.into();
        }
        value
    }
}
async fn await_execution(
    execution: impl std::future::Future<Output = Result<Value>>,
    cancelled: Arc<AtomicBool>,
) -> Result<Value> {
    tokio::pin!(execution);
    loop {
        tokio::select! {
            result = &mut execution => return result,
            _ = tokio::time::sleep(Duration::from_millis(10)) => {
                if cancelled.load(Ordering::Acquire) { return Err(err("CANCELLED")); }
            }
        }
    }
}

/// Publish only the selected, successfully executed plan's partial costs. The
/// map identity and parent-audited amounts are checked again at publication.
pub(crate) fn disclose(
    quote: &mut Value,
    ranking: &Value,
    context: &Context,
    plan_hash: &str,
    input: &Amount,
    output: &Amount,
) -> Result<()> {
    let bad = || err("CANDIDATE_COST_DISCLOSURE_BINDING");
    if ranking["selectedPlanHash"] != plan_hash || quote["planHash"] != plan_hash {
        return Err(bad());
    }
    let execution = &ranking["selectedExecution"];
    if execution != &ranking["candidateExecutions"][plan_hash] {
        return Err(bad());
    }
    if execution.is_null() || execution["verification"]["success"] != true {
        if ranking["costAdjustmentApplied"] == true {
            return Err(bad());
        }
        return Ok(());
    }
    if execution["quoteKind"] != quote["kind"] {
        return Err(bad());
    }
    let binding = execution["bindingHash"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(bad)?;
    let exact_out = quote["kind"] == "EXACT_OUT";
    let cost = audited_cost(execution, binding, context, input, output, exact_out)?;
    if ranking["priceBookHash"].is_string()
        && ranking["priceBookHash"] != execution["costs"]["transactionEconomics"]["priceBookHash"]
    {
        return Err(bad());
    }
    if ranking["costAdjustmentApplied"] == true {
        let cost = cost.ok_or_else(bad)?.to_string();
        if ranking["optimizer"]["selected_cost_token_units"].as_str() != Some(cost.as_str()) {
            return Err(bad());
        }
    }
    let original = quote["costs"].clone();
    let mut costs = execution["costs"].clone();
    if !costs.is_object()
        || costs["totalNativeEstimate"] != Value::Null
        || costs["netBuyAmountEstimate"] != Value::Null
        || costs["netSellAmountEstimate"] != Value::Null
        || costs["deductedFromQuoteOutput"] != false
    {
        return Err(bad());
    }
    // Preserve the existing declaration that unvalued activation penalties do
    // not become transaction fees or silently alter the gross output.
    for field in [
        "proposalActivationCostBuyToken",
        "ignoredConfiguredActivationCostBuyToken",
        "configuredActivationCostApplied",
    ] {
        costs[field] = original[field].clone();
    }
    costs["scope"] = "SELECTED_LOCAL_CANDIDATE_TRANSACTION".into();
    costs["selectedPlanHash"] = plan_hash.into();
    costs["completeQuoteCosts"] = false.into();
    costs["quoteRankingApplied"] = ranking["costAdjustmentApplied"].clone();
    costs["tokenValuationStatus"] = if costs["transactionEconomics"]["buyTokenFee"].is_string()
        || costs["transactionEconomics"]["sellTokenFee"].is_string()
    {
        "CONFIGURED_ORACLE_AUTHORIZED_WITH_PER_SIDE_AVAILABILITY"
    } else {
        "UNAVAILABLE"
    }
    .into();
    costs["sourceEvidence"] = json!({"kind":"SAME_SELECTED_PLAN_LOCAL_EXECUTION",
        "planHash":plan_hash,"bindingHash":binding,"bundleHash":execution["bundleHash"],
        "workerHash":execution["workerHash"],"quoteViewId":quote["quoteViewId"],
        "snapshotRef":quote["snapshotRef"],"productionVerified":false});
    if ranking["priceBookHash"].is_string() {
        costs["sourceEvidence"]["priceBookHash"] = ranking["priceBookHash"].clone();
        costs["sourceEvidence"]["publicationRevision"] = ranking["publicationRevision"].clone();
    }
    quote["costs"] = costs;
    Ok(())
}

fn audited_cost(
    response: &Value,
    binding: &str,
    context: &Context,
    input: &Amount,
    output: &Amount,
    exact_out: bool,
) -> Result<Option<Amount>> {
    if response["bindingHash"] != binding
        || response["transaction"]["from"] != context.caller
        || response["transaction"]["chain_key"] != context.chain_key
    {
        return Err(err("CANDIDATE_COST_BINDING"));
    }
    if response["verification"]["success"] != true {
        return Ok(None);
    }
    if response["outcome"]["actual_input_consumed"] != input.to_string()
        || response["outcome"]["actual_buy_amount"] != output.to_string()
        || response["costs"]["balanceReconciliation"]["status"] != "MATCHED"
        || response["costs"]["bindingHash"] != binding
        || response["costs"]["chainFeeCompleteness"] != "LOCAL_EVM_ONLY"
    {
        return Err(err("CANDIDATE_COST_MEASUREMENT"));
    }
    let economics = &response["costs"]["transactionEconomics"];
    if economics.is_null() {
        return Ok(None);
    }
    if economics["bindingHash"] != binding
        || economics["recipient"] != context.recipient
        || economics["payerNetNativeDebit"] != response["costs"]["payerNetNativeDebit"]
        || economics["actualInput"] != input.to_string()
        || economics["actualOutput"] != output.to_string()
        || economics["costBasis"] != "SOURCE_AUDITED_PAYER_NET_NATIVE_DEBIT"
    {
        return Err(err("CANDIDATE_COST_VALUATION_BINDING"));
    }
    // Fee conversion can remain valid when the signed net objective is negative
    // or 257 bits wide. The optimizer compares it without clamping/overflow.
    economics[if exact_out {
        "sellTokenFee"
    } else {
        "buyTokenFee"
    }]
    .as_str()
    .map(str::parse)
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(fee: &str) -> (Context, Value) {
        let context = Context {
            chain_key: "evm:31337".into(),
            caller: "caller".into(),
            recipient: "recipient".into(),
            sell_token: "sell".into(),
            buy_token: "buy".into(),
            slippage_bps: 50,
            deadline_ms: 123,
        };
        let response = json!({"bindingHash":"binding","transaction":{"from":"caller","chain_key":"evm:31337"},
            "verification":{"success":true},"outcome":{"actual_input_consumed":"12","actual_buy_amount":"10"},
            "costs":{"bindingHash":"binding","chainFeeCompleteness":"LOCAL_EVM_ONLY","balanceReconciliation":{"status":"MATCHED"},"payerNetNativeDebit":"3",
                "transactionEconomics":{"bindingHash":"binding","recipient":"recipient","payerNetNativeDebit":"3","actualInput":"12","actualOutput":"10","costBasis":"SOURCE_AUDITED_PAYER_NET_NATIVE_DEBIT","buyTokenFee":fee,"sellTokenFee":fee}}});
        (context, response)
    }
    #[test]
    fn candidate_execution_costs_reject_cross_plan_amount_recipient_and_fee_audits() {
        let (context, response) = record("4");
        let input = "12".parse().unwrap();
        let output = "10".parse().unwrap();
        for exact in [false, true] {
            assert_eq!(
                audited_cost(&response, "binding", &context, &input, &output, exact).unwrap(),
                Some("4".parse().unwrap())
            );
            for pointer in [
                "/bindingHash",
                "/transaction/from",
                "/transaction/chain_key",
                "/outcome/actual_input_consumed",
                "/outcome/actual_buy_amount",
                "/costs/bindingHash",
                "/costs/chainFeeCompleteness",
                "/costs/balanceReconciliation/status",
                "/costs/transactionEconomics/bindingHash",
                "/costs/transactionEconomics/recipient",
                "/costs/transactionEconomics/payerNetNativeDebit",
                "/costs/transactionEconomics/actualInput",
                "/costs/transactionEconomics/actualOutput",
                "/costs/transactionEconomics/costBasis",
            ] {
                let mut bad = response.clone();
                *bad.pointer_mut(pointer).unwrap() = "changed".into();
                assert!(
                    audited_cost(&bad, "binding", &context, &input, &output, exact).is_err(),
                    "{pointer}"
                );
            }
        }
    }
    #[test]
    fn candidate_costs_preserve_negative_and_wide_objectives_and_unknown_execution() {
        let (context, mut response) = record("20");
        let input = "12".parse().unwrap();
        let output = "10".parse().unwrap();
        response["costs"]["transactionEconomics"]["feeAdjustedBuyAmount"] = Value::Null;
        response["costs"]["transactionEconomics"]["buyUnavailableReason"] =
            "NETWORK_FEE_EXCEEDS_GROSS_OUTPUT".into();
        assert_eq!(
            audited_cost(&response, "binding", &context, &input, &output, false).unwrap(),
            Some("20".parse().unwrap())
        );
        let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
        response["costs"]["transactionEconomics"]["sellTokenFee"] = max.into();
        assert_eq!(
            audited_cost(&response, "binding", &context, &input, &output, true).unwrap(),
            Some(max.parse().unwrap())
        );
        response["costs"]["transactionEconomics"]["buyTokenFee"] = Value::Null;
        assert!(
            audited_cost(&response, "binding", &context, &input, &output, false)
                .unwrap()
                .is_none()
        );
        response["verification"]["success"] = false.into();
        assert!(
            audited_cost(&response, "binding", &context, &input, &output, true)
                .unwrap()
                .is_none()
        );
    }
    fn disclosure_record(exact: bool, fee: &str) -> (Context, Value, Value) {
        let (context, mut execution) = record(fee);
        execution["quoteKind"] = if exact { "EXACT_OUT" } else { "EXACT_IN" }.into();
        execution["bundleHash"] = "bundle".into();
        execution["workerHash"] = "worker".into();
        let c = &mut execution["costs"];
        c["scope"] = "DECLARED_LOCAL_EVM_TRANSACTION".into();
        c["authority"] = "PROVIDED_LOCAL_STATE".into();
        c["gasUnits"] = "42".into();
        c["gasNative"] = "3".into();
        c["nativeKnownSubtotal"] = "3".into();
        c["totalNativeEstimate"] = Value::Null;
        c["netBuyAmountEstimate"] = Value::Null;
        c["netSellAmountEstimate"] = Value::Null;
        c["deductedFromQuoteOutput"] = false.into();
        c["feeBreakdown"] = json!([{"kind":"EVM_EXECUTION","amountNative":"3"}]);
        let ranking = json!({"selectedPlanHash":"plan","selectedExecution":execution,
            "candidateExecutions":{"plan":execution},"costAdjustmentApplied":true,
            "optimizer":{"selected_cost_token_units":fee}});
        let quote = json!({"kind":if exact {"EXACT_OUT"} else {"EXACT_IN"},"planHash":"plan",
            "quoteViewId":"view","snapshotRef":{"block":"source"},
            "costs":crate::cost_fallback::costs(&"7".parse().unwrap())});
        (context, ranking, quote)
    }
    #[test]
    fn selected_candidate_cost_disclosure_reuses_audited_fees_without_claiming_complete_net() {
        for exact in [false, true] {
            for fee in [
                "4",
                "20",
                "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            ] {
                let (context, ranking, mut quote) = disclosure_record(exact, fee);
                disclose(
                    &mut quote,
                    &ranking,
                    &context,
                    "plan",
                    &"12".parse().unwrap(),
                    &"10".parse().unwrap(),
                )
                .unwrap();
                let costs = &quote["costs"];
                for field in [
                    "gasUnits",
                    "gasNative",
                    "nativeKnownSubtotal",
                    "payerNetNativeDebit",
                    "transactionEconomics",
                    "feeBreakdown",
                    "bindingHash",
                ] {
                    assert_eq!(
                        costs[field], ranking["selectedExecution"]["costs"][field],
                        "{field}"
                    );
                }
                assert_eq!(costs["scope"], "SELECTED_LOCAL_CANDIDATE_TRANSACTION");
                assert_eq!(costs["sourceEvidence"]["planHash"], "plan");
                assert_eq!(costs["sourceEvidence"]["quoteViewId"], "view");
                assert_eq!(costs["sourceEvidence"]["snapshotRef"], quote["snapshotRef"]);
                assert_eq!(costs["completeQuoteCosts"], false);
                assert_eq!(costs["sourceEvidence"]["productionVerified"], false);
                assert_eq!(costs["ignoredConfiguredActivationCostBuyToken"], "7");
                assert_eq!(costs["proposalActivationCostBuyToken"], "0");
                assert_eq!(costs["configuredActivationCostApplied"], false);
                assert!(costs["totalNativeEstimate"].is_null());
                assert!(costs["netBuyAmountEstimate"].is_null());
                assert!(costs["netSellAmountEstimate"].is_null());
                assert_eq!(costs["deductedFromQuoteOutput"], false);
            }
        }
    }
    #[test]
    fn selected_candidate_disclosure_does_not_borrow_other_plans_or_price_missing_fees() {
        for exact in [false, true] {
            let (context, mut ranking, mut quote) = disclosure_record(exact, "4");
            let original = quote.clone();
            ranking["candidateExecutions"]["other"] = ranking["selectedExecution"].clone();
            ranking["candidateExecutions"]
                .as_object_mut()
                .unwrap()
                .remove("plan");
            ranking["selectedExecution"] = Value::Null;
            ranking["costAdjustmentApplied"] = false.into();
            disclose(
                &mut quote,
                &ranking,
                &context,
                "plan",
                &"12".parse().unwrap(),
                &"10".parse().unwrap(),
            )
            .unwrap();
            assert_eq!(quote, original);
            let (context, mut ranking, mut quote) = disclosure_record(exact, "4");
            ranking["costAdjustmentApplied"] = false.into();
            ranking["selectedExecution"]["verification"]["success"] = false.into();
            ranking["candidateExecutions"]["plan"] = ranking["selectedExecution"].clone();
            disclose(
                &mut quote,
                &ranking,
                &context,
                "plan",
                &"12".parse().unwrap(),
                &"10".parse().unwrap(),
            )
            .unwrap();
            assert_eq!(quote, original);
            let (context, mut ranking, mut quote) = disclosure_record(exact, "4");
            ranking["costAdjustmentApplied"] = false.into();
            ranking["selectedExecution"]["costs"]["transactionEconomics"]["buyTokenFee"] =
                Value::Null;
            ranking["selectedExecution"]["costs"]["transactionEconomics"]["sellTokenFee"] =
                Value::Null;
            ranking["candidateExecutions"]["plan"] = ranking["selectedExecution"].clone();
            disclose(
                &mut quote,
                &ranking,
                &context,
                "plan",
                &"12".parse().unwrap(),
                &"10".parse().unwrap(),
            )
            .unwrap();
            assert_eq!(quote["costs"]["gasNative"], "3");
            assert_eq!(quote["costs"]["tokenValuationStatus"], "UNAVAILABLE");
            assert_eq!(quote["costs"]["quoteRankingApplied"], false);
        }
    }
    #[test]
    fn selected_candidate_disclosure_rejects_cross_plan_bindings_amounts_and_scores() {
        for mode in [
            "selected-plan",
            "quote-plan",
            "map",
            "binding",
            "caller",
            "recipient",
            "input",
            "output",
            "kind",
            "score",
            "unknown-score",
            "unknown-fee",
            "total",
            "net",
            "deducted",
            "price-book",
        ] {
            let (context, mut ranking, mut quote) = disclosure_record(false, "4");
            match mode {
                "selected-plan" => ranking["selectedPlanHash"] = "other".into(),
                "quote-plan" => quote["planHash"] = "other".into(),
                "map" => ranking["candidateExecutions"]["plan"] = Value::Null,
                "binding" => ranking["selectedExecution"]["costs"]["bindingHash"] = "other".into(),
                "caller" => ranking["selectedExecution"]["transaction"]["from"] = "other".into(),
                "recipient" => {
                    ranking["selectedExecution"]["costs"]["transactionEconomics"]["recipient"] =
                        "other".into()
                }
                "input" => {
                    ranking["selectedExecution"]["outcome"]["actual_input_consumed"] = "13".into()
                }
                "output" => {
                    ranking["selectedExecution"]["outcome"]["actual_buy_amount"] = "11".into()
                }
                "kind" => ranking["selectedExecution"]["quoteKind"] = "EXACT_OUT".into(),
                "score" => ranking["optimizer"]["selected_cost_token_units"] = "5".into(),
                "unknown-score" => ranking["optimizer"]["selected_cost_token_units"] = Value::Null,
                "unknown-fee" => {
                    ranking["selectedExecution"]["costs"]["transactionEconomics"]["buyTokenFee"] =
                        Value::Null
                }
                "total" => {
                    ranking["selectedExecution"]["costs"]["totalNativeEstimate"] = "3".into()
                }
                "net" => ranking["selectedExecution"]["costs"]["netBuyAmountEstimate"] = "6".into(),
                "price-book" => ranking["priceBookHash"] = "other-book".into(),
                _ => ranking["selectedExecution"]["costs"]["deductedFromQuoteOutput"] = true.into(),
            }
            if mode != "map" {
                ranking["candidateExecutions"]["plan"] = ranking["selectedExecution"].clone();
            }
            assert!(
                disclose(
                    &mut quote,
                    &ranking,
                    &context,
                    "plan",
                    &"12".parse().unwrap(),
                    &"10".parse().unwrap()
                )
                .is_err(),
                "{mode}"
            );
        }
    }
    #[tokio::test]
    async fn candidate_cost_bridge_cancels_owned_execution_and_preserves_errors() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = crate::CancelOnDrop(dropped.clone());
        let execution = async move {
            let _guard = guard;
            std::future::pending::<Result<Value>>().await
        };
        let signal = cancelled.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            signal.store(true, Ordering::Release);
        });
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            await_execution(execution, cancelled),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.0, "CANCELLED");
        assert!(dropped.load(Ordering::Acquire));
        let control = Arc::new(AtomicBool::new(false));
        for code in [
            "SIMULATION_WORKER_PROTOCOL",
            "SIMULATION_BUSY",
            "SIMULATION_TIMEOUT",
        ] {
            let error = await_execution(async { Err(err(code)) }, control.clone())
                .await
                .unwrap_err();
            assert_eq!(error.0, code);
            assert!(!control.load(Ordering::Acquire));
        }
        assert_eq!(
            await_execution(async { Ok(json!(1)) }, control.clone())
                .await
                .unwrap(),
            json!(1)
        );
        assert!(!control.load(Ordering::Acquire));
    }
}
