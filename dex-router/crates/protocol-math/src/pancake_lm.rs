//! Original Pancake V3 fees, LM reward accounting and Oracle on the V3 curve.
use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};
use xone_domain::{
    err,
    pancake_lm::{signed56, PancakeLmState, PancakeObservation},
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
fn signed_wrap(n: i128) -> i64 {
    let n = n & ((1i128 << 56) - 1);
    (if n & (1i128 << 55) != 0 {
        n - (1i128 << 56)
    } else {
        n
    }) as i64
}
fn transform(
    last: &PancakeObservation,
    time: u32,
    tick: i32,
    liquidity: &Amount,
) -> Result<PancakeObservation> {
    let dt = time.wrapping_sub(last.block_timestamp);
    Ok(PancakeObservation {
        block_timestamp: time,
        tick_cumulative: signed_wrap(
            i128::from(signed56(&last.tick_cumulative)?) + i128::from(tick) * i128::from(dt),
        )
        .to_string(),
        seconds_per_liquidity_cumulative_x128: Amount(add(
            &last.seconds_per_liquidity_cumulative_x128.0,
            &((BigUint::from(dt) << 128usize) / liquidity.0.clone().max(BigUint::one())),
            160,
        )),
        initialized: true,
    })
}
fn adjust(value: &BigUint, net: i128, zero: bool) -> Result<BigUint> {
    let amount = BigUint::from(net.unsigned_abs());
    let next = if (net < 0) ^ zero {
        if value < &amount {
            return Err(err("PANCAKE_LM_LIQUIDITY_UNDERFLOW"));
        }
        value - amount
    } else {
        value + amount
    };
    if next.bits() > 128 {
        return Err(err("PANCAKE_LM_LIQUIDITY_OVERFLOW"));
    }
    Ok(next)
}
fn reward(next: &mut PancakeLmState) -> Result<()> {
    let time = next.timestamp as u32;
    let last = next.lm.last_reward_timestamp;
    // Solidity compares the uint32 values directly, before any subtraction.
    if time <= last {
        return Ok(());
    }
    if !next.lm.liquidity.0.is_zero() {
        let end = (&next.master_chef.view_end_time.0 & mask(32))
            .to_u32()
            .ok_or_else(|| err("PANCAKE_LM_REWARD_END_WIDTH"))?;
        let duration = if end > time {
            time - last
        } else {
            end.saturating_sub(last)
        };
        if duration != 0 {
            let scaled = (&next.master_chef.view_reward_per_second.0 << 128usize)
                / BigUint::from(1_000_000_000_000u64);
            if scaled.bits() > 256 {
                return Err(err("PANCAKE_LM_REWARD_FULLMATH_WIDTH"));
            }
            let growth = BigUint::from(duration) * scaled / &next.lm.liquidity.0;
            if growth.bits() > 256 {
                return Err(err("PANCAKE_LM_REWARD_GROWTH_WIDTH"));
            }
            next.lm.reward_growth_global =
                Amount(add(&next.lm.reward_growth_global.0, &growth, 256));
        }
    }
    next.lm.last_reward_timestamp = time;
    Ok(())
}
pub fn quote_state(
    parent: &Pool,
    before: &PancakeLmState,
    input: &Amount,
    zero: bool,
) -> Result<(PancakeLmState, Amount)> {
    parent.validate()?;
    if parent.pancake_lm.as_ref() != Some(before) {
        return Err(err("PANCAKE_LM_STATE_PARENT_MISMATCH"));
    }
    let mut thin = parent.clone();
    thin.pancake_lm = None;
    thin.v3 = Some(before.core.clone());
    thin.protocol = "pancake-v3".into();
    thin.adapter_version = "pancake-v3/core-1".into();
    let (q, traces) = crate::v3::quote_exact_in_with_trace(&thin, input, zero)?;
    settle_state(parent, before, input, zero, q, traces)
}
fn settle_state(
    parent: &Pool,
    before: &PancakeLmState,
    input: &Amount,
    zero: bool,
    q: crate::Swap,
    traces: Vec<crate::v3::V3StepTrace>,
) -> Result<(PancakeLmState, Amount)> {
    let mut next = before.clone();
    reward(&mut next)?;
    let core = q
        .next_pool
        .v3
        .ok_or_else(|| err("PANCAKE_LM_CORE_REQUIRED"))?;
    let raw = if zero {
        &before.raw_balance1
    } else {
        &before.raw_balance0
    };
    if raw.0 < q.amount_out.0 {
        return Err(err("PANCAKE_LM_RAW_BALANCE"));
    }
    let time = before.timestamp as u32;
    let latest = transform(
        &before.observations[before.observation_index as usize],
        time,
        before.core.tick,
        &before.core.liquidity,
    )?;
    let rate = if zero {
        before.fee_protocol & 65535
    } else {
        before.fee_protocol >> 16
    };
    let mut protocol = BigUint::zero();
    let mut global = if zero {
        before.fee_growth_global0.0.clone()
    } else {
        before.fee_growth_global1.0.clone()
    };
    for step in traces {
        let product = &step.fee_amount * rate;
        if product.bits() > 256 {
            return Err(err("PANCAKE_LM_PROTOCOL_MULTIPLICATION_OVERFLOW"));
        }
        let delta = product / 10000u32;
        protocol = add(&protocol, &(&delta & mask(128)), 128);
        if !step.liquidity.is_zero() {
            let growth = ((step.fee_amount - delta) << 128usize) / &step.liquidity;
            if growth.bits() > 256 {
                return Err(err("PANCAKE_LM_FEE_GROWTH_WIDTH"));
            }
            global = add(&global, &growth, 256);
        }
        if let Some(index) = step.crossed_tick {
            let tick = next
                .ticks
                .iter_mut()
                .find(|t| t.tick == index)
                .ok_or_else(|| err("PANCAKE_LM_CORE_TICK_REQUIRED"))?;
            tick.fee_growth_outside0 = Amount(sub(
                if zero {
                    &global
                } else {
                    &before.fee_growth_global0.0
                },
                &tick.fee_growth_outside0.0,
                256,
            ));
            tick.fee_growth_outside1 = Amount(sub(
                if zero {
                    &before.fee_growth_global1.0
                } else {
                    &global
                },
                &tick.fee_growth_outside1.0,
                256,
            ));
            tick.tick_cumulative_outside = signed_wrap(
                i128::from(signed56(&latest.tick_cumulative)?)
                    - i128::from(signed56(&tick.tick_cumulative_outside)?),
            )
            .to_string();
            tick.seconds_per_liquidity_outside_x128 = Amount(sub(
                &latest.seconds_per_liquidity_cumulative_x128.0,
                &tick.seconds_per_liquidity_outside_x128.0,
                160,
            ));
            tick.seconds_outside = time.wrapping_sub(tick.seconds_outside);
            let lm = next
                .lm
                .ticks
                .iter_mut()
                .find(|t| t.tick == index)
                .ok_or_else(|| err("PANCAKE_LM_TICK_REQUIRED"))?;
            if !lm.liquidity_gross.0.is_zero() {
                lm.reward_growth_outside = Amount(sub(
                    &next.lm.reward_growth_global.0,
                    &lm.reward_growth_outside.0,
                    256,
                ));
                next.lm.liquidity = Amount(adjust(
                    &next.lm.liquidity.0,
                    xone_domain::v3::parse_liquidity_net(&lm.liquidity_net)?,
                    zero,
                )?);
            }
        }
    }
    if core.tick != before.core.tick {
        let last = &before.observations[before.observation_index as usize];
        if last.block_timestamp != time {
            let cardinality = if before.observation_cardinality_next
                > before.observation_cardinality
                && before.observation_index == before.observation_cardinality - 1
            {
                before.observation_cardinality_next
            } else {
                before.observation_cardinality
            };
            let index = (before.observation_index + 1) % cardinality;
            next.observations[index as usize] = latest;
            next.observation_index = index;
            next.observation_cardinality = cardinality;
        }
    }
    next.tick = core.tick;
    next.core = core;
    if zero {
        next.fee_growth_global0 = Amount(global);
        next.protocol_fees0 = Amount(add(&before.protocol_fees0.0, &protocol, 128));
        next.raw_balance0 = Amount::checked(&before.raw_balance0.0 + &input.0)?;
        next.raw_balance1 = Amount::checked(&before.raw_balance1.0 - &q.amount_out.0)?;
    } else {
        next.fee_growth_global1 = Amount(global);
        next.protocol_fees1 = Amount(add(&before.protocol_fees1.0, &protocol, 128));
        next.raw_balance1 = Amount::checked(&before.raw_balance1.0 + &input.0)?;
        next.raw_balance0 = Amount::checked(&before.raw_balance0.0 - &q.amount_out.0)?;
    }
    next.derived = true;
    next.validate_pool(parent)?;
    Ok((next, q.amount_out))
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero: bool) -> Result<crate::Swap> {
    let state = pool
        .pancake_lm
        .as_ref()
        .ok_or_else(|| err("PANCAKE_LM_CONTEXT_REQUIRED"))?;
    let (next, amount_out) = quote_state(pool, state, input, zero)?;
    let mut next_pool = pool.clone();
    next_pool.pancake_lm = Some(next);
    Ok(crate::Swap {
        amount_out,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool,
    })
}

/// Independent negative-amount curve execution; shares only full ledger settlement.
pub fn quote_exact_out(pool: &Pool, output: &Amount, zero: bool) -> Result<crate::Swap> {
    pool.validate()?;
    let before = pool
        .pancake_lm
        .as_ref()
        .ok_or_else(|| err("PANCAKE_LM_CONTEXT_REQUIRED"))?;
    let mut thin = pool.clone();
    thin.pancake_lm = None;
    thin.v3 = Some(before.core.clone());
    thin.protocol = "pancake-v3".into();
    thin.adapter_version = "pancake-v3/core-1".into();
    let (q, traces) = crate::v3::quote_exact_out_with_trace(&thin, output, zero)?;
    let input = q.consumed_in.clone();
    let (next, received) = settle_state(pool, before, &input, zero, q, traces)?;
    if received != *output {
        return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
    }
    let mut next_pool = pool.clone();
    next_pool.pancake_lm = Some(next);
    Ok(crate::Swap {
        amount_out: received,
        consumed_in: input,
        remaining_in: Amount::zero(),
        next_pool,
    })
}
