//! Design §13.1: absent reliable costs/valuation, rank raw amounts within the
//! declared search constraints. An arbitrary token amount is not a chain fee.
use serde_json::{json, Value};
use xone_domain::Amount;

pub(super) fn costs(configured_activation_cost: &Amount) -> Value {
    json!({
        "unavailableReason":"CHAIN_FEE_MODEL_NOT_IMPLEMENTED",
        "gasUnits":Value::Null,"gasNative":Value::Null,
        "l1DataNative":Value::Null,"approvalNative":Value::Null,
        "totalNativeEstimate":Value::Null,"feeBreakdown":[],
        "proposalActivationCostBuyToken":"0",
        "ignoredConfiguredActivationCostBuyToken":configured_activation_cost,
        "configuredActivationCostApplied":false,
        "deductedFromQuoteOutput":false,
        "chainFeeCompleteness":"UNAVAILABLE",
        "tokenValuationStatus":"UNAVAILABLE"
    })
}

pub(super) fn ranking(exact_out: bool) -> Value {
    json!({
        "version":"xone-cost-unavailable-ranking/1",
        "objective":if exact_out {"MINIMUM_REQUIRED_SELL_AMOUNT"} else {"MAXIMUM_GROSS_BUY_AMOUNT"},
        "scope":"EVALUATED_CANDIDATES_AND_ORDERED_ALLOCATION_GRID",
        "costAdjustmentApplied":false,
        "chainFeeCompleteness":"UNAVAILABLE",
        "tokenValuationStatus":"UNAVAILABLE"
    })
}
