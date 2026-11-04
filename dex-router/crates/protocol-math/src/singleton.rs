//! Static, zero-hook V4 / Infinity CL amount state. Independent protocol state,
//! directional protocol/LP fee composition and signed BalanceDelta bounds.
use crate::Swap;
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::singleton::ConcentratedState;
use xone_domain::{
    err,
    v3::{parse_liquidity_net, sqrt_ratio_at_tick, tick_at_sqrt_ratio, MAX_TICK, MIN_TICK},
    Amount, Pool, Result,
};

fn ceil_div(n: BigUint, d: &BigUint) -> Result<BigUint> {
    if d.is_zero() {
        return Err(err("SINGLETON_DIVISION_BY_ZERO"));
    }
    Ok((&n + d - BigUint::one()) / d)
}
fn amount0_delta(a: &BigUint, b: &BigUint, l: &BigUint, up: bool) -> Result<BigUint> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    if lo.is_zero() {
        return Err(err("SINGLETON_PRICE_RANGE"));
    }
    let n = (l << 96usize) * (hi - lo);
    let d = hi * lo;
    let result = if up { ceil_div(n, &d)? } else { n / d };
    if result.bits() > 256 {
        return Err(err("SINGLETON_AMOUNT_OVERFLOW"));
    }
    Ok(result)
}
fn amount1_delta(a: &BigUint, b: &BigUint, l: &BigUint, up: bool) -> Result<BigUint> {
    let diff = if a <= b { b - a } else { a - b };
    let n = l * diff;
    let q96 = BigUint::one() << 96usize;
    let result = if up { ceil_div(n, &q96)? } else { n / q96 };
    if result.bits() > 256 {
        return Err(err("SINGLETON_AMOUNT_OVERFLOW"));
    }
    Ok(result)
}
fn next_price(sqrt: &BigUint, l: &BigUint, amount: &BigUint, zero: bool) -> Result<BigUint> {
    if sqrt.is_zero() || l.is_zero() {
        return Err(err("SINGLETON_NO_ACTIVE_LIQUIDITY"));
    }
    let result = if zero {
        let numerator = l << 96usize;
        let product = amount * sqrt;
        let denominator = &numerator + &product;
        if product.bits() <= 256 && denominator.bits() <= 256 {
            ceil_div(&numerator * sqrt, &denominator)?
        } else {
            ceil_div(numerator.clone(), &(&numerator / sqrt + amount))?
        }
    } else {
        sqrt + ((amount << 96usize) / l)
    };
    if result.is_zero() || result.bits() > 160 {
        return Err(err("SINGLETON_PRICE_WIDTH"));
    }
    Ok(result)
}
// Match TickBitmap's per-word boundaries even for words with no initialized
// ticks. Fees are rounded per step, so jumping over an empty word is incorrect.
fn next_tick(state: &ConcentratedState, tick: i32, zero: bool) -> Result<(i32, bool)> {
    let compressed = tick.div_euclid(state.tick_spacing) + i32::from(!zero);
    let pos = compressed.div_euclid(256);
    let bit = compressed.rem_euclid(256);
    let w = state
        .tick_words
        .iter()
        .find(|w| w.word_pos == pos)
        .ok_or_else(|| err("SINGLETON_TICK_RANGE_INCOMPLETE"))?;
    let found = if zero {
        (0..=bit).rev().find(|b| w.bitmap.0.bit(*b as u64))
    } else {
        (bit..256).find(|b| w.bitmap.0.bit(*b as u64))
    };
    let next = pos * 256 + found.unwrap_or(if zero { 0 } else { 255 });
    Ok((
        (next * state.tick_spacing).clamp(MIN_TICK, MAX_TICK),
        found.is_some(),
    ))
}
fn cross_liquidity(l: &BigUint, net: i128, zero: bool) -> Result<BigUint> {
    let magnitude = BigUint::from(net.unsigned_abs());
    let subtract = (net < 0) ^ zero;
    let next = if subtract {
        if l < &magnitude {
            return Err(err("SINGLETON_LIQUIDITY_UNDERFLOW"));
        }
        l - &magnitude
    } else {
        l + &magnitude
    };
    if next.bits() > 128 {
        return Err(err("SINGLETON_LIQUIDITY_OVERFLOW"));
    }
    Ok(next)
}

pub fn quote_exact_in(pool: &Pool, input: &Amount, zero: bool) -> Result<Swap> {
    pool.validate()?;
    if xone_domain::protocols::protocol(&pool.protocol).is_none_or(|p| p.family != "singleton") {
        return Err(err("PROTOCOL_UNSUPPORTED"));
    }
    if pool.token_behavior != "standard" {
        return Err(err("TOKEN_BEHAVIOR_UNSUPPORTED"));
    }
    if input.0.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    if input.0 > (BigUint::one() << 127usize) {
        return Err(err("SINGLETON_INPUT_INT256_RANGE"));
    }
    let singleton = pool
        .singleton_cl
        .as_ref()
        .ok_or_else(|| err("SINGLETON_STATE_REQUIRED"))?;
    let (core, output, _, _) = quote_core(singleton, input, zero, false)?;
    let mut next_pool = pool.clone();
    next_pool
        .singleton_cl
        .as_mut()
        .ok_or_else(|| err("SINGLETON_STATE_REQUIRED"))?
        .core = core;
    next_pool.validate()?;
    Ok(Swap {
        amount_out: Amount::checked(output)?,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool,
    })
}

