//! Output-driven static zero-hook CL swaps. The native delta sign differs from V3.
use super::{amount0_delta, amount1_delta, ceil_div, cross_liquidity, next_tick};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{
    err,
    singleton::ConcentratedState,
    v3::{parse_liquidity_net, sqrt_ratio_at_tick, tick_at_sqrt_ratio, MAX_TICK, MIN_TICK},
    Amount, Pool, Result,
};

fn next_price_output(
    sqrt: &BigUint,
    liquidity: &BigUint,
    output: &BigUint,
    zero: bool,
) -> Result<BigUint> {
    if liquidity.is_zero() || sqrt.is_zero() {
        return Err(err("SINGLETON_NO_ACTIVE_LIQUIDITY"));
    }
    let next = if zero {
        let quotient = ceil_div(output << 96usize, liquidity)?;
        if sqrt <= &quotient {
            return Err(err("SINGLETON_OUTPUT_PRICE_UNDERFLOW"));
        }
        sqrt - quotient
    } else {
        let numerator = liquidity << 96usize;
        let product = output * sqrt;
        if product.bits() > 256 || numerator <= product {
            return Err(err("SINGLETON_OUTPUT_PRICE_UNDERFLOW"));
        }
        ceil_div(&numerator * sqrt, &(&numerator - product))?
    };
    if next.is_zero() || next.bits() > 160 {
        return Err(err("SINGLETON_PRICE_WIDTH"));
    }
    Ok(next)
}

pub fn quote_exact_out(
    pool: &Pool,
    output: &Amount,
    zero: bool,
) -> Result<crate::exact_out::ExactOutSwap> {
    pool.validate()?;
    if !matches!(pool.protocol.as_str(), "uniswap-v4" | "pancake-infinity-cl")
        || pool.token_behavior != "standard"
    {
        return Err(err("EXACT_OUT_PROTOCOL_UNSUPPORTED"));
    }
    if output.0.is_zero() {
        return Err(err("EXACT_OUT_ZERO_OUTPUT"));
    }
    if output.0.bits() > 127 {
        return Err(err("SINGLETON_OUTPUT_INT256_RANGE"));
    }
    let singleton = pool
        .singleton_cl
        .as_ref()
        .ok_or_else(|| err("SINGLETON_STATE_REQUIRED"))?;
    let state = &singleton.core;
    let mut remaining = output.0.clone();
    let mut input = BigUint::zero();
    let mut sqrt = state.sqrt_price_x96.0.clone();
    let mut tick = state.tick;
    let mut liquidity = state.liquidity.0.clone();
    let protocol_fee = if zero {
        singleton.protocol_fee0
    } else {
        singleton.protocol_fee1
    };
    let combined = u64::from(protocol_fee) + u64::from(singleton.lp_fee)
        - u64::from(protocol_fee) * u64::from(singleton.lp_fee) / 1_000_000;
    if combined >= 1_000_000 {
        return Err(err("SINGLETON_NO_OUTPUT_FEE"));
    }
    let factor = BigUint::from(1_000_000 - combined);
    let fee = BigUint::from(combined);
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
        let start = sqrt.clone();
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
        let available = if zero {
            amount1_delta(&target, &sqrt, &liquidity, false)?
        } else {
            amount0_delta(&sqrt, &target, &liquidity, false)?
        };
        let next_sqrt = if remaining >= available {
            target.clone()
        } else {
            next_price_output(&sqrt, &liquidity, &remaining, zero)?
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
        }
        .min(remaining.clone());
        let fee_amount = ceil_div(&consumed * &fee, &factor)?;
        input += consumed + fee_amount;
        if input > (BigUint::one() << 127usize) {
            return Err(err("SINGLETON_INPUT_INT256_RANGE"));
        }
        remaining -= received;
        sqrt = next_sqrt;
        if sqrt == tick_sqrt {
            if initialized {
                let i = state
                    .ticks
                    .binary_search_by_key(&next, |t| t.tick)
                    .map_err(|_| err("SINGLETON_INCOMPLETE_TICKS"))?;
                liquidity = cross_liquidity(
                    &liquidity,
                    parse_liquidity_net(&state.ticks[i].liquidity_net)?,
                    zero,
                )?;
            }
            tick = if zero { next - 1 } else { next };
        } else if sqrt != start {
            tick = tick_at_sqrt_ratio(&sqrt)?;
        }
        if sqrt == limit && !remaining.is_zero() {
            return Err(err("SINGLETON_PRICE_LIMIT_REACHED"));
        }
    }
    if input.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_pool = pool.clone();
    let s = next_pool
        .singleton_cl
        .as_mut()
        .ok_or_else(|| err("SINGLETON_STATE_REQUIRED"))?;
    s.core = ConcentratedState {
        sqrt_price_x96: Amount::checked(sqrt)?,
        tick,
        liquidity: Amount::checked(liquidity)?,
        ..state.clone()
    };
    next_pool.validate()?;
    Ok(crate::exact_out::ExactOutSwap {
        amount_in: Amount::checked(input)?,
        amount_out: output.clone(),
        next_pool,
    })
}
