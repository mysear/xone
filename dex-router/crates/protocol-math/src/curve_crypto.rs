//! curvefi/curve-crypto-contract@d7d04cd9ae038970e40be850df99de8c1ff7241b,
//! original two/CurveCryptoSwap2.vy, Vyper 0.3.3. ANN uses A_MULTIPLIER=10000.
//! This is gamma-based CryptoSwap math, not StableSwap. Price adjustments and
//! LP admin-claim/gulp branches remain explicit legacy boundaries; the full API
//! below executes them for the pinned original LP and nominal ERC20 profile.
use crate::curve_multi::{add, distance, div, ensure, mul, sub, u, PRECISION};
use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};
use xone_domain::{err, Result};

fn wad() -> BigUint {
    u(PRECISION)
}
fn pow(n: u32) -> BigUint {
    u(10).pow(n)
}
// Vyper unsafe_* has EVM uint256 wrapping semantics (division by zero => zero).
fn wrap(v: BigUint) -> BigUint {
    v & ((BigUint::one() << 256) - BigUint::one())
}
fn wm(a: &BigUint, b: &BigUint) -> BigUint {
    wrap(a * b)
}
fn wa(a: &BigUint, b: &BigUint) -> BigUint {
    wrap(a + b)
}
fn wd(a: &BigUint, b: &BigUint) -> BigUint {
    if b.is_zero() {
        u(0)
    } else {
        a / b
    }
}
fn params(amp: &BigUint, gamma: &BigUint) -> Result<()> {
    ensure(amp)?;
    ensure(gamma)?;
    if amp < &u(4000) || amp > &u(4_000_000_000) {
        return Err(err("CURVE_CRYPTO_UNSAFE_A"));
    }
    if gamma < &pow(10) || gamma > &u(20_000_000_000_000_000) {
        return Err(err("CURVE_CRYPTO_UNSAFE_GAMMA"));
    }
    Ok(())
}
fn geometric_mean(xp: &[BigUint]) -> Result<BigUint> {
    if xp.len() != 2 {
        return Err(err("CURVE_CRYPTO_INVALID_COIN_COUNT"));
    }
    let (x0, x1) = if xp[0] >= xp[1] {
        (&xp[0], &xp[1])
    } else {
        (&xp[1], &xp[0])
    };
    let mut d = x0.clone();
    for _ in 0..255 {
        let old = d.clone();
        d = wd(&add(&d, &div(&mul(x0, x1)?, &d)?)?, &u(2));
        let diff = distance(&d, &old);
        if diff <= u(1) || mul(&diff, &wad())? < d {
            return Ok(d);
        }
    }
    Err(err("CURVE_CRYPTO_GEOMEAN_DID_NOT_CONVERGE"))
}
/// Cached invariant refresh during ramp and post-swap price upkeep.
pub fn newton_d(amp: &BigUint, gamma: &BigUint, xp: &[BigUint]) -> Result<BigUint> {
    params(amp, gamma)?;
    if xp.len() != 2 {
        return Err(err("CURVE_CRYPTO_INVALID_COIN_COUNT"));
    }
    for v in xp {
        ensure(v)?;
    }
    let x = if xp[0] >= xp[1] {
        xp.to_vec()
    } else {
        vec![xp[1].clone(), xp[0].clone()]
    };
    if x[0] < pow(9) || x[0] > pow(33) || div(&mul(&x[1], &wad())?, &x[0])? < pow(14) {
        return Err(err("CURVE_CRYPTO_UNSAFE_BALANCE"));
    }
    let mut d = mul(&u(2), &geometric_mean(&x)?)?;
    let s = add(&x[0], &x[1])?;
    let g = add(gamma, &wad())?;
    for _ in 0..255 {
        let old = d.clone();
        if d.is_zero() {
            return Err(err("CURVE_CRYPTO_ZERO_D"));
        }
        let k = wd(
            &mul(&wd(&mul(&u(4_000_000_000_000_000_000), &x[0])?, &d), &x[1])?,
            &d,
        );
        let gk = add(&distance(&g, &k), &u(1))?;
        let m1 = wd(
            &mul(
                &mul(&wd(&mul(&wd(&mul(&wad(), &d)?, gamma), &gk)?, gamma), &gk)?,
                &u(10000),
            )?,
            amp,
        );
        let m2 = wd(&mul(&u(4_000_000_000_000_000_000), &k)?, &gk);
        let neg = sub(
            &add(
                &add(&s, &wd(&mul(&s, &m2)?, &wad()))?,
                &div(&mul(&m1, &u(2))?, &k)?,
            )?,
            &wd(&mul(&m2, &d)?, &wad()),
        )?;
        let plus = div(&mul(&d, &add(&neg, &s)?)?, &neg)?;
        let mut minus = div(&mul(&d, &d)?, &neg)?;
        let adjust = div(
            &mul(
                &wd(&mul(&d, &div(&m1, &neg)?)?, &wad()),
                &distance(&wad(), &k),
            )?,
            &k,
        )?;
        minus = if wad() > k {
            add(&minus, &adjust)?
        } else {
            sub(&minus, &adjust)?
        };
        d = if plus > minus {
            plus - minus
        } else {
            (minus - plus) / u(2)
        };
        if mul(&distance(&d, &old), &pow(14))? < std::cmp::max(pow(16), d.clone()) {
            for v in &x {
                let f = div(&mul(v, &wad())?, &d)?;
                if f < pow(16) || f > pow(20) {
                    return Err(err("CURVE_CRYPTO_UNSAFE_FRACTION"));
                }
            }
            return Ok(d);
        }
    }
    Err(err("CURVE_CRYPTO_D_DID_NOT_CONVERGE"))
}

