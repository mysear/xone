//! Exact-in math for Curve Factory Plain2Basic, Vyper 0.3.1, at
//! curvefi/curve-factory commit 5272fd2b1199f185d9cd94822be9262c387dc2c2.
//!
//! Mirrors `exchange`, including removal of admin fees from internal balances.
//! Only two standard ERC20 coins with fixed `rate_multipliers` are modeled.
//! Callers supply the already evaluated `A_precise` for the quote block; this
//! module does not interpolate ramps, read rate providers, or model transfers.

use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};

const PRECISION: u64 = 1_000_000_000_000_000_000;
const A_PRECISION: u64 = 100;
const MAX_A: u64 = 1_000_000;
const FEE_DENOMINATOR: u64 = 10_000_000_000;
const ADMIN_FEE: u64 = 5_000_000_000;
const MAX_ITERATIONS: usize = 255;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurveQuote {
    pub amount_out: BigUint,
    /// Internal `balances`, excluding accrued admin fees, in original coin order.
    pub next_balances: [BigUint; 2],
}

/// Quote `exchange(i, j, dx, 0)` and its internal balance changes.
///
/// `balances` are raw internal balances, not ERC20 balances including admin fees.
/// `rates` are fixed source rate multipliers: `xp = rates * balances / 1e18`.
/// Amplification is `A_precise`, scaled by 100, including fractional ramp values.
/// The supported A interval is `[100, MAX_A * 100)`, using the `ramp_A` bound;
/// `initialize` itself does not enforce this A interval. Fees use denominator
/// 1e10: trade fee must satisfy the pinned Factory's inclusive `[4e6, 1e8]`
/// deployment bounds, and admin fee must equal Plain2Basic's constant `5e9`.
///
/// Every arithmetic intermediate is checked as a Vyper uint256, in source
/// evaluation order. A zero raw output is valid if source arithmetic succeeds.
/// This API does not enforce a caller's slippage minimum or token transfer rules.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    balances: [&BigUint; 2],
    rates: [&BigUint; 2],
    amplification: &BigUint,
    fee: u64,
    admin_fee: u64,
    input: &BigUint,
    zero_for_one: bool,
) -> Result<CurveQuote> {
    for value in [
        balances[0],
        balances[1],
        rates[0],
        rates[1],
        amplification,
        input,
    ] {
        ensure_uint256(value)?;
    }
    if amplification < &BigUint::from(A_PRECISION)
        || amplification >= &BigUint::from(MAX_A * A_PRECISION)
    {
        return Err(err("CURVE_INVALID_AMPLIFICATION"));
    }
    if !(4_000_000..=100_000_000).contains(&fee) {
        return Err(err("CURVE_INVALID_FEE"));
    }
    if admin_fee != ADMIN_FEE {
        return Err(err("CURVE_INVALID_ADMIN_FEE"));
    }
    if rates.iter().any(|rate| rate.is_zero()) {
        return Err(err("CURVE_INVALID_RATE"));
    }

    let precision = BigUint::from(PRECISION);
    let fee_denominator = BigUint::from(FEE_DENOMINATOR);
    let xp = [
        div(&mul(rates[0], balances[0])?, &precision)?,
        div(&mul(rates[1], balances[1])?, &precision)?,
    ];
    let i = usize::from(!zero_for_one);
    let j = 1 - i;
    // Normalize dx independently; normalizing (balance + dx) changes rounding.
    let x = add(&xp[i], &div(&mul(input, rates[i])?, &precision)?)?;
    let y = get_y(&x, &xp, amplification)?;
    let dy = sub(&sub(&xp[j], &y)?, &BigUint::one())?;
    let dy_fee = div(&mul(&dy, &BigUint::from(fee))?, &fee_denominator)?;
    let amount_out = div(&mul(&sub(&dy, &dy_fee)?, &precision)?, rates[j])?;

    // exchange rounds the normalized admin fee before converting to raw units.
    let admin_normalized = div(&mul(&dy_fee, &BigUint::from(admin_fee))?, &fee_denominator)?;
    let admin_raw = div(&mul(&admin_normalized, &precision)?, rates[j])?;
    let mut next_balances = [balances[0].clone(), balances[1].clone()];
    next_balances[i] = add(balances[i], input)?;
    next_balances[j] = sub(&sub(balances[j], &amount_out)?, &admin_raw)?;
    Ok(CurveQuote {
        amount_out,
        next_balances,
    })
}

