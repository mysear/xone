//! Pure integer Orca Whirlpools cc683ec9 static SPL exact-input math.
use crate::Swap;
use num_bigint::BigUint;
use num_traits::Zero;
use xone_domain::{err, Result};
use xone_domain::{
    orca::OrcaWhirlpoolState,
    orca_math::{
        sqrt_price_from_tick, tick_from_sqrt_price, MAX_SQRT_PRICE_X64, MAX_TICK,
        MIN_SQRT_PRICE_X64, MIN_TICK,
    },
    Amount, Pool,
};

#[derive(Debug)]
pub(crate) struct SwapStep {
    pub amount_in: u64,
    pub amount_out: u64,
    pub next_price: u128,
    pub fee_amount: u64,
}

fn arithmetic() -> xone_domain::Error {
    err("ORCA_ARITHMETIC")
}
fn bounded(n: BigUint, bits: u64) -> Result<BigUint> {
    if n.bits() > bits {
        Err(arithmetic())
    } else {
        Ok(n)
    }
}
fn ceil_div(n: BigUint, d: BigUint, up: bool) -> Result<BigUint> {
    if d.is_zero() {
        return Err(arithmetic());
    }
    let remainder = &n % &d;
    Ok(n / d + BigUint::from(u8::from(up && !remainder.is_zero())))
}
fn as_u64(n: BigUint) -> Result<u64> {
    u64::try_from(n).map_err(|_| arithmetic())
}
fn as_u128(n: BigUint) -> Result<u128> {
    u128::try_from(n).map_err(|_| arithmetic())
}

/// None is the pinned source's AmountDeltaU64::ExceedsMax. A's checked
/// shift overflow is a hard error; B's u128 product overflow is ExceedsMax.
fn amount_delta(
    p: u128,
    target: u128,
    liquidity: u128,
    token_a: bool,
    up: bool,
) -> Result<Option<u64>> {
    let difference = p.abs_diff(target);
    if token_a {
        let numerator = bounded(
            (BigUint::from(liquidity) * BigUint::from(difference)) << 64usize,
            256,
        )?;
        let denominator = BigUint::from(p) * BigUint::from(target);
        let n = ceil_div(numerator, denominator, up)?;
        Ok(u64::try_from(n).ok())
    } else {
        let Some(product) = liquidity.checked_mul(difference) else {
            return Ok(None);
        };
        let result = (product >> 64) as u64;
        let round = up && product & u128::from(u64::MAX) != 0;
        Ok(if round {
            result.checked_add(1)
        } else {
            Some(result)
        })
    }
}

pub(crate) fn compute_swap(
    remaining: u64,
    fee: u32,
    liquidity: u128,
    price: u128,
    target: u128,
    left: bool,
) -> Result<SwapStep> {
    if fee > 60000 {
        return Err(err("ORCA_FEE_RATE"));
    }
    let initial = amount_delta(price, target, liquidity, left, true)?;
    let available = (u128::from(remaining) * (1_000_000 - u128::from(fee)) / 1_000_000) as u64;
    let next_price = if initial.is_some_and(|n| n <= available) {
        target
    } else if left {
        if available == 0 {
            price
        } else {
            // Exact original A formula: no alternative formula on U256 overflow.
            let numerator = bounded(
                (BigUint::from(liquidity) * BigUint::from(price)) << 64usize,
                256,
            )?;
            let denominator = bounded(
                (BigUint::from(liquidity) << 64usize)
                    + BigUint::from(price) * BigUint::from(available),
                256,
            )?;
            let next = as_u128(ceil_div(numerator, denominator, true)?)?;
            if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&next) {
                return Err(arithmetic());
            }
            next
        }
    } else {
        let delta = (u128::from(available) << 64)
            .checked_div(liquidity)
            .ok_or_else(arithmetic)?;
        price.checked_add(delta).ok_or_else(arithmetic)?
    };
    // Source computes the unfixed delta first, and requires an actual u64.
    let amount_out =
        amount_delta(price, next_price, liquidity, !left, false)?.ok_or_else(arithmetic)?;
    let at_target = next_price == target;
    let amount_in = if !at_target || initial.is_none() {
        amount_delta(price, next_price, liquidity, left, true)?.ok_or_else(arithmetic)?
    } else {
        initial.ok_or_else(arithmetic)?
    };
    let fee_amount = if !at_target {
        remaining.checked_sub(amount_in).ok_or_else(arithmetic)?
    } else {
        as_u64(ceil_div(
            BigUint::from(amount_in) * BigUint::from(fee),
            BigUint::from(1_000_000u32 - fee),
            true,
        )?)?
    };
    Ok(SwapStep {
        amount_in,
        amount_out,
        next_price,
        fee_amount,
    })
}

