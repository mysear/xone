use serde::Serialize;
use xone_domain::{err, hash, singleton::ZERO_ADDRESS, Amount, Pool, Result, SnapshotRef};
use xone_routing::Edge;

use crate::{
    address,
    bound_v2::{code_hash, nonzero_address},
    bound_v2_exact_out::validate_evm_scope,
    exact_out::{self, ExactOutPlan},
    verify_plan_hash, Context, Plan, Transaction,
};

#[derive(Clone, Debug, Serialize)]
pub struct BoundV4Swap {
    pub transaction: Transaction,
    pub executor_code_sha256: String,
    pub manager: String,
    pub manager_code_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infinity: Option<BoundInfinityVault>,
    pub pool_state_hash: String,
    pub input_limit: Amount,
    pub output_limit: Amount,
    pub binding_hash: String,
}

pub const INFINITY_CL: &str = "pancake-infinity-cl-advanced";
pub const INFINITY_BIN: &str = "pancake-infinity-bin";

#[derive(Clone, Debug, Serialize)]
pub struct BoundInfinityVault {
    pub vault: String,
    pub vault_code_sha256: String,
    pub is_bin: bool,
}

#[derive(Clone, Copy)]
pub struct Executor<'a> {
    pub address: &'a str,
    pub code_sha256: &'a str,
    pub infinity_vault: Option<(&'a str, &'a str)>,
}

pub fn bind_exact_in(plan: &Plan, pools: &[Pool], executor: Executor<'_>) -> Result<BoundV4Swap> {
    verify_plan_hash(plan)?;
    let [route] = plan.allocation.routes.as_slice() else {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    };
    if route.amount_in != plan.allocation.sell_amount
        || route.amount_out != plan.allocation.gross_buy_amount
        || plan.allocation.refund != Amount::zero()
    {
        return Err(err("V4_SINGLE_ALLOCATION"));
    }
    bind(
        &plan.context,
        &plan.snapshot,
        &plan.plan_hash,
        pools,
        &route.path.edges,
        executor,
        false,
        &plan.allocation.sell_amount,
        &plan.min_buy_amount,
    )
}

pub fn bind_exact_out(
    plan: &ExactOutPlan,
    pools: &[Pool],
    executor: Executor<'_>,
) -> Result<BoundV4Swap> {
    exact_out::verify(plan)?;
    let [route] = plan.allocation.routes.as_slice() else {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    };
    if route.amount_in != plan.allocation.required_input
        || route.amount_out != plan.allocation.target_output
    {
        return Err(err("V4_SINGLE_ALLOCATION"));
    }
    // The executor and PoolManager return signed 128-bit deltas.
    let execution_max = Amount::checked((i128::MAX as u128).into())?;
    let input_limit = plan.max_sell_amount.clone().min(execution_max);
    if plan.allocation.required_input > input_limit {
        return Err(err("V4_SINGLE_AMOUNT_RANGE"));
    }
    bind(
        &plan.context,
        &plan.snapshot,
        &plan.plan_hash,
        pools,
        &route.edges,
        executor,
        true,
        &input_limit,
        &plan.allocation.target_output,
    )
}

