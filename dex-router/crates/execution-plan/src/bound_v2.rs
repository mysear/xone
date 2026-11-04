use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use xone_domain::{err, hash, protocols, Amount, Pool, Result, Snapshot};
use xone_optimizer::evaluate_allocation;

use crate::{evm_transaction, verify_plan_hash, Plan, Transaction};

const ROUTER_VERSION: &str = "uniswap-v2-router02/1.1.0-beta.0";

/// Declared router and factory identities to check against the execution image.
/// Binding does not attest that these code bytes are present on any chain.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorIdentity {
    pub target: String,
    pub code_sha256: String,
    pub factory: String,
    pub factory_code_sha256: String,
    pub adapter_version: String,
}

/// A deterministic BOUND envelope, not an execution proof or VERIFIED result.
/// The execution backend must independently check router.factory(), factory.getPair(),
/// code SHA-256, block/state identity, balances and allowance before executing.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundEvmExecution {
    pub plan: Plan,
    pub executor: ExecutorIdentity,
    pub transaction: Transaction,
    pub touched_pool_ids: Vec<String>,
    pub pool_state_hash: String,
    pub binding_hash: String,
}

fn nonzero_address(value: &str) -> Result<()> {
    protocols::validate_address(value)?;
    if crate::address(value)? == [0u8; 20] {
        return Err(err("ZERO_EVM_ADDRESS"));
    }
    Ok(())
}

fn code_hash(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || value.bytes().all(|b| b == b'0')
    {
        return Err(err("INVALID_EXECUTION_CODE_HASH"));
    }
    Ok(())
}