fn get_d(xp: &[BigUint; 2], amp: &BigUint) -> Result<BigUint> {
    let sum = add(&xp[0], &xp[1])?;
    if sum.is_zero() {
        return Ok(BigUint::zero());
    }
    let two = BigUint::from(2u32);
    let three = BigUint::from(3u32);
    let four = BigUint::from(4u32);
    let precision = BigUint::from(A_PRECISION);
    let ann = mul(amp, &two)?;
    let mut d = sum.clone();
    for _ in 0..MAX_ITERATIONS {
        // Pinned Plain2Basic: D * D / xp[0] * D / xp[1] / 4.
        // Dividing by (xp[i] * 2) in a loop gives different floor rounding.
        let d_p = div(
            &div(&mul(&div(&mul(&d, &d)?, &xp[0])?, &d)?, &xp[1])?,
            &four,
        )?;
        let previous = d;
        let amplified_sum = div(&mul(&ann, &sum)?, &precision)?;
        let numerator = mul(&add(&amplified_sum, &mul(&d_p, &two)?)?, &previous)?;
        let denominator = add(
            &div(&mul(&sub(&ann, &precision)?, &previous)?, &precision)?,
            &mul(&three, &d_p)?,
        )?;
        d = div(&numerator, &denominator)?;
        if within_one(&d, &previous) {
            return Ok(d);
        }
    }
    Err(err("CURVE_D_DID_NOT_CONVERGE"))
}

fn get_y(x: &BigUint, xp: &[BigUint; 2], amp: &BigUint) -> Result<BigUint> {
    let d = get_d(xp, amp)?;
    let two = BigUint::from(2u32);
    let precision = BigUint::from(A_PRECISION);
    let ann = mul(amp, &two)?;
    // With two distinct coin indices the source loop has exactly one term, x.
    let c = div(&mul(&d, &d)?, &mul(x, &two)?)?;
    let c = div(&mul(&mul(&c, &d)?, &precision)?, &mul(&ann, &two)?)?;
    let b = add(x, &div(&mul(&d, &precision)?, &ann)?)?;
    let mut y = d.clone();
    for _ in 0..MAX_ITERATIONS {
        let previous = y;
        let numerator = add(&mul(&previous, &previous)?, &c)?;
        let denominator = sub(&add(&mul(&two, &previous)?, &b)?, &d)?;
        y = div(&numerator, &denominator)?;
        if within_one(&y, &previous) {
            return Ok(y);
        }
    }
    Err(err("CURVE_Y_DID_NOT_CONVERGE"))
}

fn ensure_uint256(value: &BigUint) -> Result<()> {
    if value.bits() > 256 {
        Err(err("CURVE_UINT256_OVERFLOW"))
    } else {
        Ok(())
    }
}

fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    let value = a + b;
    ensure_uint256(&value)?;
    Ok(value)
}

fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    let value = a * b;
    ensure_uint256(&value)?;
    Ok(value)
}

fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        Err(err("CURVE_UINT256_UNDERFLOW"))
    } else {
        Ok(a - b)
    }
}

fn div(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("CURVE_DIVISION_BY_ZERO"))
    } else {
        Ok(a / b)
    }
}

fn within_one(a: &BigUint, b: &BigUint) -> bool {
    if a >= b {
        a - b <= BigUint::one()
    } else {
        b - a <= BigUint::one()
    }
}
