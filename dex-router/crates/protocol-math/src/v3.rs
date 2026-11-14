use crate::Swap;
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{
    err,
    v3::{
        parse_liquidity_net, sqrt_ratio_at_tick, tick_at_sqrt_ratio, V3State, MAX_TICK, MIN_TICK,
    },
    Amount, Pool, Result,
};

fn ceil_div(n: BigUint, d: &BigUint) -> Result<BigUint> {
    if d.is_zero() {
        return Err(err("V3_DIVISION_BY_ZERO"));
    }
    Ok((&n + d - BigUint::one()) / d)
}
fn amount0_delta(a: &BigUint, b: &BigUint, l: &BigUint, up: bool) -> Result<BigUint> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    if lo.is_zero() {
        return Err(err("V3_PRICE_RANGE"));
    }
    let n = (l << 96usize) * (hi - lo);
    let d = hi * lo;
    let result = if up { ceil_div(n, &d)? } else { n / d };
    if result.bits() > 256 {
        return Err(err("V3_AMOUNT_OVERFLOW"));
    }
    Ok(result)
}
fn amount1_delta(a: &BigUint, b: &BigUint, l: &BigUint, up: bool) -> Result<BigUint> {
    let diff = if a <= b { b - a } else { a - b };
    let n = l * diff;
    let q96 = BigUint::one() << 96usize;
    let result = if up { ceil_div(n, &q96)? } else { n / q96 };
    if result.bits() > 256 {
        return Err(err("V3_AMOUNT_OVERFLOW"));
    }
    Ok(result)
}
fn next_price(sqrt: &BigUint, l: &BigUint, amount: &BigUint, zero: bool) -> Result<BigUint> {
    if sqrt.is_zero() || l.is_zero() {
        return Err(err("V3_NO_ACTIVE_LIQUIDITY"));
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
        return Err(err("V3_PRICE_WIDTH"));
    }
    Ok(result)
}
// Match TickBitmap's per-word boundaries even for words with no initialized
// ticks. Fees are rounded per step, so jumping over an empty word is incorrect.
fn next_tick(state: &V3State, tick: i32, zero: bool) -> Result<(i32, bool)> {
    let compressed = tick.div_euclid(state.tick_spacing) + i32::from(!zero);
    let pos = compressed.div_euclid(256);
    let bit = compressed.rem_euclid(256);
    let w = state
        .tick_words
        .iter()
        .find(|w| w.word_pos == pos)
        .ok_or_else(|| err("V3_TICK_RANGE_INCOMPLETE"))?;
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
            return Err(err("V3_LIQUIDITY_UNDERFLOW"));
        }
        l - &magnitude
    } else {
        l + &magnitude
    };
    if next.bits() > 128 {
        return Err(err("V3_LIQUIDITY_OVERFLOW"));
    }
    Ok(next)
}

