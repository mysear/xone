//! Independent exact-output commitment. The original Router pulls actual input;
//! its unused allowance/budget is not an on-chain refund.
use crate::Context;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use xone_domain::{err, hash, v2_swap, Amount, Result, SnapshotRef};
use xone_optimizer::exact_out::ExactOutAllocation;

pub const ENGINE: &str = "xone-router-exact-out/0.1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactOutPlan {
    pub engine: String,
    pub context: Context,
    pub snapshot: SnapshotRef,
    pub allocation: ExactOutAllocation,
    pub min_buy_amount: Amount,
    pub max_sell_amount: Amount,
    pub economic_policy_hash: String,
    pub plan_hash: String,
}

pub fn build(
    context: &Context,
    allocation: &ExactOutAllocation,
    snapshot: &SnapshotRef,
    policy: &str,
) -> Result<ExactOutPlan> {
    if context.chain_key != snapshot.block.chain_key
        || !context.chain_key.starts_with("evm:")
        || context.slippage_bps > 10000
        || context.sell_token == context.buy_token
        || allocation.routes.is_empty()
        || allocation.target_output == Amount::zero()
        || allocation.required_input == Amount::zero()
        || allocation.required_input.0 > allocation.input_budget.0
        || allocation.unused_input_budget.0
            != &allocation.input_budget.0 - &allocation.required_input.0
    {
        return Err(err("EXACT_OUT_INVALID_PLAN"));
    }
    Amount::checked(allocation.gross_output().0.clone())?;
    for amount in [
        &allocation.required_input,
        &allocation.target_output,
        &allocation.input_budget,
        &allocation.unused_input_budget,
    ] {
        Amount::checked(amount.0.clone())?;
    }
    for address in [
        &context.caller,
        &context.recipient,
        &context.sell_token,
        &context.buy_token,
    ] {
        xone_domain::protocols::validate_address(address)?;
        if address == xone_domain::singleton::ZERO_ADDRESS {
            return Err(err("EXACT_OUT_INVALID_PLAN"));
        }
    }
    let mut input = BigUint::from(0u8);
    let mut output = BigUint::from(0u8);
    let mut targeted = BigUint::from(0u8);
    for route in &allocation.routes {
        if route.edges.is_empty()
            || route.tokens.len() != route.edges.len() + 1
            || route.amounts.len() != route.tokens.len()
            || route.tokens.first() != Some(&context.sell_token)
            || route.tokens.last() != Some(&context.buy_token)
            || route.amounts.first() != Some(&route.amount_in)
            || route.amounts.last() != Some(&route.amount_out)
            || route.amounts.iter().any(|a| *a == Amount::zero())
            || route.tokens.iter().collect::<BTreeSet<_>>().len() != route.tokens.len()
            || route
                .edges
                .iter()
                .map(|e| &e.pool_id)
                .collect::<BTreeSet<_>>()
                .len()
                != route.edges.len()
            || route
                .edges
                .iter()
                .zip(&route.tokens[1..])
                .any(|(edge, to)| {
                    !matches!(
                        edge.protocol.as_str(),
                        v2_swap::PROFILE
                            | xone_domain::pancake_bsc_lm::PROFILE
                            | xone_domain::pancake_lm::PROFILE
                            | xone_domain::slipstream_context::PROFILE
                            | xone_domain::lfj::PROFILE
                    ) || (matches!(
                        edge.protocol.as_str(),
                        xone_domain::pancake_bsc_lm::PROFILE
                            | xone_domain::pancake_lm::PROFILE
                            | xone_domain::slipstream_context::PROFILE
                            | xone_domain::lfj::PROFILE
                    ) && route.edges.len() != 1)
                        || edge.from_token.is_some()
                        || &edge.to != to
                })
        {
            return Err(err("EXACT_OUT_INVALID_PLAN"));
        }
        if let Some(target) = &route.target_output {
            if route.edges.len() != 1
                || route.edges[0].protocol != xone_domain::lfj::PROFILE
                || *target == Amount::zero()
                || target.0 >= route.amount_out.0
            {
                return Err(err("EXACT_OUT_INVALID_PLAN"));
            }
            Amount::checked(target.0.clone())?;
        }
        targeted += &route.output_target().0;
        for amount in &route.amounts {
            Amount::checked(amount.0.clone())?;
        }
        for token in &route.tokens {
            xone_domain::protocols::validate_address(token)?;
        }
        xone_domain::protocols::validate_address(&route.factory)?;
        input += &route.amount_in.0;
        output += &route.amount_out.0;
    }
    let actual = allocation.gross_output();
    if allocation
        .gross_buy_amount
        .as_ref()
        .is_some_and(|gross| gross.0 <= allocation.target_output.0)
        || input != allocation.required_input.0
        || targeted != allocation.target_output.0
        || output != actual.0
        || output < targeted
    {
        return Err(err("EXACT_OUT_INVALID_PLAN"));
    }
    // Round upward, then cap by the user's hard budget. Wide intermediate math
    // permits an input close to uint256::MAX without overflowing before the cap.
    let slippage_cap = (&input * BigUint::from(10000u32 + u32::from(context.slippage_bps))
        + BigUint::from(9999u32))
        / BigUint::from(10000u32);
    let maximum = Amount::checked(slippage_cap.min(allocation.input_budget.0.clone()))?;
    let minimum = allocation.target_output.clone();
    let commitment = hash(&serde_json::json!({"engine": ENGINE, "context": context,
        "snapshot": snapshot, "allocation": allocation, "min_buy_amount": minimum,
        "max_sell_amount": maximum, "economic_policy_hash": policy}))?;
    Ok(ExactOutPlan {
        engine: ENGINE.into(),
        context: context.clone(),
        snapshot: snapshot.clone(),
        allocation: allocation.clone(),
        min_buy_amount: minimum,
        max_sell_amount: maximum,
        economic_policy_hash: policy.into(),
        plan_hash: commitment,
    })
}

pub fn verify(plan: &ExactOutPlan) -> Result<()> {
    let expected = build(
        &plan.context,
        &plan.allocation,
        &plan.snapshot,
        &plan.economic_policy_hash,
    )?;
    if plan.engine != ENGINE
        || plan.min_buy_amount != expected.min_buy_amount
        || plan.max_sell_amount != expected.max_sell_amount
        || plan.plan_hash != expected.plan_hash
    {
        return Err(err("PLAN_HASH_MISMATCH"));
    }
    Ok(())
}
