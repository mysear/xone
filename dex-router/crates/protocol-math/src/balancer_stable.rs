//! Bounded two-token, non-composable Balancer V2 standard StableMath exact-in.
//!
//! Arithmetic/rounding follows balancer/balancer-v2-monorepo at
//! e281ff40583c39bf1d46c322334b2b13936e0e33 (historical non-composable StablePool):
//! `pkg/pool-stable/contracts/StableMath.sol` (_calculateInvariant,
//! _calcOutGivenIn, _getTokenBalanceGivenInvariantAndAllOtherBalances),
//! `pkg/pool-utils/contracts/{BasePool,BaseGeneralPool}.sol`, and
//! `pkg/solidity-utils/contracts/{helpers/ScalingHelpers,math/FixedPoint}.sol`.
//! The swap fee is retained in the Vault; it is not a protocol-fee withdrawal.
//! Pool identity, pause/recovery state and token behavior are caller checks.

use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};

const AMP_PRECISION: u32 = 1000;
const MAX_ITERATIONS: usize = 255;
const FIXED_ONE: u64 = 1_000_000_000_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StableQuote {
    pub amount_out: BigUint,
    /// Raw atomic balances in the original token order, including the full input.
    pub next_balances: [BigUint; 2],
}

/// Quote a complete exact-in swap, rejecting zero output and unsupported state.
///
/// Balances/input/output are atomic units. `amplification` is the current scaled
/// value (1000..=5_000_000), not the human-readable A. Scaling factors are raw
/// Solidity fixed-point values `1e18 * 10^(18 - decimals)`, for decimals 0..=18.
/// Rate-adjusted scaling, BPT and composable pools are outside this function.
/// The mathematical fee range is 0..=1e18; 100% fees produce no usable quote.
/// Deployment-specific fee limits belong to the caller's pool validation.
///
/// Both existing and resulting Vault balances must fit uint112. Every arithmetic
/// intermediate must fit uint256, even when a subsequent division would fit.
/// Both Newton iterations have the Solidity 255-step limit and one-unit stop.
/// StableMath imposes no WeightedMath-style max input/output ratio.
/// Errors use BALANCER_* for invalid parameters/arithmetic and the shared
/// ZERO_INPUT, INSUFFICIENT_LIQUIDITY and INSUFFICIENT_OUTPUT quote errors.
pub fn quote_exact_in(
    balances: [&BigUint; 2],
    amplification: &BigUint,
    scaling_factors: [&BigUint; 2],
    swap_fee_percentage: &BigUint,
    input: &BigUint,
    zero_for_one: bool,
) -> Result<StableQuote> {
    for value in [
        balances[0],
        balances[1],
        amplification,
        scaling_factors[0],
        scaling_factors[1],
        swap_fee_percentage,
        input,
    ] {
        ensure_uint256(value)?;
    }
    if amplification < &BigUint::from(AMP_PRECISION) || amplification > &BigUint::from(5_000_000u32)
    {
        return Err(err("BALANCER_INVALID_AMPLIFICATION"));
    }
    let one = BigUint::from(FIXED_ONE);
    for factor in scaling_factors {
        let mut allowed = one.clone();
        let mut valid = false;
        for _ in 0..=18 {
            if factor == &allowed {
                valid = true;
                break;
            }
            allowed *= 10u32;
        }
        if !valid {
            return Err(err("BALANCER_INVALID_SCALING_FACTOR"));
        }
    }
    if swap_fee_percentage > &one {
        return Err(err("BALANCER_INVALID_SWAP_FEE"));
    }
    if input.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    for balance in balances {
        if balance.bits() > 112 {
            return Err(err("BALANCER_RESERVE_OVERFLOW"));
        }
        if balance.is_zero() {
            return Err(err("INSUFFICIENT_LIQUIDITY"));
        }
    }
    let index_in = usize::from(!zero_for_one);
    let index_out = 1 - index_in;
    let next_in = add(balances[index_in], input)?;
    if next_in.bits() > 112 {
        return Err(err("BALANCER_RESERVE_OVERFLOW"));
    }

    // BaseGeneralPool subtracts the raw fee before upscaling. mulUp rounds the
    // fee upward; the entire raw input is nevertheless credited to the Vault.
    let fee = div_up(&mul(input, swap_fee_percentage)?, &one)?;
    let net_input = sub(input, &fee)?;
    if net_input.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut scaled = [
        div_down(&mul(balances[0], scaling_factors[0])?, &one)?,
        div_down(&mul(balances[1], scaling_factors[1])?, &one)?,
    ];
    let invariant = calculate_invariant(amplification, &scaled)?;
    let scaled_input = div_down(&mul(&net_input, scaling_factors[index_in])?, &one)?;
    scaled[index_in] = add(&scaled[index_in], &scaled_input)?;
    let final_out = token_balance(amplification, &scaled, &invariant, index_out)?;

    // _calcOutGivenIn subtracts one *scaled* unit after the upward-rounded
    // token balance solve. Downscaling itself is a checked fixed-point division.
    if final_out >= scaled[index_out] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let scaled_output = sub(&sub(&scaled[index_out], &final_out)?, &BigUint::one())?;
    let amount_out = div_down(&mul(&scaled_output, &one)?, scaling_factors[index_out])?;
    if amount_out.is_zero() || &amount_out >= balances[index_out] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = [balances[0].clone(), balances[1].clone()];
    next_balances[index_in] = next_in;
    next_balances[index_out] = sub(balances[index_out], &amount_out)?;
    Ok(StableQuote {
        amount_out,
        next_balances,
    })
}