pub struct V3StepTrace {
    pub liquidity: BigUint,
    pub fee_amount: BigUint,
    pub crossed_tick: Option<i32>,
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero: bool) -> Result<Swap> {
    Ok(quote_inner(pool, input, zero, false)?.0)
}
pub fn quote_exact_in_with_trace(
    pool: &Pool,
    input: &Amount,
    zero: bool,
) -> Result<(Swap, Vec<V3StepTrace>)> {
    quote_inner(pool, input, zero, true)
}
fn quote_inner(
    pool: &Pool,
    input: &Amount,
    zero: bool,
    collect: bool,
) -> Result<(Swap, Vec<V3StepTrace>)> {
    pool.validate()?;
    if xone_domain::protocols::protocol(&pool.protocol).is_none_or(|p| p.family != "v3") {
        return Err(err("PROTOCOL_UNSUPPORTED"));
    }
    if pool.token_behavior != "standard" {
        return Err(err("TOKEN_BEHAVIOR_UNSUPPORTED"));
    }
    if input.0.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    if input.0.bits() > 255 {
        return Err(err("V3_INPUT_INT256_RANGE"));
    }
    let state = pool.v3.as_ref().ok_or_else(|| err("V3_STATE_REQUIRED"))?;
    let mut remaining = input.0.clone();
    let mut output = BigUint::zero();
    let mut sqrt = state.sqrt_price_x96.0.clone();
    let mut tick = state.tick;
    let mut liquidity = state.liquidity.0.clone();
    let million = BigUint::from(1_000_000u64);
    let fee = BigUint::from(pool.fee_numerator);
    let factor = &million - &fee;
    let limit = if zero {
        sqrt_ratio_at_tick(MIN_TICK)? + BigUint::one()
    } else {
        sqrt_ratio_at_tick(MAX_TICK)? - BigUint::one()
    };
    let mut traces = Vec::new();
    let mut steps = 0;
    while !remaining.is_zero() {
        if steps >= state.max_steps {
            return Err(err("V3_MAX_STEPS"));
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
            return Err(err("V3_PRICE_TICK_MISMATCH"));
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
        let fee_amount = if next_sqrt == target {
            ceil_div(&consumed * &fee, &factor)?
        } else {
            if consumed > remaining {
                return Err(err("V3_INPUT_CONSERVATION"));
            }
            &remaining - &consumed
        };
        if collect {
            traces.push(V3StepTrace {
                liquidity: liquidity.clone(),
                fee_amount: fee_amount.clone(),
                crossed_tick: if initialized && next_sqrt == tick_sqrt {
                    Some(next)
                } else {
                    None
                },
            });
        }
        let spent = &consumed + fee_amount;
        if spent > remaining {
            return Err(err("V3_INPUT_CONSERVATION"));
        }
        remaining -= spent;
        output += received;
        if output.bits() > 255 {
            return Err(err("V3_OUTPUT_INT256_RANGE"));
        }
        sqrt = next_sqrt;
        if sqrt == tick_sqrt {
            if initialized {
                let info = state
                    .ticks
                    .binary_search_by_key(&next, |t| t.tick)
                    .map_err(|_| err("V3_INCOMPLETE_TICKS"))?;
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
            return Err(err("V3_PRICE_LIMIT_REACHED"));
        }
    }
    let mut next_pool = pool.clone();
    let next_state = next_pool
        .v3
        .as_mut()
        .ok_or_else(|| err("V3_STATE_REQUIRED"))?;
    next_state.sqrt_price_x96 = Amount::checked(sqrt)?;
    next_state.tick = tick;
    next_state.liquidity = Amount::checked(liquidity)?;
    next_pool.validate()?;
    Ok((
        Swap {
            amount_out: Amount::checked(output)?,
            consumed_in: input.clone(),
            remaining_in: Amount::zero(),
            next_pool,
        },
        traces,
    ))
}

// SqrtPriceMath.getNextSqrtPriceFromOutput: output token1 subtracts a rounded-up
// quotient; output token0 uses the rounded-up reciprocal with checked product.
fn next_price_from_output(
    sqrt: &BigUint,
    l: &BigUint,
    amount: &BigUint,
    zero: bool,
) -> Result<BigUint> {
    if sqrt.is_zero() || l.is_zero() {
        return Err(err("V3_NO_ACTIVE_LIQUIDITY"));
    }
    let result = if zero {
        let quotient = ceil_div(amount << 96usize, l)?;
        if sqrt <= &quotient {
            return Err(err("V3_OUTPUT_PRICE_UNDERFLOW"));
        }
        sqrt - quotient
    } else {
        let numerator = l << 96usize;
        let product = amount * sqrt;
        if product.bits() > 256 || numerator <= product {
            return Err(err("V3_OUTPUT_PRICE_OVERFLOW"));
        }
        ceil_div(&numerator * sqrt, &(numerator - product))?
    };
    if result.is_zero() || result.bits() > 160 {
        return Err(err("V3_PRICE_WIDTH"));
    }
    Ok(result)
}

/// Standard Pool callback-funding templates; Slipstream keeps its own origin context.
pub fn supports_exact_out(protocol: &str) -> bool {
    matches!(protocol, "uniswap-v3" | "pancake-v3")
}
pub fn quote_exact_out(pool: &Pool, output: &Amount, zero: bool) -> Result<Swap> {
    if !supports_exact_out(&pool.protocol) {
        return Err(err("EXACT_OUT_PROTOCOL_UNSUPPORTED"));
    }
    Ok(quote_exact_out_with_trace(pool, output, zero)?.0)
}
pub fn quote_exact_out_with_trace(
    pool: &Pool,
    input: &Amount,
    zero: bool,
) -> Result<(Swap, Vec<V3StepTrace>)> {
    pool.validate()?;
    if xone_domain::protocols::protocol(&pool.protocol).is_none_or(|p| p.family != "v3") {
        return Err(err("PROTOCOL_UNSUPPORTED"));
    }
    if pool.token_behavior != "standard" {
        return Err(err("TOKEN_BEHAVIOR_UNSUPPORTED"));
    }
    if input.0.is_zero() {
        return Err(err("EXACT_OUT_ZERO_OUTPUT"));
    }
    if input.0.bits() > 255 {
        return Err(err("V3_OUTPUT_INT256_RANGE"));
    }
    let state = pool.v3.as_ref().ok_or_else(|| err("V3_STATE_REQUIRED"))?;
    let mut remaining = input.0.clone();
    let mut required_input = BigUint::zero();
    let mut sqrt = state.sqrt_price_x96.0.clone();
    let mut tick = state.tick;
    let mut liquidity = state.liquidity.0.clone();
    let million = BigUint::from(1_000_000u64);
    let fee = BigUint::from(pool.fee_numerator);
    let factor = &million - &fee;
    let limit = if zero {
        sqrt_ratio_at_tick(MIN_TICK)? + BigUint::one()
    } else {
        sqrt_ratio_at_tick(MAX_TICK)? - BigUint::one()
    };
    let mut traces = Vec::new();
    let mut steps = 0;
    while !remaining.is_zero() {
        if steps >= state.max_steps {
            return Err(err("V3_MAX_STEPS"));
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
            return Err(err("V3_PRICE_TICK_MISMATCH"));
        }
        let available_output = if zero {
            amount1_delta(&target, &sqrt, &liquidity, false)?
        } else {
            amount0_delta(&sqrt, &target, &liquidity, false)?
        };
        let next_sqrt = if remaining >= available_output {
            target.clone()
        } else {
            next_price_from_output(&sqrt, &liquidity, &remaining, zero)?
        };
        let consumed = if zero {
            amount0_delta(&next_sqrt, &sqrt, &liquidity, true)?
        } else {
            amount1_delta(&sqrt, &next_sqrt, &liquidity, true)?
        };
        let received = if next_sqrt == target {
            available_output
        } else if zero {
            amount1_delta(&next_sqrt, &sqrt, &liquidity, false)?
        } else {
            amount0_delta(&sqrt, &next_sqrt, &liquidity, false)?
        }
        .min(remaining.clone());
        let fee_amount = ceil_div(&consumed * &fee, &factor)?;
        {
            traces.push(V3StepTrace {
                liquidity: liquidity.clone(),
                fee_amount: fee_amount.clone(),
                crossed_tick: if initialized && next_sqrt == tick_sqrt {
                    Some(next)
                } else {
                    None
                },
            });
        }
        required_input += consumed + fee_amount;
        if required_input.bits() > 255 {
            return Err(err("V3_INPUT_INT256_RANGE"));
        }
        remaining -= received;
        sqrt = next_sqrt;
        if sqrt == tick_sqrt {
            if initialized {
                let info = state
                    .ticks
                    .binary_search_by_key(&next, |t| t.tick)
                    .map_err(|_| err("V3_INCOMPLETE_TICKS"))?;
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
            return Err(err("V3_PRICE_LIMIT_REACHED"));
        }
    }
    let mut next_pool = pool.clone();
    let next_state = next_pool
        .v3
        .as_mut()
        .ok_or_else(|| err("V3_STATE_REQUIRED"))?;
    next_state.sqrt_price_x96 = Amount::checked(sqrt)?;
    next_state.tick = tick;
    next_state.liquidity = Amount::checked(liquidity)?;
    next_pool.validate()?;
    Ok((
        Swap {
            amount_out: input.clone(),
            consumed_in: Amount::checked(required_input)?,
            remaining_in: Amount::zero(),
            next_pool,
        },
        traces,
    ))
}
