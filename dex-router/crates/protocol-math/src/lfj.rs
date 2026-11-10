//! Pinned Liquidity Book v2.1.0 exact-in, complete declared bin set and oracle.
use crate::Swap;
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{
    err,
    lfj::{field, set_field, LfjState, PROFILE},
    Amount, Pool, Result,
};
fn narrow(value: BigUint) -> Result<Amount> {
    if value.bits() > 128 {
        return Err(err("LFJ_UINT128_OVERFLOW"));
    }
    Ok(Amount(value))
}
fn ceil(value: BigUint, divisor: &BigUint) -> BigUint {
    (value + divisor - 1u8) / divisor
}
fn update_oracle(state: &mut LfjState, active: u32) -> Result<()> {
    let id = field(&state.parameters, 216, 16) as u32;
    if id == 0 {
        return Ok(());
    }
    let index = state
        .oracle_samples
        .iter()
        .position(|s| s.index == id)
        .ok_or_else(|| err("LFJ_ORACLE_MISSING"))?;
    let mut sample = state.oracle_samples[index].data.clone();
    let mut created = field(&sample, 216, 40);
    let mut lifetime = field(&sample, 208, 8);
    let last = created
        .checked_add(lifetime)
        .ok_or_else(|| err("LFJ_ORACLE_CLOCK"))?;
    if state.timestamp > last {
        let delta = state.timestamp - last;
        let original_active = field(&state.parameters, 232, 24);
        for (offset, value) in [
            (16, u64::from(active)),
            (80, field(&state.parameters, 112, 20)),
            (144, original_active.abs_diff(u64::from(active))),
        ] {
            let weighted = value.wrapping_mul(delta);
            let cumulative = weighted
                .checked_add(field(&sample, offset, 64))
                .ok_or_else(|| err("LFJ_ORACLE_UINT64_OVERFLOW"))?;
            set_field(&mut sample, offset, 64, cumulative);
        }
        lifetime = state.timestamp - created;
        let destination = if lifetime > 120 {
            lifetime = 0;
            created = state.timestamp;
            let length = field(&sample, 0, 16);
            let next = (u64::from(id) % length + 1) as u32;
            set_field(&mut state.parameters, 216, 16, u64::from(next));
            next
        } else {
            id
        };
        set_field(&mut sample, 208, 8, lifetime);
        set_field(&mut sample, 216, 40, created);
        let target = state
            .oracle_samples
            .iter_mut()
            .find(|s| s.index == destination)
            .ok_or_else(|| err("LFJ_ORACLE_NEXT_MISSING"))?;
        target.data = sample;
    }
    Ok(())
}
fn reference_parameters(state: &LfjState) -> Amount {
    let mut parameters = state.parameters.clone();
    let active = field(&parameters, 232, 24) as u32;
    let elapsed = state.timestamp - field(&parameters, 176, 40);
    if elapsed >= field(&parameters, 16, 12) {
        set_field(&mut parameters, 152, 24, u64::from(active));
        let reference = if elapsed < field(&parameters, 28, 12) {
            field(&parameters, 112, 20) * field(&parameters, 40, 14) / 10000
        } else {
            0
        };
        set_field(&mut parameters, 132, 20, reference);
    }
    set_field(&mut parameters, 176, 40, state.timestamp);
    parameters
}
fn step_fee(parameters: &mut Amount, active: u32, bin_step: u32) -> Result<BigUint> {
    let delta = u64::from(active).abs_diff(field(parameters, 152, 24));
    let volatility = (field(parameters, 132, 20) + delta * 10000).min(field(parameters, 92, 20));
    set_field(parameters, 112, 20, volatility);
    let product = u128::from(volatility) * u128::from(bin_step);
    let fee = u128::from(field(parameters, 0, 16)) * u128::from(bin_step) * 10_000_000_000u128
        + (product * product * u128::from(field(parameters, 54, 24))).div_ceil(100);
    if fee > 100_000_000_000_000_000 {
        return Err(err("LFJ_MAX_FEE"));
    }
    Ok(BigUint::from(fee))
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero: bool) -> Result<Swap> {
    pool.validate()?;
    if pool.protocol != PROFILE {
        return Err(err("LFJ_PROFILE"));
    }
    if input.0.is_zero() || input.0.bits() > 128 {
        return Err(err("LFJ_INPUT_UINT128"));
    }
    let mut next = pool.clone();
    let state = next.lfj.as_mut().ok_or_else(|| err("LFJ_STATE_REQUIRED"))?;
    state.parameters = reference_parameters(state);
    let mut active = field(&state.parameters, 232, 24) as u32;
    let mut remaining = input.0.clone();
    let mut out = BigUint::zero();
    let scale = BigUint::one() << 128usize;
    let precision = BigUint::from(10u8).pow(18);
    let mut steps = 0;
    loop {
        steps += 1;
        if steps > state.max_steps {
            return Err(err("LFJ_MAX_STEPS"));
        }
        if let Ok(index) = state.bins.binary_search_by_key(&active, |b| b.id) {
            let available = if zero {
                &state.bins[index].reserve1.0
            } else {
                &state.bins[index].reserve0.0
            };
            if !available.is_zero() {
                let fee = step_fee(&mut state.parameters, active, state.bin_step)?;
                let price = crate::infinity_bin::price_from_id(active, state.bin_step as u16)
                    .map_err(|_| err("LFJ_PRICE_RANGE"))?;
                let max_net = if zero {
                    ceil(available * &scale, &price)
                } else {
                    ceil(available * &price, &scale)
                };
                narrow(max_net.clone())?;
                let max_fee = ceil(&max_net * &fee, &(&precision - &fee));
                let max_input = narrow(&max_net + &max_fee)?.0;
                let (spent, output, total_fee) = if remaining >= max_input {
                    (max_input, available.clone(), max_fee)
                } else {
                    let charge = ceil(&remaining * &fee, &precision);
                    let effective = &remaining - &charge;
                    let output = if zero {
                        effective * &price / &scale
                    } else {
                        effective * &scale / &price
                    };
                    let output = narrow(output)?.0.min(available.clone());
                    (remaining.clone(), output, charge)
                };
                let protocol = &total_fee * field(&state.parameters, 78, 14) / 10000u32;
                let bin = &mut state.bins[index];
                if zero {
                    bin.reserve0 = narrow(&bin.reserve0.0 + &spent - &protocol)?;
                    bin.reserve1 = narrow(&bin.reserve1.0 - &output)?;
                    state.protocol_fees0 = narrow(&state.protocol_fees0.0 + protocol)?;
                } else {
                    bin.reserve1 = narrow(&bin.reserve1.0 + &spent - &protocol)?;
                    bin.reserve0 = narrow(&bin.reserve0.0 - &output)?;
                    state.protocol_fees1 = narrow(&state.protocol_fees1.0 + protocol)?;
                }
                remaining -= spent;
                out = narrow(out + output)?.0;
            }
        }
        if remaining.is_zero() {
            break;
        }
        active = if zero {
            state.bins.iter().rev().find(|b| b.id < active)
        } else {
            state.bins.iter().find(|b| b.id > active)
        }
        .ok_or_else(|| err("LFJ_OUT_OF_LIQUIDITY"))?
        .id;
    }
    if out.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    if zero {
        state.raw_reserve0 = narrow(&state.raw_reserve0.0 + &input.0)?;
        state.raw_reserve1 = narrow(&state.raw_reserve1.0 - &out)?;
    } else {
        state.raw_reserve1 = narrow(&state.raw_reserve1.0 + &input.0)?;
        state.raw_reserve0 = narrow(&state.raw_reserve0.0 - &out)?;
    }
    update_oracle(state, active)?;
    set_field(&mut state.parameters, 232, 24, u64::from(active));
    next.validate()?;
    Ok(Swap {
        amount_out: Amount::checked(out)?,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}

/// Original getSwapIn output-driven integer steps: gross input and total fee.
/// The physical Pair may deliver surplus because its swap accepts only input.
pub fn input_for_target(pool: &Pool, output: &Amount, zero: bool) -> Result<(Amount, Amount)> {
    pool.validate()?;
    if pool.protocol != PROFILE {
        return Err(err("LFJ_PROFILE"));
    }
    if output.0.is_zero() || output.0.bits() > 128 {
        return Err(err("LFJ_OUTPUT_UINT128"));
    }
    let state = pool.lfj.as_ref().ok_or_else(|| err("LFJ_STATE_REQUIRED"))?;
    let mut parameters = reference_parameters(state);
    let mut active = field(&parameters, 232, 24) as u32;
    let mut remaining = output.0.clone();
    let mut required = BigUint::zero();
    let mut fees = BigUint::zero();
    let scale = BigUint::one() << 128usize;
    let precision = BigUint::from(10u8).pow(18);
    let mut steps = 0;
    loop {
        steps += 1;
        if steps > state.max_steps {
            return Err(err("LFJ_MAX_STEPS"));
        }
        if let Ok(index) = state.bins.binary_search_by_key(&active, |b| b.id) {
            let available = if zero {
                &state.bins[index].reserve1.0
            } else {
                &state.bins[index].reserve0.0
            };
            if !available.is_zero() {
                let target = available.min(&remaining).clone();
                let fee = step_fee(&mut parameters, active, state.bin_step)?;
                let price = crate::infinity_bin::price_from_id(active, state.bin_step as u16)
                    .map_err(|_| err("LFJ_PRICE_RANGE"))?;
                let net = narrow(if zero {
                    ceil(&target * &scale, &price)
                } else {
                    ceil(&target * &price, &scale)
                })?
                .0;
                let charge = ceil(&net * &fee, &(&precision - &fee));
                required = narrow(required + net + &charge)?.0;
                fees = narrow(fees + charge)?.0;
                remaining -= target;
            }
        }
        if remaining.is_zero() {
            break;
        }
        active = if zero {
            state.bins.iter().rev().find(|b| b.id < active)
        } else {
            state.bins.iter().find(|b| b.id > active)
        }
        .ok_or_else(|| err("LFJ_OUT_OF_LIQUIDITY"))?
        .id;
    }
    Ok((narrow(required)?, narrow(fees)?))
}
/// Original Pair delivery may exceed the minimum target. Return the physical
/// amount and unchanged ledger, so the route can account for every output atom.
pub fn quote_exact_out(pool: &Pool, output: &Amount, zero: bool) -> Result<Swap> {
    let (input, _) = input_for_target(pool, output, zero)?;
    let settled = quote_exact_in(pool, &input, zero)?;
    if settled.amount_out.0 < output.0 {
        return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
    }
    Ok(settled)
}
