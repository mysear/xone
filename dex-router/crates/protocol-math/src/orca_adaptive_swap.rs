//! Bounded, classic-SPL Orca adaptive exact-input swap composition.
//!
//! Fixed arrays are complete mathematical snapshots: absent bitmap slots are
//! uninitialized. Account authentication/decoding is the caller's responsibility.
//! The rewards entrypoint models initialized rewards; the dynamic entrypoint also admits mixed arrays. Explicit price limits can
//! return a native partial fill; missing array coverage and step exhaustion fail.
use crate::orca_adaptive::{AdaptiveFeeContext, AdaptiveFeeInfo, FeeRateManager};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use xone_domain::{
    err,
    orca_math::{
        sqrt_price_from_tick, tick_from_sqrt_price, MAX_SQRT_PRICE_X64, MAX_TICK,
        MIN_SQRT_PRICE_X64, MIN_TICK,
    },
    Result,
};

pub const CLASSIC_SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
macro_rules! decimal {
    ($name:ident,$ty:ty) => {
        mod $name {
            use serde::{Deserialize, Deserializer, Serializer};
            pub fn serialize<S: Serializer>(v: &$ty, s: S) -> std::result::Result<S::Ok, S::Error> {
                s.serialize_str(&v.to_string())
            }
            pub fn deserialize<'de, D: Deserializer<'de>>(
                d: D,
            ) -> std::result::Result<$ty, D::Error> {
                let s = String::deserialize(d)?;
                let n = s.parse::<$ty>().map_err(serde::de::Error::custom)?;
                if n.to_string() != s {
                    return Err(serde::de::Error::custom("noncanonical integer"));
                }
                Ok(n)
            }
        }
    };
}
decimal!(decimal_u128, u128);
decimal!(decimal_i128, i128);
mod reward_growths {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(a: &[u128; 3], s: S) -> std::result::Result<S::Ok, S::Error> {
        a.map(|n| n.to_string()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<[u128; 3], D::Error> {
        let strings = <[String; 3]>::deserialize(d)?;
        let mut values = [0; 3];
        for (out, s) in values.iter_mut().zip(strings) {
            *out = s.parse().map_err(serde::de::Error::custom)?;
            if out.to_string() != s {
                return Err(serde::de::Error::custom("noncanonical integer"));
            }
        }
        Ok(values)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveSwapReward {
    pub mint: [u8; 32],
    pub vault: [u8; 32],
    pub extension: [u8; 32],
    #[serde(with = "decimal_u128")]
    pub emissions_per_second_x64: u128,
    #[serde(with = "decimal_u128")]
    pub growth_global_x64: u128,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveSwapOracle {
    pub whirlpool: [u8; 32],
    pub trade_enable_timestamp: u64,
    pub adaptive_fee_info: AdaptiveFeeInfo,
    pub reserved: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveSwapTick {
    pub tick: i32,
    #[serde(with = "decimal_u128")]
    pub liquidity_gross: u128,
    #[serde(with = "decimal_i128")]
    pub liquidity_net: i128,
    #[serde(with = "decimal_u128")]
    pub fee_growth_outside_a: u128,
    #[serde(with = "decimal_u128")]
    pub fee_growth_outside_b: u128,
    #[serde(with = "reward_growths")]
    pub reward_growths_outside: [u128; 3],
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveSwapTickArray {
    pub whirlpool: [u8; 32],
    pub start_tick_index: i32,
    pub layout: String,
    #[serde(with = "decimal_u128")]
    pub initialized_bitmap: u128,
    pub ticks: Vec<AdaptiveSwapTick>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveSwapState {
    pub whirlpool: [u8; 32],
    pub token_program: String,
    pub tick_spacing: u16,
    pub fee_rate: u16,
    pub protocol_fee_rate: u16,
    #[serde(with = "decimal_u128")]
    pub sqrt_price_x64: u128,
    pub tick_current_index: i32,
    #[serde(with = "decimal_u128")]
    pub liquidity: u128,
    #[serde(with = "decimal_u128")]
    pub fee_growth_global_a: u128,
    #[serde(with = "decimal_u128")]
    pub fee_growth_global_b: u128,
    pub protocol_fee_owed_a: u64,
    pub protocol_fee_owed_b: u64,
    pub reward_last_updated_timestamp: u64,
    pub reward_infos: [AdaptiveSwapReward; 3],
    pub oracle: AdaptiveSwapOracle,
    pub tick_arrays: Vec<AdaptiveSwapTickArray>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveExactInParams {
    pub amount: u64,
    pub a_to_b: bool,
    /// Zero is the original no-explicit-price-limit sentinel.
    #[serde(with = "decimal_u128")]
    pub sqrt_price_limit: u128,
    pub timestamp: u64,
    pub max_steps: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct AdaptiveSwapStep {
    pub group: i32,
    pub fee_rate: u32,
    pub skip: bool,
    #[serde(with = "decimal_u128")]
    pub target: u128,
    #[serde(with = "decimal_u128")]
    pub next_price: u128,
    #[serde(with = "decimal_u128")]
    pub liquidity_before: u128,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee_amount: u64,
    pub crossed_initialized_tick: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct AdaptiveSwapResult {
    pub amount_in: u64,
    pub amount_out: u64,
    pub amount_remaining: u64,
    pub lp_fee: u64,
    pub protocol_fee: u64,
    pub next_state: AdaptiveSwapState,
    pub steps: Vec<AdaptiveSwapStep>,
}
fn price(t: i32) -> Result<u128> {
    u128::try_from(sqrt_price_from_tick(t)?).map_err(|_| err("ORCA_ADAPTIVE_SWAP_ARITHMETIC"))
}
fn arithmetic() -> xone_domain::Error {
    err("ORCA_ADAPTIVE_SWAP_ARITHMETIC")
}
fn validate(
    state: &AdaptiveSwapState,
    p: AdaptiveExactInParams,
    allow_rewards: bool,
    allow_dynamic: bool,
) -> Result<()> {
    if state.token_program != CLASSIC_SPL_TOKEN_PROGRAM {
        return Err(err("ORCA_ADAPTIVE_SWAP_TOKEN_PROGRAM_UNSUPPORTED"));
    }
    if state.whirlpool == [0; 32]
        || state.oracle.whirlpool != state.whirlpool
        || state.oracle.reserved.len() != 128
    {
        return Err(err("ORCA_ADAPTIVE_SWAP_ORACLE_BINDING"));
    }
    if !allow_rewards
        && state.reward_infos.iter().any(|r| {
            r.mint != [0; 32]
                || r.vault != [0; 32]
                || r.emissions_per_second_x64 != 0
                || r.growth_global_x64 != 0
        })
    {
        return Err(err("ORCA_ADAPTIVE_SWAP_REWARDS_UNSUPPORTED"));
    }
    if allow_rewards
        && state.reward_infos.iter().enumerate().any(|(i, r)| {
            i > 0 && r.extension != [0; 32] || r.mint != [0; 32] && r.vault == [0; 32]
        })
    {
        return Err(err("ORCA_ADAPTIVE_SWAP_REWARD_CONTROL"));
    }
    if p.timestamp < state.reward_last_updated_timestamp {
        return Err(err("ORCA_ADAPTIVE_SWAP_REWARD_TIMESTAMP"));
    }
    if state.tick_spacing == 0
        || state.fee_rate > 60_000
        || state.protocol_fee_rate > 2500
        || !(1..=8192).contains(&p.max_steps)
    {
        return Err(err("ORCA_ADAPTIVE_SWAP_INVALID_CONTEXT"));
    }
    if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&state.sqrt_price_x64) {
        return Err(err("ORCA_ADAPTIVE_SWAP_PRICE"));
    }
    let pt = tick_from_sqrt_price(&BigUint::from(state.sqrt_price_x64))?;
    if state.tick_current_index != pt
        && !(state.tick_current_index == pt - 1 && state.sqrt_price_x64 == price(pt)?)
    {
        return Err(err("ORCA_ADAPTIVE_SWAP_TICK_PRICE"));
    }
    if state.tick_arrays.is_empty()
        || state.tick_arrays.len() > 3
        || (allow_rewards || allow_dynamic) && state.tick_arrays.len() != 3
    {
        return Err(err("ORCA_ADAPTIVE_SWAP_ARRAY_PROOF"));
    }
    let spacing = i32::from(state.tick_spacing);
    let span = 88 * spacing;
    for (i, a) in state.tick_arrays.iter().enumerate() {
        if a.layout != "fixed" && !(allow_dynamic && a.layout == "dynamic") {
            return Err(err("ORCA_ADAPTIVE_SWAP_ARRAY_LAYOUT_UNSUPPORTED"));
        }
        let valid_start = a.start_tick_index == MIN_TICK.div_euclid(span) * span
            || (a.start_tick_index >= MIN_TICK
                && a.start_tick_index <= MAX_TICK
                && a.start_tick_index % span == 0);
        if a.whirlpool != state.whirlpool
            || !valid_start
            || a.initialized_bitmap >> 88 != 0
            || i > 0 && a.start_tick_index != state.tick_arrays[i - 1].start_tick_index + span
        {
            return Err(err("ORCA_ADAPTIVE_SWAP_ARRAY_PROOF"));
        }
        let mut bitmap = 0;
        let mut prev = None;
        for t in &a.ticks {
            if !(MIN_TICK..=MAX_TICK).contains(&t.tick)
                || t.tick < a.start_tick_index
                || t.tick >= a.start_tick_index + span
                || t.tick % spacing != 0
                || prev.is_some_and(|v| t.tick <= v)
                || t.liquidity_gross == 0
                || t.liquidity_net.unsigned_abs() > t.liquidity_gross
                || !allow_rewards && t.reward_growths_outside != [0; 3]
            {
                return Err(err("ORCA_ADAPTIVE_SWAP_TICK_PROOF"));
            }
            bitmap |= 1u128 << ((t.tick - a.start_tick_index) / spacing);
            prev = Some(t.tick);
        }
        if bitmap != a.initialized_bitmap {
            return Err(err("ORCA_ADAPTIVE_SWAP_BITMAP"));
        }
    }
    Ok(())
}
fn next_tick(
    state: &AdaptiveSwapState,
    order: &[usize],
    mut seq: usize,
    mut tick: i32,
    left: bool,
) -> Result<(usize, i32)> {
    let spacing = i32::from(state.tick_spacing);
    let span = 88 * spacing;
    loop {
        let a = order
            .get(seq)
            .and_then(|i| state.tick_arrays.get(*i))
            .ok_or_else(|| err("ORCA_ADAPTIVE_SWAP_PROOF_EXHAUSTED"))?;
        let shift = if left { 0 } else { spacing };
        if tick < a.start_tick_index - shift || tick >= a.start_tick_index + span - shift {
            return Err(err("ORCA_ADAPTIVE_SWAP_ARRAY_SEARCH"));
        }
        let candidate = if left {
            a.ticks.iter().rev().find(|t| t.tick <= tick)
        } else {
            a.ticks.iter().find(|t| t.tick > tick)
        };
        if let Some(t) = candidate {
            return Ok((seq, t.tick));
        }
        if left && a.start_tick_index <= MIN_TICK {
            return Ok((seq, MIN_TICK));
        }
        if !left && a.start_tick_index + span > MAX_TICK {
            return Ok((seq, MAX_TICK));
        }
        if seq + 1 == order.len() {
            return Ok((
                seq,
                if left {
                    a.start_tick_index
                } else {
                    a.start_tick_index + span - 1
                },
            ));
        }
        tick = if left {
            a.start_tick_index - 1
        } else {
            a.start_tick_index + span - 1
        };
        seq += 1;
    }
}
/// Exact-in composition of the original adaptive swap loop over a proved fixed-array window.
/// The input is immutable; every error discards all tentative pool/tick/Oracle changes.
pub fn quote_exact_in(
    state: &AdaptiveSwapState,
    p: AdaptiveExactInParams,
) -> Result<AdaptiveSwapResult> {
    quote_internal(state, p, false, false, true)
}
/// Classic SPL exact-input swap with pinned cc683ec9 reward accrual and crossing.
pub fn quote_exact_in_with_rewards(
    state: &AdaptiveSwapState,
    p: AdaptiveExactInParams,
) -> Result<AdaptiveSwapResult> {
    quote_internal(state, p, true, false, true)
}
/// Classic SPL adaptive swap over exactly three complete fixed/dynamic arrays.
pub fn quote_exact_in_dynamic(
    state: &AdaptiveSwapState,
    p: AdaptiveExactInParams,
) -> Result<AdaptiveSwapResult> {
    quote_internal(state, p, true, true, true)
}
/// The fields match the input parameters; `amount` is an output target here.
pub type AdaptiveExactOutParams = AdaptiveExactInParams;
/// Exact-output amount; amount_remaining is the unfilled output target.
pub fn quote_exact_out(
    state: &AdaptiveSwapState,
    p: AdaptiveExactOutParams,
) -> Result<AdaptiveSwapResult> {
    quote_internal(state, p, false, false, false)
}
pub fn quote_exact_out_with_rewards(
    state: &AdaptiveSwapState,
    p: AdaptiveExactOutParams,
) -> Result<AdaptiveSwapResult> {
    quote_internal(state, p, true, false, false)
}
pub fn quote_exact_out_dynamic(
    state: &AdaptiveSwapState,
    p: AdaptiveExactOutParams,
) -> Result<AdaptiveSwapResult> {
    quote_internal(state, p, true, true, false)
}
fn quote_internal(
    state: &AdaptiveSwapState,
    p: AdaptiveExactInParams,
    allow_rewards: bool,
    allow_dynamic: bool,
    exact_in: bool,
) -> Result<AdaptiveSwapResult> {
    validate(state, p, allow_rewards, allow_dynamic)?;
    let left = p.a_to_b;
    let limit = if p.sqrt_price_limit == 0 {
        if left {
            MIN_SQRT_PRICE_X64
        } else {
            MAX_SQRT_PRICE_X64
        }
    } else {
        p.sqrt_price_limit
    };
    if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&limit)
        || (left && limit >= state.sqrt_price_x64)
        || (!left && limit <= state.sqrt_price_x64)
    {
        return Err(err("ORCA_ADAPTIVE_SWAP_PRICE_LIMIT"));
    }
    if p.amount == 0 {
        return Err(err("ORCA_ADAPTIVE_SWAP_ZERO_AMOUNT"));
    }
    let mut manager = FeeRateManager::begin(
        AdaptiveFeeContext {
            a_to_b: left,
            current_tick_index: state.tick_current_index,
            timestamp: p.timestamp,
            static_fee_rate: state.fee_rate,
            tick_spacing: state.tick_spacing,
            trade_enable_timestamp: state.oracle.trade_enable_timestamp,
        },
        state.oracle.adaptive_fee_info,
    )?;
    let span = 88 * i32::from(state.tick_spacing);
    let shift = if left {
        0
    } else {
        i32::from(state.tick_spacing)
    };
    let first = state
        .tick_arrays
        .iter()
        .position(|a| {
            state.tick_current_index >= a.start_tick_index - shift
                && state.tick_current_index < a.start_tick_index + span - shift
        })
        .ok_or_else(|| err("ORCA_ADAPTIVE_SWAP_ARRAY_SEARCH"))?;
    let order: Vec<usize> = if left {
        (0..=first).rev().collect()
    } else {
        (first..state.tick_arrays.len()).collect()
    };
    let mut next = state.clone();
    // Original whirlpool_manager accrues once using the initial liquidity. A
    // checked u128 product overflow halts this slot's increment, before division.
    if allow_rewards && state.liquidity != 0 && p.timestamp != state.reward_last_updated_timestamp {
        let elapsed = u128::from(p.timestamp - state.reward_last_updated_timestamp);
        for reward in &mut next.reward_infos {
            if reward.mint == [0; 32] {
                continue;
            }
            let delta = elapsed
                .checked_mul(reward.emissions_per_second_x64)
                .map(|n| n / state.liquidity)
                .unwrap_or(0);
            reward.growth_global_x64 = reward.growth_global_x64.wrapping_add(delta);
        }
    }
    let next_rewards = next.reward_infos.clone();
    let mut remaining = p.amount;
    let mut output = 0u64;
    let mut consumed = 0u64;
    let mut fees = 0u64;
    let mut protocol = 0u64;
    let mut global = if left {
        state.fee_growth_global_a
    } else {
        state.fee_growth_global_b
    };
    let other = if left {
        state.fee_growth_global_b
    } else {
        state.fee_growth_global_a
    };
    let mut current = state.sqrt_price_x64;
    let mut tick = state.tick_current_index;
    let mut liquidity = state.liquidity;
    let mut seq = 0;
    let mut steps = Vec::new();
    while remaining > 0 && current != limit {
        let (array_index, found_tick) = next_tick(&next, &order, seq, tick, left)?;
        let tick_price = price(found_tick)?;
        let target = if left {
            limit.max(tick_price)
        } else {
            limit.min(tick_price)
        };
        loop {
            if steps.len() >= p.max_steps as usize {
                return Err(err("ORCA_ADAPTIVE_SWAP_STEP_BUDGET"));
            }
            manager.update_volatility_accumulator()?;
            let fee_rate = manager.get_total_fee_rate();
            let group = manager.tick_group_index();
            let (bounded, skip) = manager.get_bounded_sqrt_price_target(target, liquidity)?;
            let available = if exact_in {
                (u128::from(remaining) * u128::from(1_000_000 - fee_rate) / 1_000_000) as u64
            } else {
                remaining
            };
            // Reuse the existing original Q64 price/amount math without its static fee guard.
            // Zero-fee exact-in(available) has exactly the original adaptive step's price and deltas.
            let s = crate::orca::compute_swap_mode(
                available, 0, liquidity, current, bounded, exact_in, left,
            )?;
            let fee = if exact_in && s.next_price != bounded {
                remaining.checked_sub(s.amount_in).ok_or_else(arithmetic)?
            } else {
                let n = u128::from(s.amount_in) * u128::from(fee_rate);
                u64::try_from(n.div_ceil(u128::from(1_000_000 - fee_rate)))
                    .map_err(|_| arithmetic())?
            };
            let paid = s.amount_in.checked_add(fee).ok_or_else(arithmetic)?;
            consumed = consumed.checked_add(paid).ok_or_else(arithmetic)?;
            remaining = remaining
                .checked_sub(if exact_in { paid } else { s.amount_out })
                .ok_or_else(arithmetic)?;
            output = output.checked_add(s.amount_out).ok_or_else(arithmetic)?;
            fees = fees.checked_add(fee).ok_or_else(arithmetic)?;
            let protocol_delta =
                (u128::from(fee) * u128::from(state.protocol_fee_rate) / 10_000) as u64;
            protocol = protocol.wrapping_add(protocol_delta);
            if liquidity > 0 {
                global = global.wrapping_add((u128::from(fee - protocol_delta) << 64) / liquidity);
            }
            let mut crossed = false;
            let liquidity_before = liquidity;
            // Only an actual next-tick price crosses a tick; a group/limit boundary does not.
            if s.next_price == tick_price {
                let array = &mut next.tick_arrays[order[array_index]];
                if let Some(t) = array.ticks.iter_mut().find(|t| t.tick == found_tick) {
                    let delta = if left {
                        t.liquidity_net.wrapping_neg()
                    } else {
                        t.liquidity_net
                    };
                    liquidity = if delta >= 0 {
                        liquidity.checked_add(delta as u128)
                    } else {
                        liquidity.checked_sub(delta.unsigned_abs())
                    }
                    .ok_or_else(arithmetic)?;
                    let (a, b) = if left {
                        (global, other)
                    } else {
                        (other, global)
                    };
                    t.fee_growth_outside_a = a.wrapping_sub(t.fee_growth_outside_a);
                    t.fee_growth_outside_b = b.wrapping_sub(t.fee_growth_outside_b);
                    if allow_rewards {
                        for (i, reward) in next_rewards.iter().enumerate() {
                            if reward.mint != [0; 32] {
                                t.reward_growths_outside[i] = reward
                                    .growth_global_x64
                                    .wrapping_sub(t.reward_growths_outside[i]);
                            }
                        }
                    }
                    crossed = true;
                }
                let offset =
                    (found_tick - array.start_tick_index).div_euclid(i32::from(state.tick_spacing));
                seq = if (left && offset == 0) || (!left && offset == 87) {
                    array_index + 1
                } else {
                    array_index
                };
                tick = if left { found_tick - 1 } else { found_tick };
            } else if s.next_price != current {
                tick = tick_from_sqrt_price(&BigUint::from(s.next_price))?;
            }
            current = s.next_price;
            if skip {
                manager.advance_tick_group_after_skip(current, tick_price, found_tick)?;
            } else {
                manager.advance_tick_group()?;
            }
            steps.push(AdaptiveSwapStep {
                group,
                fee_rate,
                skip,
                target: bounded,
                next_price: current,
                liquidity_before,
                amount_in: s.amount_in,
                amount_out: s.amount_out,
                fee_amount: fee,
                crossed_initialized_tick: crossed,
            });
            if remaining == 0 || current == target {
                break;
            }
        }
    }
    if !exact_in && remaining > 0 && p.sqrt_price_limit == 0 {
        return Err(err("ORCA_ADAPTIVE_PARTIAL_FILL_UNSUPPORTED"));
    }
    manager.update_major_swap_timestamp(state.sqrt_price_x64, current)?;
    next.oracle.adaptive_fee_info = manager.finish();
    next.sqrt_price_x64 = current;
    next.tick_current_index = tick;
    next.liquidity = liquidity;
    // Original pool post-update advances the timestamp even when no rewards accrue.
    next.reward_last_updated_timestamp = p.timestamp;
    if left {
        next.fee_growth_global_a = global;
        next.protocol_fee_owed_a = next.protocol_fee_owed_a.wrapping_add(protocol);
    } else {
        next.fee_growth_global_b = global;
        next.protocol_fee_owed_b = next.protocol_fee_owed_b.wrapping_add(protocol);
    }
    Ok(AdaptiveSwapResult {
        amount_in: consumed,
        amount_out: output,
        amount_remaining: remaining,
        lp_fee: fees.checked_sub(protocol).ok_or_else(arithmetic)?,
        protocol_fee: protocol,
        next_state: next,
        steps,
    })
}