/// Original sparse builder selects at most three directional arrays from the
/// explicitly proven ascending accounts; then SwapTickSequence scans them.
/// Empty arrays are searched through; they do not create extra price/fee steps.
fn next_tick(
    state: &OrcaWhirlpoolState,
    order: &[usize],
    mut sequence_index: usize,
    mut tick: i32,
    left: bool,
) -> Result<(usize, i32)> {
    let spacing = state.tick_spacing as i32;
    let span = 88 * spacing;
    loop {
        let array = order
            .get(sequence_index)
            .and_then(|i| state.tick_arrays.get(*i))
            .ok_or_else(|| err("ORCA_ARRAY_PROOF_EXHAUSTED"))?;
        let start = array.start_tick_index;
        let shift = if left { 0 } else { spacing };
        if tick < start - shift || tick >= start + span - shift {
            return Err(err("ORCA_ARRAY_SEARCH_RANGE"));
        }
        let candidate = if left {
            array.ticks.iter().rev().find(|t| t.tick <= tick)
        } else {
            array.ticks.iter().find(|t| t.tick > tick)
        };
        if let Some(t) = candidate {
            return Ok((sequence_index, t.tick));
        }
        if left && start <= MIN_TICK {
            return Ok((sequence_index, MIN_TICK));
        }
        if !left && start + span > MAX_TICK {
            return Ok((sequence_index, MAX_TICK));
        }
        if sequence_index + 1 == order.len() {
            return Ok((sequence_index, if left { start } else { start + span - 1 }));
        }
        tick = if left { start - 1 } else { start + span - 1 };
        sequence_index += 1;
    }
}

/// Pinned cc683ec9 next_whirlpool_reward_infos: accrue once using pre-swap liquidity.
/// The original checked u128 multiplication deliberately suppresses overflow growth.
pub fn update_rewards(state: &mut OrcaWhirlpoolState) -> Result<()> {
    state.validate_reward_state()?;
    let elapsed = state
        .bank_unix_timestamp
        .checked_sub(state.reward_last_updated_timestamp)
        .ok_or_else(|| err("ORCA_REWARD_TIMESTAMP"))?;
    let liquidity = as_u128(state.liquidity.0.clone())?;
    if liquidity != 0 && elapsed != 0 {
        for r in &mut state.reward_infos {
            if !r.initialized() {
                continue;
            }
            let emissions = as_u128(r.emissions_per_second_x64.0.clone())?;
            let growth = as_u128(r.growth_global_x64.0.clone())?;
            let delta = u128::from(elapsed)
                .checked_mul(emissions)
                .map(|n| n / liquidity)
                .unwrap_or(0);
            r.growth_global_x64 = Amount(BigUint::from(growth.wrapping_add(delta)));
        }
    }
    state.reward_last_updated_timestamp = state.bank_unix_timestamp;
    Ok(())
}

/// Original tick_manager crosses only initialized reward slots, with u128 wrapping.
pub fn cross_reward_growths(
    rewards: &[xone_domain::orca::OrcaRewardInfo],
    outside: &mut [Amount],
) -> Result<()> {
    if rewards.len() != outside.len() {
        return Err(err("ORCA_REWARD_TICK_WIDTH"));
    }
    for (r, value) in rewards.iter().zip(outside) {
        if r.initialized() {
            *value = Amount(BigUint::from(
                as_u128(r.growth_global_x64.0.clone())?.wrapping_sub(as_u128(value.0.clone())?),
            ));
        }
    }
    Ok(())
}

