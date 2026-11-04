//! Fixed original core-1.0.1 Router amount-out and Pair.swap settlement.
//! Source-bank metadata is immutable. Explicit-clock transitions are derived
//! contract writes, not a new canonical bank; routing uses one current-bank clock.
use crate::Swap;
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{
    err,
    v2_swap::{V2SwapState, PROFILE},
    Amount, Pool, Result,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    pub reserve0: Amount,
    pub reserve1: Amount,
    pub state: V2SwapState,
    pub amount0_in: Amount,
    pub amount1_in: Amount,
    pub amount0_out: Amount,
    pub amount1_out: Amount,
}
fn checked(value: BigUint) -> Result<BigUint> {
    if value.bits() > 256 {
        return Err(err("V2_SWAP_UINT256_OVERFLOW"));
    }
    Ok(value)
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a + b)
}
fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b > a {
        return Err(err("V2_SWAP_UINT256_UNDERFLOW"));
    }
    Ok(a - b)
}
fn reserves_valid(reserves: [&Amount; 2]) -> Result<()> {
    if reserves.iter().any(|r| r.0.bits() > 112) {
        return Err(err("V2_SWAP_RESERVE_WIDTH"));
    }
    Ok(())
}
/// Settle the exact raw balances observed after outgoing transfers and any
/// caller funding. This models Pair.swap, including donated inputs on both sides.
/// Reuse the explicitly selected execution clock for every step of one plan.
pub fn settle_swap(
    reserves: [&Amount; 2],
    source: &V2SwapState,
    outputs: [&Amount; 2],
    balances_after: [&Amount; 2],
    execution_timestamp: u64,
) -> Result<Transition> {
    source.validate()?;
    reserves_valid(reserves)?;
    if execution_timestamp < source.bank_unix_timestamp
        || execution_timestamp.checked_mul(1000).is_none()
    {
        return Err(err("V2_SWAP_EXECUTION_TIME"));
    }
    for value in outputs.iter().chain(balances_after.iter()) {
        if value.0.bits() > 256 {
            return Err(err("V2_SWAP_UINT256_OVERFLOW"));
        }
    }
    if outputs[0].0.is_zero() && outputs[1].0.is_zero() {
        return Err(err("V2_SWAP_OUTPUT_REQUIRED"));
    }
    if outputs[0].0 >= reserves[0].0 || outputs[1].0 >= reserves[1].0 {
        return Err(err("V2_SWAP_INSUFFICIENT_LIQUIDITY"));
    }
    let expected0 = &reserves[0].0 - &outputs[0].0;
    let expected1 = &reserves[1].0 - &outputs[1].0;
    let incoming0 = if balances_after[0].0 > expected0 {
        &balances_after[0].0 - &expected0
    } else {
        BigUint::zero()
    };
    let incoming1 = if balances_after[1].0 > expected1 {
        &balances_after[1].0 - &expected1
    } else {
        BigUint::zero()
    };
    if incoming0.is_zero() && incoming1.is_zero() {
        return Err(err("V2_SWAP_INPUT_REQUIRED"));
    }
    let thousand = BigUint::from(1000u16);
    let three = BigUint::from(3u8);
    let adjusted0 = sub(
        &mul(&balances_after[0].0, &thousand)?,
        &mul(&incoming0, &three)?,
    )?;
    let adjusted1 = sub(
        &mul(&balances_after[1].0, &thousand)?,
        &mul(&incoming1, &three)?,
    )?;
    let target = mul(
        &mul(&reserves[0].0, &reserves[1].0)?,
        &BigUint::from(1_000_000u32),
    )?;
    if mul(&adjusted0, &adjusted1)? < target {
        return Err(err("V2_SWAP_INVARIANT"));
    }
    // _update checks uint112 capacity only after the adjusted-balance K check.
    if balances_after.iter().any(|b| b.0.bits() > 112) {
        return Err(err("V2_SWAP_RESERVE_OVERFLOW"));
    }
    let mut next = source.clone();
    next.raw_balance0 = balances_after[0].clone();
    next.raw_balance1 = balances_after[1].clone();
    let timestamp = execution_timestamp as u32;
    let elapsed = timestamp.wrapping_sub(source.block_timestamp_last);
    if elapsed != 0 && !reserves[0].0.is_zero() && !reserves[1].0.is_zero() {
        let modulus = BigUint::one() << 256usize;
        let elapsed = BigUint::from(elapsed);
        next.price0_cumulative_last = Amount(
            (&source.price0_cumulative_last.0
                + ((&reserves[1].0 << 112usize) / &reserves[0].0) * &elapsed)
                % &modulus,
        );
        next.price1_cumulative_last = Amount(
            (&source.price1_cumulative_last.0
                + ((&reserves[0].0 << 112usize) / &reserves[1].0) * &elapsed)
                % &modulus,
        );
    }
    next.block_timestamp_last = timestamp;
    Ok(Transition {
        reserve0: balances_after[0].clone(),
        reserve1: balances_after[1].clone(),
        state: next,
        amount0_in: Amount(incoming0),
        amount1_in: Amount(incoming1),
        amount0_out: outputs[0].clone(),
        amount1_out: outputs[1].clone(),
    })
}
/// Original Router getAmountOut followed by full standard-token Pair settlement.
/// This explicit clock is for independent native comparison/execution contexts.
pub fn quote_transition(
    reserves: [&Amount; 2],
    source: &V2SwapState,
    input: &Amount,
    zero_for_one: bool,
    execution_timestamp: u64,
) -> Result<Transition> {
    source.validate()?;
    reserves_valid(reserves)?;
    if input.0.is_zero() {
        return Err(err("V2_SWAP_INPUT_REQUIRED"));
    }
    if input.0.bits() > 256 {
        return Err(err("V2_SWAP_UINT256_OVERFLOW"));
    }
    if reserves.iter().any(|r| r.0.is_zero()) {
        return Err(err("V2_SWAP_INSUFFICIENT_LIQUIDITY"));
    }
    let (rin, rout) = if zero_for_one {
        (reserves[0], reserves[1])
    } else {
        (reserves[1], reserves[0])
    };
    let weighted = mul(&input.0, &BigUint::from(997u16))?;
    let numerator = mul(&weighted, &rout.0)?;
    let denominator = add(&mul(&rin.0, &BigUint::from(1000u16))?, &weighted)?;
    let output = Amount(numerator / denominator);
    let zero = Amount::zero();
    let (out0, out1, balance0, balance1) = if zero_for_one {
        (
            &zero,
            &output,
            Amount(add(&source.raw_balance0.0, &input.0)?),
            Amount(sub(&source.raw_balance1.0, &output.0)?),
        )
    } else {
        (
            &output,
            &zero,
            Amount(sub(&source.raw_balance0.0, &output.0)?),
            Amount(add(&source.raw_balance1.0, &input.0)?),
        )
    };
    settle_swap(
        reserves,
        source,
        [out0, out1],
        [&balance0, &balance1],
        execution_timestamp,
    )
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero_for_one: bool) -> Result<Swap> {
    pool.validate()?;
    if pool.protocol != PROFILE {
        return Err(err("V2_SWAP_PROFILE_MISMATCH"));
    }
    let source = pool
        .v2
        .as_ref()
        .and_then(|v| v.swap.as_ref())
        .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
    let t = quote_transition(
        [&pool.reserve0, &pool.reserve1],
        source,
        input,
        zero_for_one,
        source.bank_unix_timestamp,
    )?;
    let output = if zero_for_one {
        t.amount1_out
    } else {
        t.amount0_out
    };
    let mut next = pool.clone();
    next.reserve0 = t.reserve0;
    next.reserve1 = t.reserve1;
    next.v2
        .as_mut()
        .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?
        .swap = Some(t.state);
    next.validate()?;
    Ok(Swap {
        amount_out: output,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}
