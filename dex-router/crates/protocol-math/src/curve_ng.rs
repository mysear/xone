//! StableSwapNG@3332cd44656ec64b3c048885e1dd71955254b262 (Vyper 0.3.10).
//! Models exchange with fixed, already evaluated stored_rates()/get_balances().
//! NG uses its own D rounding and off-peg fee at mean pre/post balances.
//! Effective next balances exclude accrued admin fees. The full-state API also
//! reproduces original exchange oracle upkeep and raw stored/admin balances.
use crate::curve_multi::{
    add, distance, div, ensure, mul, normalize, solve_y, sub, u, validate, FEE_DENOMINATOR,
    PRECISION,
};
use num_bigint::BigUint;
use num_traits::Zero;
use xone_domain::{err, Result};

#[path = "curve_ng_oracle.rs"]
mod oracle;
pub use oracle::{get_p, wad_exp};

/// Complete mutable exchange state for standard fixed-rate (asset_type=0) coins.
/// Effective balances must equal stored_balances - admin_balances. Packed words
/// use low128=spot/time_price, high128=EMA/time_D. Timestamp is the execution
/// block timestamp, and remains unchanged in next_state. Window/timestamp u64
/// bounds are an explicit supported profile; on-chain fields are uint256.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NgOracleState {
    pub stored_balances: Vec<BigUint>,
    pub admin_balances: Vec<BigUint>,
    pub last_prices_packed: Vec<BigUint>,
    pub last_d_packed: BigUint,
    pub ma_last_time: BigUint,
    pub ma_exp_time: u64,
    pub d_ma_time: u64,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NgFullQuote {
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
    pub next_state: NgOracleState,
    pub admin_fee_amount: BigUint,
    pub fee_rate: BigUint,
    pub invariant: BigUint,
}

