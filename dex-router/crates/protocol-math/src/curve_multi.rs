//! Original Factory Plain3Basic, Plain4Basic and MetaUSD (direct coin/LP exchange).
//! curvefi/curve-factory@5272fd2b1199f185d9cd94822be9262c387dc2c2, Vyper 0.3.1.
//! Balances exclude admin fees. Rates and A_precise must be evaluated at the same
//! block. No transfer/rebase or external provider calls occur in this module.
use num_bigint::BigUint;
use num_traits::Zero;
use xone_domain::{err, Result};

pub(crate) const PRECISION: u64 = 1_000_000_000_000_000_000;
pub(crate) const FEE_DENOMINATOR: u64 = 10_000_000_000;
pub(crate) const ADMIN_FEE: u64 = 5_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurveQuote {
    pub amount_out: BigUint,
    /// Internal balances after removing both user output and admin fee.
    pub next_balances: Vec<BigUint>,
    /// Raw output-token fee, rounded independently (informational).
    pub fee_amount: BigUint,
    pub admin_fee_amount: BigUint,
}

/// Plain3Basic/Plain4Basic only. Fee denominator is 1e10, A is scaled by 100.
/// Source arithmetic is checked at each uint256 intermediate, before division.
/// Zero input is deliberately passed through: source rounding may revert or
/// return dust; this function does not silently return a fabricated zero quote.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    balances: &[BigUint],
    amp: &BigUint,
    rates: &[BigUint],
    fee: &BigUint,
    admin_fee: &BigUint,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<CurveQuote> {
    if ![3, 4].contains(&balances.len()) {
        return Err(err("CURVE_MULTI_UNSUPPORTED_COIN_COUNT"));
    }
    quote(
        balances, amp, rates, fee, admin_fee, input, index_in, index_out,
    )
}