#[allow(clippy::too_many_arguments)]
fn bind(
    context: &Context,
    snapshot: &SnapshotRef,
    plan_hash: &str,
    pools: &[Pool],
    edges: &[Edge],
    executor: Executor<'_>,
    exact_out: bool,
    input: &Amount,
    output: &Amount,
) -> Result<BoundV4Swap> {
    let [edge] = edges else {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    };
    validate_evm_scope(snapshot, pools, context)?;
    let pool = pools
        .iter()
        .find(|pool| pool.pool_id == edge.pool_id)
        .ok_or_else(|| err("MISSING_POOL"))?;
    if pool.protocol != edge.protocol {
        return Err(err("V4_SINGLE_POOL_BINDING"));
    }
    let (sell, buy) = if edge.zero_for_one {
        (&pool.token0, &pool.token1)
    } else {
        (&pool.token1, &pool.token0)
    };
    if sell != &context.sell_token
        || buy != &context.buy_token
        || &edge.to != buy
        || edge.from_token.as_ref().is_some_and(|from| from != sell)
    {
        return Err(err("V4_SINGLE_POOL_BINDING"));
    }
    let (manager, manager_digest, hooks, key_fee, parameters, spacing) =
        match pool.protocol.as_str() {
            "uniswap-v4-advanced" | INFINITY_CL => {
                let state = pool
                    .singleton_advanced
                    .as_ref()
                    .filter(|state| state.snapshot_block_hash == snapshot.block.hash)
                    .ok_or_else(|| err("V4_SINGLE_POOL_BINDING"))?;
                let s = &state.snapshot;
                (
                    &s.manager,
                    &s.manager_code_sha256,
                    &s.hooks,
                    state.key_fee,
                    &s.parameters,
                    s.core.tick_spacing,
                )
            }
            INFINITY_BIN => {
                let s = pool
                    .infinity_bin
                    .as_ref()
                    .filter(|s| s.block_hash == snapshot.block.hash)
                    .ok_or_else(|| err("V4_SINGLE_POOL_BINDING"))?;
                (
                    &s.manager,
                    &s.manager_code_sha256,
                    &s.hooks,
                    s.key_fee(),
                    &s.parameters,
                    0,
                )
            }
            _ => return Err(err("EXECUTION_ADAPTER_UNSUPPORTED")),
        };
    let infinity = match (pool.protocol.as_str(), executor.infinity_vault) {
        ("uniswap-v4-advanced", None) => None,
        (INFINITY_CL | INFINITY_BIN, Some((vault, digest))) => {
            nonzero_address(vault)?;
            code_hash(digest)?;
            if [manager.as_str(), hooks.as_str(), sell, buy].contains(&vault)
                || pool
                    .infinity_bin
                    .as_ref()
                    .is_some_and(|s| s.vault != vault || s.vault_code_sha256 != digest)
            {
                return Err(err("INFINITY_SINGLE_VAULT_BINDING"));
            }
            Some(BoundInfinityVault {
                vault: vault.into(),
                vault_code_sha256: digest.into(),
                is_bin: pool.protocol == INFINITY_BIN,
            })
        }
        _ => return Err(err("EXECUTION_ADAPTER_UNSUPPORTED")),
    };
    for owner in [executor.address, &context.caller, &context.recipient] {
        nonzero_address(owner)?;
        if [manager.as_str(), hooks.as_str(), sell, buy].contains(&owner)
            || infinity
                .as_ref()
                .is_some_and(|binding| binding.vault == owner)
        {
            return Err(err("V4_SINGLE_OWNER_ALIAS"));
        }
    }
    if executor.address == context.caller || executor.address == context.recipient {
        return Err(err("V4_SINGLE_OWNER_ALIAS"));
    }
    code_hash(executor.code_sha256)?;
    if [input, output]
        .iter()
        .any(|a| !(1..=127).contains(&a.0.bits()))
    {
        return Err(err("V4_SINGLE_AMOUNT_RANGE"));
    }
    let address_word = |value: &str| -> Result<[u8; 32]> {
        let mut word = [0; 32];
        word[12..].copy_from_slice(&address(value)?);
        Ok(word)
    };
    let word = |number: u64| {
        let mut word = [0; 32];
        word[24..].copy_from_slice(&number.to_be_bytes());
        word
    };
    let deadline = context.deadline_ms / 1000;
    if deadline <= snapshot.block.timestamp_ms / 1000 {
        return Err(err("INVALID_EXECUTION_DEADLINE"));
    }
    let mut fields = vec![address_word(&pool.token0)?, address_word(&pool.token1)?];
    let mut data = if infinity.is_some() {
        let parameters: [u8; 32] = hex::decode(
            parameters
                .strip_prefix("0x")
                .ok_or_else(|| err("INFINITY_SINGLE_KEY"))?,
        )
        .map_err(|_| err("INFINITY_SINGLE_KEY"))?
        .try_into()
        .map_err(|_| err("INFINITY_SINGLE_KEY"))?;
        fields.extend([
            address_word(hooks)?,
            address_word(manager)?,
            word(u64::from(key_fee)),
            parameters,
        ]);
        // execute(((address,address,address,address,uint24,bytes32),bool,bool,uint128,uint128,address,uint256))
        vec![0xaa, 0x53, 0x97, 0xac]
    } else {
        fields.extend([
            word(u64::from(key_fee)),
            word(spacing as u64),
            address_word(hooks)?,
        ]);
        vec![0x31, 0xd2, 0x46, 0x1f]
    };
    fields.extend([
        word(u64::from(edge.zero_for_one)),
        word(u64::from(exact_out)),
        input.to_be_bytes()?,
        output.to_be_bytes()?,
        address_word(&context.recipient)?,
        word(deadline),
    ]);
    for field in fields {
        data.extend_from_slice(&field);
    }
    let mut bound = BoundV4Swap {
        transaction: Transaction {
            chain_key: context.chain_key.clone(),
            from: context.caller.clone(),
            to: executor.address.into(),
            data: format!("0x{}", hex::encode(data)),
            value: if sell == ZERO_ADDRESS {
                input.clone()
            } else {
                Amount::zero()
            },
        },
        executor_code_sha256: executor.code_sha256.into(),
        manager: manager.clone(),
        manager_code_sha256: manager_digest.clone(),
        infinity,
        pool_state_hash: hash(pool)?,
        input_limit: input.clone(),
        output_limit: output.clone(),
        binding_hash: String::new(),
    };
    bound.binding_hash = hash(&(plan_hash, &bound))?;
    Ok(bound)
}