/// Core post-state and per-step manager fee allocation, bound to an explicit snapshot.
#[derive(Clone, Debug)]
pub struct SingletonAdvancedSwap {
    pub amount_out: Amount,
    pub consumed_in: Amount,
    pub remaining_in: Amount,
    pub swap_fee: u32,
    pub protocol_amount: Amount,
    pub next_state: xone_domain::singleton::SingletonAdvancedState,
}
pub fn quote_advanced_exact_in(
    state: &xone_domain::singleton::SingletonAdvancedState,
    input: &Amount,
    zero: bool,
) -> Result<SingletonAdvancedSwap> {
    state.validate()?;
    if input.0.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    if input.0 > (BigUint::one() << 127usize) {
        return Err(err("SINGLETON_INPUT_INT256_RANGE"));
    }
    let (core, output, protocol_amount, swap_fee) = quote_core(&state.snapshot, input, zero, true)?;
    let mut next_state = state.clone();
    next_state.snapshot.core = core;
    next_state.validate()?;
    Ok(SingletonAdvancedSwap {
        amount_out: Amount::checked(output)?,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        protocol_amount: Amount::checked(protocol_amount)?,
        swap_fee,
        next_state,
    })
}
fn quote_core(
    singleton: &xone_domain::singleton::SingletonCLState,
    input: &Amount,
    zero: bool,
    advanced: bool,
) -> Result<(ConcentratedState, BigUint, BigUint, u32)> {
    let state = &singleton.core;
    let mut remaining = input.0.clone();
    let mut output = BigUint::zero();
    let mut protocol_amount = BigUint::zero();
    let mut sqrt = state.sqrt_price_x96.0.clone();
    let mut tick = state.tick;
    let mut liquidity = state.liquidity.0.clone();
    let million = BigUint::from(1_000_000u64);
    let protocol_fee = if zero {
        singleton.protocol_fee0
    } else {
        singleton.protocol_fee1
    };
    let combined = u64::from(protocol_fee) + u64::from(singleton.lp_fee)
        - u64::from(protocol_fee) * u64::from(singleton.lp_fee) / 1_000_000;
    if combined > 1_000_000 || !advanced && combined == 1_000_000 {
        return Err(err("SINGLETON_NO_OUTPUT_FEE"));
    }
    let fee = BigUint::from(combined);
    let factor = &million - &fee;
    let limit = if zero {
        sqrt_ratio_at_tick(MIN_TICK)? + BigUint::one()
    } else {
        sqrt_ratio_at_tick(MAX_TICK)? - BigUint::one()
    };
    let mut steps = 0;
    while !remaining.is_zero() {
        if steps >= state.max_steps {
            return Err(err("SINGLETON_MAX_STEPS"));
        }
        steps += 1;
        let start_sqrt = sqrt.clone();
        let (next, initialized) = next_tick(state, tick, zero)?;
        let tick_sqrt = sqrt_ratio_at_tick(next)?;
        let target = if zero {
            tick_sqrt.clone().max(limit.clone())
        } else {
            tick_sqrt.clone().min(limit.clone())
        };
        if zero && target > sqrt || !zero && target < sqrt {
            return Err(err("SINGLETON_PRICE_TICK_MISMATCH"));
        }
        let available = &remaining * &factor / &million;
        let required = if zero {
            amount0_delta(&target, &sqrt, &liquidity, true)?
        } else {
            amount1_delta(&sqrt, &target, &liquidity, true)?
        };
        let next_sqrt = if available >= required {
            target.clone()
        } else {
            next_price(&sqrt, &liquidity, &available, zero)?
        };
        let consumed = if zero {
            amount0_delta(&next_sqrt, &sqrt, &liquidity, true)?
        } else {
            amount1_delta(&sqrt, &next_sqrt, &liquidity, true)?
        };
        let received = if zero {
            amount1_delta(&next_sqrt, &sqrt, &liquidity, false)?
        } else {
            amount0_delta(&sqrt, &next_sqrt, &liquidity, false)?
        };
        let fee_amount = if combined == 1_000_000 {
            remaining.clone()
        } else if next_sqrt == target {
            ceil_div(&consumed * &fee, &factor)?
        } else {
            if consumed > remaining {
                return Err(err("SINGLETON_INPUT_CONSERVATION"));
            }
            &remaining - &consumed
        };
        let spent = &consumed + &fee_amount;
        if protocol_fee > 0 {
            protocol_amount += if combined == u64::from(protocol_fee) {
                fee_amount
            } else {
                &spent * BigUint::from(protocol_fee) / &million
            };
        }
        if spent > remaining {
            return Err(err("SINGLETON_INPUT_CONSERVATION"));
        }
        remaining -= spent;
        output += received;
        if output >= (BigUint::one() << 127usize) {
            return Err(err("SINGLETON_OUTPUT_INT256_RANGE"));
        }
        sqrt = next_sqrt;
        if sqrt == tick_sqrt {
            if initialized {
                let info = state
                    .ticks
                    .binary_search_by_key(&next, |t| t.tick)
                    .map_err(|_| err("SINGLETON_INCOMPLETE_TICKS"))?;
                liquidity = cross_liquidity(
                    &liquidity,
                    parse_liquidity_net(&state.ticks[info].liquidity_net)?,
                    zero,
                )?;
            }
            tick = if zero { next - 1 } else { next };
        } else if sqrt != start_sqrt {
            tick = tick_at_sqrt_ratio(&sqrt)?;
        }
        if sqrt == limit && !remaining.is_zero() {
            return Err(err("SINGLETON_PRICE_LIMIT_REACHED"));
        }
    }
    if output.is_zero() && !advanced {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next = state.clone();
    next.sqrt_price_x96 = Amount::checked(sqrt)?;
    next.tick = tick;
    next.liquidity = Amount::checked(liquidity)?;
    Ok((next, output, protocol_amount, combined as u32))
}

#[path = "infinity_bin.rs"]
pub mod infinity_bin;
