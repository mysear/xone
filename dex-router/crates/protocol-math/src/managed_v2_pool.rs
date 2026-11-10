//! Full pricing-state adapter for the published ManagedPool V2 AUM profile.
use crate::{
    managed_v2_aum::{collect_aum, view_aum, AumAccrual, AumCollectionInput},
    managed_v2_swap::{quote_regular, quote_scheduled, WeightedInputs},
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
    if pool.protocol != PROTOCOL
        && pool.protocol != xone_domain::managed_v2_schedule::PROTOCOL
        && pool.protocol != xone_domain::managed_v2_bpt_state::PROTOCOL
        && pool.protocol != xone_domain::managed_v2_circuit_state::PROTOCOL
    {
        return Err(err("MANAGED_V2_PROFILE"));
    }
    crate::managed_v2_breaker::validate_pool_view(pool)?;
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
/// Project a validated current view to a monotonic computation clock. Source H,
/// raw schedules and every registered/recipient ledger retain their identity.
pub fn reclock(pool: &Pool, execution_block: u64, execution_timestamp: u64) -> Result<Pool> {
    let m = state(pool)?;
    validate_source_view(m)?;
    clock(m, execution_block, execution_timestamp)?;
    let future = view_aum(
        &m.virtual_supply,
        execution_timestamp,
        m.last_collection_timestamp,
        &effective_fee(m),
    )?;
    let scheduled = if m.schedule.is_some() {
        Some(
            m.schedule
                .as_ref()
                .ok_or_else(|| err("MANAGED_V2_SCHEDULE_REQUIRED"))?
                .at(execution_timestamp)?,
        )
    } else {
        None
    };
    let mut next = pool.clone();
    let s = next
        .multi_asset
        .as_mut()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let m = s
        .managed_v2
        .as_mut()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    if let Some(view) = scheduled {
        s.weights = view.weights;
        for (asset, scale) in s.assets.iter_mut().zip(view.scaling) {
            asset.scaling = scale;
        }
        next.fee_numerator = view
            .swap_fee
            .0
            .to_u64_digits()
            .first()
            .copied()
            .unwrap_or(0);
    }
    m.pending_aum = future.pending_bpt;
    m.actual_supply = future.actual_supply;
    advance_context(m, execution_block, execution_timestamp);
    crate::managed_v2_breaker::refresh_pool_view(&mut next)?;
    next.validate()?;
    Ok(next)
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
    let current = reclock(pool, execution_block, execution_timestamp)?;
    let pool = &current;
    let m = state(pool)?;
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
    let inputs = WeightedInputs {
        cash: &cash,
        managed: &managed,
        weights: &weights,
        scales: &scales,
        fee: &fee,
    };
    let q = if m.circuit_breakers.is_some() {
        let raw = m
            .schedule
            .as_ref()
            .ok_or_else(|| err("MANAGED_V2_SCHEDULE_REQUIRED"))?;
        let view = raw.at(execution_timestamp)?;
        let calculation = crate::managed_v2_swap::calculate_scheduled(
            inputs,
            raw,
            execution_timestamp,
            &view,
            &input.0,
            i,
            j,
        )?;
        crate::managed_v2_breaker::check_regular(
            pool,
            i,
            j,
            &calculation.gross_input_scaled,
            &calculation.scaled_amount_out,
        )?;
        calculation.settle()?
    } else if m.schedule.is_some() {
        // state(pool) checks raw token identities against the ordered assets.
        let raw = m
            .schedule
            .as_ref()
            .ok_or_else(|| err("MANAGED_V2_SCHEDULE_REQUIRED"))?;
        let view = raw.at(execution_timestamp)?;
        quote_scheduled(inputs, raw, execution_timestamp, &view, &input.0, i, j)?
    } else {
        quote_regular(inputs, &input.0, i, j)?
    };
    // ERC20 transfers affect the Vault's global wallet, independently of this
    // pool's registered cash/managed ledger. Input includes the full swap fee.
    let wallet_in = add_wallet_balance(&m.token_info[i].vault_actual_balance, &input.0)?;
    if m.token_info[j].vault_actual_balance.0 < q.amount_out {
        return Err(err("MANAGED_V2_INSUFFICIENT_VAULT_WALLET"));
    }
    let wallet_out = Amount(&m.token_info[j].vault_actual_balance.0 - &q.amount_out);
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
    let current = reclock(pool, execution_block, execution_timestamp)?;
    let pool = &current;
    let m = state(pool)?;
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

/// Explicit Vault funds context. The BPT allowlist applies to the sender of
/// a join, independently of its recipient; exit swaps do not test membership.
#[derive(Clone, Copy, Debug)]
pub struct BptExecution<'a> {
    pub sender: &'a str,
    pub recipient: &'a str,
    pub block_number: u64,
    pub timestamp: u64,
}
fn subtract_balance(balance: &Amount, delta: &BigUint) -> Result<Amount> {
    if &balance.0 < delta {
        return Err(err("MANAGED_V2_BPT_INSUFFICIENT_BALANCE"));
    }
    Ok(Amount(&balance.0 - delta))
}
/// One standard ERC20 BPT movement, reflected once in each observed wallet
/// even when manager, collector or Vault denote the same actual account.
fn transfer_bpt_wallets(
    m: &mut BalancerManagedV2State,
    vault: &str,
    sender: &str,
    recipient: &str,
    amount: &BigUint,
) -> Result<()> {
    for (who, balance) in [
        (vault, &mut m.vault_bpt_actual_balance),
        (m.manager.as_str(), &mut m.manager_bpt_balance),
        (
            m.protocol_collector.as_str(),
            &mut m.protocol_collector_bpt_balance,
        ),
    ] {
        if sender == recipient {
            if who == sender && balance.0 < *amount {
                return Err(err("MANAGED_V2_BPT_INSUFFICIENT_BALANCE"));
            }
        } else if who == sender {
            *balance = subtract_balance(balance, amount)?;
        } else if who == recipient {
            *balance = add_wallet_balance(balance, amount)?;
        }
    }
    Ok(())
}
/// Complete GIVEN_IN BPT transition at a monotonic execution clock. AUM is
/// minted before pricing; swap BPT itself moves through the Vault's preminted
/// registered cash. The retained source header always remains the original H.
pub fn quote_bpt_exact_in_at(
    pool: &Pool,
    input: &Amount,
    token_in: &str,
    token_out: &str,
    execution: BptExecution<'_>,
) -> Result<Swap> {
    use crate::managed_v2_bpt::{quote_exit_exact_in, quote_join_exact_in, BptSwapInputs};
    if pool.protocol != xone_domain::managed_v2_bpt_state::PROTOCOL
        && pool.protocol != xone_domain::managed_v2_circuit_state::PROTOCOL
    {
        return Err(err("MANAGED_V2_BPT_PROFILE"));
    }
    for address in [execution.sender, execution.recipient] {
        xone_domain::protocols::validate_address(address)?;
        if address == "0x0000000000000000000000000000000000000000" {
            return Err(err("MANAGED_V2_BPT_ZERO_FUNDS_ADDRESS"));
        }
    }
    let current = reclock(pool, execution.block_number, execution.timestamp)?;
    // Callback pricing and every circuit condition share this exact execution
    // view. Its actualSupply preview already equals post-collection supply;
    // the original source H remains retained inside the derived pool.
    let pool = &current;
    let m = state(pool)?;
    let b = m
        .bpt
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_BPT_STATE_REQUIRED"))?;
    if !m.swaps_enabled || !b.join_exit_enabled {
        return Err(err("MANAGED_V2_BPT_SWAPS_OR_JOIN_EXIT_DISABLED"));
    }
    if m.paused || m.vault_paused {
        return Err(err("MANAGED_V2_PAUSED"));
    }
    let join = token_out == pool.pool_id && token_in != pool.pool_id;
    let exit = token_in == pool.pool_id && token_out != pool.pool_id;
    if !join && !exit {
        return Err(err("MANAGED_V2_BPT_PAIR"));
    }
    if join && m.allowlist_enabled && !b.member_allowed(execution.sender)? {
        return Err(err("MANAGED_V2_BPT_ADDRESS_NOT_ALLOWLISTED"));
    }
    // collect_at retains source identity and uses the same exact mint/split
    // kernel as explicit collection. A later rejection discards this clone.
    let mut next = collect_at(&current, execution.block_number, execution.timestamp)?.next_pool;
    let s = next
        .multi_asset
        .as_mut()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let m = s
        .managed_v2
        .as_mut()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let underlying = if join { token_in } else { token_out };
    let i = s
        .assets
        .iter()
        .position(|a| a.token == underlying)
        .ok_or_else(|| err("MANAGED_V2_BPT_UNDERLYING_TOKEN"))?;
    let fee = BigUint::from(next.fee_numerator);
    let inputs = BptSwapInputs {
        cash: &m.token_info[i].cash.0,
        managed: &m.token_info[i].managed.0,
        scaling: &s.assets[i].scaling.0,
        weight: &s.weights[i].0,
        fee: &fee,
        actual_supply: &m.actual_supply.0,
    };
    let output = if m.circuit_breakers.is_some() {
        if join {
            let calculation = crate::managed_v2_bpt::calculate_join_exact_in(inputs, &input.0)?;
            crate::managed_v2_breaker::check_bpt(
                pool,
                i,
                &input.0,
                &calculation.amount_out,
                &calculation.gross_input_scaled,
                true,
            )?;
            calculation.settle()?
        } else {
            let calculation = crate::managed_v2_bpt::calculate_exit_exact_in(inputs, &input.0)?;
            crate::managed_v2_breaker::check_bpt(
                pool,
                i,
                &input.0,
                &calculation.amount_out,
                &calculation.scaled_amount_out,
                false,
            )?;
            calculation.settle()?
        }
    } else if join {
        quote_join_exact_in(inputs, &input.0)?
    } else {
        quote_exit_exact_in(inputs, &input.0)?
    };
    if join {
        m.vault_bpt_cash = subtract_balance(&m.vault_bpt_cash, &output)?;
        m.token_info[i].cash = add_wallet_balance(&m.token_info[i].cash, &input.0)?;
        if execution.sender != s.vault {
            m.token_info[i].vault_actual_balance =
                add_wallet_balance(&m.token_info[i].vault_actual_balance, &input.0)?;
        }
        transfer_bpt_wallets(m, &s.vault, &s.vault, execution.recipient, &output)?;
    } else {
        m.vault_bpt_cash = add_wallet_balance(&m.vault_bpt_cash, &input.0)?;
        if m.vault_bpt_cash.0.bits() > 112 {
            return Err(err("MANAGED_V2_BPT_VAULT_UINT112"));
        }
        m.token_info[i].cash = subtract_balance(&m.token_info[i].cash, &output)?;
        if execution.recipient != s.vault {
            m.token_info[i].vault_actual_balance =
                subtract_balance(&m.token_info[i].vault_actual_balance, &output)?;
        }
        transfer_bpt_wallets(m, &s.vault, execution.sender, &s.vault, &input.0)?;
    }
    m.virtual_supply = subtract_balance(&m.total_supply, &m.vault_bpt_cash.0)?;
    m.actual_supply = m.virtual_supply.clone();
    m.pending_aum = Amount::zero();
    let block = execution.block_number & u64::from(u32::MAX);
    m.token_info[i].last_change_block = block;
    m.bpt
        .as_mut()
        .ok_or_else(|| err("MANAGED_V2_BPT_STATE_REQUIRED"))?
        .bpt_last_change_block = block;
    s.assets[i].balance = Amount(&m.token_info[i].cash.0 + &m.token_info[i].managed.0);
    next.reserve0 = s.assets[0].balance.clone();
    next.reserve1 = s.assets[1].balance.clone();
    next.validate()?;
    Ok(Swap {
        amount_out: Amount(output),
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}
