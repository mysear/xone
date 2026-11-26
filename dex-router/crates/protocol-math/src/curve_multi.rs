//! Original Factory Plain3Basic, Plain4Basic and MetaUSD (direct coin/LP exchange).
//! curvefi/curve-factory@5272fd2b1199f185d9cd94822be9262c387dc2c2, Vyper 0.3.1.
//! Balances exclude admin fees. Rates and A_precise must be evaluated at the same
//! block. No transfer/rebase or external provider calls occur in this module.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};

pub(crate) const PRECISION: u64 = 1_000_000_000_000_000_000;
pub(crate) const FEE_DENOMINATOR: u64 = 10_000_000_000;
pub(crate) const ADMIN_FEE: u64 = 5_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurveQuote {
    pub amount_in: BigUint,
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

/// Solve the pinned plain-pool integer exchange for an exact output target.
/// The source contract exposes an input-oriented exchange, so the inverse is
/// found with a bounded monotonic search and then re-evaluated for the final
/// post-state. A target that cannot be represented by the contract's integer
/// rounding is rejected instead of being rounded up silently.
#[allow(clippy::too_many_arguments)]
pub fn quote_exact_out(
    balances: &[BigUint],
    amp: &BigUint,
    rates: &[BigUint],
    fee: &BigUint,
    admin_fee: &BigUint,
    output: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<CurveQuote> {
    if output.is_zero() {
        return Err(err("CURVE_EXACT_OUT_ZERO"));
    }
    validate(
        balances,
        rates,
        amp,
        fee,
        admin_fee,
        &BigUint::from(1u8),
        index_in,
        index_out,
    )?;
    if ![3, 4].contains(&balances.len()) {
        return Err(err("CURVE_MULTI_UNSUPPORTED_COIN_COUNT"));
    }
    ensure(output)?;
    let mut high = BigUint::one();
    let mut upper = None;
    for _ in 0..=256 {
        match quote_exact_in(
            balances, amp, rates, fee, admin_fee, &high, index_in, index_out,
        ) {
            Ok(quote) if quote.amount_out >= *output => {
                upper = Some(high);
                break;
            }
            Ok(_) => high <<= 1usize,
            Err(_) => {
                upper = Some(high);
                break;
            }
        }
    }
    let mut high = upper.ok_or_else(|| err("CURVE_EXACT_OUT_UNREACHABLE"))?;
    let mut low = BigUint::zero();
    while low < high {
        let mid = (&low + &high) >> 1usize;
        match quote_exact_in(
            balances, amp, rates, fee, admin_fee, &mid, index_in, index_out,
        ) {
            Ok(quote) if quote.amount_out >= *output => high = mid,
            Ok(_) => low = mid + BigUint::one(),
            Err(_) => high = mid,
        }
    }
    let quote = match quote_exact_in(
        balances, amp, rates, fee, admin_fee, &low, index_in, index_out,
    ) {
        Ok(quote) => quote,
        Err(_) => return Err(err("CURVE_EXACT_OUT_UNREACHABLE")),
    };
    if quote.amount_out != *output {
        return Err(err("CURVE_EXACT_OUT_UNREACHABLE"));
    }
    Ok(quote)
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

/// Solve the pinned MetaUSD direct integer exchange for an exact output target.
/// The base pool virtual price is a read-only dependency for this operation.
#[allow(clippy::too_many_arguments)]
pub fn quote_meta_exact_out(
    balances: &[BigUint],
    amp: &BigUint,
    meta_rate: &BigUint,
    base_virtual_price: &BigUint,
    fee: &BigUint,
    admin_fee: &BigUint,
    output: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<CurveQuote> {
    if output.is_zero() {
        return Err(err("CURVE_EXACT_OUT_ZERO"));
    }
    if balances.len() != 2 {
        return Err(err("CURVE_META_INVALID_COIN_COUNT"));
    }
    validate(
        balances,
        &[meta_rate.clone(), base_virtual_price.clone()],
        amp,
        fee,
        admin_fee,
        &BigUint::from(1u8),
        index_in,
        index_out,
    )?;
    ensure(output)?;
    let mut high = BigUint::one();
    let mut upper = None;
    for _ in 0..=256 {
        match quote_meta_exact_in(
            balances,
            amp,
            meta_rate,
            base_virtual_price,
            fee,
            admin_fee,
            &high,
            index_in,
            index_out,
        ) {
            Ok(quote) if quote.amount_out >= *output => {
                upper = Some(high);
                break;
            }
            Ok(_) => high <<= 1usize,
            Err(_) => {
                upper = Some(high);
                break;
            }
        }
    }
    let mut high = upper.ok_or_else(|| err("CURVE_EXACT_OUT_UNREACHABLE"))?;
    let mut low = BigUint::zero();
    while low < high {
        let mid = (&low + &high) >> 1usize;
        match quote_meta_exact_in(
            balances,
            amp,
            meta_rate,
            base_virtual_price,
            fee,
            admin_fee,
            &mid,
            index_in,
            index_out,
        ) {
            Ok(quote) if quote.amount_out >= *output => high = mid,
            Ok(_) => low = mid + BigUint::one(),
            Err(_) => high = mid,
        }
    }
    let quote = match quote_meta_exact_in(
        balances,
        amp,
        meta_rate,
        base_virtual_price,
        fee,
        admin_fee,
        &low,
        index_in,
        index_out,
    ) {
        Ok(quote) => quote,
        Err(_) => return Err(err("CURVE_EXACT_OUT_UNREACHABLE")),
    };
    if quote.amount_out != *output {
        return Err(err("CURVE_EXACT_OUT_UNREACHABLE"));
    }
    Ok(quote)
}

/// The exact external actions needed by MetaUSD `exchange_underlying`.
/// A rate alone cannot determine base LP mint/burn amounts or their poststate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetaUnderlyingBoundary {
    BaseExchange { index_in: usize, index_out: usize },
    BaseDepositThenMeta { base_index: usize },
    MetaThenBaseWithdraw { base_index: usize },
}

/// The state and result of one-sided liquidity operations on MetaUSD's pinned
/// 3pool dependency. The base pool must be supplied explicitly because the
/// Meta pool only stores the base LP token and virtual price.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseLiquidityQuote {
    pub amount_in: BigUint,
    pub amount_out: BigUint,
    pub next_balances: Vec<BigUint>,
    pub next_total_supply: BigUint,
    pub fee_amounts: Vec<BigUint>,
    pub admin_fee_amount: BigUint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseLiquidityState {
    pub balances: Vec<BigUint>,
    pub rates: Vec<BigUint>,
    pub amplification: BigUint,
    pub fee: BigUint,
    pub admin_fee: BigUint,
    pub total_supply: BigUint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaUnderlyingQuote {
    pub amount_in: BigUint,
    pub amount_out: BigUint,
    pub next_meta_balances: Vec<BigUint>,
    pub next_meta_admin_balances: Vec<BigUint>,
    pub next_base_balances: Vec<BigUint>,
    pub next_base_total_supply: BigUint,
    pub base_fee_amounts: Vec<BigUint>,
    pub meta_fee_amount: BigUint,
    pub base_admin_fee_amount: BigUint,
    pub meta_admin_fee_amount: BigUint,
}

/// Compose the pinned MetaUSD exchange-underlying actions with an explicit
/// base-pool snapshot. The snapshot is required for LP mint/burn accounting,
/// so this helper is intentionally separate from the online Meta pool quote.
#[allow(clippy::too_many_arguments)]
pub fn quote_meta_underlying_exact_in(
    meta_balances: &[BigUint],
    meta_admin_balances: &[BigUint],
    meta_rates: &[BigUint],
    meta_amp: &BigUint,
    meta_fee: &BigUint,
    meta_admin_fee: &BigUint,
    base: &BaseLiquidityState,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<MetaUnderlyingQuote> {
    if meta_balances.len() != 2
        || meta_admin_balances.len() != 2
        || meta_rates.len() != 2
        || meta_rates[1].is_zero()
    {
        return Err(err("CURVE_META_INVALID_STATE"));
    }
    let action = meta_underlying_boundary(index_in, index_out)?;
    let mut next_meta_balances = meta_balances.to_vec();
    let mut next_meta_admin_balances = meta_admin_balances.to_vec();
    match action {
        MetaUnderlyingBoundary::BaseExchange {
            index_in: i,
            index_out: j,
        } => {
            let q = quote_exact_in(
                &base.balances,
                &base.amplification,
                &base.rates,
                &base.fee,
                &base.admin_fee,
                input,
                i,
                j,
            )?;
            let mut base_fee_amounts = vec![u(0); base.balances.len()];
            base_fee_amounts[j] = q.fee_amount.clone();
            return Ok(MetaUnderlyingQuote {
                amount_in: input.clone(),
                amount_out: q.amount_out,
                next_meta_balances,
                next_meta_admin_balances,
                next_base_balances: q.next_balances,
                next_base_total_supply: base.total_supply.clone(),
                base_fee_amounts,
                meta_fee_amount: u(0),
                base_admin_fee_amount: q.admin_fee_amount,
                meta_admin_fee_amount: u(0),
            });
        }
        MetaUnderlyingBoundary::BaseDepositThenMeta { base_index } => {
            let deposit = quote_base_add_liquidity_one_coin(
                &base.balances,
                &base.rates,
                &base.amplification,
                &base.fee,
                &base.admin_fee,
                &base.total_supply,
                input,
                base_index,
            )?;
            let meta = quote_meta_exact_in(
                meta_balances,
                meta_amp,
                &meta_rates[0],
                &meta_rates[1],
                meta_fee,
                meta_admin_fee,
                &deposit.amount_out,
                1,
                0,
            )?;
            next_meta_balances = meta.next_balances.clone();
            next_meta_admin_balances[0] =
                add(&next_meta_admin_balances[0], &meta.admin_fee_amount)?;
            return Ok(MetaUnderlyingQuote {
                amount_in: input.clone(),
                amount_out: meta.amount_out,
                next_meta_balances,
                next_meta_admin_balances,
                next_base_balances: deposit.next_balances,
                next_base_total_supply: deposit.next_total_supply,
                base_fee_amounts: deposit.fee_amounts,
                meta_fee_amount: meta.fee_amount,
                base_admin_fee_amount: deposit.admin_fee_amount,
                meta_admin_fee_amount: meta.admin_fee_amount,
            });
        }
        MetaUnderlyingBoundary::MetaThenBaseWithdraw { base_index } => {
            let meta = quote_meta_exact_in(
                meta_balances,
                meta_amp,
                &meta_rates[0],
                &meta_rates[1],
                meta_fee,
                meta_admin_fee,
                input,
                0,
                1,
            )?;
            let withdrawal = quote_base_remove_liquidity_one_coin(
                &base.balances,
                &base.rates,
                &base.amplification,
                &base.fee,
                &base.admin_fee,
                &base.total_supply,
                &meta.amount_out,
                base_index,
            )?;
            next_meta_balances = meta.next_balances.clone();
            next_meta_admin_balances[1] =
                add(&next_meta_admin_balances[1], &meta.admin_fee_amount)?;
            return Ok(MetaUnderlyingQuote {
                amount_in: input.clone(),
                amount_out: withdrawal.amount_out,
                next_meta_balances,
                next_meta_admin_balances,
                next_base_balances: withdrawal.next_balances,
                next_base_total_supply: withdrawal.next_total_supply,
                base_fee_amounts: withdrawal.fee_amounts,
                meta_fee_amount: meta.fee_amount,
                base_admin_fee_amount: withdrawal.admin_fee_amount,
                meta_admin_fee_amount: meta.admin_fee_amount,
            });
        }
    }
}

/// Mirrors Plain3Basic `add_liquidity` for a one-coin deposit. The returned
/// balances are the pool's internal balances after removing the admin-fee
/// portion, and the LP amount is the exact integer amount minted by the source.
#[allow(clippy::too_many_arguments)]
pub fn quote_base_add_liquidity_one_coin(
    balances: &[BigUint],
    rates: &[BigUint],
    amp: &BigUint,
    fee: &BigUint,
    admin_fee: &BigUint,
    total_supply: &BigUint,
    input: &BigUint,
    index: usize,
) -> Result<BaseLiquidityQuote> {
    validate_base_liquidity(
        balances,
        rates,
        amp,
        fee,
        admin_fee,
        total_supply,
        input,
        index,
    )?;
    if total_supply.is_zero() {
        return Err(err("CURVE_BASE_INITIAL_DEPOSIT_UNSUPPORTED"));
    }
    let old_xp = normalize(balances, rates)?;
    let d0 = invariant(&old_xp, amp)?;
    if d0.is_zero() {
        return Err(err("CURVE_BASE_EMPTY"));
    }
    let mut new_balances = balances.to_vec();
    new_balances[index] = add(&new_balances[index], input)?;
    let d1 = invariant(&normalize(&new_balances, rates)?, amp)?;
    if d1 <= d0 {
        return Err(err("CURVE_BASE_DEPOSIT_NO_LP"));
    }
    let n = u(balances.len() as u64);
    let base_fee = div(&mul(fee, &n)?, &mul(&u(4), &u(balances.len() as u64 - 1))?)?;
    let fd = u(FEE_DENOMINATOR);
    let mut fees = vec![u(0); balances.len()];
    let mut fee_adjusted = normalize(&new_balances, rates)?;
    for i in 0..balances.len() {
        let ideal = div(&mul(&d1, &old_xp[i])?, &d0)?;
        let difference = distance(&ideal, &fee_adjusted[i]);
        fees[i] = div(&mul(&base_fee, &difference)?, &fd)?;
        let admin_part = div(&mul(&fees[i], admin_fee)?, &fd)?;
        fee_adjusted[i] = sub(&fee_adjusted[i], &fees[i])?;
        let internal = sub(
            &new_balances[i],
            &div(&mul(&admin_part, &u(PRECISION))?, &rates[i])?,
        )?;
        new_balances[i] = internal;
    }
    let d2 = invariant(&fee_adjusted, amp)?;
    let minted = div(&mul(total_supply, &sub(&d2, &d0)?)?, &d0)?;
    if minted.is_zero() {
        return Err(err("CURVE_BASE_DEPOSIT_NO_LP"));
    }
    let admin_fee_amount = fees
        .iter()
        .zip(rates)
        .map(|(fee_xp, rate)| {
            div(
                &mul(&div(&mul(fee_xp, admin_fee)?, &fd)?, &u(PRECISION))?,
                rate,
            )
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .try_fold(u(0), |sum, value| add(&sum, &value))?;
    Ok(BaseLiquidityQuote {
        amount_in: input.clone(),
        amount_out: minted.clone(),
        next_balances: new_balances,
        next_total_supply: add(total_supply, &minted)?,
        fee_amounts: fees,
        admin_fee_amount,
    })
}

/// Mirrors Plain3Basic `remove_liquidity_one_coin`, including the source's
/// reduced-xp fee calculation and the separate admin-fee balance accounting.
#[allow(clippy::too_many_arguments)]
pub fn quote_base_remove_liquidity_one_coin(
    balances: &[BigUint],
    rates: &[BigUint],
    amp: &BigUint,
    fee: &BigUint,
    admin_fee: &BigUint,
    total_supply: &BigUint,
    burn_amount: &BigUint,
    index: usize,
) -> Result<BaseLiquidityQuote> {
    validate_base_liquidity(
        balances,
        rates,
        amp,
        fee,
        admin_fee,
        total_supply,
        burn_amount,
        index,
    )?;
    if burn_amount.is_zero() || burn_amount >= total_supply {
        return Err(err("CURVE_BASE_INVALID_BURN"));
    }
    let xp = normalize(balances, rates)?;
    let d0 = invariant(&xp, amp)?;
    if d0.is_zero() {
        return Err(err("CURVE_BASE_EMPTY"));
    }
    let d1 = sub(&d0, &div(&mul(burn_amount, &d0)?, total_supply)?)?;
    let new_y = solve_y_d(amp, index, &xp, &d1)?;
    let n = u(balances.len() as u64);
    let base_fee = div(&mul(fee, &n)?, &mul(&u(4), &u(balances.len() as u64 - 1))?)?;
    let fd = u(FEE_DENOMINATOR);
    let mut reduced = xp.clone();
    let mut fee_amounts = vec![u(0); balances.len()];
    for i in 0..balances.len() {
        let expected = if i == index {
            sub(&div(&mul(&xp[i], &d1)?, &d0)?, &new_y)?
        } else {
            sub(&xp[i], &div(&mul(&xp[i], &d1)?, &d0)?)?
        };
        let fee_xp = div(&mul(&base_fee, &expected)?, &fd)?;
        fee_amounts[i] = div(&mul(&fee_xp, &u(PRECISION))?, &rates[i])?;
        reduced[i] = sub(&xp[i], &fee_xp)?;
    }
    let reduced_y = solve_y_d(amp, index, &reduced, &d1)?;
    let dy_xp = sub(&sub(&reduced[index], &reduced_y)?, &u(1))?;
    let dy = div(&mul(&dy_xp, &u(PRECISION))?, &rates[index])?;
    let dy0 = div(
        &mul(&sub(&xp[index], &new_y)?, &u(PRECISION))?,
        &rates[index],
    )?;
    let admin_fee_amount = sub(&dy0, &dy)?;
    let admin_part = div(&mul(&admin_fee_amount, admin_fee)?, &fd)?;
    let mut next_balances = balances.to_vec();
    next_balances[index] = sub(&next_balances[index], &add(&dy, &admin_part)?)?;
    Ok(BaseLiquidityQuote {
        amount_in: burn_amount.clone(),
        amount_out: dy,
        next_balances,
        next_total_supply: sub(total_supply, burn_amount)?,
        fee_amounts,
        admin_fee_amount: admin_part,
    })
}

fn validate_base_liquidity(
    balances: &[BigUint],
    rates: &[BigUint],
    amp: &BigUint,
    fee: &BigUint,
    admin_fee: &BigUint,
    total_supply: &BigUint,
    amount: &BigUint,
    index: usize,
) -> Result<()> {
    if balances.len() != 3 || rates.len() != 3 || index >= 3 {
        return Err(err("CURVE_BASE_INVALID_SHAPE"));
    }
    validate(
        balances,
        rates,
        amp,
        fee,
        admin_fee,
        amount,
        index,
        (index + 1) % 3,
    )?;
    ensure(total_supply)
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
        amount_in: input.clone(),
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

/// Solve the source `get_y_D` quadratic while holding every coin except `i`
/// constant and targeting an explicit invariant value.
fn solve_y_d(amp: &BigUint, i: usize, xp: &[BigUint], d: &BigUint) -> Result<BigUint> {
    if i >= xp.len() || xp.len() < 2 {
        return Err(err("CURVE_INVALID_INDEX"));
    }
    let n = u(xp.len() as u64);
    let ann = mul(amp, &n)?;
    let mut sum = u(0);
    let mut c = d.clone();
    for (index, value) in xp.iter().enumerate() {
        if index == i {
            continue;
        }
        sum = add(&sum, value)?;
        c = div(&mul(&c, d)?, &mul(value, &n)?)?;
    }
    c = div(&mul(&mul(&c, d)?, &u(100))?, &mul(&ann, &n)?)?;
    let b = add(&sum, &div(&mul(d, &u(100))?, &ann)?)?;
    let mut y = d.clone();
    for _ in 0..255 {
        let previous = y.clone();
        y = div(
            &add(&mul(&y, &y)?, &c)?,
            &sub(&add(&mul(&u(2), &y)?, &b)?, d)?,
        )?;
        if distance(&y, &previous) <= u(1) {
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
