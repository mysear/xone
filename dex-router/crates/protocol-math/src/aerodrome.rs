use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};
#[derive(Debug)]
pub struct AeroQuote {
    pub amount_out: BigUint,
    pub next_balances: [BigUint; 2],
}
fn checked(n: BigUint) -> Result<BigUint> {
    if n.bits() > 256 {
        return Err(err("AERO_UINT256_OVERFLOW"));
    }
    Ok(n)
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a + b)
}
fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        return Err(err("AERO_UINT256_UNDERFLOW"));
    }
    Ok(a - b)
}
fn div(a: BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        return Err(err("AERO_DIVISION_BY_ZERO"));
    }
    Ok(a / b)
}
fn f(x: &BigUint, y: &BigUint) -> Result<BigUint> {
    let e = BigUint::from(10u8).pow(18);
    let a = div(mul(x, y)?, &e)?;
    let b = add(&div(mul(x, x)?, &e)?, &div(mul(y, y)?, &e)?)?;
    div(mul(&a, &b)?, &e)
}
fn derivative(x: &BigUint, y: &BigUint) -> Result<BigUint> {
    let e = BigUint::from(10u8).pow(18);
    let first = div(
        mul(&mul(&BigUint::from(3u8), x)?, &div(mul(y, y)?, &e)?)?,
        &e,
    )?;
    let second = div(mul(&div(mul(x, x)?, &e)?, x)?, &e)?;
    add(&first, &second)
}
fn k_raw(x: &BigUint, y: &BigUint, dec: [&BigUint; 2]) -> Result<BigUint> {
    let e = BigUint::from(10u8).pow(18);
    f(&div(mul(x, &e)?, dec[0])?, &div(mul(y, &e)?, dec[1])?)
}
fn get_y(
    x: &BigUint,
    target: &BigUint,
    mut y: BigUint,
    dec: [&BigUint; 2],
    velodrome: bool,
) -> Result<BigUint> {
    let e = BigUint::from(10u8).pow(18);
    for _ in 0..255 {
        let value = f(x, &y)?;
        if value < *target {
            let mut dy = div(mul(&sub(target, &value)?, &e)?, &derivative(x, &y)?)?;
            if dy.is_zero() {
                if value == *target {
                    return Ok(y);
                }
                let yp = add(&y, &BigUint::one())?;
                // Velodrome b3065d8b checks already-normalized balances
                // with _f. Aerodrome 1ba30815 calls _k and scales again.
                let next = if velodrome {
                    f(x, &yp)?
                } else {
                    k_raw(x, &yp, dec)?
                };
                if next > *target {
                    return Ok(yp);
                }
                dy = BigUint::one();
            }
            y = add(&y, &dy)?;
        } else {
            let mut dy = div(mul(&sub(&value, target)?, &e)?, &derivative(x, &y)?)?;
            if dy.is_zero() {
                if value == *target || f(x, &sub(&y, &BigUint::one())?)? < *target {
                    return Ok(y);
                }
                dy = BigUint::one();
            }
            y = sub(&y, &dy)?;
        }
    }
    Err(err("AERO_STABLE_DID_NOT_CONVERGE"))
}
pub fn quote_exact_in(
    balances: [&BigUint; 2],
    decimals: [&BigUint; 2],
    fee: u64,
    input: &BigUint,
    zero: bool,
    stable: bool,
) -> Result<AeroQuote> {
    quote_model(balances, decimals, fee, input, zero, stable, false)
}
/// Pinned Velodrome b3065d8b semantics, including its normalized rounding check.
pub fn quote_exact_in_velodrome(
    balances: [&BigUint; 2],
    decimals: [&BigUint; 2],
    fee: u64,
    input: &BigUint,
    zero: bool,
    stable: bool,
) -> Result<AeroQuote> {
    quote_model(balances, decimals, fee, input, zero, stable, true)
}
fn quote_model(
    balances: [&BigUint; 2],
    decimals: [&BigUint; 2],
    fee: u64,
    input: &BigUint,
    zero: bool,
    stable: bool,
    velodrome: bool,
) -> Result<AeroQuote> {
    if fee > 300
        || input.is_zero()
        || balances.iter().any(|b| b.is_zero())
        || decimals.iter().any(|d| d.is_zero() || d.bits() > 60)
    {
        return Err(err("AERO_INPUT_PARAMETERS"));
    }
    for n in [balances[0], balances[1], input] {
        checked(n.clone())?;
    }
    let fee_amount = mul(input, &BigUint::from(fee))? / BigUint::from(10000u64);
    let effective = sub(input, &fee_amount)?;
    let (i, j) = if zero { (0, 1) } else { (1, 0) };
    let amount_out = if !stable {
        div(
            mul(&effective, balances[j])?,
            &add(balances[i], &effective)?,
        )?
    } else {
        let e = BigUint::from(10u8).pow(18);
        let target = k_raw(balances[0], balances[1], decimals)?;
        let x = div(mul(balances[i], &e)?, decimals[i])?;
        let y = div(mul(balances[j], &e)?, decimals[j])?;
        let dx = div(mul(&effective, &e)?, decimals[i])?;
        let next = get_y(&add(&x, &dx)?, &target, y.clone(), decimals, velodrome)?;
        div(mul(&sub(&y, &next)?, decimals[j])?, &e)?
    };
    if amount_out.is_zero() || amount_out >= *balances[j] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = [balances[0].clone(), balances[1].clone()];
    next_balances[i] = add(balances[i], &effective)?;
    next_balances[j] = sub(balances[j], &amount_out)?;
    // Pool.swap checks the post-transfer invariant, not only its public quote.
    let before = if stable {
        k_raw(balances[0], balances[1], decimals)?
    } else {
        mul(balances[0], balances[1])?
    };
    let after = if stable {
        k_raw(&next_balances[0], &next_balances[1], decimals)?
    } else {
        mul(&next_balances[0], &next_balances[1])?
    };
    if after < before {
        return Err(err("AERO_POST_SWAP_INVARIANT"));
    }
    Ok(AeroQuote {
        amount_out,
        next_balances,
    })
}
