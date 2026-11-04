//! ComposableStable math at official Balancer revision
//! e91a2b643a49856f51a648d175667c1b48cf3377 (separately pinned from Weighted).
//! Regular swap path drops BPT before the N-token StableMath invariant/solve.
//! BPT math is exposed only AFTER pending protocol-fee accounting; this module
//! does not reconstruct actual supply, rate-cache refreshes or fee minting.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};
const ONE: u64 = 1_000_000_000_000_000_000;
const PRECISION: u32 = 1000;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposableQuote {
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BptMathQuote {
    pub amount_out: BigUint,
    pub next_underlying_balances: Vec<BigUint>,
    /// Supply after the single-token operation, on a supplied post-fee basis.
    pub next_actual_supply: BigUint,
}
fn checked(x: BigUint) -> Result<BigUint> {
    if x.bits() > 256 {
        Err(err("BALANCER_COMPOSABLE_UINT256"))
    } else {
        Ok(x)
    }
}
fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a + b)
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        Err(err("BALANCER_COMPOSABLE_UNDERFLOW"))
    } else {
        Ok(a - b)
    }
}
fn down(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("BALANCER_COMPOSABLE_ZERO_DIVISION"))
    } else {
        Ok(a / b)
    }
}
fn up(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("BALANCER_COMPOSABLE_ZERO_DIVISION"))
    } else if a.is_zero() {
        Ok(a.clone())
    } else {
        Ok((a - BigUint::one()) / b + BigUint::one())
    }
}
fn md(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    down(&mul(a, b)?, &BigUint::from(ONE))
}
fn mu(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    up(&mul(a, b)?, &BigUint::from(ONE))
}
fn dd(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    down(&mul(a, &BigUint::from(ONE))?, b)
}
fn du(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    up(&mul(a, &BigUint::from(ONE))?, b)
}
fn complement(x: &BigUint) -> BigUint {
    let one = BigUint::from(ONE);
    if x >= &one {
        BigUint::zero()
    } else {
        one - x
    }
}
fn sum(xs: &[BigUint]) -> Result<BigUint> {
    xs.iter().try_fold(BigUint::zero(), |a, b| add(&a, b))
}
fn near(a: &BigUint, b: &BigUint) -> bool {
    if a > b {
        a - b <= BigUint::one()
    } else {
        b - a <= BigUint::one()
    }
}
fn validate(
    b: &[BigUint],
    amp: &BigUint,
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
) -> Result<()> {
    if !(2..=5).contains(&b.len()) || b.len() != scales.len() {
        return Err(err("BALANCER_COMPOSABLE_LENGTH"));
    }
    for x in [amp, fee, input] {
        checked(x.clone())?;
    }
    if amp < &BigUint::from(1000u32) || amp > &BigUint::from(5_000_000u32) {
        return Err(err("BALANCER_COMPOSABLE_AMP"));
    }
    if fee > &BigUint::from(ONE) {
        return Err(err("BALANCER_COMPOSABLE_FEE"));
    }
    if input.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    for (balance, scale) in b.iter().zip(scales) {
        if balance.bits() > 112 {
            return Err(err("BALANCER_COMPOSABLE_UINT112"));
        }
        if balance.is_zero() {
            return Err(err("INSUFFICIENT_LIQUIDITY"));
        }
        checked(scale.clone())?;
        // Effective decimal/rate factors are caller-observed, not assumed powers of ten.
        if scale.is_zero() {
            return Err(err("BALANCER_COMPOSABLE_SCALE"));
        }
    }
    Ok(())
}
fn invariant(amp: &BigUint, b: &[BigUint]) -> Result<BigUint> {
    let n = BigUint::from(b.len());
    let p = BigUint::from(PRECISION);
    let s = sum(b)?;
    if s.is_zero() {
        return Ok(s);
    }
    let ann = mul(amp, &n)?;
    let mut d = s.clone();
    for _ in 0..255 {
        let mut dp = d.clone();
        for x in b {
            dp = down(&mul(&dp, &d)?, &mul(x, &n)?)?;
        }
        let previous = d;
        let numerator = mul(
            &add(&down(&mul(&ann, &s)?, &p)?, &mul(&dp, &n)?)?,
            &previous,
        )?;
        let denominator = add(
            &down(&mul(&sub(&ann, &p)?, &previous)?, &p)?,
            &mul(&(&n + 1u8), &dp)?,
        )?;
        d = down(&numerator, &denominator)?;
        if near(&d, &previous) {
            return Ok(d);
        }
    }
    Err(err("BALANCER_COMPOSABLE_INVARIANT_CONVERGENCE"))
}
fn token_balance(amp: &BigUint, b: &[BigUint], d: &BigUint, index: usize) -> Result<BigUint> {
    let n = BigUint::from(b.len());
    let ann = mul(amp, &n)?;
    let precision = BigUint::from(PRECISION);
    // Preserve P_D's token order and each intermediate division.
    let mut pd = mul(&b[0], &n)?;
    for x in &b[1..] {
        pd = down(&mul(&mul(&pd, x)?, &n)?, d)?;
    }
    let s = sub(&sum(b)?, &b[index])?;
    let inv2 = mul(d, d)?;
    let c = mul(&mul(&up(&inv2, &mul(&ann, &pd)?)?, &precision)?, &b[index])?;
    let linear = add(&s, &mul(&down(d, &ann)?, &precision)?)?;
    let mut y = up(&add(&inv2, &c)?, &add(d, &linear)?)?;
    for _ in 0..255 {
        let prev = y;
        y = up(
            &add(&mul(&prev, &prev)?, &c)?,
            &sub(&add(&mul(&prev, &BigUint::from(2u8))?, &linear)?, d)?,
        )?;
        if near(&y, &prev) {
            return Ok(y);
        }
    }
    Err(err("BALANCER_COMPOSABLE_BALANCE_CONVERGENCE"))
}

