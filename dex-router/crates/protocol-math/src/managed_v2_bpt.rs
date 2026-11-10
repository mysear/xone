//! Published 20230411 ManagedPool single-token BPT GIVEN_IN arithmetic.
//!
//! Exact port of the pinned WeightedMath single-token join/exit helpers. This
//! amount kernel does not establish source identity, collect AUM, check caller
//! allowlists/circuit breakers, expose BPT graph edges or perform full ledgers.
//! Those belong to the separately versioned complete-state BPT adapter.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Result};

#[derive(Clone, Copy, Debug)]
pub struct BptSwapInputs<'a> {
    pub cash: &'a BigUint,
    pub managed: &'a BigUint,
    pub scaling: &'a BigUint,
    pub weight: &'a BigUint,
    pub fee: &'a BigUint,
    /// Original actualSupply AFTER the pending AUM collection. Its value
    /// equals the successful source getActualSupply view at that same clock.
    pub actual_supply: &'a BigUint,
}
fn uint(n: BigUint) -> Result<BigUint> {
    if n.bits() > 256 {
        Err(err("MANAGED_V2_BPT_UINT256_OVERFLOW"))
    } else {
        Ok(n)
    }
}
fn add(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    uint(a + b)
}
fn sub(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if a < b {
        Err(err("MANAGED_V2_BPT_UNDERFLOW"))
    } else {
        Ok(a - b)
    }
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    uint(a * b)
}
fn one() -> BigUint {
    BigUint::from(1_000_000_000_000_000_000u64)
}
fn up(n: BigUint, d: &BigUint) -> Result<BigUint> {
    if d.is_zero() {
        return Err(err("MANAGED_V2_BPT_ZERO_DIVISION"));
    }
    Ok(if n.is_zero() {
        n
    } else {
        (n - BigUint::one()) / d + BigUint::one()
    })
}
fn md(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    Ok(mul(a, b)? / one())
}
fn mu(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    up(mul(a, b)?, &one())
}
fn dd(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    if b.is_zero() {
        return Err(err("MANAGED_V2_BPT_ZERO_DIVISION"));
    }
    Ok(mul(a, &one())? / b)
}
fn du(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    up(mul(a, &one())?, b)
}
fn complement(n: &BigUint) -> BigUint {
    let o = one();
    if n >= &o {
        BigUint::zero()
    } else {
        o - n
    }
}
fn validate(s: BptSwapInputs<'_>, input: &BigUint) -> Result<BigUint> {
    for n in [
        s.cash,
        s.managed,
        s.scaling,
        s.weight,
        s.fee,
        s.actual_supply,
        input,
    ] {
        uint(n.clone())?;
    }
    let o = one();
    if input.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    if s.weight < &(&o / 100u8) || s.weight > &o || s.fee > &o || s.actual_supply.is_zero() {
        return Err(err("MANAGED_V2_BPT_PARAMETERS"));
    }
    if !(0..=18).any(|d| s.scaling == &(&o * BigUint::from(10u8).pow(d))) {
        return Err(err("MANAGED_V2_BPT_SCALING"));
    }
    let balance = add(s.cash, s.managed)?;
    if balance.bits() > 112 {
        return Err(err("MANAGED_V2_BPT_VAULT_UINT112"));
    }
    if balance.is_zero() {
        return Err(err("INSUFFICIENT_LIQUIDITY"));
    }
    md(&balance, s.scaling)
}
/// Underlying exact input -> BPT output. Unlike the regular token swap,
/// fees apply only to the virtual non-proportional component. No vector weight
/// normalization and no regular-swap30% input cap is introduced here.
#[derive(Clone, Debug)]
pub struct BptJoinCalculation<'a> {
    pub amount_out: BigUint,
    pub gross_input_scaled: BigUint,
    state: BptSwapInputs<'a>,
    input: &'a BigUint,
}
impl BptJoinCalculation<'_> {
    pub(crate) fn settle(self) -> Result<BigUint> {
        if add(&add(self.state.cash, self.state.managed)?, self.input)?.bits() > 112 {
            return Err(err("MANAGED_V2_BPT_VAULT_UINT112"));
        }
        Ok(self.amount_out)
    }
}
pub fn calculate_join_exact_in<'a>(
    s: BptSwapInputs<'a>,
    input: &'a BigUint,
) -> Result<BptJoinCalculation<'a>> {
    let balance = validate(s, input)?;
    calculate_join_inner(s, input, balance)
}
pub fn quote_join_exact_in(s: BptSwapInputs<'_>, input: &BigUint) -> Result<BigUint> {
    let balance = validate(s, input)?;
    // Retain the existing profile's admission/error order. The breaker adapter
    // uses calculate then checks the original callback before Vault settlement.
    if add(&add(s.cash, s.managed)?, input)?.bits() > 112 {
        return Err(err("MANAGED_V2_BPT_VAULT_UINT112"));
    }
    calculate_join_inner(s, input, balance)?.settle()
}
fn calculate_join_inner<'a>(
    s: BptSwapInputs<'a>,
    input: &'a BigUint,
    balance: BigUint,
) -> Result<BptJoinCalculation<'a>> {
    let scaled = md(input, s.scaling)?;
    let o = one();
    let ratio_with_fee = dd(&add(&balance, &scaled)?, &balance)?;
    let invariant_with_fees = add(&md(&ratio_with_fee, s.weight)?, &complement(s.weight))?;
    let without_fee = if ratio_with_fee > invariant_with_fees {
        let non_taxable = if invariant_with_fees > o {
            md(&balance, &sub(&invariant_with_fees, &o)?)?
        } else {
            BigUint::zero()
        };
        let taxable = sub(&scaled, &non_taxable)?;
        let fee = mu(&taxable, s.fee)?;
        add(&non_taxable, &sub(&taxable, &fee)?)?
    } else {
        scaled.clone()
    };
    let amount_out = if without_fee.is_zero() {
        BigUint::zero()
    } else {
        let ratio = dd(&add(&balance, &without_fee)?, &balance)?;
        let invariant = crate::balancer_log_exp::pow_down(&ratio, s.weight)?;
        if invariant > o {
            md(s.actual_supply, &sub(&invariant, &o)?)?
        } else {
            BigUint::zero()
        }
    };
    Ok(BptJoinCalculation {
        amount_out,
        gross_input_scaled: scaled,
        state: s,
        input,
    })
}
/// BPT exact input -> raw underlying output. Original divUp invariant guard
/// can admit a raw unit beyond a naive30% BPT input cap; reproduce its ratio.
#[derive(Clone, Debug)]
pub struct BptExitCalculation<'a> {
    pub amount_out: BigUint,
    pub scaled_amount_out: BigUint,
    state: BptSwapInputs<'a>,
}
impl BptExitCalculation<'_> {
    pub(crate) fn settle(self) -> Result<BigUint> {
        if &self.amount_out > self.state.cash {
            return Err(err("MANAGED_V2_BPT_INSUFFICIENT_REGISTERED_CASH"));
        }
        Ok(self.amount_out)
    }
}
pub fn quote_exit_exact_in(s: BptSwapInputs<'_>, input: &BigUint) -> Result<BigUint> {
    calculate_exit_exact_in(s, input)?.settle()
}
pub fn calculate_exit_exact_in<'a>(
    s: BptSwapInputs<'a>,
    input: &BigUint,
) -> Result<BptExitCalculation<'a>> {
    let balance = validate(s, input)?;
    let invariant = du(&sub(s.actual_supply, input)?, s.actual_supply)?;
    if invariant < BigUint::from(700_000_000_000_000_000u64) {
        return Err(err("MANAGED_V2_BPT_MIN_INVARIANT_RATIO"));
    }
    let power = crate::balancer_log_exp::pow_up(&invariant, &dd(&one(), s.weight)?)?;
    let no_fee = md(&balance, &complement(&power))?;
    let taxable = mu(&no_fee, &complement(s.weight))?;
    let non_taxable = sub(&no_fee, &taxable)?;
    let minus_fee = mu(&taxable, &complement(s.fee))?;
    let scaled_amount_out = add(&non_taxable, &minus_fee)?;
    let amount_out = dd(&scaled_amount_out, s.scaling)?;
    Ok(BptExitCalculation {
        amount_out,
        scaled_amount_out,
        state: s,
    })
}
