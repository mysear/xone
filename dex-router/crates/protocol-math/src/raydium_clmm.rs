//! Pure Raydium CLMM 6d7a0f82 Q64 exact-input/output math with token-selected fees.
#[path = "rayclmm_dynamic.rs"]
mod dynamic;
use crate::Swap;
use num_bigint::BigUint;
use num_traits::Zero;
use xone_domain::rayclmm_math::{
    sqrt_price_from_tick, tick_from_sqrt_price, MAX_SQRT_PRICE_X64, MIN_SQRT_PRICE_X64,
};
use xone_domain::{err, Amount, Pool, Result};
#[derive(Debug)]
pub struct SwapStep {
    pub amount_in: u64,
    pub amount_out: u64,
    pub next_price: u128,
    pub fee_amount: u64,
}
fn arithmetic() -> xone_domain::Error {
    err("RAYCLMM_ARITHMETIC")
}
fn bounded(n: BigUint, bits: u64) -> Result<BigUint> {
    if n.bits() > bits {
        Err(arithmetic())
    } else {
        Ok(n)
    }
}
fn as_u64(n: BigUint) -> Result<u64> {
    u64::try_from(n).map_err(|_| arithmetic())
}
fn as_u128(n: BigUint) -> Result<u128> {
    u128::try_from(n).map_err(|_| arithmetic())
}
fn div(n: BigUint, d: BigUint, up: bool) -> Result<BigUint> {
    if d.is_zero() {
        return Err(arithmetic());
    }
    let remainder = &n % &d;
    Ok(n / d + BigUint::from(u8::from(up && !remainder.is_zero())))
}

/// Source get_delta_amounts_for_swap checks the two U256 products, widens
/// mul_pow2_div to U512, then checks BOTH amounts against u64::MAX. None is
/// MaxTokenOverflow, the only target-side error that permits a partial step.
fn amounts(price: u128, target: u128, liquidity: u128, left: bool) -> Result<Option<(u64, u64)>> {
    let low = price.min(target);
    if low == 0 {
        return Err(arithmetic());
    }
    let product = bounded(BigUint::from(price) * BigUint::from(target), 256)?;
    let delta = bounded(
        BigUint::from(liquidity) * BigUint::from(price.abs_diff(target)),
        256,
    )?;
    let amount0 = bounded(div(&delta << 64usize, product, left)?, 256)?;
    let amount1 = if left {
        delta >> 64usize
    } else {
        bounded(delta + BigUint::from(u64::MAX), 256)? >> 64usize
    };
    let (Ok(a), Ok(b)) = (u64::try_from(amount0), u64::try_from(amount1)) else {
        return Ok(None);
    };
    Ok(Some(if left { (a, b) } else { (b, a) }))
}

/// Compute one exact-input step with the legacy input-side fee semantics.
pub fn step_exact_in(
    remaining: u64,
    fee: u32,
    liquidity: u128,
    price: u128,
    target: u128,
    left: bool,
) -> Result<SwapStep> {
    step_exact_in_with_fee(remaining, fee, liquidity, price, target, left, true)
}

// Retain the original step entry point used by the static-profile tests.
#[allow(dead_code)]
pub(crate) fn compute_swap(
    remaining: u64,
    fee: u32,
    liquidity: u128,
    price: u128,
    target: u128,
    left: bool,
) -> Result<SwapStep> {
    step_exact_in(remaining, fee, liquidity, price, target, left)
}

