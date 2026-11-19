//! Commitment for one call to OrderedV2Split. No runtime/code trust is inferred here.
use crate::bound_v2::{code_hash, nonzero_address};
use crate::bound_v2_exact_out::{validate_complete_path, validate_v2_scope};
use crate::{exact_out, verify_plan_hash, Context, ExecutorIdentity, Plan, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use xone_domain::{err, hash, Amount, Pool, Result};
use xone_optimizer::{evaluate_allocation, exact_out as optimizer_out, MAX_SPLITS};
use xone_routing::{Edge, Funds};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "plan",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum SplitPlan {
    ExactIn(Plan),
    ExactOut(exact_out::ExactOutPlan),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitExecutor {
    pub target: String,
    pub code_sha256: String,
    pub router: ExecutorIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundSplit {
    pub plan: SplitPlan,
    pub executor: SplitExecutor,
    pub transaction: Transaction,
    /// Includes repeated pool IDs: order and per-leg boundaries are significant.
    pub leg_pools: Vec<Vec<String>>,
    pub pool_state_hash: String,
    pub binding_hash: String,
}

struct Leg<'a> {
    amount: &'a Amount,
    tokens: &'a [String],
    edges: &'a [Edge],
}

pub fn bind(
    plan: &SplitPlan,
    pools: &[Pool],
    executor: &SplitExecutor,
    allowed: &[String],
) -> Result<BoundSplit> {
    let (context, snapshot, input, output, legs) = match plan {
        SplitPlan::ExactIn(p) => {
            let a = &p.allocation;
            check_count(a.routes.len())?;
            verify_plan_hash(p)?;
            (
                &p.context,
                &p.snapshot,
                &a.sell_amount,
                &p.min_buy_amount,
                a.routes
                    .iter()
                    .map(|r| Leg {
                        amount: &r.amount_in,
                        tokens: &r.path.tokens,
                        edges: &r.path.edges,
                    })
                    .collect::<Vec<_>>(),
            )
        }
        SplitPlan::ExactOut(p) => {
            let a = &p.allocation;
            check_count(a.routes.len())?;
            exact_out::verify(p)?;
            (
                &p.context,
                &p.snapshot,
                &p.max_sell_amount,
                &a.target_output,
                a.routes
                    .iter()
                    .map(|r| Leg {
                        amount: &r.amount_out,
                        tokens: &r.tokens,
                        edges: &r.edges,
                    })
                    .collect(),
            )
        }
    };
    let router = &executor.router;
    if !context.chain_key.starts_with("evm:")
        || router.adapter_version != "uniswap-v2-router02/1.1.0-beta.0"
    {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    if !allowed.contains(&executor.target) || !allowed.contains(&router.target) {
        return Err(err("EXECUTION_TARGET_NOT_ALLOWED"));
    }
    let contracts = [&executor.target, &router.target, &router.factory];
    if contracts.iter().collect::<BTreeSet<_>>().len() != 3
        || contracts.contains(&&context.caller)
        || contracts.contains(&&context.recipient)
    {
        return Err(err("EXECUTION_FUNDS_CONTEXT"));
    }
    for addr in contracts
        .into_iter()
        .chain([&context.caller, &context.recipient])
    {
        nonzero_address(addr)?;
    }
    for digest in [
        &executor.code_sha256,
        &router.code_sha256,
        &router.factory_code_sha256,
    ] {
        code_hash(digest)?;
    }
    validate_v2_scope(snapshot, pools, context)?;
    let mut leg_pools = Vec::new();
    let mut physical = BTreeMap::new();
    for leg in &legs {
        if !(1..=8).contains(&leg.edges.len())
            || leg.tokens.len() != leg.edges.len() + 1
            || leg.tokens.first() != Some(&context.sell_token)
            || leg.tokens.last() != Some(&context.buy_token)
            || leg.tokens.iter().collect::<BTreeSet<_>>().len() != leg.tokens.len()
            || leg.amount == &Amount::zero()
        {
            return Err(err("EXECUTION_PATH_MISMATCH"));
        }
        let ids = validate_complete_path(context, snapshot, leg.edges, leg.tokens, pools, router)?;
        for (pair, id) in leg.tokens.windows(2).zip(&ids) {
            if contracts.contains(&id) || pair.iter().any(|t| contracts.contains(&t)) {
                return Err(err("EXECUTION_PATH_MISMATCH"));
            }
            let key = (
                std::cmp::min(&pair[0], &pair[1]),
                std::cmp::max(&pair[0], &pair[1]),
            );
            if physical
                .insert(key, id.clone())
                .is_some_and(|prior| prior != *id)
            {
                return Err(err("EXECUTION_PATH_MISMATCH"));
            }
        }
        leg_pools.push(ids);
    }
    match plan {
        SplitPlan::ExactIn(p) => {
            let a = &p.allocation;
            let mut routes = a.routes.clone();
            for r in &mut routes {
                for e in &mut r.path.edges {
                    e.from_token = None;
                }
            }
            let replay = evaluate_allocation(pools, &routes, &Amount::zero())?;
            if replay.sell_amount != a.sell_amount
                || replay.gross_buy_amount != a.gross_buy_amount
                || a.refund != Amount::zero()
                || a.net_buy_amount > a.gross_buy_amount
                || replay.routes.iter().zip(&a.routes).any(|(v, r)| {
                    v.amount_out != r.amount_out
                        || r.path.amount_in != r.amount_in
                        || r.path.amount_out != r.amount_out
                })
            {
                return Err(err("EXECUTION_ALLOCATION_MISMATCH"));
            }
        }
        SplitPlan::ExactOut(p) => {
            let a = &p.allocation;
            let proposals = a
                .routes
                .iter()
                .map(|r| optimizer_out::ExactOutProposal {
                    amount_out: r.amount_out.clone(),
                    edges: r.edges.clone(),
                })
                .collect::<Vec<_>>();
            let replay = optimizer_out::evaluate_allocation(
                pools,
                &proposals,
                &a.target_output,
                &a.input_budget,
                Funds {
                    sender: &context.caller,
                    recipient: &context.recipient,
                },
            )?;
            if a.gross_buy_amount.is_some()
                || a.routes
                    .iter()
                    .any(|r| r.factory != router.factory || r.target_output.is_some())
                || hash(&replay)? != hash(a)?
            {
                return Err(err("EXECUTION_ALLOCATION_MISMATCH"));
            }
        }
    }
    let transaction = encode(
        context,
        &executor.target,
        matches!(plan, SplitPlan::ExactOut(_)),
        input,
        output,
        &legs,
    )?;
    let mut ordered = pools.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.pool_id.cmp(&b.pool_id));
    let pool_state_hash = hash(&ordered)?;
    let binding_hash = hash(&(
        "xone-ordered-v2-split/1",
        plan,
        executor,
        &transaction,
        &leg_pools,
        &pool_state_hash,
    ))?;
    Ok(BoundSplit {
        plan: plan.clone(),
        executor: executor.clone(),
        transaction,
        leg_pools,
        pool_state_hash,
        binding_hash,
    })
}

fn check_count(count: usize) -> Result<()> {
    if !(2..=MAX_SPLITS).contains(&count) {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    Ok(())
}

pub fn validate(
    bound: &BoundSplit,
    pools: &[Pool],
    expected_executor: &SplitExecutor,
    allowed: &[String],
) -> Result<()> {
    let rebuilt = bind(&bound.plan, pools, expected_executor, allowed)?;
    if hash(&rebuilt)? != hash(bound)? {
        return Err(err("EXECUTION_BINDING_MISMATCH"));
    }
    Ok(())
}

/// Create a zero-value approval for exactly the quoted input at this bank.
/// The swap spends from the caller through the batch executor (not Router02).
/// Nonzero insufficient allowances need a token-specific replacement policy.
pub fn caller_approval(
    bound: &BoundSplit,
    pools: &[Pool],
    executor: &SplitExecutor,
    allowed: &[String],
    allowance: &Amount,
) -> Result<Option<Transaction>> {
    validate(bound, pools, executor, allowed)?;
    Amount::checked(allowance.0.clone())?;
    let (context, required) = match &bound.plan {
        SplitPlan::ExactIn(p) => (&p.context, &p.allocation.sell_amount),
        SplitPlan::ExactOut(p) => (&p.context, &p.allocation.required_input),
    };
    if allowance >= required {
        return Ok(None);
    }
    if allowance != &Amount::zero() {
        return Err(err("EXECUTION_APPROVAL_REPLACEMENT_POLICY_REQUIRED"));
    }
    let mut data = vec![0x09, 0x5e, 0xa7, 0xb3];
    data.extend(addr_word(&bound.transaction.to)?);
    data.extend(required.to_be_bytes()?);
    Ok(Some(Transaction {
        chain_key: context.chain_key.clone(),
        from: context.caller.clone(),
        to: context.sell_token.clone(),
        data: format!("0x{}", hex::encode(data)),
        value: Amount::zero(),
    }))
}

fn small(n: usize) -> [u8; 32] {
    let mut w = [0; 32];
    w[24..].copy_from_slice(&(n as u64).to_be_bytes());
    w
}
fn addr_word(addr: &str) -> Result<[u8; 32]> {
    nonzero_address(addr)?;
    let mut w = [0; 32];
    w[12..].copy_from_slice(&crate::address(addr)?);
    Ok(w)
}
fn encode(
    context: &Context,
    target: &str,
    exact_out: bool,
    input: &Amount,
    output: &Amount,
    legs: &[Leg<'_>],
) -> Result<Transaction> {
    // execute(bool,address,uint256,uint256,uint256,(uint256,address[])[])
    let mut bytes = vec![0x99, 0xed, 0xab, 0x89];
    bytes.extend(small(usize::from(exact_out)));
    bytes.extend(addr_word(&context.recipient)?);
    bytes.extend(input.to_be_bytes()?);
    bytes.extend(output.to_be_bytes()?);
    bytes.extend(Amount((context.deadline_ms / 1000).into()).to_be_bytes()?);
    bytes.extend(small(6 * 32));
    bytes.extend(small(legs.len()));
    // Tuple offsets start at the word after the array length, not the length word.
    let mut offset = 32 * legs.len();
    for leg in legs {
        bytes.extend(small(offset));
        offset += 32 * (3 + leg.tokens.len());
    }
    for leg in legs {
        bytes.extend(leg.amount.to_be_bytes()?);
        bytes.extend(small(64));
        bytes.extend(small(leg.tokens.len()));
        for token in leg.tokens {
            bytes.extend(addr_word(token)?);
        }
    }
    Ok(Transaction {
        chain_key: context.chain_key.clone(),
        from: context.caller.clone(),
        to: target.to_owned(),
        data: format!("0x{}", hex::encode(bytes)),
        value: Amount::zero(),
    })
}