pub fn newton_y(
    amp: &BigUint,
    gamma: &BigUint,
    xp: &[BigUint],
    d: &BigUint,
    j: usize,
) -> Result<BigUint> {
    params(amp, gamma)?;
    ensure(d)?;
    if xp.len() != 2 || j > 1 {
        return Err(err("CURVE_CRYPTO_INVALID_INDEX"));
    }
    for v in xp {
        ensure(v)?;
    }
    if d < &pow(17) || d > &pow(33) {
        return Err(err("CURVE_CRYPTO_UNSAFE_D"));
    }
    let x = &xp[1 - j];
    let mut y = div(&mul(d, d)?, &mul(x, &u(4))?)?;
    let ki = div(&mul(&u(2_000_000_000_000_000_000), x)?, d)?;
    if ki < u(20_000_000_000_000_000) || ki > pow(20) * u(2) {
        return Err(err("CURVE_CRYPTO_UNSAFE_FRACTION"));
    }
    let limit = std::cmp::max(std::cmp::max(x / pow(14), d / pow(14)), u(100));
    let g = add(gamma, &wad())?;
    for _ in 0..255 {
        let old = y.clone();
        let k = wd(&mul(&mul(&ki, &y)?, &u(2))?, d);
        let s = add(x, &y)?;
        let gk = add(&distance(&g, &k), &u(1))?;
        let m1 = wd(
            &mul(
                &mul(&wd(&mul(&wd(&mul(&wad(), d)?, gamma), &gk)?, gamma), &gk)?,
                &u(10000),
            )?,
            amp,
        );
        let m2 = wd(&add(&wad(), &mul(&u(2_000_000_000_000_000_000), &k)?)?, &gk);
        let yf = add(&add(&mul(&wad(), &y)?, &mul(&s, &m2)?)?, &m1)?;
        let dyf = mul(d, &m2)?;
        if yf < dyf {
            y = old / u(2);
            continue;
        }
        let yf = yf - dyf;
        let f = div(&yf, &y)?;
        let mut minus = div(&m1, &f)?;
        let plus = add(
            &div(&add(&yf, &mul(&wad(), d)?)?, &f)?,
            &div(&mul(&minus, &wad())?, &k)?,
        )?;
        minus = add(&minus, &div(&mul(&wad(), &s)?, &f)?)?;
        y = if plus < minus {
            old.clone() / u(2)
        } else {
            plus - minus
        };
        if distance(&y, &old) < std::cmp::max(limit.clone(), wd(&y, &pow(14))) {
            let frac = wd(&mul(&y, &wad())?, d);
            if frac < pow(16) || frac > pow(20) {
                return Err(err("CURVE_CRYPTO_UNSAFE_FRACTION"));
            }
            return Ok(y);
        }
    }
    Err(err("CURVE_CRYPTO_Y_DID_NOT_CONVERGE"))
}
/// Original binomial 0.5**power, scaled by 1e18, including its 1e10 cutoff.
pub fn halfpow(power: &BigUint) -> Result<BigUint> {
    ensure(power)?;
    let int = power / wad();
    if int > u(59) {
        return Ok(u(0));
    }
    let other = power - &int * wad();
    let result = wad() / (BigUint::one() << int.to_usize().unwrap());
    if other.is_zero() {
        return Ok(result);
    }
    let mut term = wad();
    let mut s = wad();
    let mut neg = false;
    for i in 1..256u64 {
        let k = wm(&u(i), &wad());
        let c = &k - wad();
        let c = if other > c {
            neg = !neg;
            &other - c
        } else {
            c - &other
        };
        term = wd(&wm(&term, &wd(&c, &u(2))), &k);
        s = if neg {
            sub(&s, &term)?
        } else {
            add(&s, &term)?
        };
        if term < pow(10) {
            return Ok(wd(&mul(&result, &s)?, &wad()));
        }
    }
    Err(err("CURVE_CRYPTO_HALFPOW_DID_NOT_CONVERGE"))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptoParams {
    pub amp: BigUint,
    pub gamma: BigUint,
    pub mid_fee: BigUint,
    pub out_fee: BigUint,
    pub fee_gamma: BigUint,
    pub allowed_extra_profit: BigUint,
    pub adjustment_step: BigUint,
    pub admin_fee: BigUint,
    pub ma_half_time: BigUint,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptoState {
    pub invariant: BigUint,
    pub price_scale: BigUint,
    pub price_oracle: BigUint,
    pub last_prices: BigUint,
    pub last_prices_timestamp: u64,
    pub xcp_profit: BigUint,
    pub xcp_profit_a: BigUint,
    pub virtual_price: BigUint,
    pub not_adjusted: bool,
    pub future_a_gamma_time: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptoQuote {
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
    pub fee_rate: BigUint,
    pub next_state: CryptoState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CryptoPoststateBoundary {
    PriceScaleAdjustment,
    AdminFeeClaimAndGulp,
}
impl CryptoPoststateBoundary {
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::PriceScaleAdjustment => "CURVE_CRYPTO_BOUNDARY_PRICE_SCALE_ADJUSTMENT",
            Self::AdminFeeClaimAndGulp => "CURVE_CRYPTO_BOUNDARY_ADMIN_FEE_CLAIM_AND_GULP",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptoSwapAmounts {
    pub amount_out: BigUint,
    /// Balances after transfers and before tweak_price. Use quote_exact_in for
    /// complete supported poststate; an admin-claim branch may gulp balances.
    pub next_balances: Vec<BigUint>,
    pub fee_rate: BigUint,
    pub invariant_used: BigUint,
}
fn fee(xp: &[BigUint], p: &CryptoParams) -> Result<BigUint> {
    let sum = add(&xp[0], &xp[1])?;
    let k = wd(
        &mul(
            &div(&mul(&u(4_000_000_000_000_000_000), &xp[0])?, &sum)?,
            &xp[1],
        )?,
        &sum,
    );
    let f = div(
        &wm(&p.fee_gamma, &wad()),
        &sub(&wa(&p.fee_gamma, &wad()), &k)?,
    )?;
    Ok(wd(
        &add(&mul(&p.mid_fee, &f)?, &mul(&p.out_fee, &sub(&wad(), &f)?)?)?,
        &wad(),
    ))
}
fn normalize(b: &[BigUint], prec: &[BigUint], price: &BigUint) -> Result<Vec<BigUint>> {
    Ok(vec![
        mul(&b[0], &prec[0])?,
        div(&mul(&mul(&b[1], price)?, &prec[1])?, &wad())?,
    ])
}
#[allow(clippy::too_many_arguments)]
fn amounts(
    b: &[BigUint],
    prec: &[BigUint],
    p: &CryptoParams,
    s: &CryptoState,
    input: &BigUint,
    i: usize,
    j: usize,
) -> Result<CryptoSwapAmounts> {
    if b.len() != 2 || prec.len() != 2 || i > 1 || j > 1 || i == j {
        return Err(err("CURVE_CRYPTO_INVALID_SHAPE_OR_INDEX"));
    }
    for v in b.iter().chain(prec).chain([
        input,
        &p.amp,
        &p.gamma,
        &p.mid_fee,
        &p.out_fee,
        &p.fee_gamma,
        &p.allowed_extra_profit,
        &p.adjustment_step,
        &p.admin_fee,
        &p.ma_half_time,
        &s.invariant,
        &s.price_scale,
        &s.price_oracle,
        &s.last_prices,
        &s.xcp_profit,
        &s.xcp_profit_a,
        &s.virtual_price,
    ]) {
        ensure(v)?;
    }
    params(&p.amp, &p.gamma)?;
    if prec.iter().any(Zero::is_zero) || s.price_scale.is_zero() {
        return Err(err("CURVE_CRYPTO_INVALID_PRECISION_OR_PRICE"));
    }
    if p.mid_fee > pow(10) || p.out_fee > pow(10) || p.admin_fee > pow(10) {
        return Err(err("CURVE_CRYPTO_INVALID_FEE"));
    }
    let oldxp = normalize(b, prec, &s.price_scale)?;
    let d = if s.future_a_gamma_time > 0 {
        newton_d(&p.amp, &p.gamma, &oldxp)?
    } else {
        s.invariant.clone()
    };
    let mut next_balances = b.to_vec();
    next_balances[i] = add(&b[i], input)?;
    let mut xp = normalize(&next_balances, prec, &s.price_scale)?;
    let y = newton_y(&p.amp, &p.gamma, &xp, &d, j)?;
    let mut dy = sub(&sub(&xp[j], &y)?, &u(1))?;
    xp[j] = y;
    if j > 0 {
        dy = div(&mul(&dy, &wad())?, &s.price_scale)?;
    }
    dy = div(&dy, &prec[j])?;
    let fee_rate = fee(&xp, p)?;
    let amount_out = sub(&dy, &div(&mul(&fee_rate, &dy)?, &pow(10))?)?;
    next_balances[j] = sub(&b[j], &amount_out)?;
    Ok(CryptoSwapAmounts {
        amount_out,
        next_balances,
        fee_rate,
        invariant_used: d,
    })
}
/// Original get_dy output, including gamma, current price scale and dynamic fee.
/// Does not claim that tweak_price/LP admin claims would permit execution.
#[allow(clippy::too_many_arguments)]
pub fn quote_get_dy(
    balances: &[BigUint],
    precisions: &[BigUint],
    params: &CryptoParams,
    state: &CryptoState,
    input: &BigUint,
    i: usize,
    j: usize,
) -> Result<CryptoSwapAmounts> {
    amounts(balances, precisions, params, state, input, i, j)
}
/// Exact exchange and complete pricing poststate when no price adjustment or
/// admin claim is entered. Those branches fail with distinct boundary codes.
/// Assumes pool is live, transfers exact amounts, and no transfer callback.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    balances: &[BigUint],
    precisions: &[BigUint],
    p: &CryptoParams,
    s: &CryptoState,
    total_supply: &BigUint,
    timestamp: u64,
    input: &BigUint,
    i: usize,
    j: usize,
) -> Result<CryptoQuote> {
    if input.is_zero() {
        return Err(err("CURVE_CRYPTO_ZERO_INPUT"));
    }
    ensure(total_supply)?;
    let q = amounts(balances, precisions, p, s, input, i, j)?;
    let xp = normalize(&q.next_balances, precisions, &s.price_scale)?;
    let mut next = s.clone();
    if s.future_a_gamma_time > 0 && timestamp >= s.future_a_gamma_time {
        next.future_a_gamma_time = 1;
    }
    if s.last_prices_timestamp < timestamp {
        let power = wd(
            &wm(&u(timestamp - s.last_prices_timestamp), &wad()),
            &p.ma_half_time,
        );
        let alpha = halfpow(&power)?;
        next.price_oracle = wd(
            &add(
                &mul(&s.last_prices, &sub(&wad(), &alpha)?)?,
                &mul(&s.price_oracle, &alpha)?,
            )?,
            &wad(),
        );
        next.last_prices_timestamp = timestamp;
    }
    let d = newton_d(&p.amp, &p.gamma, &xp)?;
    let price = if input > &u(100000) && q.amount_out > u(100000) {
        let dx = mul(input, &precisions[i])?;
        let dy = mul(&q.amount_out, &precisions[j])?;
        if i == 0 {
            div(&mul(&dx, &wad())?, &dy)?
        } else {
            div(&mul(&dy, &wad())?, &dx)?
        }
    } else {
        u(0)
    };
    next.last_prices = if !price.is_zero() {
        price
    } else {
        let mut xp_derivative = xp.clone();
        let dx = wd(&xp[0], &pow(6));
        xp_derivative[0] = add(&xp_derivative[0], &dx)?;
        let y = newton_y(&p.amp, &p.gamma, &xp_derivative, &d, 1)?;
        div(&mul(&s.price_scale, &dx)?, &sub(&xp[1], &y)?)?
    };
    let mut virtual_price = wad();
    let mut profit = wad();
    if !s.virtual_price.is_zero() {
        let xcp = geometric_mean(&[
            wd(&d, &u(2)),
            div(&mul(&d, &wad())?, &mul(&u(2), &s.price_scale)?)?,
        ])?;
        virtual_price = div(&mul(&wad(), &xcp)?, total_supply)?;
        profit = div(&mul(&s.xcp_profit, &virtual_price)?, &s.virtual_price)?;
        if virtual_price < s.virtual_price && next.future_a_gamma_time == 0 {
            return Err(err("CURVE_CRYPTO_LOSS"));
        }
        if next.future_a_gamma_time == 1 {
            next.future_a_gamma_time = 0;
        }
    }
    let norm = distance(
        &div(&mul(&next.price_oracle, &wad())?, &s.price_scale)?,
        &wad(),
    );
    let step = std::cmp::max(p.adjustment_step.clone(), wd(&norm, &u(10)));
    let mut needs_adjustment = s.not_adjusted;
    // Preserve source short-circuit evaluation and checked underflow behavior.
    if !needs_adjustment
        && sub(&mul(&virtual_price, &u(2))?, &wad())?
            > add(&profit, &wm(&p.allowed_extra_profit, &u(2)))?
        && norm > step
        && !s.virtual_price.is_zero()
    {
        needs_adjustment = true;
    }
    if needs_adjustment {
        return Err(err((if norm > step && !s.virtual_price.is_zero() {
            CryptoPoststateBoundary::PriceScaleAdjustment
        } else {
            CryptoPoststateBoundary::AdminFeeClaimAndGulp
        })
        .error_code()));
    }
    next.invariant = d;
    next.virtual_price = virtual_price;
    next.xcp_profit = profit;
    Ok(CryptoQuote {
        amount_out: q.amount_out,
        next_balances: q.next_balances,
        fee_rate: q.fee_rate,
        next_state: next,
    })
}

/// Origin of an active packed A/gamma ramp. CryptoParams holds the future values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptoRamp {
    pub initial_amp: BigUint,
    pub initial_gamma: BigUint,
    pub initial_time: u64,
}
/// Nominal ERC20 transfer profile: no callbacks, rebasing, transfer fees or
/// aliases between pool/trader/receiver. raw_balances are actual pool ERC20
/// balances BEFORE the exchange; receiver_balance is the admin receiver's LP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptoFullLedger {
    pub total_supply: BigUint,
    pub receiver_balance: BigUint,
    pub raw_balances: Vec<BigUint>,
    pub admin_receiver_enabled: bool,
    pub ramp: Option<CryptoRamp>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptoFullQuote {
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
    pub fee_rate: BigUint,
    pub next_pricing_state: CryptoState,
    pub next_total_supply: BigUint,
    pub next_receiver_balance: BigUint,
    pub next_raw_balances: Vec<BigUint>,
    pub minted_admin: BigUint,
}
fn full_params(
    p: &CryptoParams,
    s: &CryptoState,
    l: &CryptoFullLedger,
    timestamp: u64,
) -> Result<CryptoParams> {
    let mut p = p.clone();
    if timestamp < s.future_a_gamma_time {
        let r = l
            .ramp
            .as_ref()
            .ok_or_else(|| err("CURVE_CRYPTO_MISSING_RAMP_ORIGIN"))?;
        // Original _A_gamma interpolates a weighted sum (including down ramps).
        let duration = sub(&u(s.future_a_gamma_time), &u(r.initial_time))?;
        let elapsed = sub(&u(timestamp), &u(r.initial_time))?;
        let remaining = sub(&duration, &elapsed)?;
        p.amp = div(
            &add(&mul(&r.initial_amp, &remaining)?, &mul(&p.amp, &elapsed)?)?,
            &duration,
        )?;
        p.gamma = div(
            &add(
                &mul(&r.initial_gamma, &remaining)?,
                &mul(&p.gamma, &elapsed)?,
            )?,
            &duration,
        )?;
    }
    Ok(p)
}
fn validate_full_ledger(l: &CryptoFullLedger) -> Result<()> {
    if l.raw_balances.len() != 2 {
        return Err(err("CURVE_CRYPTO_INVALID_RAW_BALANCES"));
    }
    for v in l
        .raw_balances
        .iter()
        .chain([&l.total_supply, &l.receiver_balance])
    {
        ensure(v)?;
    }
    if l.receiver_balance > l.total_supply {
        return Err(err("CURVE_CRYPTO_INVALID_LP_LEDGER"));
    }
    if let Some(r) = &l.ramp {
        ensure(&r.initial_amp)?;
        ensure(&r.initial_gamma)?;
        if r.initial_amp.bits() > 128 || r.initial_gamma.bits() > 128 {
            return Err(err("CURVE_CRYPTO_INVALID_PACKED_RAMP"));
        }
    }
    Ok(())
}
fn full_xcp(d: &BigUint, scale: &BigUint) -> Result<BigUint> {
    geometric_mean(&[wd(d, &u(2)), div(&mul(d, &wad())?, &mul(scale, &u(2))?)?])
}
fn full_claim(
    p: &CryptoParams,
    precision: &[BigUint],
    q: &mut CryptoFullQuote,
    enabled: bool,
) -> Result<()> {
    let s = &mut q.next_pricing_state;
    let mut profit = s.xcp_profit.clone();
    let old_profit_a = s.xcp_profit_a.clone();
    // Gulp precedes fee arithmetic even when the receiver is zero.
    q.next_balances = q.next_raw_balances.clone();
    if profit > old_profit_a {
        let fees = wd(
            &mul(&sub(&profit, &old_profit_a)?, &p.admin_fee)?,
            &u(20_000_000_000),
        );
        if !fees.is_zero() && enabled {
            let fraction = sub(
                &div(
                    &mul(&s.virtual_price, &wad())?,
                    &sub(&s.virtual_price, &fees)?,
                )?,
                &wad(),
            )?;
            // Original CurveTokenV4 @ d7d04cd9, mint_relative lines 175-188.
            // Its multiplication and both additions are CHECKED uint256.
            q.minted_admin = div(&mul(&q.next_total_supply, &fraction)?, &wad())?;
            q.next_total_supply = add(&q.next_total_supply, &q.minted_admin)?;
            q.next_receiver_balance = add(&q.next_receiver_balance, &q.minted_admin)?;
            profit = sub(&profit, &wm(&fees, &u(2)))?;
            s.xcp_profit = profit.clone();
        }
    }
    s.invariant = newton_d(
        &p.amp,
        &p.gamma,
        &normalize(&q.next_balances, precision, &s.price_scale)?,
    )?;
    s.virtual_price = div(
        &mul(&wad(), &full_xcp(&s.invariant, &s.price_scale)?)?,
        &q.next_total_supply,
    )?;
    if profit > old_profit_a {
        s.xcp_profit_a = profit;
    }
    Ok(())
}
/// Original external claim_admin_fees with the same bounded token profile as
/// quote_full_exact_in. Returns a fresh value; any error leaves inputs intact.
pub fn quote_full_claim_admin(
    precisions: &[BigUint],
    p: &CryptoParams,
    s: &CryptoState,
    ledger: &CryptoFullLedger,
    timestamp: u64,
) -> Result<CryptoFullQuote> {
    validate_full_ledger(ledger)?;
    if precisions.len() != 2 || precisions.iter().any(Zero::is_zero) {
        return Err(err("CURVE_CRYPTO_INVALID_PRECISION_OR_PRICE"));
    }
    for v in precisions.iter().chain([
        &p.admin_fee,
        &s.price_scale,
        &s.virtual_price,
        &s.xcp_profit,
        &s.xcp_profit_a,
    ]) {
        ensure(v)?;
    }
    let p = full_params(p, s, ledger, timestamp)?;
    let mut q = CryptoFullQuote {
        amount_out: u(0),
        next_balances: ledger.raw_balances.clone(),
        fee_rate: u(0),
        next_pricing_state: s.clone(),
        next_total_supply: ledger.total_supply.clone(),
        next_receiver_balance: ledger.receiver_balance.clone(),
        next_raw_balances: ledger.raw_balances.clone(),
        minted_admin: u(0),
    };
    full_claim(&p, precisions, &mut q, ledger.admin_receiver_enabled)?;
    Ok(q)
}

/// Full original exchange, tweak_price and conditional admin claim/gulp.
/// Keeps the legacy quote_exact_in boundary contract intact.
#[allow(clippy::too_many_arguments)]
pub fn quote_full_exact_in(
    balances: &[BigUint],
    precisions: &[BigUint],
    p: &CryptoParams,
    s: &CryptoState,
    ledger: &CryptoFullLedger,
    timestamp: u64,
    input: &BigUint,
    i: usize,
    j: usize,
) -> Result<CryptoFullQuote> {
    if input.is_zero() {
        return Err(err("CURVE_CRYPTO_ZERO_INPUT"));
    }
    validate_full_ledger(ledger)?;
    let future_params = p;
    let effective = full_params(p, s, ledger, timestamp)?;
    let p = &effective;
    let total_supply = &ledger.total_supply;
    let q = amounts(balances, precisions, p, s, input, i, j)?;
    let xp = normalize(&q.next_balances, precisions, &s.price_scale)?;
    let mut next = s.clone();
    if s.future_a_gamma_time > 0 && timestamp >= s.future_a_gamma_time {
        next.future_a_gamma_time = 1;
    }
    if s.last_prices_timestamp < timestamp {
        let power = wd(
            &wm(&u(timestamp - s.last_prices_timestamp), &wad()),
            &p.ma_half_time,
        );
        let alpha = halfpow(&power)?;
        next.price_oracle = wd(
            &add(
                &mul(&s.last_prices, &sub(&wad(), &alpha)?)?,
                &mul(&s.price_oracle, &alpha)?,
            )?,
            &wad(),
        );
        next.last_prices_timestamp = timestamp;
    }
    let d = newton_d(&p.amp, &p.gamma, &xp)?;
    let price = if input > &u(100000) && q.amount_out > u(100000) {
        let dx = mul(input, &precisions[i])?;
        let dy = mul(&q.amount_out, &precisions[j])?;
        if i == 0 {
            div(&mul(&dx, &wad())?, &dy)?
        } else {
            div(&mul(&dy, &wad())?, &dx)?
        }
    } else {
        u(0)
    };
    next.last_prices = if !price.is_zero() {
        price
    } else {
        let mut xp_derivative = xp.clone();
        let dx = wd(&xp[0], &pow(6));
        xp_derivative[0] = add(&xp_derivative[0], &dx)?;
        let y = newton_y(&p.amp, &p.gamma, &xp_derivative, &d, 1)?;
        div(&mul(&s.price_scale, &dx)?, &sub(&xp[1], &y)?)?
    };
    let mut virtual_price = wad();
    let mut profit = wad();
    if !s.virtual_price.is_zero() {
        let xcp = geometric_mean(&[
            wd(&d, &u(2)),
            div(&mul(&d, &wad())?, &mul(&u(2), &s.price_scale)?)?,
        ])?;
        virtual_price = div(&mul(&wad(), &xcp)?, total_supply)?;
        profit = div(&mul(&s.xcp_profit, &virtual_price)?, &s.virtual_price)?;
        if virtual_price < s.virtual_price && next.future_a_gamma_time == 0 {
            return Err(err("CURVE_CRYPTO_LOSS"));
        }
        if next.future_a_gamma_time == 1 {
            next.future_a_gamma_time = 0;
        }
    }
    let norm = distance(
        &div(&mul(&next.price_oracle, &wad())?, &s.price_scale)?,
        &wad(),
    );
    let step = std::cmp::max(p.adjustment_step.clone(), wd(&norm, &u(10)));
    let mut needs_adjustment = s.not_adjusted;
    // Preserve source short-circuit evaluation and checked underflow behavior.
    if !needs_adjustment
        && sub(&mul(&virtual_price, &u(2))?, &wad())?
            > add(&profit, &wm(&p.allowed_extra_profit, &u(2)))?
        && norm > step
        && !s.virtual_price.is_zero()
    {
        needs_adjustment = true;
    }
    next.invariant = d.clone();
    next.virtual_price = virtual_price.clone();
    next.xcp_profit = profit.clone();
    next.not_adjusted = needs_adjustment;
    let mut raw = ledger.raw_balances.clone();
    raw[i] = add(&raw[i], input)?;
    raw[j] = sub(&raw[j], &q.amount_out)?;
    let mut out = CryptoFullQuote {
        amount_out: q.amount_out,
        next_balances: q.next_balances,
        fee_rate: q.fee_rate,
        next_pricing_state: next,
        next_total_supply: ledger.total_supply.clone(),
        next_receiver_balance: ledger.receiver_balance.clone(),
        next_raw_balances: raw,
        minted_admin: u(0),
    };
    if needs_adjustment {
        if norm > step && !s.virtual_price.is_zero() {
            let p_new = wd(
                &add(
                    &mul(&s.price_scale, &sub(&norm, &step)?)?,
                    &mul(&step, &out.next_pricing_state.price_oracle)?,
                )?,
                &norm,
            );
            let adjusted_xp = vec![xp[0].clone(), div(&mul(&xp[1], &p_new)?, &s.price_scale)?];
            let adjusted_d = newton_d(&p.amp, &p.gamma, &adjusted_xp)?;
            let adjusted_vp = div(&mul(&wad(), &full_xcp(&adjusted_d, &p_new)?)?, total_supply)?;
            if adjusted_vp > wad() && sub(&mul(&u(2), &adjusted_vp)?, &wad())? > profit {
                out.next_pricing_state.price_scale = p_new;
                out.next_pricing_state.invariant = adjusted_d;
                out.next_pricing_state.virtual_price = adjusted_vp;
                return Ok(out);
            }
        }
        out.next_pricing_state.not_adjusted = false;
        // _claim_admin_fees re-reads _A_gamma after the ramp sentinel may clear.
        let claim_p = full_params(future_params, &out.next_pricing_state, ledger, timestamp)?;
        // A still-active ramp uses original future endpoints on this re-read.
        full_claim(
            &claim_p,
            precisions,
            &mut out,
            ledger.admin_receiver_enabled,
        )?;
    }
    Ok(out)
}
