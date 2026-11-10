//! Actual 20230411 ManagedPool regular TOKEN-to-TOKEN GIVEN_IN arithmetic.
//! Fee is applied after upscaling, unlike the retained historical profiles.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};
#[derive(Clone, Copy, Debug)]
pub struct WeightedInputs<'a> {
    pub cash: &'a [BigUint],
    pub managed: &'a [BigUint],
    pub weights: &'a [BigUint],
    pub scales: &'a [BigUint],
    pub fee: &'a BigUint,
}
#[derive(Clone, Debug)]
pub struct RegularCalculation<'a> {
    pub amount_out: BigUint,
    pub scaled_amount_out: BigUint,
    pub gross_input_scaled: BigUint,
    state: WeightedInputs<'a>,
    input: &'a BigUint,
    index_in: usize,
    index_out: usize,
}
impl RegularCalculation<'_> {
    /// Original raw Vault settlement. The existing ordinary quote wrapper keeps
    /// its positive-output policy; circuit checks can inspect zero raw dust first.
    pub(crate) fn settle(self) -> Result<RegularQuote> {
        let state = self.state;
        if self.amount_out > state.cash[self.index_out] {
            return Err(err("MANAGED_V2_INSUFFICIENT_VAULT_CASH"));
        }
        let mut next = state.cash.to_vec();
        next[self.index_in] = checked(&next[self.index_in] + self.input)?;
        if checked(&next[self.index_in] + &state.managed[self.index_in])?.bits() > 112 {
            return Err(err("MANAGED_V2_VAULT_UINT112"));
        }
        next[self.index_out] -= &self.amount_out;
        Ok(RegularQuote {
            amount_out: self.amount_out,
            next_cash: next,
            next_managed: state.managed.to_vec(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegularQuote {
    pub amount_out: BigUint,
    pub next_cash: Vec<BigUint>,
    pub next_managed: Vec<BigUint>,
}
fn checked(v: BigUint) -> Result<BigUint> {
    if v.bits() > 256 {
        return Err(err("MANAGED_V2_UINT256_OVERFLOW"));
    }
    Ok(v)
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    checked(a * b)
}
fn up(n: BigUint, d: &BigUint) -> Result<BigUint> {
    if d.is_zero() {
        return Err(err("MANAGED_V2_DIVISION_BY_ZERO"));
    }
    Ok(if n.is_zero() {
        n
    } else {
        (n - BigUint::one()) / d + BigUint::one()
    })
}
/// Frozen terminal-weight/fee inputs from one source bank. The full-state
/// adapter establishes code, pause/swap flags, token behavior and schedule scope.
pub fn quote_regular(
    state: WeightedInputs<'_>,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<RegularQuote> {
    quote_inner(state, input, index_in, index_out, true)
}
/// Only the exact raw projection may bypass the old whole-vector sum guard.
/// Selected weights still enter the original integer ratio and LogExp kernel.
pub(crate) fn quote_scheduled(
    state: WeightedInputs<'_>,
    schedule: &xone_domain::managed_v2_schedule::ManagedV2Schedule,
    timestamp: u64,
    view: &xone_domain::managed_v2_schedule::ManagedV2ScheduleView,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<RegularQuote> {
    let calculation =
        calculate_scheduled(state, schedule, timestamp, view, input, index_in, index_out)?;
    if calculation.amount_out.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    calculation.settle()
}
pub(crate) fn calculate_scheduled<'a>(
    state: WeightedInputs<'a>,
    schedule: &xone_domain::managed_v2_schedule::ManagedV2Schedule,
    timestamp: u64,
    view: &xone_domain::managed_v2_schedule::ManagedV2ScheduleView,
    input: &'a BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<RegularCalculation<'a>> {
    let expected = schedule.at(timestamp)?;
    if &expected != view
        || state.weights.len() != expected.weights.len()
        || state.scales.len() != expected.scaling.len()
        || state
            .weights
            .iter()
            .zip(&expected.weights)
            .any(|(a, b)| a != &b.0)
        || state
            .scales
            .iter()
            .zip(&expected.scaling)
            .any(|(a, b)| a != &b.0)
        || state.fee != &expected.swap_fee.0
    {
        return Err(err("MANAGED_V2_SCHEDULE_GETTER_MISMATCH"));
    }
    calculate_inner(state, input, index_in, index_out, false)
}
pub fn calculate_regular<'a>(
    state: WeightedInputs<'a>,
    input: &'a BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<RegularCalculation<'a>> {
    calculate_inner(state, input, index_in, index_out, true)
}
fn quote_inner(
    state: WeightedInputs<'_>,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
    require_exact_sum: bool,
) -> Result<RegularQuote> {
    let calculation = calculate_inner(state, input, index_in, index_out, require_exact_sum)?;
    if calculation.amount_out.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    calculation.settle()
}
fn calculate_inner<'a>(
    state: WeightedInputs<'a>,
    input: &'a BigUint,
    index_in: usize,
    index_out: usize,
    require_exact_sum: bool,
) -> Result<RegularCalculation<'a>> {
    let count = state.cash.len();
    if !(2..=50).contains(&count)
        || state.managed.len() != count
        || state.weights.len() != count
        || state.scales.len() != count
    {
        return Err(err("MANAGED_V2_TOKEN_COUNT"));
    }
    if index_in >= count || index_out >= count || index_in == index_out {
        return Err(err("MANAGED_V2_TOKEN_INDEX"));
    }
    let one = BigUint::from(1_000_000_000_000_000_000u64);
    let minimum = &one / 100u8;
    if state.weights.iter().any(|w| w < &minimum || w > &one)
        || (require_exact_sum && state.weights.iter().sum::<BigUint>() != one)
    {
        return Err(err("MANAGED_V2_NORMALIZED_WEIGHTS"));
    }
    for s in state.scales {
        if !(0..=18).any(|d| *s == &one * BigUint::from(10u8).pow(d)) {
            return Err(err("MANAGED_V2_SCALING_FACTOR"));
        }
    }
    if state.fee > &one || input.bits() > 256 {
        return Err(err("MANAGED_V2_SWAP_PARAMETERS"));
    }
    if input.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    let mut totals = Vec::with_capacity(count);
    for (cash, managed) in state.cash.iter().zip(state.managed) {
        let total = checked(cash + managed)?;
        if total.bits() > 112 {
            return Err(err("MANAGED_V2_VAULT_UINT112"));
        }
        totals.push(total);
    }
    if totals[index_in].is_zero() || totals[index_out].is_zero() {
        return Err(err("INSUFFICIENT_LIQUIDITY"));
    }
    let balance_in = mul(&totals[index_in], &state.scales[index_in])? / &one;
    let balance_out = mul(&totals[index_out], &state.scales[index_out])? / &one;
    let gross_scaled = mul(input, &state.scales[index_in])? / &one;
    let net_scaled = mul(&gross_scaled, &(&one - state.fee))? / &one;
    if net_scaled > mul(&balance_in, &BigUint::from(300_000_000_000_000_000u64))? / &one {
        return Err(err("MANAGED_V2_MAX_IN_RATIO"));
    }
    let denominator = checked(&balance_in + &net_scaled)?;
    let base = up(mul(&balance_in, &one)?, &denominator)?;
    let exponent = mul(&state.weights[index_in], &one)? / &state.weights[index_out];
    let power = crate::balancer_log_exp::pow_up(&base, &exponent)?;
    let complement = if power >= one {
        BigUint::zero()
    } else {
        &one - power
    };
    let scaled_out = mul(&balance_out, &complement)? / &one;
    let amount_out = mul(&scaled_out, &one)? / &state.scales[index_out];
    if amount_out >= totals[index_out] {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    Ok(RegularCalculation {
        amount_out,
        scaled_amount_out: scaled_out,
        gross_input_scaled: gross_scaled,
        state,
        input,
        index_in,
        index_out,
    })
}