fn step_exact_in_with_fee(
    remaining: u64,
    fee: u32,
    liquidity: u128,
    price: u128,
    target: u128,
    left: bool,
    fee_on_input: bool,
) -> Result<SwapStep> {
    step_mode(
        remaining,
        fee,
        liquidity,
        price,
        target,
        left,
        FeeMode {
            exact_in: true,
            on_input: fee_on_input,
        },
    )
}
/// Original output-specified step, including input- or output-token fees.
pub fn step_exact_out_with_fee(
    remaining: u64,
    fee: u32,
    liquidity: u128,
    price: u128,
    target: u128,
    left: bool,
    fee_on_input: bool,
) -> Result<SwapStep> {
    step_mode(
        remaining,
        fee,
        liquidity,
        price,
        target,
        left,
        FeeMode {
            exact_in: false,
            on_input: fee_on_input,
        },
    )
}
#[derive(Clone, Copy)]
struct FeeMode {
    exact_in: bool,
    on_input: bool,
}
fn step_mode(
    remaining: u64,
    fee: u32,
    liquidity: u128,
    price: u128,
    target: u128,
    left: bool,
    mode: FeeMode,
) -> Result<SwapStep> {
    let FeeMode {
        exact_in,
        on_input: fee_on_input,
    } = mode;
    if fee >= 1_000_000
        || price == 0
        || target == 0
        || (left && target > price)
        || (!left && target < price)
    {
        return Err(arithmetic());
    }
    let available = if exact_in && fee_on_input {
        (u128::from(remaining) * u128::from(1_000_000 - fee) / 1_000_000) as u64
    } else if !exact_in && !fee_on_input {
        as_u64(div(
            BigUint::from(remaining) * BigUint::from(1_000_000u32),
            BigUint::from(1_000_000 - fee),
            true,
        )?)?
    } else {
        remaining
    };
    let full = amounts(price, target, liquidity, left)?;
    let (next_price, mut amount_in, mut amount_out) = match full {
        Some((a, b)) if available >= if exact_in { a } else { b } => (target, a, b),
        _ => {
            if liquidity == 0 || !(MIN_SQRT_PRICE_X64..MAX_SQRT_PRICE_X64).contains(&price) {
                return Err(arithmetic());
            }
            let next = if left == exact_in {
                let n = BigUint::from(liquidity) << 64usize;
                let product = bounded(BigUint::from(available) * BigUint::from(price), 256)?;
                let denominator = if exact_in {
                    bounded(&n + product, 256)?
                } else {
                    if n <= product {
                        return Err(arithmetic());
                    }
                    &n - product
                };
                let wide = bounded(div(n * BigUint::from(price), denominator, true)?, 256)?;
                as_u128(wide & BigUint::from(u128::MAX))?
            } else {
                let delta = as_u128(div(
                    BigUint::from(available) << 64usize,
                    BigUint::from(liquidity),
                    !exact_in,
                )?)?;
                if exact_in {
                    price.checked_add(delta)
                } else {
                    price.checked_sub(delta)
                }
                .ok_or_else(arithmetic)?
            };
            let (a, b) = amounts(price, next, liquidity, left)?.ok_or_else(arithmetic)?;
            (next, a, b)
        }
    };
    if (left && next_price < target) || (!left && next_price > target) {
        return Err(arithmetic());
    }
    let fee_amount = if exact_in {
        if !fee_on_input {
            let f = as_u64(div(
                BigUint::from(amount_out) * BigUint::from(fee),
                BigUint::from(1_000_000u32),
                true,
            )?)?;
            amount_out = amount_out.checked_sub(f).ok_or_else(arithmetic)?;
            if next_price != target {
                amount_in = remaining;
            }
            f
        } else if next_price != target {
            remaining.checked_sub(amount_in).ok_or_else(arithmetic)?
        } else {
            as_u64(div(
                BigUint::from(amount_in) * BigUint::from(fee),
                BigUint::from(1_000_000 - fee),
                true,
            )?)?
        }
    } else if fee_on_input {
        amount_out = amount_out.min(remaining);
        as_u64(div(
            BigUint::from(amount_in) * BigUint::from(fee),
            BigUint::from(1_000_000 - fee),
            true,
        )?)?
    } else {
        let f = as_u64(div(
            BigUint::from(amount_out) * BigUint::from(fee),
            BigUint::from(1_000_000u32),
            true,
        )?)?;
        let net = amount_out.checked_sub(f).ok_or_else(arithmetic)?;
        if net > remaining {
            let f = amount_out.checked_sub(remaining).ok_or_else(arithmetic)?;
            amount_out = remaining;
            f
        } else {
            amount_out = net;
            f
        }
    };
    Ok(SwapStep {
        amount_in,
        amount_out,
        next_price,
        fee_amount,
    })
}

