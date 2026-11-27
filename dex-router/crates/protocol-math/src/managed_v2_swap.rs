//! Published 20230411 ManagedPool regular token-to-token arithmetic.
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
#[derive(Clone, Copy)]
pub(crate) enum SwapAmount<'a> {
    In(&'a BigUint),
    Out(&'a BigUint),
}

#[derive(Clone, Debug)]
pub struct RegularCalculation<'a> {
    pub amount_out: BigUint,
    pub scaled_amount_out: BigUint,
    pub gross_input_scaled: BigUint,
    state: WeightedInputs<'a>,
    input: BigUint,
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
        next[self.index_in] = checked(&next[self.index_in] + &self.input)?;
        if checked(&next[self.index_in] + &state.managed[self.index_in])?.bits() > 112 {
            return Err(err("MANAGED_V2_VAULT_UINT112"));
        }
        next[self.index_out] -= &self.amount_out;
        Ok(RegularQuote {
            amount_in: self.input,
            amount_out: self.amount_out,
            next_cash: next,
            next_managed: state.managed.to_vec(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegularQuote {
    pub amount_in: BigUint,
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
    let calculation = calculate_regular(state, input, index_in, index_out)?;
    if calculation.amount_out.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    calculation.settle()
}

pub fn quote_regular_exact_out(
    state: WeightedInputs<'_>,
    output: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<RegularQuote> {
    calculate_inner(state, SwapAmount::Out(output), index_in, index_out, true)?.settle()
}

/// The raw schedule projection authenticates compressed weights and current fee.
pub(crate) fn calculate_scheduled<'a>(
    state: WeightedInputs<'a>,
    schedule: &xone_domain::managed_v2_schedule::ManagedV2Schedule,
    timestamp: u64,
    view: &xone_domain::managed_v2_schedule::ManagedV2ScheduleView,
    amount: SwapAmount<'_>,
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
    calculate_inner(state, amount, index_in, index_out, false)
}
pub fn calculate_regular<'a>(
    state: WeightedInputs<'a>,
    input: &BigUint,
    index_in: usize,
    index_out: usize,
) -> Result<RegularCalculation<'a>> {
    calculate_inner(state, SwapAmount::In(input), index_in, index_out, true)
}
fn calculate_inner<'a>(
    state: WeightedInputs<'a>,
    amount: SwapAmount<'_>,
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
    let (target, zero_error) = match amount {
        SwapAmount::In(input) => (input, "ZERO_INPUT"),
        SwapAmount::Out(output) => (output, "EXACT_OUT_ZERO_OUTPUT"),
    };
    if state.fee > &one || target.bits() > 256 {
        return Err(err("MANAGED_V2_SWAP_PARAMETERS"));
    }
    if target.is_zero() {
        return Err(err(zero_error));
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
    let fee_complement = &one - state.fee;
    let max_ratio = BigUint::from(300_000_000_000_000_000u64);
    let (input, amount_out, gross_scaled, scaled_out) = match amount {
        SwapAmount::In(input) => {
            let gross = mul(input, &state.scales[index_in])? / &one;
            let net = mul(&gross, &fee_complement)? / &one;
            if net > mul(&balance_in, &max_ratio)? / &one {
                return Err(err("MANAGED_V2_MAX_IN_RATIO"));
            }
            let denominator = checked(&balance_in + &net)?;
            let base = up(mul(&balance_in, &one)?, &denominator)?;
            let exponent = mul(&state.weights[index_in], &one)? / &state.weights[index_out];
            let power = crate::balancer_log_exp::pow_up(&base, &exponent)?;
            let complement = if power >= one {
                BigUint::zero()
            } else {
                &one - power
            };
            let scaled = mul(&balance_out, &complement)? / &one;
            let output = mul(&scaled, &one)? / &state.scales[index_out];
            (input.clone(), output, gross, scaled)
        }
        SwapAmount::Out(output) => {
            let scaled = mul(output, &state.scales[index_out])? / &one;
            if scaled > mul(&balance_out, &max_ratio)? / &one {
                return Err(err("MANAGED_V2_MAX_OUT_RATIO"));
            }
            let base = up(mul(&balance_out, &one)?, &(&balance_out - &scaled))?;
            let exponent = up(
                mul(&state.weights[index_out], &one)?,
                &state.weights[index_in],
            )?;
            let power = crate::balancer_log_exp::pow_up(&base, &exponent)?;
            if power < one {
                return Err(err("MANAGED_V2_WEIGHTED_POWER"));
            }
            let net = up(mul(&balance_in, &(power - &one))?, &one)?;
            let gross = up(mul(&net, &one)?, &fee_complement)?;
            let input = up(mul(&gross, &one)?, &state.scales[index_in])?;
            (input, output.clone(), gross, scaled)
        }
    };
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