/// Bind one exact-input, one-hop Uniswap V2 standard ERC20 swap with zero ETH.
/// `pools` must contain the complete snapshot scope. Quote math is recomputed only
/// as an internal consistency check; it supplies no independent execution evidence.
/// Current-time freshness and actual code/account identity are backend obligations.
pub fn bind_v2_execution(
    plan: &Plan,
    pools: &[Pool],
    executor: &ExecutorIdentity,
    allowlist: &[String],
) -> Result<BoundEvmExecution> {
    verify_plan_hash(plan)?;
    let context = &plan.context;
    xone_domain::validate_chain(&context.chain_key)?;
    if !context.chain_key.starts_with("evm:") {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    for address in [
        &context.caller,
        &context.recipient,
        &context.sell_token,
        &context.buy_token,
        &executor.target,
        &executor.factory,
    ] {
        nonzero_address(address)?;
    }
    if !allowlist.contains(&executor.target) {
        return Err(err("EXECUTION_TARGET_NOT_ALLOWED"));
    }
    if executor.adapter_version != ROUTER_VERSION {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    code_hash(&executor.code_sha256)?;
    code_hash(&executor.factory_code_sha256)?;
    if context.deadline_ms == 0 || context.deadline_ms < plan.snapshot.block.timestamp_ms {
        return Err(err("INVALID_EXECUTION_DEADLINE"));
    }
    for block_hash in [&plan.snapshot.block.hash, &plan.snapshot.block.parent_hash] {
        code_hash(
            block_hash
                .strip_prefix("0x")
                .ok_or_else(|| err("INVALID_EXECUTION_BLOCK_HASH"))?,
        )?;
    }
    // SnapshotRef has no validate method; use the domain's full-snapshot validator.
    Snapshot {
        reference: plan.snapshot.clone(),
        pools: pools.to_vec(),
        source: "BOUND_EXECUTION".into(),
        history_gaps: Vec::new(),
    }
    .validate(
        &plan.snapshot.owner_service,
        plan.snapshot.owner_assignment_version,
    )?;
    let adapters = pools
        .iter()
        .map(|p| &p.adapter_version)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if hash(&adapters)? != plan.snapshot.adapter_set_hash {
        return Err(err("ADAPTER_SET_HASH_MISMATCH"));
    }
    let allocation = &plan.allocation;
    if allocation.routes.len() != 1 {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let route = &allocation.routes[0];
    if route.path.edges.len() != 1 || route.path.tokens.len() != 2 {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let edge = &route.path.edges[0];
    let pool = pools
        .iter()
        .find(|p| p.pool_id == edge.pool_id)
        .ok_or_else(|| err("MISSING_POOL"))?;
    let complete = pool.protocol == xone_domain::v2_swap::PROFILE
        && pool.adapter_version == xone_domain::v2_swap::ADAPTER;
    let legacy = pool.protocol == "uniswap-v2" && pool.adapter_version == "uniswap-v2/1";
    if !(legacy || complete)
        || edge.protocol != pool.protocol
        || pool.token_behavior != "standard"
        || pool.fee_numerator != 3
        || pool.fee_denominator != 1000
    {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    for address in [&pool.pool_id, &pool.token0, &pool.token1] {
        nonzero_address(address)?;
    }
    let identity = pool
        .v2
        .as_ref()
        .ok_or_else(|| err("V2_IDENTITY_REQUIRED"))?;
    nonzero_address(&identity.factory)?;
    code_hash(&identity.code_sha256)?;
    if complete {
        let swap = identity
            .swap
            .as_ref()
            .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
        if identity.code_sha256 != xone_domain::v2_swap::PAIR_CODE_SHA256
            || swap.factory_code_sha256 != xone_domain::v2_swap::FACTORY_CODE_SHA256
            || swap.factory_code_sha256 != executor.factory_code_sha256
        {
            return Err(err("EXECUTION_FACTORY_CODE_MISMATCH"));
        }
    }
    if identity.factory != executor.factory {
        return Err(err("EXECUTION_FACTORY_MISMATCH"));
    }
    let (sell, buy) = if edge.zero_for_one {
        (&pool.token0, &pool.token1)
    } else {
        (&pool.token1, &pool.token0)
    };
    if &context.sell_token != sell
        || &context.buy_token != buy
        || &route.path.tokens[0] != sell
        || &route.path.tokens[1] != buy
        || &edge.to != buy
        || edge.from_token.as_ref().is_some_and(|from| from != sell)
    {
        return Err(err("EXECUTION_PATH_MISMATCH"));
    }
    if allocation.sell_amount == Amount::zero()
        || allocation.refund != Amount::zero()
        || route.amount_in != allocation.sell_amount
        || route.path.amount_in != route.amount_in
        || route.path.amount_out != route.amount_out
        || allocation.net_buy_amount > allocation.gross_buy_amount
    {
        return Err(err("EXECUTION_ALLOCATION_MISMATCH"));
    }
    // Routing's legacy two-token evaluator expects an absent from_token. Its value
    // was checked above; normalize only that redundant field for mathematical replay.
    let mut routes = allocation.routes.clone();
    routes[0].path.edges[0].from_token = None;
    let expected = evaluate_allocation(pools, &routes, &Amount::zero())?;
    if expected.sell_amount != allocation.sell_amount
        || expected.gross_buy_amount != allocation.gross_buy_amount
        || expected.routes[0].amount_out != route.amount_out
    {
        return Err(err("EXECUTION_ALLOCATION_MISMATCH"));
    }
    let mut ordered_pools = pools.iter().collect::<Vec<_>>();
    ordered_pools.sort_by(|a, b| a.pool_id.cmp(&b.pool_id));
    let mut bound = BoundEvmExecution {
        plan: plan.clone(),
        executor: executor.clone(),
        transaction: evm_transaction(plan, &executor.target, allowlist)?,
        touched_pool_ids: vec![pool.pool_id.clone()],
        pool_state_hash: hash(&ordered_pools)?,
        binding_hash: String::new(),
    };
    bound.binding_hash = hash(&serde_json::json!({
        "plan": bound.plan,
        "executor": bound.executor,
        "transaction": bound.transaction,
        "touched_pool_ids": bound.touched_pool_ids,
        "pool_state_hash": bound.pool_state_hash,
    }))?;
    Ok(bound)
}

/// Rebuild every derived field from the committed plan and supplied complete scope.
/// A caller-supplied binding hash, even correctly recomputed, is never sufficient.
pub fn validate_bound_execution(
    bound: &BoundEvmExecution,
    pools: &[Pool],
    allowlist: &[String],
) -> Result<()> {
    let expected = bind_v2_execution(&bound.plan, pools, &bound.executor, allowlist)?;
    let canonical =
        |value: &BoundEvmExecution| serde_json::to_value(value).map_err(|_| err("HASH_ENCODING"));
    if canonical(bound)? != canonical(&expected)? {
        return Err(err("EXECUTION_BINDING_MISMATCH"));
    }
    Ok(())
}
