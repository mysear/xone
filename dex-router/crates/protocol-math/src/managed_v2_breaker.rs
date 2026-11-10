//! Exact published CircuitBreakerStorageLib/CircuitBreakerLib primitives.
//!
//! View prices include the getter's token scaling. Execution checks instead
//! consume the original INTERNAL scaled balance before output downscaling.
//! A zero bound short-circuits division exactly as the original library does.
use num_bigint::BigUint;
use num_traits::Zero;
use xone_domain::{err, Result};
fn one() -> BigUint {
    BigUint::from(1_000_000_000_000_000_000u64)
}
fn uint(value: BigUint) -> Result<BigUint> {
    if value.bits() > 256 {
        Err(err("MANAGED_V2_CIRCUIT_UINT256"))
    } else {
        Ok(value)
    }
}
fn mul(a: &BigUint, b: &BigUint) -> Result<BigUint> {
    uint(a * b)
}
fn divide_up(n: BigUint, d: &BigUint) -> Result<BigUint> {
    if d.is_zero() {
        return Err(err("MANAGED_V2_CIRCUIT_ZERO_DIVISION"));
    }
    Ok(if n.is_zero() { n } else { (n - 1u8) / d + 1u8 })
}
fn fixed(a: &BigUint, b: &BigUint, up: bool) -> Result<BigUint> {
    let n = mul(a, b)?;
    if up {
        divide_up(n, &one())
    } else {
        Ok(n / one())
    }
}
fn field(word: &BigUint, offset: usize, bits: usize) -> BigUint {
    (word >> offset) & ((BigUint::from(1u8) << bits) - 1u8)
}
fn complement(weight: &BigUint) -> BigUint {
    let o = one();
    if weight >= &o {
        BigUint::zero()
    } else {
        o - weight
    }
}
/// Original internal boundary. Cached ratios retain their stored32-bit value;
/// they must not be recomputed from already quantized percentage fields.
pub fn price_bound(word: &BigUint, weight: &BigUint, is_lower: bool) -> Result<BigUint> {
    uint(word.clone())?;
    uint(weight.clone())?;
    let bound = field(word, if is_lower { 0 } else { 16 }, 16) << 48usize;
    if bound.is_zero() {
        return Ok(BigUint::zero());
    }
    let reference = field(word, 32, 64);
    let ratio = if &reference == weight {
        let compressed = field(word, if is_lower { 192 } else { 224 }, 32);
        mul(&compressed, &(one() * 10u8))? / BigUint::from(u32::MAX)
    } else if is_lower {
        crate::balancer_log_exp::pow_up(&bound, &complement(weight))?
    } else {
        crate::balancer_log_exp::pow_down(&bound, &complement(weight))?
    };
    fixed(&field(word, 96, 96), &ratio, is_lower)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CircuitView {
    pub reference_bpt_price: BigUint,
    pub reference_weight: BigUint,
    pub lower_percentage: BigUint,
    pub upper_percentage: BigUint,
    pub lower_bpt_price_bound: BigUint,
    pub upper_bpt_price_bound: BigUint,
}
impl CircuitView {
    pub fn as_values(&self) -> Vec<BigUint> {
        vec![
            self.reference_bpt_price.clone(),
            self.reference_weight.clone(),
            self.lower_percentage.clone(),
            self.upper_percentage.clone(),
            self.lower_bpt_price_bound.clone(),
            self.upper_bpt_price_bound.clone(),
        ]
    }
}
/// getCircuitBreakerState restores prices to manager-facing raw-token units.
pub fn derive_view(word: &BigUint, weight: &BigUint, scaling: &BigUint) -> Result<CircuitView> {
    uint(word.clone())?;
    uint(weight.clone())?;
    uint(scaling.clone())?;
    if !(0..=18).any(|n| scaling == &(one() * BigUint::from(10u8).pow(n))) {
        return Err(err("MANAGED_V2_CIRCUIT_SCALING"));
    }
    Ok(CircuitView {
        reference_bpt_price: fixed(&field(word, 96, 96), scaling, false)?,
        reference_weight: field(word, 32, 64),
        lower_percentage: field(word, 0, 16) << 48usize,
        upper_percentage: field(word, 16, 16) << 48usize,
        lower_bpt_price_bound: fixed(&price_bound(word, weight, true)?, scaling, false)?,
        upper_bpt_price_bound: fixed(&price_bound(word, weight, false)?, scaling, false)?,
    })
}
/// Round current price DOWN for lower checks and UP for upper checks. Equality
/// passes. The caller supplies post-operation supply and the original scaled
/// hypothetical balance; raw wallet output may still round to zero afterwards.
pub fn check_side(
    word: &BigUint,
    supply: &BigUint,
    weight: &BigUint,
    balance_scaled: &BigUint,
    is_lower: bool,
) -> Result<()> {
    for value in [word, supply, weight, balance_scaled] {
        uint(value.clone())?;
    }
    let bound = price_bound(word, weight, is_lower)?;
    if bound.is_zero() {
        return Ok(());
    }
    let n = mul(supply, weight)?;
    let current = if is_lower {
        if balance_scaled.is_zero() {
            return Err(err("MANAGED_V2_CIRCUIT_ZERO_DIVISION"));
        }
        n / balance_scaled
    } else {
        divide_up(n, balance_scaled)?
    };
    if (is_lower && current < bound) || (!is_lower && current > bound) {
        return Err(err("MANAGED_V2_CIRCUIT_BREAKER_TRIPPED"));
    }
    Ok(())
}

/// Domain keeps raw structure and cached views; pricing must independently
/// verify every current dynamic getter with the original pow kernel.
pub fn validate_pool_view(pool: &xone_domain::Pool) -> Result<()> {
    let Some(s) = &pool.multi_asset else {
        return Ok(());
    };
    let Some(m) = &s.managed_v2 else {
        return Ok(());
    };
    let Some(c) = &m.circuit_breakers else {
        return Ok(());
    };
    if c.tokens.len() != s.assets.len() || s.weights.len() != s.assets.len() {
        return Err(err("MANAGED_V2_CIRCUIT_TOKEN_COUNT"));
    }
    for ((token, asset), weight) in c.tokens.iter().zip(&s.assets).zip(&s.weights) {
        let current = derive_view(&token.state_word.0, &weight.0, &asset.scaling.0)?;
        let observed = vec![
            token.reference_bpt_price.0.clone(),
            token.reference_weight.0.clone(),
            token.lower_percentage.0.clone(),
            token.upper_percentage.0.clone(),
            token.lower_bpt_price_bound.0.clone(),
            token.upper_bpt_price_bound.0.clone(),
        ];
        if token.token != asset.token || current.as_values() != observed {
            return Err(err("MANAGED_V2_CIRCUIT_CURRENT_GETTER"));
        }
    }
    Ok(())
}
/// Recompute dynamic getter views at a derived execution clock; raw words and
/// cached reference values remain unchanged and are never normalized.
pub(crate) fn refresh_pool_view(pool: &mut xone_domain::Pool) -> Result<()> {
    let Some(s) = pool.multi_asset.as_mut() else {
        return Ok(());
    };
    let Some(m) = s.managed_v2.as_mut() else {
        return Ok(());
    };
    let Some(c) = m.circuit_breakers.as_mut() else {
        return Ok(());
    };
    for ((token, asset), weight) in c.tokens.iter_mut().zip(&s.assets).zip(&s.weights) {
        let view = derive_view(&token.state_word.0, &weight.0, &asset.scaling.0)?;
        token.reference_bpt_price = xone_domain::Amount(view.reference_bpt_price);
        token.reference_weight = xone_domain::Amount(view.reference_weight);
        token.lower_percentage = xone_domain::Amount(view.lower_percentage);
        token.upper_percentage = xone_domain::Amount(view.upper_percentage);
        token.lower_bpt_price_bound = xone_domain::Amount(view.lower_bpt_price_bound);
        token.upper_bpt_price_bound = xone_domain::Amount(view.upper_bpt_price_bound);
    }
    Ok(())
}
/// Raw original callback conditions for a regular underlying swap. Input uses
/// the gross scaled amount (fees remain in the pool); output is pre-downscale.
pub(crate) fn check_regular(
    pool: &xone_domain::Pool,
    index_in: usize,
    index_out: usize,
    gross_scaled: &BigUint,
    out_scaled: &BigUint,
) -> Result<()> {
    let s = pool
        .multi_asset
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let m = s
        .managed_v2
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let Some(c) = &m.circuit_breakers else {
        return Ok(());
    };
    let balances = s
        .assets
        .iter()
        .map(|a| fixed(&a.balance.0, &a.scaling.0, false))
        .collect::<Result<Vec<_>>>()?;
    let input = uint(&balances[index_in] + gross_scaled)?;
    if &balances[index_out] < out_scaled {
        return Err(err("MANAGED_V2_CIRCUIT_UNDERFLOW"));
    }
    let output = &balances[index_out] - out_scaled;
    check_side(
        &c.tokens[index_in].state_word.0,
        &m.actual_supply.0,
        &s.weights[index_in].0,
        &input,
        true,
    )?;
    check_side(
        &c.tokens[index_out].state_word.0,
        &m.actual_supply.0,
        &s.weights[index_out].0,
        &output,
        false,
    )
}
/// A BPT swap changes actual supply and hence every token's BPT price. Check
/// both directions for every registered underlying, even an untouched token.
pub(crate) fn check_bpt(
    pool: &xone_domain::Pool,
    index: usize,
    input: &BigUint,
    output: &BigUint,
    scaled_underlying_amount: &BigUint,
    join: bool,
) -> Result<()> {
    let s = pool
        .multi_asset
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let m = s
        .managed_v2
        .as_ref()
        .ok_or_else(|| err("MANAGED_V2_STATE_REQUIRED"))?;
    let Some(c) = &m.circuit_breakers else {
        return Ok(());
    };
    let supply = if join {
        uint(&m.actual_supply.0 + output)?
    } else {
        if m.actual_supply.0 < *input {
            return Err(err("MANAGED_V2_CIRCUIT_UNDERFLOW"));
        }
        &m.actual_supply.0 - input
    };
    for (i, asset) in s.assets.iter().enumerate() {
        let mut balance = fixed(&asset.balance.0, &asset.scaling.0, false)?;
        if i == index {
            if join {
                balance = uint(balance + scaled_underlying_amount)?;
            } else {
                if balance < *scaled_underlying_amount {
                    return Err(err("MANAGED_V2_CIRCUIT_UNDERFLOW"));
                }
                balance -= scaled_underlying_amount;
            }
        }
        check_side(
            &c.tokens[i].state_word.0,
            &supply,
            &s.weights[i].0,
            &balance,
            true,
        )?;
        check_side(
            &c.tokens[i].state_word.0,
            &supply,
            &s.weights[i].0,
            &balance,
            false,
        )?;
    }
    Ok(())
}