pub fn quote_exact_in(pool: &Pool, input: &Amount, left: bool) -> Result<Swap> {
    if matches!(
        pool.protocol.as_str(),
        xone_domain::orca_adaptive_state::PROFILE
            | xone_domain::orca_adaptive_state::REWARDS_PROFILE
            | xone_domain::orca_adaptive_state::DYNAMIC_PROFILE
    ) {
        return crate::orca_adaptive_online::quote_exact_in(pool, input, left);
    }
    pool.validate()?;
    if !matches!(
        pool.protocol.as_str(),
        "orca-whirlpools-static" | "orca-whirlpools-rewards"
    ) {
        return Err(err("PROTOCOL_UNSUPPORTED"));
    }
    let original = pool
        .orca_whirlpool
        .as_ref()
        .ok_or_else(|| err("ORCA_STATE_REQUIRED"))?;
    original.validate(pool)?;
    let amount = u64::try_from(&input.0).map_err(|_| err("ORCA_U64_AMOUNT"))?;
    if amount == 0 {
        return Err(err("ZERO_INPUT"));
    }
    let mut price = as_u128(original.sqrt_price_x64.0.clone())?;
    let limit = if left {
        MIN_SQRT_PRICE_X64
    } else {
        MAX_SQRT_PRICE_X64
    };
    if (left && price <= limit) || (!left && price >= limit) {
        return Err(err("ORCA_PRICE_DIRECTION"));
    }
    let spacing = original.tick_spacing as i32;
    let span = 88 * spacing;
    let shift = if left { 0 } else { spacing };
    // The first B->A search array can be the next array before price reaches
    // that array: FixedTickArray::in_search_range shifts both ends by spacing.
    let first = original
        .tick_arrays
        .iter()
        .position(|a| {
            original.tick_current_index >= a.start_tick_index - shift
                && original.tick_current_index < a.start_tick_index + span - shift
        })
        .ok_or_else(|| err("ORCA_ARRAY_SEARCH_RANGE"))?;
    let order: Vec<usize> = if left {
        (0..=first).rev().take(3).collect()
    } else {
        (first..original.tick_arrays.len()).take(3).collect()
    };
    let mut next = pool.clone();
    let state = next
        .orca_whirlpool
        .as_mut()
        .ok_or_else(|| err("ORCA_STATE_REQUIRED"))?;
    let mut remaining = amount;
    let mut output = 0u64;
    update_rewards(state)?;
    let mut liquidity = as_u128(state.liquidity.0.clone())?;
    let mut tick = state.tick_current_index;
    let mut sequence_index = 0usize;
    let mut global = as_u128(if left {
        state.fee_growth_global0.0.clone()
    } else {
        state.fee_growth_global1.0.clone()
    })?;
    let other_global = as_u128(if left {
        state.fee_growth_global1.0.clone()
    } else {
        state.fee_growth_global0.0.clone()
    })?;
    let mut protocol_fee = 0u64;
    let mut fee_sum = 0u64;
    let mut steps = 0u32;
    while remaining > 0 && price != limit {
        if steps >= state.max_steps {
            return Err(err("ORCA_STEP_BUDGET"));
        }
        steps += 1;
        let (found_array, found_tick) = next_tick(state, &order, sequence_index, tick, left)?;
        let target = as_u128(sqrt_price_from_tick(found_tick)?)?;
        let step = compute_swap(remaining, state.fee_rate, liquidity, price, target, left)?;
        remaining = remaining
            .checked_sub(step.amount_in)
            .and_then(|r| r.checked_sub(step.fee_amount))
            .ok_or_else(arithmetic)?;
        output = output.checked_add(step.amount_out).ok_or_else(arithmetic)?;
        fee_sum = fee_sum
            .checked_add(step.fee_amount)
            .ok_or_else(arithmetic)?;
        let protocol_delta =
            (u128::from(step.fee_amount) * u128::from(state.protocol_fee_rate) / 10000) as u64;
        protocol_fee = protocol_fee.wrapping_add(protocol_delta);
        if liquidity > 0 {
            global = global
                .wrapping_add((u128::from(step.fee_amount - protocol_delta) << 64) / liquidity);
        }
        if step.next_price == target {
            let array = &mut state.tick_arrays[order[found_array]];
            if let Some(t) = array.ticks.iter_mut().find(|t| t.tick == found_tick) {
                let net = t
                    .liquidity_net
                    .parse::<i128>()
                    .map_err(|_| err("ORCA_LIQUIDITY_NET"))?;
                // Original release profile disables overflow checks, including
                // the unary negation in calculate_update. Keep that width.
                let delta = if left { net.wrapping_neg() } else { net };
                liquidity = if delta >= 0 {
                    liquidity.checked_add(delta as u128)
                } else {
                    liquidity.checked_sub(delta.unsigned_abs())
                }
                .ok_or_else(arithmetic)?;
                let (g0, g1) = if left {
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
            }
            let offset = (found_tick - array.start_tick_index).div_euclid(spacing);
            sequence_index = if (left && offset == 0) || (!left && offset == 87) {
                found_array + 1
            } else {
                found_array
            };
            tick = if left { found_tick - 1 } else { found_tick };
        } else if step.next_price != price {
            tick = tick_from_sqrt_price(&BigUint::from(step.next_price))?;
        }
        price = step.next_price;
    }
    // Original manager permits exact-input partial fills at a price limit.
    // This API promises the full input and must reject every partial result.
    if remaining != 0 {
        return Err(err("ORCA_PARTIAL_FILL"));
    }
    if output == 0 {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    state.sqrt_price_x64 = Amount(BigUint::from(price));
    state.tick_current_index = tick;
    state.liquidity = Amount(BigUint::from(liquidity));
    state.reward_last_updated_timestamp = state.bank_unix_timestamp;
    if left {
        state.protocol_fee_owed0 = state.protocol_fee_owed0.wrapping_add(protocol_fee);
        state.fee_growth_global0 = Amount(BigUint::from(global));
        state.raw_vault0 = state
            .raw_vault0
            .checked_add(amount)
            .ok_or_else(arithmetic)?;
        state.raw_vault1 = state
            .raw_vault1
            .checked_sub(output)
            .ok_or_else(arithmetic)?;
    } else {
        state.protocol_fee_owed1 = state.protocol_fee_owed1.wrapping_add(protocol_fee);
        state.fee_growth_global1 = Amount(BigUint::from(global));
        state.raw_vault1 = state
            .raw_vault1
            .checked_add(amount)
            .ok_or_else(arithmetic)?;
        state.raw_vault0 = state
            .raw_vault0
            .checked_sub(output)
            .ok_or_else(arithmetic)?;
    }
    next.validate()?;
    Ok(Swap {
        amount_out: Amount(BigUint::from(output)),
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}