/// Token-to-token exact input, full registered order INCLUDING BPT. Supports
/// 2..=5 underlying assets. Effective scales must match the callback AFTER
/// `_cacheTokenRatesIfNecessary`; the caller proves rate/cache and block binding.
/// Current amplification is scaled by 1000. `bpt_index` may be anywhere.
/// BPT input/output is deliberately rejected here. No supply/fee history is
/// required for regular swaps; every uint256/uint112 arithmetic bound is checked.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    balances: &[BigUint],
    amplification: &BigUint,
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
    bpt_index: usize,
) -> Result<ComposableQuote> {
    if !(3..=6).contains(&balances.len()) || scales.len() != balances.len() {
        return Err(err("BALANCER_COMPOSABLE_LENGTH"));
    }
    if [index_in, index_out, bpt_index]
        .iter()
        .any(|&i| i >= balances.len())
        || index_in == index_out
    {
        return Err(err("BALANCER_COMPOSABLE_INDEX"));
    }
    if index_in == bpt_index || index_out == bpt_index {
        return Err(err("BALANCER_COMPOSABLE_BPT_REQUIRES_SUPPLY"));
    }
    if scales[bpt_index] != BigUint::from(ONE) || balances[bpt_index].bits() > 112 {
        return Err(err("BALANCER_COMPOSABLE_BPT_STATE"));
    }
    let underlying: Vec<_> = balances
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != bpt_index)
        .map(|(_, v)| v.clone())
        .collect();
    let factors: Vec<_> = scales
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != bpt_index)
        .map(|(_, v)| v.clone())
        .collect();
    validate(&underlying, amplification, &factors, fee, input)?;
    let i = index_in - usize::from(index_in > bpt_index);
    let j = index_out - usize::from(index_out > bpt_index);
    // BaseGeneralPool: raw fee first, then balance/input upscaling.
    let net = sub(input, &mu(input, fee)?)?;
    let mut b = underlying
        .iter()
        .zip(&factors)
        .map(|(v, s)| md(v, s))
        .collect::<Result<Vec<_>>>()?;
    let d = invariant(amplification, &b)?;
    b[i] = add(&b[i], &md(&net, &factors[i])?)?;
    let final_out = token_balance(amplification, &b, &d, j)?;
    let scaled_out = sub(&sub(&b[j], &final_out)?, &BigUint::one())?;
    let amount_out = dd(&scaled_out, &factors[j])?;
    if amount_out.is_zero() || amount_out >= balances[index_out] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = balances.to_vec();
    next_balances[index_in] = add(&balances[index_in], input)?;
    if next_balances[index_in].bits() > 112 {
        return Err(err("BALANCER_COMPOSABLE_UINT112"));
    }
    next_balances[index_out] = sub(&balances[index_out], &amount_out)?;
    Ok(ComposableQuote {
        amount_out,
        next_balances,
    })
}

