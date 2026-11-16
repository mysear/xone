//! Full binding for original Router02 output-specified simple paths.
//! Recomputed quote consistency is distinct from RPC or independent execution.
use crate::{
    address as decode_address,
    bound_v2::{code_hash, nonzero_address},
};
use crate::{
    exact_out::{self, ExactOutPlan},
    ExecutorIdentity, Transaction,
};
use serde::{Deserialize, Serialize};
use xone_domain::{err, hash, v2_swap, Amount, Pool, Result, Snapshot};
use xone_optimizer::exact_out::{evaluate_allocation, ExactOutProposal};
use xone_routing::Funds;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundEvmExactOutExecution {
    pub plan: ExactOutPlan,
    pub executor: ExecutorIdentity,
    pub transaction: Transaction,
    pub touched_pool_ids: Vec<String>,
    pub pool_state_hash: String,
    pub binding_hash: String,
}
fn address(value: &str) -> Result<Vec<u8>> {
    nonzero_address(value)?;
    Ok(decode_address(value)?.to_vec())
}
fn word(value: &Amount) -> Result<Vec<u8>> {
    Ok(value.to_be_bytes()?.to_vec())
}
pub fn bind_v2_exact_out_execution(
    plan: &ExactOutPlan,
    pools: &[Pool],
    executor: &ExecutorIdentity,
    allowed: &[String],
) -> Result<BoundEvmExactOutExecution> {
    exact_out::verify(plan)?;
    if !plan.context.chain_key.starts_with("evm:")
        || plan.allocation.routes.len() != 1
        || executor.adapter_version != "uniswap-v2-router02/1.1.0-beta.0"
    {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    if !allowed.contains(&executor.target) {
        return Err(err("EXECUTION_TARGET_NOT_ALLOWED"));
    }
    for value in [
        &executor.target,
        &executor.factory,
        &plan.context.caller,
        &plan.context.recipient,
        &plan.context.sell_token,
        &plan.context.buy_token,
    ] {
        address(value)?;
    }
    code_hash(&executor.code_sha256)?;
    code_hash(&executor.factory_code_sha256)?;
    for h in [&plan.snapshot.block.hash, &plan.snapshot.block.parent_hash] {
        code_hash(
            h.strip_prefix("0x")
                .ok_or_else(|| err("PREPARATION_EVM_SOURCE_HASH"))?,
        )?;
    }
    if plan.context.deadline_ms <= plan.snapshot.block.timestamp_ms {
        return Err(err("INVALID_EXECUTION_DEADLINE"));
    }
    Snapshot {
        reference: plan.snapshot.clone(),
        pools: pools.to_vec(),
        source: "BOUND_EXACT_OUT".into(),
        history_gaps: Vec::new(),
    }
    .validate(
        &plan.snapshot.owner_service,
        plan.snapshot.owner_assignment_version,
    )?;
    let adapters = pools
        .iter()
        .map(|p| &p.adapter_version)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if hash(&adapters)? != plan.snapshot.adapter_set_hash {
        return Err(err("ADAPTER_SET_HASH_MISMATCH"));
    }
    let route = &plan.allocation.routes[0];
    if route.edges.is_empty()
        || route.edges.len() > 8
        || route.tokens.len() != route.edges.len() + 1
        || route.amount_out != plan.allocation.target_output
        || route.target_output.is_some()
        || route.factory != executor.factory
        || plan.allocation.gross_output() != &plan.allocation.target_output
    {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let mut touched = Vec::new();
    for (i, edge) in route.edges.iter().enumerate() {
        let pool = pools
            .iter()
            .find(|p| p.pool_id == edge.pool_id)
            .ok_or_else(|| err("MISSING_POOL"))?;
        if pool.protocol != v2_swap::PROFILE
            || pool.adapter_version != v2_swap::ADAPTER
            || pool.token_behavior != "standard"
            || pool.fee_numerator != 3
            || pool.fee_denominator != 1000
            || edge.protocol != pool.protocol
        {
            return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
        }
        let v2 = pool
            .v2
            .as_ref()
            .ok_or_else(|| err("V2_IDENTITY_REQUIRED"))?;
        let state = v2
            .swap
            .as_ref()
            .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
        if v2.factory != executor.factory
            || v2.code_sha256 != v2_swap::PAIR_CODE_SHA256
            || state.factory_code_sha256 != v2_swap::FACTORY_CODE_SHA256
            || executor.factory_code_sha256 != state.factory_code_sha256
        {
            return Err(err("EXECUTION_FACTORY_CODE_MISMATCH"));
        }
        if state.bank_block_number != plan.snapshot.block.number_or_slot
            || state.bank_block_hash != plan.snapshot.block.hash
            || state.bank_unix_timestamp.checked_mul(1000) != Some(plan.snapshot.block.timestamp_ms)
        {
            return Err(err("PREPARATION_EVM_POOL_SOURCE"));
        }
        let (from, to) = if edge.zero_for_one {
            (&pool.token0, &pool.token1)
        } else {
            (&pool.token1, &pool.token0)
        };
        if &route.tokens[i] != from
            || &route.tokens[i + 1] != to
            || &edge.to != to
            || edge.from_token.as_ref().is_some_and(|value| value != from)
        {
            return Err(err("EXECUTION_PATH_MISMATCH"));
        }
        address(&pool.pool_id)?;
        if [from, to, &pool.pool_id, &executor.target, &executor.factory]
            .iter()
            .any(|a| **a == plan.context.caller || **a == plan.context.recipient)
        {
            return Err(err("EXECUTION_FUNDS_CONTEXT"));
        }
        touched.push(pool.pool_id.clone());
    }
    let proposal = ExactOutProposal {
        edges: route.edges.clone(),
        amount_out: route.amount_out.clone(),
    };
    let evaluated = evaluate_allocation(
        pools,
        &[proposal],
        &plan.allocation.target_output,
        &plan.allocation.input_budget,
        Funds {
            sender: &plan.context.caller,
            recipient: &plan.context.recipient,
        },
    )?;
    if serde_json::to_value(&evaluated).map_err(|_| err("HASH_ENCODING"))?
        != serde_json::to_value(&plan.allocation).map_err(|_| err("HASH_ENCODING"))?
    {
        return Err(err("EXECUTION_ALLOCATION_MISMATCH"));
    }
    let mut bytes = vec![0x88, 0x03, 0xdb, 0xee];
    bytes.extend(word(&plan.allocation.target_output)?);
    bytes.extend(word(&plan.max_sell_amount)?);
    bytes.extend(word(&Amount::checked(160u64.into())?)?);
    bytes.extend([0u8; 12]);
    bytes.extend(address(&plan.context.recipient)?);
    bytes.extend(word(&Amount::checked(
        (plan.context.deadline_ms / 1000).into(),
    )?)?);
    bytes.extend(word(&Amount::checked(route.tokens.len().into())?)?);
    for token in &route.tokens {
        bytes.extend([0u8; 12]);
        bytes.extend(address(token)?);
    }
    let transaction = Transaction {
        chain_key: plan.context.chain_key.clone(),
        from: plan.context.caller.clone(),
        to: executor.target.clone(),
        data: format!("0x{}", hex::encode(bytes)),
        value: Amount::zero(),
    };
    let mut ordered = pools.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.pool_id.cmp(&b.pool_id));
    let mut bound = BoundEvmExactOutExecution {
        plan: plan.clone(),
        executor: executor.clone(),
        transaction,
        touched_pool_ids: touched,
        pool_state_hash: hash(&ordered)?,
        binding_hash: String::new(),
    };
    let mut value = serde_json::to_value(&bound).map_err(|_| err("HASH_ENCODING"))?;
    value
        .as_object_mut()
        .expect("binding object")
        .remove("binding_hash");
    bound.binding_hash = hash(&value)?;
    Ok(bound)
}
pub fn validate_bound_exact_out_execution(
    bound: &BoundEvmExactOutExecution,
    pools: &[Pool],
    allowed: &[String],
) -> Result<()> {
    let rebuilt = bind_v2_exact_out_execution(&bound.plan, pools, &bound.executor, allowed)?;
    if serde_json::to_value(&rebuilt).map_err(|_| err("HASH_ENCODING"))?
        != serde_json::to_value(bound).map_err(|_| err("HASH_ENCODING"))?
    {
        return Err(err("EXECUTION_BINDING_MISMATCH"));
    }
    Ok(())
}
