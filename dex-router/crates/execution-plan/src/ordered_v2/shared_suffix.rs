use super::{
    addr_word, append_legs, append_path, small, transaction, Leg, SplitExecutor, SplitPlan,
};
use crate::{minimum_output, Plan, Transaction};
use serde::{Deserialize, Serialize};
use xone_domain::{err, hash, Amount, Pool, Result};
use xone_optimizer::shared_v2::{quote_suffix, CANDIDATE_VERSION};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundSharedSuffix {
    pub basis_plan: Plan,
    pub executor: SplitExecutor,
    pub transaction: Transaction,
    pub step_pools: Vec<Vec<String>>,
    pub join_token: String,
    pub joined_amount: Amount,
    pub buy_amount: Amount,
    pub min_buy_amount: Amount,
    pub candidate_hash: String,
    pub pool_state_hash: String,
    pub binding_hash: String,
}

pub fn bind(
    plan: &Plan,
    pools: &[Pool],
    executor: &SplitExecutor,
    allowed: &[String],
) -> Result<BoundSharedSuffix> {
    let basis = super::bind(&SplitPlan::ExactIn(plan.clone()), pools, executor, allowed)?;
    if plan.allocation.gross_buy_amount != plan.allocation.net_buy_amount {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let candidate = quote_suffix(pools, &plan.allocation)
        .ok_or_else(|| err("EXECUTION_SHARED_SUFFIX_UNAVAILABLE"))?;
    let candidate_hash = hash(&(CANDIDATE_VERSION, &plan.plan_hash, &candidate))?;
    let min_buy_amount = minimum_output(&candidate.buy_amount, plan.context.slippage_bps)?;
    let (suffix, prefixes) = candidate
        .steps
        .split_last()
        .ok_or_else(|| err("EXECUTION_PATH_MISMATCH"))?;
    let legs = prefixes
        .iter()
        .zip(&plan.allocation.routes)
        .map(|(step, route)| Leg {
            amount: &step.amount_in,
            tokens: &route.path.tokens[..=step.edges.len()],
            edges: &step.edges,
        })
        .collect::<Vec<_>>();
    let suffix_tokens = &plan.allocation.routes[0].path.tokens[legs[0].edges.len()..];
    let context = &plan.context;
    // executeExactIn(address,uint256,uint256,uint256,(uint256,address[])[],address[])
    let mut bytes = vec![0x5d, 0xcf, 0xea, 0x7f];
    bytes.extend(addr_word(&context.recipient)?);
    bytes.extend(candidate.sell_amount.to_be_bytes()?);
    bytes.extend(min_buy_amount.to_be_bytes()?);
    bytes.extend(Amount((context.deadline_ms / 1000).into()).to_be_bytes()?);
    let mut encoded_legs = Vec::new();
    append_legs(&mut encoded_legs, &legs)?;
    bytes.extend(small(6 * 32));
    bytes.extend(small(6 * 32 + encoded_legs.len()));
    bytes.extend(encoded_legs);
    append_path(&mut bytes, suffix_tokens)?;
    let transaction = transaction(context, &executor.target, &bytes);
    let step_pools = candidate
        .steps
        .iter()
        .map(|step| step.edges.iter().map(|edge| edge.pool_id.clone()).collect())
        .collect::<Vec<Vec<_>>>();
    let binding_hash = hash(&(
        "xone-bound-shared-v2-suffix/1",
        &candidate_hash,
        executor,
        &basis.pool_state_hash,
        &transaction,
        &step_pools,
    ))?;
    Ok(BoundSharedSuffix {
        basis_plan: plan.clone(),
        executor: executor.clone(),
        transaction,
        step_pools,
        join_token: suffix_tokens[0].clone(),
        joined_amount: suffix.amount_in.clone(),
        buy_amount: candidate.buy_amount,
        min_buy_amount,
        candidate_hash,
        pool_state_hash: basis.pool_state_hash,
        binding_hash,
    })
}

pub fn validate(
    bound: &BoundSharedSuffix,
    pools: &[Pool],
    executor: &SplitExecutor,
    allowed: &[String],
) -> Result<()> {
    let expected = bind(&bound.basis_plan, pools, executor, allowed)?;
    if hash(&expected)? != hash(bound)? {
        return Err(err("EXECUTION_BINDING_MISMATCH"));
    }
    Ok(())
}