/// Single-token exact-input BPT math, using original StableMath fee allocation.
/// This is a mathematical kernel, NOT an executable Composable pool quote.
/// `balances`/`scales` EXCLUDE BPT; `actual_supply_after_fees` MUST already
/// include all pending protocol swap/yield fee BPT. `token_to_bpt=true` joins
/// with an underlying token, otherwise input is BPT and output is that token.
/// Does not enforce Vault BPT inventory, mint pending fees, update rate caches,
/// minimum BPT/token balances or lastJoinExitData. Integration must execute those
/// obligations before admitting BPT swaps. Underlying cash and math are checked.
#[allow(clippy::too_many_arguments)]
pub fn quote_bpt_exact_in_after_fees(
    balances: &[BigUint],
    amplification: &BigUint,
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    actual_supply_after_fees: &BigUint,
    token_index: usize,
    token_to_bpt: bool,
) -> Result<BptMathQuote> {
    validate(balances, amplification, scales, fee, input)?;
    if token_index >= balances.len() {
        return Err(err("BALANCER_COMPOSABLE_INDEX"));
    }
    checked(actual_supply_after_fees.clone())?;
    if actual_supply_after_fees.is_zero() {
        return Err(err("BALANCER_COMPOSABLE_SUPPLY"));
    }
    let one = BigUint::from(ONE);
    let mut b = balances
        .iter()
        .zip(scales)
        .map(|(v, s)| md(v, s))
        .collect::<Result<Vec<_>>>()?;
    let d = invariant(amplification, &b)?;
    let total = sum(&b)?;
    let amount_out;
    let next_actual_supply;
    let mut next_underlying_balances = balances.to_vec();
    if token_to_bpt {
        let scaled_input = md(input, &scales[token_index])?;
        let mut ratios = Vec::with_capacity(b.len());
        let mut ratio_with_fees = BigUint::zero();
        for (k, x) in b.iter().enumerate() {
            let amount = if k == token_index {
                scaled_input.clone()
            } else {
                BigUint::zero()
            };
            let ratio = dd(&add(x, &amount)?, x)?;
            ratio_with_fees = add(&ratio_with_fees, &md(&ratio, &dd(x, &total)?)?)?;
            ratios.push(ratio);
        }
        for k in 0..b.len() {
            let amount = if k == token_index {
                scaled_input.clone()
            } else {
                BigUint::zero()
            };
            let net = if ratios[k] > ratio_with_fees {
                let non_taxable = md(&b[k], &sub(&ratio_with_fees, &one)?)?;
                let taxable = sub(&amount, &non_taxable)?;
                add(&non_taxable, &md(&taxable, &sub(&one, fee)?)?)?
            } else {
                amount
            };
            b[k] = add(&b[k], &net)?;
        }
        let ratio = dd(&invariant(amplification, &b)?, &d)?;
        amount_out = if ratio > one {
            md(actual_supply_after_fees, &(ratio - &one))?
        } else {
            BigUint::zero()
        };
        next_actual_supply = add(actual_supply_after_fees, &amount_out)?;
        next_underlying_balances[token_index] = add(&balances[token_index], input)?;
        if next_underlying_balances[token_index].bits() > 112 {
            return Err(err("BALANCER_COMPOSABLE_UINT112"));
        }
    } else {
        if input >= actual_supply_after_fees {
            return Err(err("BALANCER_COMPOSABLE_SUPPLY"));
        }
        next_actual_supply = sub(actual_supply_after_fees, input)?;
        let new_d = mu(&du(&next_actual_supply, actual_supply_after_fees)?, &d)?;
        let y = token_balance(amplification, &b, &new_d, token_index)?;
        let gross = sub(&b[token_index], &y)?;
        let taxable = mu(&gross, &complement(&dd(&b[token_index], &total)?))?;
        let non_taxable = sub(&gross, &taxable)?;
        let scaled_out = add(&non_taxable, &md(&taxable, &sub(&one, fee)?)?)?;
        amount_out = dd(&scaled_out, &scales[token_index])?;
        if amount_out >= balances[token_index] {
            return Err(err("INSUFFICIENT_OUTPUT"));
        }
        next_underlying_balances[token_index] = sub(&balances[token_index], &amount_out)?;
    }
    if amount_out.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    Ok(BptMathQuote {
        amount_out,
        next_underlying_balances,
        next_actual_supply,
    })
}