/// Search the complete bitmap proof, then require the selected account in the
/// bounded proof. Set bits outside the proof are valid until traversal needs
/// one; a missing account never becomes an implicitly empty array.
fn selected_array(
    state: &xone_domain::raydium_clmm::RaydiumCLMMState,
    index: i32,
) -> Result<usize> {
    let span = 60 * state.tick_spacing as i32;
    state
        .tick_arrays
        .iter()
        .position(|a| a.start_tick_index == index * span)
        .ok_or_else(|| err("RAYCLMM_ARRAY_UNPROVEN"))
}
fn next_array(
    state: &xone_domain::raydium_clmm::RaydiumCLMMState,
    mut index: i32,
    left: bool,
) -> Result<usize> {
    let span = 60 * state.tick_spacing as i32;
    let min_index = xone_domain::raydium_clmm::MIN_TICK.div_euclid(span);
    let max_index = xone_domain::raydium_clmm::MAX_TICK.div_euclid(span);
    loop {
        index += if left { -1 } else { 1 };
        if state.bitmap_extension.is_some() {
            // The full extension proves empty arrays only inside the original
            // tick domain. In particular its lowest array starts below MIN_TICK.
            if !(min_index..=max_index).contains(&index) {
                return Err(err("RAYCLMM_INSUFFICIENT_LIQUIDITY"));
            }
        } else if !(-512..=511).contains(&index) {
            // Preserve the old profiles' missing-extension boundary failures.
            return Err(err(if state.tick_spacing >= 15 {
                "RAYCLMM_INSUFFICIENT_LIQUIDITY"
            } else {
                "RAYCLMM_BITMAP_EXTENSION_REQUIRED"
            }));
        }
        if state.array_initialized(index)? {
            return selected_array(state, index);
        }
    }
}

fn first_tick(
    state: &xone_domain::raydium_clmm::RaydiumCLMMState,
    array: usize,
    left: bool,
) -> Result<usize> {
    let ticks = &state.tick_arrays[array].ticks;
    if ticks.is_empty() {
        return Err(err("RAYCLMM_BITMAP_PROOF"));
    }
    Ok(if left { ticks.len() - 1 } else { 0 })
}
fn update_observation(state: &mut xone_domain::raydium_clmm::RaydiumCLMMState, old_tick: i32) {
    let timestamp = state.bank_unix_timestamp as u32;
    let index = state.observation_index as usize;
    if !state.observation_initialized {
        state.observation_initialized = true;
        state.observations[index].block_timestamp = timestamp;
        state.observations[index].tick_cumulative = 0;
    } else {
        let last = &state.observations[index];
        let elapsed = timestamp.saturating_sub(last.block_timestamp);
        if elapsed < 15 {
            return;
        }
        let cumulative = last
            .tick_cumulative
            .wrapping_add(i64::from(old_tick).saturating_mul(i64::from(elapsed)));
        let next = if index == 99 { 0 } else { index + 1 };
        state.observations[next].block_timestamp = timestamp;
        state.observations[next].tick_cumulative = cumulative;
        state.observation_index = next as u32;
    }
}

/// Pinned 6d7a0f82 PoolState::update_reward_infos, before the first swap step.
/// Emitted tokens round up; growth rounds down; exhausted emission freezes a slot.
pub fn update_rewards(state: &mut xone_domain::raydium_clmm::RaydiumCLMMState) -> Result<()> {
    state.validate_reward_state()?;
    let liquidity = as_u128(state.liquidity.0.clone())?;
    let now = state.bank_unix_timestamp;
    for r in &mut state.reward_infos {
        if !r.initialized() || r.reward_total_emitted == u64::MAX || now <= r.open_time {
            continue;
        }
        let latest = now.min(r.end_time);
        if liquidity != 0 {
            let elapsed = latest.saturating_sub(r.last_update_time);
            if elapsed == 0 {
                continue;
            }
            let product = BigUint::from(elapsed) * &r.emissions_per_second_x64.0;
            let q64 = BigUint::from(1u128 << 64);
            let emission = (&product + &q64 - BigUint::from(1u8)) / &q64;
            // Original mul_div_* returns None if the quotient exceeds U128.
            let emitted = u128::try_from(emission).unwrap_or(0);
            let mut delta = u128::try_from(&product / BigUint::from(liquidity)).unwrap_or(0);
            let remaining = u64::MAX - r.reward_total_emitted;
            if emitted <= u128::from(remaining) {
                r.reward_total_emitted += emitted as u64;
            } else {
                r.reward_total_emitted = u64::MAX;
                delta = (u128::from(remaining) << 64) / liquidity;
            }
            let growth = as_u128(r.reward_growth_global_x64.0.clone())?;
            r.reward_growth_global_x64 = Amount(BigUint::from(growth.wrapping_add(delta)));
        }
        r.last_update_time = latest;
        if latest >= r.open_time && latest < r.end_time {
            r.reward_state = 2;
        } else if latest == r.end_time {
            r.reward_state = 3;
        }
    }
    state.recent_epoch = state.bank_epoch;
    Ok(())
}

