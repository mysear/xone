//! Fixed-version Slipstream curve, fee/gauge/reward ledger and Oracle updates.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{
    err,
    slipstream_context::{policy, signed56, SlipstreamState},
    Amount, Pool, Result,
};
fn mask(bits: usize) -> BigUint {
    (BigUint::one() << bits) - 1u8
}
fn add(a: &BigUint, b: &BigUint, bits: usize) -> BigUint {
    (a + b) & mask(bits)
}
fn sub(a: &BigUint, b: &BigUint, bits: usize) -> BigUint {
    ((a + (BigUint::one() << bits)) - b) & mask(bits)
}
fn ceil(n: BigUint, d: &BigUint) -> BigUint {
    (n + d - 1u8) / d
}
fn adjust(value: &BigUint, net: i128, zero: bool) -> Result<BigUint> {
    let magnitude = BigUint::from(net.unsigned_abs());
    let result = if (net < 0) ^ zero {
        if value < &magnitude {
            return Err(err("SLIPSTREAM_STAKED_LIQUIDITY_RANGE"));
        }
        value - &magnitude
    } else {
        value + &magnitude
    };
    if result.bits() > 128 {
        return Err(err("SLIPSTREAM_STAKED_LIQUIDITY_RANGE"));
    }
    Ok(result)
}
fn update_reward(next: &mut SlipstreamState, before: &SlipstreamState) {
    let time = before.timestamp as u32;
    let delta = sub(
        &BigUint::from(time),
        &BigUint::from(before.last_updated),
        256,
    );
    if delta.is_zero() {
        return;
    }
    let reward = ((&before.reward_rate.0 * delta) & mask(256)).min(before.reward_reserve.0.clone());
    next.reward_reserve = Amount(&before.reward_reserve.0 - &reward);
    if before.staked_liquidity.0.is_zero() {
        next.rollover = Amount(add(&before.rollover.0, &reward, 256));
    }
    // FullMath's output-width failure is checked by the caller before assignment.
    else {
        next.reward_growth_global = Amount(add(
            &before.reward_growth_global.0,
            &((&reward << 128usize) / &before.staked_liquidity.0),
            256,
        ));
    }
    next.last_updated = time;
}
fn curve_context(parent: &Pool, before: &SlipstreamState, origin: &str) -> Result<(Pool, u32)> {
    before.validate_pool(parent)?;
    let origin = origin.to_ascii_lowercase();
    xone_domain::protocols::validate_address(&origin)?;
    if origin == xone_domain::singleton::ZERO_ADDRESS {
        return Err(err("SLIPSTREAM_ORIGIN_ADDRESS"));
    }
    let fee = policy::swap_fee(before, &origin)?;
    let unstaked_fee = policy::unstaked_fee(before)?;
    let mut thin = parent.clone();
    thin.slipstream_context = None;
    thin.v3 = Some(before.core.clone());
    thin.protocol = "uniswap-v3".into();
    thin.adapter_version = "uniswap-v3/core-1".into();
    thin.fee_numerator = u64::from(fee);
    thin.fee_denominator = 1000000;
    Ok((thin, unstaked_fee))
}
pub fn quote_state(
    parent: &Pool,
    before: &SlipstreamState,
    input: &Amount,
    zero: bool,
    origin: &str,
) -> Result<(SlipstreamState, Amount)> {
    let (thin, unstaked_fee) = curve_context(parent, before, origin)?;
    let (q, traces) = crate::v3::quote_exact_in_with_trace(&thin, input, zero)?;
    settle_state(parent, before, input, zero, unstaked_fee, q, traces)
}
fn settle_state(
    parent: &Pool,
    before: &SlipstreamState,
    input: &Amount,
    zero: bool,
    unstaked_fee: u32,
    q: crate::Swap,
    traces: Vec<crate::v3::V3StepTrace>,
) -> Result<(SlipstreamState, Amount)> {
    let raw_output = if zero {
        &before.raw_balance1.0
    } else {
        &before.raw_balance0.0
    };
    if raw_output < &q.amount_out.0 {
        return Err(err("SLIPSTREAM_RAW_BALANCE"));
    }
    let next_core = q
        .next_pool
        .v3
        .as_ref()
        .ok_or_else(|| err("SLIPSTREAM_CORE_REQUIRED"))?;
    let mut next = before.clone();
    let mut staked = before.staked_liquidity.0.clone();
    let mut global = if zero {
        before.fee_growth_global0.0.clone()
    } else {
        before.fee_growth_global1.0.clone()
    };
    let mut gauge = BigUint::zero();
    let scale = BigUint::one() << 128usize;
    let mut reward_updated = false;
    let latest = policy::observe_single(before, 0)?;
    for step in traces {
        if !step.liquidity.is_zero() {
            if staked > step.liquidity {
                return Err(err("SLIPSTREAM_STAKED_LIQUIDITY_RANGE"));
            }
            let (fee_lp, fee_gauge) = if staked == step.liquidity {
                (BigUint::zero(), step.fee_amount.clone())
            } else {
                let base = ceil(&step.fee_amount * &staked, &step.liquidity);
                let unstaked = &step.fee_amount - &base;
                let extra = ceil(&unstaked * unstaked_fee, &BigUint::from(1000000u32));
                (&unstaked - &extra, base + extra)
            };
            if staked != step.liquidity {
                let growth = fee_lp * &scale / (&step.liquidity - &staked);
                if growth.bits() > 256 {
                    return Err(err("SLIPSTREAM_FEE_GROWTH_OVERFLOW"));
                }
                global = add(&global, &growth, 256);
            }
            gauge = add(&gauge, &(fee_gauge & mask(128)), 128);
        }
        if let Some(tick) = step.crossed_tick {
            if !reward_updated {
                let delta = sub(
                    &BigUint::from(before.timestamp as u32),
                    &BigUint::from(before.last_updated),
                    256,
                );
                let reward = ((&before.reward_rate.0 * delta) & mask(256))
                    .min(before.reward_reserve.0.clone());
                if !before.staked_liquidity.0.is_zero()
                    && ((&reward << 128usize) / &before.staked_liquidity.0).bits() > 256
                {
                    return Err(err("SLIPSTREAM_REWARD_GROWTH_OVERFLOW"));
                }
                update_reward(&mut next, before);
                reward_updated = true;
            }
            let i = next
                .ticks
                .binary_search_by_key(&tick, |t| t.tick)
                .map_err(|_| err("SLIPSTREAM_FULL_TICK_SET"))?;
            let reward = next.reward_growth_global.0.clone();
            let fg0 = if zero {
                global.clone()
            } else {
                before.fee_growth_global0.0.clone()
            };
            let fg1 = if zero {
                before.fee_growth_global1.0.clone()
            } else {
                global.clone()
            };
            let info = &mut next.ticks[i];
            info.fee_growth_outside0 = Amount(sub(&fg0, &info.fee_growth_outside0.0, 256));
            info.fee_growth_outside1 = Amount(sub(&fg1, &info.fee_growth_outside1.0, 256));
            info.reward_growth_outside = Amount(sub(&reward, &info.reward_growth_outside.0, 256));
            info.tick_cumulative_outside = policy::wrapping_tick_cumulative(
                i128::from(latest.0) - i128::from(signed56(&info.tick_cumulative_outside)?),
            )
            .to_string();
            info.seconds_per_liquidity_outside_x128 = Amount(sub(
                &latest.1,
                &info.seconds_per_liquidity_outside_x128.0,
                160,
            ));
            info.seconds_outside = (before.timestamp as u32).wrapping_sub(info.seconds_outside);
            staked = adjust(
                &staked,
                xone_domain::v3::parse_liquidity_net(&info.staked_liquidity_net)?,
                zero,
            )?;
            let liquidity = adjust(
                &step.liquidity,
                xone_domain::v3::parse_liquidity_net(&info.liquidity_net)?,
                zero,
            )?;
            if staked > liquidity {
                return Err(err("SLIPSTREAM_STAKED_LIQUIDITY_RANGE"));
            }
        }
    }
    if next_core.tick != before.core.tick {
        let last = &before.observations[before.observation_index as usize];
        if last.block_timestamp != before.timestamp as u32 {
            let cardinality = if before.observation_cardinality_next
                > before.observation_cardinality
                && before.observation_index == before.observation_cardinality - 1
            {
                before.observation_cardinality_next
            } else {
                before.observation_cardinality
            };
            let index = (before.observation_index + 1) % cardinality;
            next.observations[index as usize] = policy::transform(
                last,
                before.timestamp as u32,
                before.core.tick,
                &before.core.liquidity,
            )?;
            next.observation_index = index;
            next.observation_cardinality = cardinality;
        }
    }
    next.core = next_core.clone();
    next.staked_liquidity = Amount::checked(staked)?;
    if zero {
        next.fee_growth_global0 = Amount(global);
        next.gauge_fees0 = Amount(add(&before.gauge_fees0.0, &gauge, 128));
        next.raw_balance0 = Amount::checked(&before.raw_balance0.0 + &input.0)?;
        next.raw_balance1 = Amount::checked(&before.raw_balance1.0 - &q.amount_out.0)?;
    } else {
        next.fee_growth_global1 = Amount(global);
        next.gauge_fees1 = Amount(add(&before.gauge_fees1.0, &gauge, 128));
        next.raw_balance1 = Amount::checked(&before.raw_balance1.0 + &input.0)?;
        next.raw_balance0 = Amount::checked(&before.raw_balance0.0 - &q.amount_out.0)?;
    }
    let views = next
        .origins
        .iter()
        .map(|o| {
            Ok((
                policy::swap_fee(&next, &o.origin)?,
                policy::unstaked_fee(&next)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    for (origin, (fee, unstaked)) in next.origins.iter_mut().zip(views) {
        origin.view_swap_fee = fee;
        origin.view_unstaked_fee = unstaked;
    }
    next.view_swap_fee = policy::swap_fee(&next, &next.view_origin)?;
    next.view_unstaked_fee = policy::unstaked_fee(&next)?;
    next.derived = true;
    next.validate_pool(parent)?;
    Ok((next, q.amount_out))
}

pub fn quote_exact_in(
    pool: &Pool,
    input: &Amount,
    zero: bool,
    origin: &str,
) -> Result<crate::Swap> {
    let state = pool
        .slipstream_context
        .as_ref()
        .ok_or_else(|| err("SLIPSTREAM_CONTEXT_REQUIRED"))?;
    let (next, amount_out) = quote_state(pool, state, input, zero, origin)?;
    let mut next_pool = pool.clone();
    next_pool.slipstream_context = Some(next);
    Ok(crate::Swap {
        amount_out,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool,
    })
}

/// Negative amountSpecified with the same caller-origin fee policy and full ledger.
pub fn quote_exact_out(
    pool: &Pool,
    output: &Amount,
    zero: bool,
    origin: &str,
) -> Result<crate::Swap> {
    let before = pool
        .slipstream_context
        .as_ref()
        .ok_or_else(|| err("SLIPSTREAM_CONTEXT_REQUIRED"))?;
    let (thin, unstaked_fee) = curve_context(pool, before, origin)?;
    let (q, traces) = crate::v3::quote_exact_out_with_trace(&thin, output, zero)?;
    if q.amount_out != *output || q.remaining_in != Amount::zero() {
        return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
    }
    let input = q.consumed_in.clone();
    let (next, amount_out) = settle_state(pool, before, &input, zero, unstaked_fee, q, traces)?;
    let mut next_pool = pool.clone();
    next_pool.slipstream_context = Some(next);
    Ok(crate::Swap {
        amount_out,
        consumed_in: input,
        remaining_in: Amount::zero(),
        next_pool,
    })
}