/// Original full exchange for exact, non-rebasing ERC20 transfers and evaluated
/// fixed rates. Caller proves asset types/rates at the snapshot block. Rebasing,
/// fee-on-transfer, optimistic exchange_received, and external rate providers
/// are outside this API. No partially mutated state escapes on any error.
#[allow(clippy::too_many_arguments)]
pub fn quote_full_exact_in(
    balances: &[BigUint],
    amp: &BigUint,
    rates: &[BigUint],
    fee: &BigUint,
    admin_fee: &BigUint,
    offpeg_fee_multiplier: &BigUint,
    input: &BigUint,
    i: usize,
    j: usize,
    state: &NgOracleState,
) -> Result<NgFullQuote> {
    let n = balances.len();
    if state.stored_balances.len() != n
        || state.admin_balances.len() != n
        || n < 2
        || state.last_prices_packed.len() != n - 1
    {
        return Err(err("CURVE_NG_INVALID_ORACLE_STATE_SHAPE"));
    }
    for value in state
        .stored_balances
        .iter()
        .chain(&state.admin_balances)
        .chain(&state.last_prices_packed)
        .chain([&state.last_d_packed, &state.ma_last_time])
    {
        ensure(value)?;
    }
    for (k, balance) in balances.iter().enumerate() {
        if sub(&state.stored_balances[k], &state.admin_balances[k])? != *balance {
            return Err(err("CURVE_NG_INCONSISTENT_EFFECTIVE_BALANCE"));
        }
    }
    let q = quote_exact_in(
        balances,
        amp,
        rates,
        fee,
        admin_fee,
        offpeg_fee_multiplier,
        input,
        i,
        j,
    )?;
    let mut next_state = state.clone();
    next_state.stored_balances[i] = add(&state.stored_balances[i], input)?;
    next_state.stored_balances[j] = sub(&state.stored_balances[j], &q.amount_out)?;
    next_state.admin_balances[j] = add(&state.admin_balances[j], &q.admin_fee_amount)?;
    // __exchange uses solved xp WITHOUT fees and the original invariant.
    // Re-normalizing next_balances instead introduces both fee and dust errors.
    let mut xp = normalize(balances, rates)?;
    let x = add(&xp[i], &div(&mul(input, &rates[i])?, &u(PRECISION))?)?;
    let y = solve_y(i, j, &x, &xp, amp, &q.invariant)?;
    xp[i] = x;
    xp[j] = y;
    oracle::upkeep(&xp, amp, &q.invariant, &mut next_state)?;
    Ok(NgFullQuote {
        amount_out: q.amount_out,
        next_balances: q.next_balances,
        next_state,
        admin_fee_amount: q.admin_fee_amount,
        fee_rate: q.fee_rate,
        invariant: q.invariant,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurveNgQuote {
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
    /// Increment to admin_balances[j], in raw output coin units.
    pub admin_fee_amount: BigUint,
    pub fee_rate: BigUint,
    /// Original invariant used during this exchange, before fee retention.
    pub invariant: BigUint,
}
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_in(
    balances: &[BigUint],
    amp: &BigUint,
    rates: &[BigUint],
    fee: &BigUint,
    admin_fee: &BigUint,
    offpeg_fee_multiplier: &BigUint,
    input: &BigUint,
    i: usize,
    j: usize,
) -> Result<CurveNgQuote> {
    if !(2..=8).contains(&balances.len()) {
        return Err(err("CURVE_NG_INVALID_COIN_COUNT"));
    }
    validate(balances, rates, amp, fee, admin_fee, input, i, j)?;
    ensure(offpeg_fee_multiplier)?;
    // Factory deployment/update parameter domain. This bounds unsafe fee ops.
    if fee > &u(5_000_000_000)
        || mul(fee, offpeg_fee_multiplier)? > u(5_000_000_000) * u(FEE_DENOMINATOR)
    {
        return Err(err("CURVE_NG_INVALID_FEE_PARAMETERS"));
    }
    if input.is_zero() {
        return Err(err("CURVE_NG_ZERO_INPUT"));
    }
    let p = u(PRECISION);
    let fd = u(FEE_DENOMINATOR);
    let xp = normalize(balances, rates)?;
    let x = add(&xp[i], &div(&mul(input, &rates[i])?, &p)?)?;
    let invariant = invariant(&xp, amp)?;
    let y = solve_y(i, j, &x, &xp, amp, &invariant)?;
    let dy = sub(&sub(&xp[j], &y)?, &u(1))?;
    let fee_rate = dynamic_fee(
        &div(&add(&xp[i], &x)?, &u(2))?,
        &div(&add(&xp[j], &y)?, &u(2))?,
        fee,
        offpeg_fee_multiplier,
    )?;
    let fee_xp = div(&mul(&dy, &fee_rate)?, &fd)?;
    let amount_out = div(&mul(&sub(&dy, &fee_xp)?, &p)?, &rates[j])?;
    let admin_fee_amount = div(&mul(&div(&mul(&fee_xp, admin_fee)?, &fd)?, &p)?, &rates[j])?;
    let mut next_balances = balances.to_vec();
    next_balances[i] = add(&balances[i], input)?;
    next_balances[j] = sub(&sub(&balances[j], &amount_out)?, &admin_fee_amount)?;
    Ok(CurveNgQuote {
        amount_out,
        next_balances,
        admin_fee_amount,
        fee_rate,
        invariant,
    })
}

pub fn dynamic_fee(
    xpi: &BigUint,
    xpj: &BigUint,
    fee: &BigUint,
    multiplier: &BigUint,
) -> Result<BigUint> {
    for v in [xpi, xpj, fee, multiplier] {
        ensure(v)?;
    }
    let fd = u(FEE_DENOMINATOR);
    if multiplier <= &fd {
        return Ok(fee.clone());
    }
    let sum = add(xpi, xpj)?;
    let square = mul(&sum, &sum)?;
    let numerator = mul(multiplier, fee)?;
    let denominator = add(
        &div(
            &mul(&mul(&mul(&sub(multiplier, &fd)?, &u(4))?, xpi)?, xpj)?,
            &square,
        )?,
        &fd,
    )?;
    div(&numerator, &denominator)
}

/// NG divides by n**n only AFTER the coin loop. Do not substitute old D.
pub fn invariant(xp: &[BigUint], amp: &BigUint) -> Result<BigUint> {
    if !(2..=8).contains(&xp.len()) {
        return Err(err("CURVE_NG_INVALID_COIN_COUNT"));
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
            dp = div(&mul(&dp, &d)?, x)?;
        }
        dp = div(&dp, &n.pow(xp.len() as u32))?;
        let old = d.clone();
        d = div(
            &mul(&add(&div(&mul(&ann, &sum)?, &ap)?, &mul(&dp, &n)?)?, &d)?,
            &add(
                &div(&mul(&sub(&ann, &ap)?, &d)?, &ap)?,
                &mul(&u(xp.len() as u64 + 1), &dp)?,
            )?,
        )?;
        if distance(&d, &old) <= u(1) {
            return Ok(d);
        }
    }
    Err(err("CURVE_NG_D_DID_NOT_CONVERGE"))
}

/// Fixed-source external rate boundary. Providers must be evaluated at quote
/// block. ERC4626 inputs are convertToAssets(call_amount) and scale_factor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RateSnapshot {
    Standard,
    Oracle {
        fetched_rate: BigUint,
    },
    Erc4626 {
        converted_assets: BigUint,
        scale_factor: BigUint,
    },
    /// Rebases require balanceOf(pool)-admin_balances in addition to rates.
    Rebasing,
}
pub fn stored_rates(multipliers: &[BigUint], snapshots: &[RateSnapshot]) -> Result<Vec<BigUint>> {
    if multipliers.len() != snapshots.len() || !(2..=8).contains(&multipliers.len()) {
        return Err(err("CURVE_NG_INVALID_RATE_SHAPE"));
    }
    multipliers
        .iter()
        .zip(snapshots)
        .map(|(r, s)| {
            ensure(r)?;
            match s {
                RateSnapshot::Standard | RateSnapshot::Rebasing => Ok(r.clone()),
                RateSnapshot::Oracle { fetched_rate } => {
                    ensure(fetched_rate)?;
                    div(&mul(r, fetched_rate)?, &u(PRECISION))
                }
                RateSnapshot::Erc4626 {
                    converted_assets,
                    scale_factor,
                } => {
                    ensure(converted_assets)?;
                    ensure(scale_factor)?;
                    div(
                        &mul(&mul(r, converted_assets)?, scale_factor)?,
                        &u(PRECISION),
                    )
                }
            }
        })
        .collect()
}