pub fn cross_reward_growths(
    rewards: &[xone_domain::raydium_clmm::RaydiumCLMMRewardInfo],
    outside: &mut [Amount],
) -> Result<()> {
    if rewards.len() != outside.len() {
        return Err(err("RAYCLMM_REWARD_TICK_WIDTH"));
    }
    for (r, value) in rewards.iter().zip(outside) {
        if r.initialized() {
            *value = Amount(BigUint::from(
                as_u128(r.reward_growth_global_x64.0.clone())?
                    .wrapping_sub(as_u128(value.0.clone())?),
            ));
        }
    }
    Ok(())
}

pub fn supports_profile(protocol: &str) -> bool {
    matches!(
        protocol,
        "raydium-clmm-static"
            | "raydium-clmm-rewards"
            | "raydium-clmm-token-fee"
            | "raydium-clmm-bitmap"
            | "raydium-clmm-custom-seed"
            | "raydium-clmm-dynamic"
    )
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, left: bool) -> Result<Swap> {
    quote_mode(pool, input, left, true)
}
pub fn quote_exact_out(pool: &Pool, output: &Amount, left: bool) -> Result<Swap> {
    if output.0.is_zero() {
        return Err(err("EXACT_OUT_ZERO_OUTPUT"));
    }
    let q = quote_mode(pool, output, left, false)?;
    if q.amount_out != *output {
        return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
    }
    Ok(q)
}
fn quote_mode(pool: &Pool, input: &Amount, left: bool, exact_in: bool) -> Result<Swap> {
    pool.validate()?;
    let amount = as_u64(input.0.clone())?;
    if amount == 0 {
        return Err(err("ZERO_INPUT"));
    }
    let mut next = pool.clone();
    let state = next
        .raydium_clmm
        .as_mut()
        .ok_or_else(|| err("RAYCLMM_STATE_REQUIRED"))?;
    if state.bank_unix_timestamp <= state.open_time {
        return Err(err("RAYCLMM_NOT_OPEN"));
    }
    update_rewards(state)?;
    let span = 60 * state.tick_spacing as i32;
    let old_tick = state.tick_current;
    let mut tick = old_tick;
    let mut price = as_u128(state.sqrt_price_x64.0.clone())?;
    let mut liquidity = as_u128(state.liquidity.0.clone())?;
    let fee_on_token0 = match state.collect_fee_on {
        0 => left,
        1 => true,
        2 => false,
        _ => return Err(err("RAYCLMM_FEE_MODE")),
    };
    let fee_on_input = fee_on_token0 == left;
    let mut global = as_u128(if fee_on_token0 {
        state.fee_growth_global0.0.clone()
    } else {
        state.fee_growth_global1.0.clone()
    })?;
    let other_global = as_u128(if fee_on_token0 {
        state.fee_growth_global1.0.clone()
    } else {
        state.fee_growth_global0.0.clone()
    })?;
    let mut contains_current = state.array_initialized(tick.div_euclid(span))?;
    let mut array = if contains_current {
        selected_array(state, tick.div_euclid(span))?
    } else {
        next_array(state, tick.div_euclid(span), left)?
    };
    let limit = if left {
        MIN_SQRT_PRICE_X64 + 1
    } else {
        MAX_SQRT_PRICE_X64 - 1
    };
    if (left && limit >= price) || (!left && limit <= price) {
        return Err(err("RAYCLMM_PRICE_LIMIT"));
    }
    let mut remaining = amount;
    let mut output = 0u64;
    let mut consumed_total = 0u64;
    let mut protocol = 0u64;
    let mut fund = 0u64;
    let mut lp = 0u64;
    let mut dynamic = state
        .dynamic_fee
        .clone()
        .map(|info| {
            dynamic::DynamicSwap::new(info, tick, state.tick_spacing, state.bank_unix_timestamp)
        })
        .transpose()?;
    let mut steps = 0u32;
    while remaining != 0 && price != limit {
        // Retain the static path's budget rejection before resolving an array.
        if steps >= state.max_steps {
            return Err(err("RAYCLMM_STEP_BUDGET"));
        }
        let a = &state.tick_arrays[array];
        let in_current = tick.div_euclid(span) * span == a.start_tick_index;
        let found = if in_current {
            if left {
                a.ticks.iter().rposition(|t| t.tick <= tick)
            } else {
                a.ticks.iter().position(|t| t.tick > tick)
            }
        } else {
            None
        };
        let tindex = if let Some(i) = found {
            i
        } else if !contains_current {
            contains_current = true;
            first_tick(state, array, left)?
        } else {
            array = next_array(state, a.start_tick_index / span, left)?;
            first_tick(state, array, left)?
        };
        let next_tick = state.tick_arrays[array].ticks[tindex].tick;
        let tick_price = as_u128(sqrt_price_from_tick(next_tick)?)?;
        if (left && (next_tick > tick || tick_price > price))
            || (!left && (next_tick <= tick || tick_price < price))
        {
            return Err(arithmetic());
        }
        let target = if left {
            tick_price.max(limit)
        } else {
            tick_price.min(limit)
        };
        // Source swap_internal retains the next initialized tick while its
        // inner loop visits every spacing group. Charge the actual iterations,
        // including a zero-price move at an exact left boundary.
        loop {
            if steps >= state.max_steps {
                return Err(err("RAYCLMM_STEP_BUDGET"));
            }
            steps += 1;
            let (fee, skipped, bounded_price, bounded_tick) = if let Some(d) = &mut dynamic {
                d.update_volatility()?;
                let fee = d.total_fee(state.trade_fee_rate)?;
                let (skipped, price, tick) = d.bounded_price(target, liquidity, left)?;
                (fee, skipped, price, tick)
            } else {
                (state.trade_fee_rate, true, target, None)
            };
            let step = if price == bounded_price {
                SwapStep {
                    amount_in: 0,
                    amount_out: 0,
                    next_price: bounded_price,
                    fee_amount: 0,
                }
            } else {
                let step_fn = if exact_in {
                    step_exact_in_with_fee
                } else {
                    step_exact_out_with_fee
                };
                step_fn(
                    remaining,
                    fee,
                    liquidity,
                    price,
                    bounded_price,
                    left,
                    fee_on_input,
                )?
            };
            let consumed = if fee_on_input {
                step.amount_in
                    .checked_add(step.fee_amount)
                    .ok_or_else(arithmetic)?
            } else {
                step.amount_in
            };
            consumed_total = consumed_total
                .checked_add(consumed)
                .ok_or_else(arithmetic)?;
            remaining = remaining
                .checked_sub(if exact_in { consumed } else { step.amount_out })
                .ok_or_else(arithmetic)?;
            output = output.checked_add(step.amount_out).ok_or_else(arithmetic)?;
            // Each step floors independently. Aggregating the fee first changes
            // the protocol/fund split and both crossed and global growth.
            let pd = (u128::from(step.fee_amount) * u128::from(state.protocol_fee_rate) / 1_000_000)
                as u64;
            let fd =
                (u128::from(step.fee_amount) * u128::from(state.fund_fee_rate) / 1_000_000) as u64;
            protocol = protocol.checked_add(pd).ok_or_else(arithmetic)?;
            fund = fund.checked_add(fd).ok_or_else(arithmetic)?;
            let lp_delta = step
                .fee_amount
                .checked_sub(pd)
                .and_then(|v| v.checked_sub(fd))
                .ok_or_else(arithmetic)?;
            if liquidity > 0 {
                let growth = (u128::from(lp_delta) << 64) / liquidity;
                global = global.wrapping_add(growth);
                lp = lp.checked_add(lp_delta).ok_or_else(arithmetic)?;
            } else {
                protocol = protocol.checked_add(lp_delta).ok_or_else(arithmetic)?;
            }
            if step.next_price == tick_price {
                let a = &mut state.tick_arrays[array];
                let t = &mut a.ticks[tindex];
                let net = t
                    .liquidity_net
                    .parse::<i128>()
                    .map_err(|_| err("RAYCLMM_LIQUIDITY_NET"))?;
                let delta = if left {
                    // Original workspace release builds enable overflow checks;
                    // the source's i128::neg traps at MIN, unlike fee-growth wrap.
                    net.checked_neg().ok_or_else(arithmetic)?
                } else {
                    net
                };
                liquidity = if delta >= 0 {
                    liquidity.checked_add(delta as u128)
                } else {
                    liquidity.checked_sub(delta.unsigned_abs())
                }
                .ok_or_else(arithmetic)?;
                let (g0, g1) = if fee_on_token0 {
                    (global, other_global)
                } else {
                    (other_global, global)
                };
                cross_reward_growths(&state.reward_infos, &mut t.reward_growths_outside)?;
                t.fee_growth_outside0 = Amount(BigUint::from(
                    g0.wrapping_sub(as_u128(t.fee_growth_outside0.0.clone())?),
                ));
                t.fee_growth_outside1 = Amount(BigUint::from(
                    g1.wrapping_sub(as_u128(t.fee_growth_outside1.0.clone())?),
                ));
                a.recent_epoch = state.bank_epoch;
                tick = if left { next_tick - 1 } else { next_tick };
            } else if step.next_price != price {
                tick = match bounded_tick {
                    Some(t) if step.next_price == bounded_price => t,
                    _ => tick_from_sqrt_price(&BigUint::from(step.next_price))?,
                };
            }
            price = step.next_price;
            if let Some(d) = &mut dynamic {
                d.advance(left, skipped, price, tick, next_tick, tick_price)?;
            }
            if remaining == 0 || price == target {
                break;
            }
        }
    }
    if remaining != 0 {
        return Err(err("RAYCLMM_PARTIAL_FILL"));
    }
    if output == 0 {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    // settle_amounts checks total fee as u64 even though it is not retained.
    lp.checked_add(protocol)
        .and_then(|v| v.checked_add(fund))
        .ok_or_else(arithmetic)?;
    if tick != old_tick {
        update_observation(state, old_tick);
    }
    state.dynamic_fee = dynamic.map(|d| d.info);
    state.tick_current = tick;
    state.sqrt_price_x64 = Amount(BigUint::from(price));
    state.liquidity = Amount(BigUint::from(liquidity));
    state.recent_epoch = state.bank_epoch;
    if fee_on_token0 {
        if lp > 0 {
            state.fee_growth_global0 = Amount(BigUint::from(global));
        }
        state.protocol_fees0 = state
            .protocol_fees0
            .checked_add(protocol)
            .ok_or_else(arithmetic)?;
        state.fund_fees0 = state.fund_fees0.checked_add(fund).ok_or_else(arithmetic)?;
    } else {
        if lp > 0 {
            state.fee_growth_global1 = Amount(BigUint::from(global));
        }
        state.protocol_fees1 = state
            .protocol_fees1
            .checked_add(protocol)
            .ok_or_else(arithmetic)?;
        state.fund_fees1 = state.fund_fees1.checked_add(fund).ok_or_else(arithmetic)?;
    }
    if left {
        state.raw_vault0 = state
            .raw_vault0
            .checked_add(consumed_total)
            .ok_or_else(arithmetic)?;
        state.raw_vault1 = state
            .raw_vault1
            .checked_sub(output)
            .ok_or_else(arithmetic)?;
    } else {
        state.raw_vault1 = state
            .raw_vault1
            .checked_add(consumed_total)
            .ok_or_else(arithmetic)?;
        state.raw_vault0 = state
            .raw_vault0
            .checked_sub(output)
            .ok_or_else(arithmetic)?;
    }
    next.validate()?;
    Ok(Swap {
        amount_out: Amount(BigUint::from(output)),
        consumed_in: Amount(BigUint::from(consumed_total)),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}

#[cfg(test)]
#[path = "rayclmm_bitmap_tests.rs"]
mod bitmap_tests;
