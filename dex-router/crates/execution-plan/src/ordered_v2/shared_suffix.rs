use super::{
    addr_word, append_legs, append_path, small, transaction, Leg, SplitExecutor, SplitPlan,
};
use crate::{exact_out::maximum_input, minimum_output, Context, Transaction};
use serde::{Deserialize, Serialize};
use xone_domain::{err, hash, Amount, Pool, Result, SnapshotRef};
use xone_optimizer::shared_v2::{quote_suffix, quote_suffix_exact_out, CANDIDATE_VERSION};
use xone_routing::Funds;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundSharedSuffix {
    pub basis_plan: SplitPlan,
    pub executor: SplitExecutor,
    pub transaction: Transaction,
    pub step_pools: Vec<Vec<String>>,
    pub join_token: String,
    pub joined_amount: Amount,
    pub buy_amount: Amount,
    pub sell_amount: Amount,
    pub max_sell_amount: Amount,
    pub min_buy_amount: Amount,
    pub candidate_hash: String,
    pub pool_state_hash: String,
    pub binding_hash: String,
}

impl BoundSharedSuffix {
    pub fn context(&self) -> &Context {
        match &self.basis_plan {
            SplitPlan::ExactIn(plan) => &plan.context,
            SplitPlan::ExactOut(plan) => &plan.context,
        }
    }

    pub fn snapshot(&self) -> &SnapshotRef {
        match &self.basis_plan {
            SplitPlan::ExactIn(plan) => &plan.snapshot,
            SplitPlan::ExactOut(plan) => &plan.snapshot,
        }
    }
}

pub fn bind(
    plan: &SplitPlan,
    pools: &[Pool],
    executor: &SplitExecutor,
    allowed: &[String],
) -> Result<BoundSharedSuffix> {
    let basis = super::bind(plan, pools, executor, allowed)?;
    let (candidate, context, plan_hash, paths, budget) = match plan {
        SplitPlan::ExactIn(plan) => {
            if plan.allocation.gross_buy_amount != plan.allocation.net_buy_amount {
                return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
            }
            (
                quote_suffix(pools, &plan.allocation),
                &plan.context,
                &plan.plan_hash,
                plan.allocation
                    .routes
                    .iter()
                    .map(|route| route.path.tokens.as_slice())
                    .collect::<Vec<_>>(),
                None,
            )
        }
        SplitPlan::ExactOut(plan) => (
            quote_suffix_exact_out(
                pools,
                &plan.allocation,
                Funds {
                    sender: &plan.context.caller,
                    recipient: &plan.context.recipient,
                },
            ),
            &plan.context,
            &plan.plan_hash,
            plan.allocation
                .routes
                .iter()
                .map(|route| route.tokens.as_slice())
                .collect(),
            Some(&plan.allocation.input_budget),
        ),
    };
    let candidate = candidate.ok_or_else(|| err("EXECUTION_SHARED_SUFFIX_UNAVAILABLE"))?;
    let candidate_hash = hash(&(CANDIDATE_VERSION, plan_hash, &candidate))?;
    let exact_out = budget.is_some();
    let (max_sell_amount, min_buy_amount) = if let Some(budget) = budget {
        (
            maximum_input(&candidate.sell_amount, budget, context.slippage_bps)?,
            candidate.buy_amount.clone(),
        )
    } else {
        (
            candidate.sell_amount.clone(),
            minimum_output(&candidate.buy_amount, context.slippage_bps)?,
        )
    };
    let (suffix, prefixes) = candidate
        .steps
        .split_last()
        .ok_or_else(|| err("EXECUTION_PATH_MISMATCH"))?;
    let legs = prefixes
        .iter()
        .zip(&paths)
        .map(|(step, tokens)| Leg {
            amount: if exact_out {
                &step.amount_out
            } else {
                &step.amount_in
            },
            tokens: &tokens[..=step.edges.len()],
            edges: &step.edges,
        })
        .collect::<Vec<_>>();
    let suffix_tokens = &paths[0][legs[0].edges.len()..];
    // executeExactIn/Out(address,uint256,uint256,uint256,(uint256,address[])[],address[])
    let mut bytes = if exact_out {
        vec![0x31, 0x20, 0x28, 0xa8]
    } else {
        vec![0x5d, 0xcf, 0xea, 0x7f]
    };
    bytes.extend(addr_word(&context.recipient)?);
    bytes.extend(max_sell_amount.to_be_bytes()?);
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
        sell_amount: candidate.sell_amount,
        max_sell_amount,
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
