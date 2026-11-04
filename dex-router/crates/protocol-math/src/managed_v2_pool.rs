//! Full pricing-state adapter for the published ManagedPool V2 AUM profile.
use crate::{
    managed_v2_aum::{collect_aum, view_aum, AumAccrual, AumCollectionInput},
    managed_v2_swap::{quote_regular, WeightedInputs},
    Swap,
};
use num_bigint::BigUint;
use xone_domain::{
    err,
    managed_v2_state::{BalancerManagedV2State, ManagedV2ExecutionContext, PROTOCOL},
    Amount, Pool, Result,
};
fn state(pool: &Pool) -> Result<&BalancerManagedV2State> {
    pool.validate()?;
    if pool.protocol != PROTOCOL {
        return Err(err("MANAGED_V2_PROFILE"));
    }
    pool.multi_asset
        .as_ref()
        .and_then(|s| s.managed_v2.as_ref())
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))
}
fn add_wallet_balance(balance: &Amount, delta: &BigUint) -> Result<Amount> {
    let next = &balance.0 + delta;
    if next.bits() > 256 {
        return Err(err("MANAGED_V2_UINT256_OVERFLOW"));
    }
    Ok(Amount(next))
}
fn effective_fee(m: &BalancerManagedV2State) -> Amount {
    if m.recovery_mode {
        Amount::zero()
    } else {
        m.management_aum_fee.clone()
    }
}
fn validate_source_view(m: &BalancerManagedV2State) -> Result<()> {
    let view = view_aum(
        &m.virtual_supply,
        current_context(m).timestamp,
        m.last_collection_timestamp,
        &effective_fee(m),
    )?;
    if view.pending_bpt != m.pending_aum || view.actual_supply != m.actual_supply {
        return Err(err("MANAGED_V2_AUM_VIEW_MISMATCH"));
    }
    Ok(())
}
fn clock(m: &BalancerManagedV2State, block: u64, time: u64) -> Result<()> {
    // getPoolTokenInfo decodes a packed uint32 even when the chain's block
    // number is wider. Reject impossible getter values before using history.
    if m.token_info
        .iter()
        .any(|info| info.last_change_block > u64::from(u32::MAX))
    {
        return Err(err("MANAGED_V2_VAULT_BLOCK_WIDTH"));
    }
    let current = current_context(m);
    if block < current.block_number
        || time < current.timestamp
        || time < u64::from(m.last_collection_timestamp)
        || m.token_info
            .iter()
            .any(|info| info.last_change_block > block)
        || time.checked_mul(1000).is_none()
    {
        return Err(err("MANAGED_V2_EXECUTION_CONTEXT"));
    }
    Ok(())
}
fn current_context(m: &BalancerManagedV2State) -> ManagedV2ExecutionContext {
    m.execution_context
        .clone()
        .unwrap_or(ManagedV2ExecutionContext {
            block_number: m.block_number,
            timestamp: m.timestamp,
        })
}
fn advance_context(m: &mut BalancerManagedV2State, block: u64, timestamp: u64) {
    // A transition is derived even when evaluated at source H. Only an actual
    // source observation may omit context and enter canonical Snapshot/wire.
    // Both getters remain paused at equality and expire independently.
    m.paused &= timestamp <= m.buffer_period_end;
    m.vault_paused &= timestamp <= m.vault_buffer_period_end;
    m.execution_context = Some(ManagedV2ExecutionContext {
        block_number: block,
        timestamp,
    });
}
/// Normal routes continue at the current computation clock (source H initially).
pub fn quote_exact_in_tokens(
    pool: &Pool,
    input: &Amount,
    token_in: &str,
    token_out: &str,
) -> Result<Swap> {
    let m = state(pool)?;
    quote_at(
        pool,
        input,
        token_in,
        token_out,
        current_context(m).block_number,
        current_context(m).timestamp,
    )
}
/// Explicit context for original-node differential replay. Source bank metadata
/// stays H; this output is a derived transition, not a canonical Snapshot.
pub fn quote_at(
    pool: &Pool,
    input: &Amount,
    token_in: &str,
    token_out: &str,
    execution_block: u64,
    execution_timestamp: u64,
) -> Result<Swap> {
    let m = state(pool)?;
    validate_source_view(m)?;
    clock(m, execution_block, execution_timestamp)?;
    if !m.swaps_enabled {
        return Err(err("MANAGED_V2_SWAPS_DISABLED"));
    }
    if (m.paused && execution_timestamp <= m.buffer_period_end)
        || (m.vault_paused && execution_timestamp <= m.vault_buffer_period_end)
    {
        return Err(err("MANAGED_V2_PAUSED"));
    }
    let s = pool
        .multi_asset
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let i = s
        .assets
        .iter()
        .position(|a| a.token == token_in)
        .ok_or_else(|| err("MULTI_ASSET_INPUT_TOKEN"))?;
    let j = s
        .assets
        .iter()
        .position(|a| a.token == token_out)
        .ok_or_else(|| err("MULTI_ASSET_OUTPUT_TOKEN"))?;
    let cash = m
        .token_info
        .iter()
        .map(|a| a.cash.0.clone())
        .collect::<Vec<_>>();
    let managed = m
        .token_info
        .iter()
        .map(|a| a.managed.0.clone())
        .collect::<Vec<_>>();
    let weights = s.weights.iter().map(|a| a.0.clone()).collect::<Vec<_>>();
    let scales = s
        .assets
        .iter()
        .map(|a| a.scaling.0.clone())
        .collect::<Vec<_>>();
    let fee = BigUint::from(pool.fee_numerator);
    let q = quote_regular(
        WeightedInputs {
            cash: &cash,
            managed: &managed,
            weights: &weights,
            scales: &scales,
            fee: &fee,
        },
        &input.0,
        i,
        j,
    )?;
    // ERC20 transfers affect the Vault's global wallet, independently of this
    // pool's registered cash/managed ledger. Input includes the full swap fee.
    let wallet_in = add_wallet_balance(&m.token_info[i].vault_actual_balance, &input.0)?;
    if m.token_info[j].vault_actual_balance.0 < q.amount_out {
        return Err(err("MANAGED_V2_INSUFFICIENT_VAULT_WALLET"));
    }
    let wallet_out = Amount(&m.token_info[j].vault_actual_balance.0 - &q.amount_out);
    let future_view = view_aum(
        &m.virtual_supply,
        execution_timestamp,
        m.last_collection_timestamp,
        &effective_fee(m),
    )?;
    let mut next = pool.clone();
    let ns = next
        .multi_asset
        .as_mut()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let nm = ns
        .managed_v2
        .as_mut()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    for index in [i, j] {
        nm.token_info[index].cash = Amount(q.next_cash[index].clone());
        nm.token_info[index].managed = Amount(q.next_managed[index].clone());
        // BalanceAllocation._pack shifts block.number by 224 bits in a
        // uint256, retaining only its low 32 bits in the Vault getter.
        nm.token_info[index].last_change_block = execution_block & u64::from(u32::MAX);
        ns.assets[index].balance = Amount(&q.next_cash[index] + &q.next_managed[index]);
    }
    nm.token_info[i].vault_actual_balance = wallet_in;
    nm.token_info[j].vault_actual_balance = wallet_out;
    advance_context(nm, execution_block, execution_timestamp);
    nm.pending_aum = future_view.pending_bpt;
    nm.actual_supply = future_view.actual_supply;
    next.reserve0 = ns.assets[0].balance.clone();
    next.reserve1 = ns.assets[1].balance.clone();
    next.validate()?;
    Ok(Swap {
        amount_out: Amount(q.amount_out),
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}
#[derive(Clone, Debug)]
pub struct CollectionTransition {
    pub next_pool: Pool,
    pub accrual: AumAccrual,
    pub execution_block: u64,
    pub execution_timestamp: u64,
}
/// Explicit public collection, distinct from regular token swaps. Identity and
/// source checks precede monetary transition; only the original ledger changes.
pub fn collect_at(
    pool: &Pool,
    execution_block: u64,
    execution_timestamp: u64,
) -> Result<CollectionTransition> {
    let m = state(pool)?;
    validate_source_view(m)?;
    clock(m, execution_block, execution_timestamp)?;
    let ledger = AumCollectionInput {
        total_supply: m.total_supply.clone(),
        vault_bpt_balance: Amount(&m.vault_bpt_cash.0 + &m.vault_bpt_managed.0),
        manager_bpt_balance: m.manager_bpt_balance.clone(),
        protocol_bpt_balance: m.protocol_collector_bpt_balance.clone(),
        same_recipient: m.manager == m.protocol_collector,
        last_collection_timestamp: m.last_collection_timestamp,
        annual_fee: m.management_aum_fee.clone(),
        cached_protocol_share: m.cached_protocol_aum_percentage.clone(),
        paused: m.paused,
        buffer_period_end: m.buffer_period_end,
        recovery_mode: m.recovery_mode,
    };
    let transition = collect_aum(&ledger, execution_timestamp)?;
    let vault = &pool
        .multi_asset
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?
        .vault;
    // Each mint contributes its own share even if both recipients are the Vault.
    // Registered BPT cash/managed still determines virtual supply and is unchanged.
    let mut vault_bpt_actual_balance = m.vault_bpt_actual_balance.clone();
    if m.manager == *vault {
        vault_bpt_actual_balance =
            add_wallet_balance(&vault_bpt_actual_balance, &transition.accrual.manager_bpt.0)?;
    }
    if m.protocol_collector == *vault {
        vault_bpt_actual_balance = add_wallet_balance(
            &vault_bpt_actual_balance,
            &transition.accrual.protocol_bpt.0,
        )?;
    }
    let mut next = pool.clone();
    let n = next
        .multi_asset
        .as_mut()
        .and_then(|s| s.managed_v2.as_mut())
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    advance_context(n, execution_block, execution_timestamp);
    n.total_supply = transition.next.total_supply;
    n.virtual_supply = transition.virtual_supply_after;
    n.actual_supply = transition.actual_supply_after;
    n.pending_aum = Amount::zero();
    n.vault_bpt_actual_balance = vault_bpt_actual_balance;
    n.manager_bpt_balance = transition.next.manager_bpt_balance;
    n.protocol_collector_bpt_balance = transition.next.protocol_bpt_balance;
    n.last_collection_timestamp = transition.next.last_collection_timestamp;
    next.validate()?;
    Ok(CollectionTransition {
        next_pool: next,
        accrual: transition.accrual,
        execution_block,
        execution_timestamp,
    })
}