/// MetaUSD `exchange`, between the meta coin and base LP token, using a supplied
/// current `BASE_POOL.get_virtual_price()`. This is not `exchange_underlying`.
#[allow(clippy::too_many_arguments)]
pub fn quote_meta_exact_in(
    balances: &[BigUint],
    amp: &BigUint,
    meta_rate: &BigUint,
    base_virtual_price: &BigUint,
    fee: &BigUint,
    admin_fee: &BigUint,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<CurveQuote> {
    if balances.len() != 2 {
        return Err(err("CURVE_META_INVALID_COIN_COUNT"));
    }
    quote(
        balances,
        amp,
        &[meta_rate.clone(), base_virtual_price.clone()],
        fee,
        admin_fee,
        input,
        index_in,
        index_out,
    )
}

/// The exact external actions needed by MetaUSD `exchange_underlying`.
/// A rate alone cannot determine base LP mint/burn amounts or their poststate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetaUnderlyingBoundary {
    BaseExchange { index_in: usize, index_out: usize },
    BaseDepositThenMeta { base_index: usize },
    MetaThenBaseWithdraw { base_index: usize },
}
pub fn meta_underlying_boundary(
    index_in: usize,
    index_out: usize,
) -> Result<MetaUnderlyingBoundary> {
    if index_in > 3 || index_out > 3 || index_in == index_out {
        return Err(err("CURVE_META_INVALID_UNDERLYING_INDEX"));
    }
    Ok(if index_in == 0 {
        MetaUnderlyingBoundary::MetaThenBaseWithdraw {
            base_index: index_out - 1,
        }
    } else if index_out == 0 {
        MetaUnderlyingBoundary::BaseDepositThenMeta {
            base_index: index_in - 1,
        }
    } else {
        MetaUnderlyingBoundary::BaseExchange {
            index_in: index_in - 1,
            index_out: index_out - 1,
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn quote(
    b: &[BigUint],
    amp: &BigUint,
    rates: &[BigUint],
    fee: &BigUint,
    admin: &BigUint,
    input: &BigUint,
    i: usize,
    j: usize,
) -> Result<CurveQuote> {
    validate(b, rates, amp, fee, admin, input, i, j)?;
    let p = u(PRECISION);
    let fd = u(FEE_DENOMINATOR);
    let xp = normalize(b, rates)?;
    let x = add(&xp[i], &div(&mul(input, &rates[i])?, &p)?)?;
    let d = invariant(&xp, amp)?;
    let y = solve_y(i, j, &x, &xp, amp, &d)?;
    let dy = sub(&sub(&xp[j], &y)?, &u(1))?;
    let fee_xp = div(&mul(&dy, fee)?, &fd)?;
    let amount_out = div(&mul(&sub(&dy, &fee_xp)?, &p)?, &rates[j])?;
    let admin_xp = div(&mul(&fee_xp, admin)?, &fd)?;
    let admin_fee_amount = div(&mul(&admin_xp, &p)?, &rates[j])?;
    let fee_amount = div(&mul(&fee_xp, &p)?, &rates[j])?;
    let mut next_balances = b.to_vec();
    next_balances[i] = add(&b[i], input)?;
    next_balances[j] = sub(&sub(&b[j], &amount_out)?, &admin_fee_amount)?;
    Ok(CurveQuote {
        amount_out,
        next_balances,
        fee_amount,
        admin_fee_amount,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate(
    b: &[BigUint],
    r: &[BigUint],
    amp: &BigUint,
    fee: &BigUint,
    admin: &BigUint,
    input: &BigUint,
    i: usize,
    j: usize,
) -> Result<()> {
    if b.len() != r.len() || b.len() < 2 || i == j || i >= b.len() || j >= b.len() {
        return Err(err("CURVE_INVALID_SHAPE_OR_INDEX"));
    }
    for v in b.iter().chain(r).chain([amp, fee, admin, input]) {
        ensure(v)?;
    }
    if amp < &u(100) || amp >= &u(100_000_000) {
        return Err(err("CURVE_INVALID_AMPLIFICATION"));
    }
    if fee > &u(FEE_DENOMINATOR) {
        return Err(err("CURVE_INVALID_FEE"));
    }
    if admin != &u(ADMIN_FEE) {
        return Err(err("CURVE_INVALID_ADMIN_FEE"));
    }
    if r.iter().any(Zero::is_zero) {
        return Err(err("CURVE_INVALID_RATE"));
    }
    Ok(())
}
pub(crate) fn normalize(b: &[BigUint], rates: &[BigUint]) -> Result<Vec<BigUint>> {
    b.iter()
        .zip(rates)
        .map(|(b, r)| div(&mul(b, r)?, &u(PRECISION)))
        .collect()
}
/// Old generic D divides by x*N at EVERY coin, preserving floor rounding.
pub fn invariant(xp: &[BigUint], amp: &BigUint) -> Result<BigUint> {
    if !(2..=4).contains(&xp.len()) {
        return Err(err("CURVE_INVALID_COIN_COUNT"));
    }
    for x in xp.iter().chain([amp]) {
        ensure(x)?;
    }
    let sum = xp.iter().try_fold(u(0), |s, x| add(&s, x))?;
    if sum.is_zero() {
        return Ok(sum);
    }
    let n = u(xp.len() as u64);
    let ap = u(100);
    let ann = mul(amp, &n)?;
    let mut d = sum.clone();
    for _ in 0..255 {
        let mut dp = d.clone();
        for x in xp {
            dp = div(&mul(&dp, &d)?, &mul(x, &n)?)?;
        }
        let old = d.clone();
        let numerator = mul(&add(&div(&mul(&ann, &sum)?, &ap)?, &mul(&dp, &n)?)?, &d)?;
        let denominator = add(
            &div(&mul(&sub(&ann, &ap)?, &d)?, &ap)?,
            &mul(&u(xp.len() as u64 + 1), &dp)?,
        )?;
        d = div(&numerator, &denominator)?;
        if distance(&d, &old) <= u(1) {
            return Ok(d);
        }
    }
    Err(err("CURVE_D_DID_NOT_CONVERGE"))
}
/// Identical quadratic step in pinned old generic pools and NG, with explicit D.
pub(crate) fn solve_y(
    i: usize,
    j: usize,
    x: &BigUint,
    xp: &[BigUint],
    amp: &BigUint,
    d: &BigUint,
) -> Result<BigUint> {
    if i == j || i >= xp.len() || j >= xp.len() {
        return Err(err("CURVE_INVALID_INDEX"));
    }
    let n = u(xp.len() as u64);
    let ann = mul(amp, &n)?;
    let mut sum = u(0);
    let mut c = d.clone();
    for (k, v) in xp.iter().enumerate() {
        if k == j {
            continue;
        }
        let v = if k == i { x } else { v };
        sum = add(&sum, v)?;
        c = div(&mul(&c, d)?, &mul(v, &n)?)?;
    }
    c = div(&mul(&mul(&c, d)?, &u(100))?, &mul(&ann, &n)?)?;
    let b = add(&sum, &div(&mul(d, &u(100))?, &ann)?)?;
    let mut y = d.clone();
    for _ in 0..255 {
        let old = y.clone();
        y = div(
            &add(&mul(&y, &y)?, &c)?,
            &sub(&add(&mul(&u(2), &y)?, &b)?, d)?,
        )?;
        if distance(&y, &old) <= u(1) {
            return Ok(y);
        }
    }
    Err(err("CURVE_Y_DID_NOT_CONVERGE"))
}
pub(crate) fn u(v: u64) -> BigUint {
    BigUint::from(v)
}
pub(crate) fn ensure(v: &BigUint) -> Result<()> {
    if v.bits() > 256 {
        Err(err("CURVE_UINT256_OVERFLOW"))
    } else {
        Ok(())
    }
}
pub(crate) fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    let v = a + b;
    ensure(&v)?;
    Ok(v)
}
pub(crate) fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    let v = a * b;
    ensure(&v)?;
    Ok(v)
}
pub(crate) fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        Err(err("CURVE_UINT256_UNDERFLOW"))
    } else {
        Ok(a - b)
    }
}
pub(crate) fn div(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        Err(err("CURVE_DIVISION_BY_ZERO"))
    } else {
        Ok(a / b)
    }
}
pub(crate) fn distance(a: &BigUint, b: &BigUint) -> BigUint {
    if a > b {
        a - b
    } else {
        b - a
    }
}