fn calculate_invariant(amp: &BigUint, balances: &[BigUint; 2]) -> Result<BigUint> {
    let two = BigUint::from(2u32);
    let three = BigUint::from(3u32);
    let precision = BigUint::from(AMP_PRECISION);
    let sum = add(&balances[0], &balances[1])?;
    let ann = mul(amp, &two)?;
    let mut invariant = sum.clone();
    for _ in 0..MAX_ITERATIONS {
        let mut d_p = invariant.clone();
        for balance in balances {
            d_p = div_down(&mul(&d_p, &invariant)?, &mul(balance, &two)?)?;
        }
        let previous = invariant;
        let amplified_sum = div_down(&mul(&ann, &sum)?, &precision)?;
        let numerator = mul(&add(&amplified_sum, &mul(&d_p, &two)?)?, &previous)?;
        let denominator = add(
            &div_down(&mul(&sub(&ann, &precision)?, &previous)?, &precision)?,
            &mul(&three, &d_p)?,
        )?;
        invariant = div_down(&numerator, &denominator)?;
        if within_one(&invariant, &previous) {
            return Ok(invariant);
        }
    }
    Err(err("BALANCER_INVARIANT_DID_NOT_CONVERGE"))
}

fn token_balance(
    amp: &BigUint,
    balances: &[BigUint; 2],
    invariant: &BigUint,
    index_out: usize,
) -> Result<BigUint> {
    let two = BigUint::from(2u32);
    let precision = BigUint::from(AMP_PRECISION);
    let ann = mul(amp, &two)?;
    // Keep P_D's original token order and multiplication/division order.
    let p_d = div_down(
        &mul(&mul(&mul(&balances[0], &two)?, &balances[1])?, &two)?,
        invariant,
    )?;
    let sum = sub(&add(&balances[0], &balances[1])?, &balances[index_out])?;
    let inv2 = mul(invariant, invariant)?;
    let c = mul(
        &mul(&div_up(&inv2, &mul(&ann, &p_d)?)?, &precision)?,
        &balances[index_out],
    )?;
    // Moving precision inside either division changes Solidity's rounding.
    let b = add(&sum, &mul(&div_down(invariant, &ann)?, &precision)?)?;
    let mut balance = div_up(&add(&inv2, &c)?, &add(invariant, &b)?)?;
    for _ in 0..MAX_ITERATIONS {
        let previous = balance;
        balance = div_up(
            &add(&mul(&previous, &previous)?, &c)?,
            &sub(&add(&mul(&previous, &two)?, &b)?, invariant)?,
        )?;
        if within_one(&balance, &previous) {
            return Ok(balance);
        }
    }
    Err(err("BALANCER_BALANCE_DID_NOT_CONVERGE"))
}

fn ensure_uint256(value: &BigUint) -> Result<()> {
    if value.bits() > 256 {
        Err(err("BALANCER_INTERMEDIATE_OVERFLOW"))
    } else {
        Ok(())
    }
}

fn checked(value: BigUint) -> Result<BigUint> {
    ensure_uint256(&value)?;
    Ok(value)
}

fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a + b)
}

fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}

fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        Err(err("BALANCER_ARITHMETIC_UNDERFLOW"))
    } else {
        Ok(a - b)
    }
}

fn div_down(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("BALANCER_ZERO_DIVISION"))
    } else {
        Ok(a / b)
    }
}

fn div_up(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("BALANCER_ZERO_DIVISION"))
    } else if a.is_zero() {
        Ok(BigUint::zero())
    } else {
        // Match Math.divUp: avoid an artificial overflow from a + b - 1.
        Ok((a - BigUint::one()) / b + BigUint::one())
    }
}

fn within_one(a: &BigUint, b: &BigUint) -> bool {
    if a >= b {
        a - b <= BigUint::one()
    } else {
        b - a <= BigUint::one()
    }
}
