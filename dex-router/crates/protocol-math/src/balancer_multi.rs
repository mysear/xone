//! Balancer V2 multi-token WeightedPool exact input, pinned to
//! balancer/balancer-v2-monorepo e281ff40583c39bf1d46c322334b2b13936e0e33.
//! Uses BaseMinimalSwapInfoPool -> BaseWeightedPool -> WeightedMath, preserving
//! raw fee subtraction, divUp base, divDown exponent and FixedPoint.powUp.
//! Pool/Vault identity, token order, no asset managers, pause/recovery and
//! deployment-specific fee limits must be established by the caller.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiQuote {
    pub amount_out: BigUint,
    /// Raw Vault cash in original token order, including gross input and fees.
    pub next_balances: Vec<BigUint>,
}
const ONE: u64 = 1_000_000_000_000_000_000;
fn checked(v: BigUint) -> Result<BigUint> {
    if v.bits() > 256 {
        Err(err("BALANCER_MULTI_UINT256"))
    } else {
        Ok(v)
    }
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn div_up(a: BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        return Err(err("BALANCER_MULTI_ZERO_DIVISION"));
    }
    Ok(if a.is_zero() {
        a
    } else {
        (a - BigUint::one()) / b + BigUint::one()
    })
}

/// Exact input for the bounded immutable 3..=20 token profile. Weights are
/// normalized 18-decimal values, each >=1%, with an exact sum of ONE. Scales
/// are ONE * 10^(18-decimals), decimals 0..=18. Fees accept 0..=ONE at this
/// arithmetic boundary; pool admission applies the native deployment limits.
/// All uint256 intermediates and every initial/final uint112 cash are checked.
/// Zero outputs are rejected as unusable quotes, even if the callback returns 0.
/// There is a 30% scaled-net-input limit and no corresponding max-output limit.
/// Snapshot/dynamic Managed weights need separate provenance and admission;
/// rate-adjusted scales, BPT, joins/exits and protocol fees are not this API.
pub fn quote_exact_in(
    balances: &[BigUint],
    weights: &[BigUint],
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<MultiQuote> {
    quote_inner(
        balances, weights, scales, fee, input, index_in, index_out, false,
    )
}

#[allow(clippy::too_many_arguments)]
fn quote_inner(
    balances: &[BigUint],
    weights: &[BigUint],
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
    managed: bool,
) -> Result<MultiQuote> {
    let count = balances.len();
    let range = if managed { 2..=38 } else { 3..=20 };
    if !range.contains(&count) || weights.len() != count || scales.len() != count {
        return Err(err("BALANCER_MULTI_LENGTH"));
    }
    if index_in >= count || index_out >= count || index_in == index_out {
        return Err(err("BALANCER_MULTI_INDEX"));
    }
    let one = BigUint::from(ONE);
    let min_weight = &one / 100u8;
    let mut sum = BigUint::zero();
    for weight in weights {
        if (managed && (weight.is_zero() || weight > &one)) || (!managed && weight < &min_weight) {
            return Err(err("BALANCER_MULTI_WEIGHTS"));
        }
        sum = checked(sum + checked(weight.clone())?)?;
    }
    if !managed && sum != one {
        return Err(err("BALANCER_MULTI_WEIGHTS"));
    }
    for scale in scales {
        checked(scale.clone())?;
        if !(0..=18).any(|d| *scale == &one * BigUint::from(10u8).pow(d)) {
            return Err(err("BALANCER_MULTI_SCALE"));
        }
    }
    checked(fee.clone())?;
    checked(input.clone())?;
    if fee > &one {
        return Err(err("BALANCER_MULTI_FEE"));
    }
    if input.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    for b in balances {
        if b.bits() > 112 {
            return Err(err("BALANCER_MULTI_UINT112"));
        }
        if b.is_zero() {
            return Err(err("INSUFFICIENT_LIQUIDITY"));
        }
    }
    // The original minimal callback upscales only the selected balances.
    let balance_in = mul(&balances[index_in], &scales[index_in])? / &one;
    let balance_out = mul(&balances[index_out], &scales[index_out])? / &one;
    let charge = div_up(mul(input, fee)?, &one)?;
    let amount = mul(&(input - charge), &scales[index_in])? / &one;
    if amount > mul(&balance_in, &BigUint::from(300_000_000_000_000_000u64))? / &one {
        return Err(err("BALANCER_MULTI_MAX_IN_RATIO"));
    }
    let denominator = checked(&balance_in + &amount)?;
    let base = div_up(mul(&balance_in, &one)?, &denominator)?;
    let exponent = mul(&weights[index_in], &one)? / &weights[index_out];
    let power = crate::balancer_log_exp::pow_up(&base, &exponent)?;
    let complement = if power >= one {
        BigUint::zero()
    } else {
        &one - power
    };
    let scaled_out = mul(&balance_out, &complement)? / &one;
    let amount_out = mul(&scaled_out, &one)? / &scales[index_out];
    if amount_out.is_zero() || amount_out >= balances[index_out] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_balances = balances.to_vec();
    next_balances[index_in] = checked(&balances[index_in] + input)?;
    if next_balances[index_in].bits() > 112 {
        return Err(err("BALANCER_MULTI_UINT112"));
    }
    next_balances[index_out] -= &amount_out;
    Ok(MultiQuote {
        amount_out,
        next_balances,
    })
}

/// Zero-fee-accounting subset of the historical ManagedPool callback.
/// These are observed contract values, not assumptions about pool defaults.
#[derive(Clone, Debug)]
pub struct ManagedSwapGuards {
    pub swaps_enabled: bool,
    pub protocol_swap_fee: BigUint,
    pub management_swap_fee: BigUint,
}

/// Same pricing as WeightedMath using the exact current getNormalizedWeights()
/// and getSwapFeePercentage() results at the admitted snapshot block/timestamp.
/// ManagedPool's 64-bit compression and time interpolation need not preserve an
/// exact weight sum. Do not renormalize or feed these to the static API.
/// Snapshot provenance and normalized-weight validity are caller obligations;
/// this layer checks nonzero weights <= ONE and supports 2..=38 assets.
/// The pinned LP allowlist is a join guard. It is not consulted on swaps.
/// Nonzero protocol/management swap fees require BPT minting state and are
/// deliberately rejected. AUM fee accrual is not part of this onSwap path.
#[allow(clippy::too_many_arguments)]
pub fn quote_managed_snapshot(
    balances: &[BigUint],
    weights: &[BigUint],
    scales: &[BigUint],
    fee: &BigUint,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
    guards: &ManagedSwapGuards,
) -> Result<MultiQuote> {
    if !guards.swaps_enabled {
        return Err(err("BALANCER_MANAGED_SWAPS_DISABLED"));
    }
    if !guards.protocol_swap_fee.is_zero() || !guards.management_swap_fee.is_zero() {
        return Err(err("BALANCER_MANAGED_BPT_FEES_UNSUPPORTED"));
    }
    quote_inner(
        balances, weights, scales, fee, input, index_in, index_out, true,
    )
}

/// Decode *stored* 64-bit start/end denormalized weights (not the lossy
/// getGradualWeightUpdateParams() normalized values), using original
/// WeightCompression.decompress(64, 1e22), GradualValueChange, and divDown
/// normalization. `denormalized_sum` is getDenormalizedWeightSum().
/// Fixed timestamps permit reproducible historical/current-block snapshots;
/// a future timestamp is a projection, not a currently executable quote.
pub fn managed_weights_at(
    start_compressed: &[u64],
    end_compressed: &[u64],
    denormalized_sum: &BigUint,
    start_time: u32,
    end_time: u32,
    timestamp: u64,
) -> Result<Vec<BigUint>> {
    if !(2..=38).contains(&start_compressed.len()) || start_compressed.len() != end_compressed.len()
    {
        return Err(err("BALANCER_MULTI_LENGTH"));
    }
    checked(denormalized_sum.clone())?;
    if denormalized_sum.is_zero() || end_time < start_time {
        return Err(err("BALANCER_MANAGED_SCHEDULE"));
    }
    let one = BigUint::from(ONE);
    let progress = if timestamp > u64::from(end_time) {
        one.clone()
    } else if timestamp < u64::from(start_time) {
        BigUint::zero()
    } else if end_time == start_time {
        one.clone()
    } else {
        BigUint::from(timestamp - u64::from(start_time)) * &one
            / BigUint::from(end_time - start_time)
    };
    let max = BigUint::from(u64::MAX);
    let max_denorm = BigUint::from(10u8).pow(22);
    let decompress = |v: u64| -> Result<BigUint> {
        let rounded = div_up(mul(&BigUint::from(v), &max_denorm)?, &one)?;
        Ok(mul(&rounded, &one)? / &max)
    };
    start_compressed
        .iter()
        .zip(end_compressed)
        .map(|(&s, &e)| {
            let a = decompress(s)?;
            let b = decompress(e)?;
            let value = if a > b {
                &a - mul(&progress, &(&a - &b))? / &one
            } else {
                &a + mul(&progress, &(&b - &a))? / &one
            };
            Ok(mul(&value, &one)? / denormalized_sum)
        })
        .collect()
}
