//! Authenticated online state to the pinned adaptive swap kernel. Online quotes
//! require complete consumption; account/range failure never becomes a partial quote.
use crate::{
    orca_adaptive::{AdaptiveFeeConstants, AdaptiveFeeInfo, AdaptiveFeeVariables},
    orca_adaptive_swap::*,
    Swap,
};
use xone_domain::{
    err,
    orca::{OrcaTick, OrcaWhirlpoolState},
    Amount, Pool, Result,
};
fn reward_key(key: &str) -> Result<[u8; 32]> {
    if key == "11111111111111111111111111111111" {
        Ok([0; 32])
    } else {
        xone_domain::raydium::validate_pubkey(key)
    }
}
fn u128_amount(a: &Amount) -> Result<u128> {
    u128::try_from(&a.0).map_err(|_| err("ORCA_U128_WIDTH"))
}
fn oracle(s: &OrcaWhirlpoolState) -> Result<AdaptiveSwapOracle> {
    let a = s
        .adaptive
        .as_ref()
        .ok_or_else(|| err("ORCA_ADAPTIVE_STATE_REQUIRED"))?;
    Ok(AdaptiveSwapOracle {
        whirlpool: xone_domain::raydium::validate_pubkey(&a.whirlpool)?,
        trade_enable_timestamp: a.trade_enable_timestamp,
        adaptive_fee_info: AdaptiveFeeInfo {
            constants: AdaptiveFeeConstants {
                filter_period: a.filter_period as u16,
                decay_period: a.decay_period as u16,
                reduction_factor: a.reduction_factor as u16,
                adaptive_fee_control_factor: a.adaptive_fee_control_factor,
                max_volatility_accumulator: a.max_volatility_accumulator,
                tick_group_size: a.tick_group_size as u16,
                major_swap_threshold_ticks: a.major_swap_threshold_ticks as u16,
                reserved: a
                    .constants_reserved
                    .clone()
                    .try_into()
                    .map_err(|_| err("ORCA_ADAPTIVE_RESERVED_WIDTH"))?,
            },
            variables: AdaptiveFeeVariables {
                last_reference_update_timestamp: a.last_reference_update_timestamp,
                last_major_swap_timestamp: a.last_major_swap_timestamp,
                volatility_reference: a.volatility_reference,
                tick_group_index_reference: a.tick_group_index_reference,
                volatility_accumulator: a.volatility_accumulator,
                reserved: a
                    .variables_reserved
                    .clone()
                    .try_into()
                    .map_err(|_| err("ORCA_ADAPTIVE_RESERVED_WIDTH"))?,
            },
        },
        reserved: a.reserved.clone(),
    })
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, left: bool) -> Result<Swap> {
    quote_mode(pool, input, left, true)
}
pub fn quote_exact_out(pool: &Pool, output: &Amount, left: bool) -> Result<Swap> {
    quote_mode(pool, output, left, false)
}
fn quote_mode(pool: &Pool, input: &Amount, left: bool, exact_in: bool) -> Result<Swap> {
    pool.validate()?;
    let with_dynamic = pool.protocol == xone_domain::orca_adaptive_state::DYNAMIC_PROFILE;
    let with_rewards = pool.protocol == xone_domain::orca_adaptive_state::REWARDS_PROFILE;
    if pool.protocol != xone_domain::orca_adaptive_state::PROFILE && !with_rewards && !with_dynamic
    {
        return Err(err("ORCA_ADAPTIVE_PROFILE_MISMATCH"));
    }
    let s = pool
        .orca_whirlpool
        .as_ref()
        .ok_or_else(|| err("ORCA_STATE_REQUIRED"))?;
    let amount = u64::try_from(&input.0).map_err(|_| err("ORCA_U64_AMOUNT"))?;
    if amount == 0 {
        return Err(err("ZERO_INPUT"));
    }
    let whirlpool = xone_domain::raydium::validate_pubkey(&pool.pool_id)?;
    let rewards = s
        .reward_infos
        .iter()
        .map(|r| {
            Ok(AdaptiveSwapReward {
                mint: reward_key(&r.mint)?,
                vault: reward_key(&r.vault)?,
                extension: r
                    .extension
                    .clone()
                    .try_into()
                    .map_err(|_| err("ORCA_REWARD_CONTROL_UNSUPPORTED"))?,
                emissions_per_second_x64: u128_amount(&r.emissions_per_second_x64)?,
                growth_global_x64: u128_amount(&r.growth_global_x64)?,
            })
        })
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .map_err(|_| err("ORCA_REWARD_COUNT"))?;
    let state = AdaptiveSwapState {
        whirlpool,
        token_program: CLASSIC_SPL_TOKEN_PROGRAM.into(),
        tick_spacing: s.tick_spacing as u16,
        fee_rate: s.fee_rate as u16,
        protocol_fee_rate: s.protocol_fee_rate as u16,
        sqrt_price_x64: u128_amount(&s.sqrt_price_x64)?,
        tick_current_index: s.tick_current_index,
        liquidity: u128_amount(&s.liquidity)?,
        fee_growth_global_a: u128_amount(&s.fee_growth_global0)?,
        fee_growth_global_b: u128_amount(&s.fee_growth_global1)?,
        protocol_fee_owed_a: s.protocol_fee_owed0,
        protocol_fee_owed_b: s.protocol_fee_owed1,
        reward_last_updated_timestamp: s.reward_last_updated_timestamp,
        reward_infos: rewards,
        oracle: oracle(s)?,
        tick_arrays: s
            .tick_arrays
            .iter()
            .map(|a| {
                Ok(AdaptiveSwapTickArray {
                    whirlpool,
                    start_tick_index: a.start_tick_index,
                    layout: a.layout.clone(),
                    initialized_bitmap: u128_amount(&a.initialized_bitmap)?,
                    ticks: a
                        .ticks
                        .iter()
                        .map(|t| {
                            Ok(AdaptiveSwapTick {
                                tick: t.tick,
                                liquidity_gross: u128_amount(&t.liquidity_gross)?,
                                liquidity_net: xone_domain::v3::parse_liquidity_net(
                                    &t.liquidity_net,
                                )?,
                                fee_growth_outside_a: u128_amount(&t.fee_growth_outside0)?,
                                fee_growth_outside_b: u128_amount(&t.fee_growth_outside1)?,
                                reward_growths_outside: t
                                    .reward_growths_outside
                                    .iter()
                                    .map(u128_amount)
                                    .collect::<Result<Vec<_>>>()?
                                    .try_into()
                                    .map_err(|_| err("ORCA_REWARD_TICK_WIDTH"))?,
                            })
                        })
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<_>>()?,
    };
    let quote = if !exact_in {
        if with_dynamic {
            crate::orca_adaptive_swap::quote_exact_out_dynamic
        } else if with_rewards {
            crate::orca_adaptive_swap::quote_exact_out_with_rewards
        } else {
            crate::orca_adaptive_swap::quote_exact_out
        }
    } else if with_dynamic {
        crate::orca_adaptive_swap::quote_exact_in_dynamic
    } else if with_rewards {
        crate::orca_adaptive_swap::quote_exact_in_with_rewards
    } else {
        crate::orca_adaptive_swap::quote_exact_in
    };
    let q = quote(
        &state,
        AdaptiveExactInParams {
            amount,
            a_to_b: left,
            sqrt_price_limit: 0,
            timestamp: s.bank_unix_timestamp,
            max_steps: s.max_steps,
        },
    )?;
    if q.amount_remaining != 0 || (if exact_in { q.amount_in } else { q.amount_out }) != amount {
        return Err(err("ORCA_ADAPTIVE_PARTIAL_FILL_UNSUPPORTED"));
    }
    if q.amount_out == 0 {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_pool = pool.clone();
    let next = next_pool
        .orca_whirlpool
        .as_mut()
        .ok_or_else(|| err("ORCA_STATE_REQUIRED"))?;
    if left {
        next.raw_vault0 = next
            .raw_vault0
            .checked_add(q.amount_in)
            .ok_or_else(|| err("ORCA_VAULT_OVERFLOW"))?;
        next.raw_vault1 = next
            .raw_vault1
            .checked_sub(q.amount_out)
            .ok_or_else(|| err("ORCA_INSUFFICIENT_VAULT"))?;
    } else {
        next.raw_vault1 = next
            .raw_vault1
            .checked_add(q.amount_in)
            .ok_or_else(|| err("ORCA_VAULT_OVERFLOW"))?;
        next.raw_vault0 = next
            .raw_vault0
            .checked_sub(q.amount_out)
            .ok_or_else(|| err("ORCA_INSUFFICIENT_VAULT"))?;
    }
    let ns = q.next_state;
    next.sqrt_price_x64 = Amount::checked(ns.sqrt_price_x64.into())?;
    next.tick_current_index = ns.tick_current_index;
    next.liquidity = Amount::checked(ns.liquidity.into())?;
    next.fee_growth_global0 = Amount::checked(ns.fee_growth_global_a.into())?;
    next.fee_growth_global1 = Amount::checked(ns.fee_growth_global_b.into())?;
    next.protocol_fee_owed0 = ns.protocol_fee_owed_a;
    next.protocol_fee_owed1 = ns.protocol_fee_owed_b;
    next.reward_last_updated_timestamp = ns.reward_last_updated_timestamp;
    for (reward, native) in next.reward_infos.iter_mut().zip(&ns.reward_infos) {
        reward.emissions_per_second_x64 = Amount::checked(native.emissions_per_second_x64.into())?;
        reward.growth_global_x64 = Amount::checked(native.growth_global_x64.into())?;
    }
    for (array, native) in next.tick_arrays.iter_mut().zip(ns.tick_arrays) {
        array.initialized_bitmap = Amount::checked(native.initialized_bitmap.into())?;
        array.ticks = native
            .ticks
            .into_iter()
            .map(|t| {
                Ok(OrcaTick {
                    tick: t.tick,
                    liquidity_gross: Amount::checked(t.liquidity_gross.into())?,
                    liquidity_net: t.liquidity_net.to_string(),
                    fee_growth_outside0: Amount::checked(t.fee_growth_outside_a.into())?,
                    fee_growth_outside1: Amount::checked(t.fee_growth_outside_b.into())?,
                    reward_growths_outside: t
                        .reward_growths_outside
                        .into_iter()
                        .map(|v| Amount::checked(v.into()))
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<_>>()?;
    }
    let a = next
        .adaptive
        .as_mut()
        .ok_or_else(|| err("ORCA_ADAPTIVE_STATE_REQUIRED"))?;
    let v = ns.oracle.adaptive_fee_info.variables;
    a.last_reference_update_timestamp = v.last_reference_update_timestamp;
    a.last_major_swap_timestamp = v.last_major_swap_timestamp;
    a.volatility_reference = v.volatility_reference;
    a.tick_group_index_reference = v.tick_group_index_reference;
    a.volatility_accumulator = v.volatility_accumulator;
    a.variables_reserved = v.reserved.to_vec();
    next_pool.validate()?;
    Ok(Swap {
        amount_out: Amount::checked(q.amount_out.into())?,
        consumed_in: Amount::checked(q.amount_in.into())?,
        remaining_in: Amount::zero(),
        next_pool,
    })
}
