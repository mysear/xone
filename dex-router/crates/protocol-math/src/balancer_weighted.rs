//! Balancer V2 immutable two-token 50/50 WeightedPool, exact input.
//! Pinned upstream e281ff40583c39bf1d46c322334b2b13936e0e33:
//! BaseMinimalSwapInfoPool.onSwap, BasePool fee/scaling, WeightedMath and
//! FixedPoint.powUp(ONE). Vault balances are pool-specific cash (no managers).
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};

#[derive(Debug, PartialEq, Eq)]
pub struct WeightedQuote {
    pub amount_out: BigUint,
    pub next_balances: [BigUint; 2],
}
const ONE: u64 = 1_000_000_000_000_000_000;
fn checked(v: BigUint) -> Result<BigUint> {
    if v.bits() > 256 {
        Err(err("WEIGHTED_INTERMEDIATE_OVERFLOW"))
    } else {
        Ok(v)
    }
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn up(a: BigUint, b: &BigUint) -> BigUint {
    if a.is_zero() {
        a
    } else {
        (a - BigUint::one()) / b + BigUint::one()
    }
}
/// The arithmetic layer accepts fee 0..=ONE for differential boundary tests;
/// the versioned pool admission additionally enforces native fee 1e12..=1e17.
/// Every intermediate matches checked uint256 and every raw Vault balance fits
/// uint112, including gross-input poststate. Excess input is rejected, not split.
pub fn quote_exact_in(
    balances: [&BigUint; 2],
    scaling: [&BigUint; 2],
    fee: &BigUint,
    input: &BigUint,
    zero_for_one: bool,
) -> Result<WeightedQuote> {
    let half = BigUint::from(500_000_000_000_000_000u64);
    quote_exact_in_weighted(balances, scaling, [&half, &half], fee, input, zero_for_one)
}
/// Immutable two-token WeightedMath with both normalized weights at least 1%.
/// The equal profile calls this with fixed half weights; all other weights
/// require the separately admitted static profile.
pub fn quote_exact_in_weighted(
    balances: [&BigUint; 2],
    scaling: [&BigUint; 2],
    weights: [&BigUint; 2],
    fee: &BigUint,
    input: &BigUint,
    zero_for_one: bool,
) -> Result<WeightedQuote> {
    let fixed_one = BigUint::from(ONE);
    let minimum = BigUint::from(10_000_000_000_000_000u64);
    if weights.iter().any(|w| w.bits() > 256 || *w < &minimum)
        || weights[0] + weights[1] != fixed_one
    {
        return Err(err("WEIGHTED_NORMALIZED_WEIGHTS"));
    }
    for v in balances.into_iter().chain(scaling).chain([fee, input]) {
        if v.bits() > 256 {
            return Err(err("WEIGHTED_INTERMEDIATE_OVERFLOW"));
        }
    }
    let one = BigUint::from(ONE);
    for s in scaling {
        if !(0..=18).any(|d| *s == &one * BigUint::from(10u8).pow(d)) {
            return Err(err("WEIGHTED_SCALING_FACTOR"));
        }
    }
    if fee > &one {
        return Err(err("WEIGHTED_FEE"));
    }
    if input.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    for b in balances {
        if b.bits() > 112 {
            return Err(err("WEIGHTED_VAULT_BALANCE_OVERFLOW"));
        }
        if b.is_zero() {
            return Err(err("INSUFFICIENT_LIQUIDITY"));
        }
    }
    let i = usize::from(!zero_for_one);
    let j = 1 - i;
    // Actual callback upscales both balances before deducting the raw fee.
    let scaled = [
        mul(balances[0], scaling[0])? / &one,
        mul(balances[1], scaling[1])? / &one,
    ];
    let charge = up(mul(input, fee)?, &one);
    let net = input - charge;
    let amount = mul(&net, scaling[i])? / &one;
    let max_in = mul(&scaled[i], &BigUint::from(300_000_000_000_000_000u64))? / &one;
    if amount > max_in {
        return Err(err("WEIGHTED_MAX_IN_RATIO"));
    }
    let denominator = checked(&scaled[i] + &amount)?;
    let base = up(mul(&scaled[i], &one)?, &denominator);
    // WeightedMath rounds the exponent down, then the power up.
    let exponent = mul(weights[i], &one)? / weights[j];
    let power = crate::balancer_log_exp::pow_up(&base, &exponent)?;
    let complement = if power >= one {
        BigUint::zero()
    } else {
        &one - power
    };
    let scaled_out = mul(&scaled[j], &complement)? / &one;
    let amount_out = mul(&scaled_out, &one)? / scaling[j];
    if amount_out.is_zero() || &amount_out >= balances[j] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = [balances[0].clone(), balances[1].clone()];
    next_balances[i] = checked(balances[i] + input)?;
    if next_balances[i].bits() > 112 {
        return Err(err("WEIGHTED_VAULT_BALANCE_OVERFLOW"));
    }
    next_balances[j] -= &amount_out;
    Ok(WeightedQuote {
        amount_out,
        next_balances,
    })
}
