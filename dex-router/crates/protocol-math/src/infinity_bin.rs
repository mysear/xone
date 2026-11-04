//! Independent pinned Infinity Bin math for one active bin. No CL approximation.
//! A quote that would traverse the liquidity tree requires a wider state capability.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use xone_domain::{err, Amount, Result};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InfinityBinState {
    pub active_id: u32,
    pub bin_step: u16,
    pub reserve0: Amount,
    pub reserve1: Amount,
    pub lp_fee: u32,
    pub protocol_fee0: u32,
    pub protocol_fee1: u32,
}
#[derive(Clone, Debug)]
pub struct InfinityBinSwap {
    pub amount_out: Amount,
    pub protocol_amount: Amount,
    pub swap_fee: u32,
    pub price_x128: Amount,
    pub next_state: InfinityBinState,
}
fn ceil(a: BigUint, b: &BigUint) -> BigUint {
    (a + b - BigUint::one()) / b
}

/// Exact PriceHelper.getPriceFromId / Uint128x128Math.pow rounding.
pub fn price_from_id(id: u32, step: u16) -> Result<BigUint> {
    if id == 0 || id >= 0xffffff || step == 0 {
        return Err(err("INFINITY_BIN_PARAMETERS"));
    }
    let exponent = i64::from(id) - (1 << 23);
    if exponent == 0 {
        return Ok(BigUint::one() << 128usize);
    }
    if exponent.unsigned_abs() >= 0x100000 {
        return Err(err("INFINITY_BIN_PRICE_RANGE"));
    }
    let scale = BigUint::one() << 128usize;
    let max = (BigUint::one() << 256usize) - BigUint::one();
    let base = &scale + BigUint::from(step) * &scale / BigUint::from(10000u32);
    // base > uint128.max: the pinned pow inverts the base before squaring.
    let mut squared = &max / base;
    let mut value = scale;
    let n = exponent.unsigned_abs();
    for bit in 0..20 {
        if n & (1 << bit) != 0 {
            value = (value * &squared) >> 128usize;
        }
        squared = (&squared * &squared) >> 128usize;
    }
    if value.is_zero() {
        return Err(err("INFINITY_BIN_PRICE_RANGE"));
    }
    Ok(if exponent > 0 { max / value } else { value })
}

/// Core-only snapshot primitive: caller must separately bind manager/key/hooks/block.
/// Inputs requiring another initialized bin fail closed instead of using a partial fill.
pub fn quote_exact_in(
    state: &InfinityBinState,
    input: &Amount,
    zero: bool,
) -> Result<InfinityBinSwap> {
    if state.lp_fee > 100000
        || state.protocol_fee0 > 4000
        || state.protocol_fee1 > 4000
        || state.reserve0.0.bits() > 128
        || state.reserve1.0.bits() > 128
    {
        return Err(err("INFINITY_BIN_STATE"));
    }
    if input.0.is_zero() || input.0 > (BigUint::one() << 127usize) {
        return Err(err("INFINITY_BIN_INPUT"));
    }
    let price = price_from_id(state.active_id, state.bin_step)?;
    let scale = BigUint::one() << 128usize;
    let million = BigUint::from(1_000_000u32);
    let protocol = if zero {
        state.protocol_fee0
    } else {
        state.protocol_fee1
    };
    let fee = protocol + state.lp_fee - protocol * state.lp_fee / 1_000_000;
    let factor = &million - BigUint::from(fee);
    let reserve_out = if zero {
        &state.reserve1.0
    } else {
        &state.reserve0.0
    };
    if reserve_out.is_zero() {
        return Err(err("INFINITY_BIN_NEXT_STATE_REQUIRED"));
    }
    let max_net = if zero {
        ceil(reserve_out * &scale, &price)
    } else {
        ceil(reserve_out * &price, &scale)
    };
    let max_fee = ceil(&max_net * BigUint::from(fee), &factor);
    let max_in = &max_net + &max_fee;
    if max_net.bits() > 128 || max_in.bits() > 128 {
        return Err(err("INFINITY_BIN_AMOUNT_WIDTH"));
    }
    if input.0 > max_in {
        return Err(err("INFINITY_BIN_NEXT_STATE_REQUIRED"));
    }
    let (output, total_fee) = if input.0 == max_in {
        (reserve_out.clone(), max_fee)
    } else {
        // BinHelper rounds gross-input fee UP, unlike CL's available-input floor rule.
        let total_fee = ceil(&input.0 * BigUint::from(fee), &million);
        let net = &input.0 - &total_fee;
        let output = if zero {
            net * &price / &scale
        } else {
            net * &scale / &price
        };
        (output.min(reserve_out.clone()), total_fee)
    };
    if output.is_zero() || output >= (BigUint::one() << 127usize) {
        return Err(err("INFINITY_BIN_OUTPUT"));
    }
    // BinPool allocates from totalFee by ratio, not CL's gross-input protocol rule.
    let protocol_amount = if fee == 0 {
        BigUint::zero()
    } else {
        &total_fee * BigUint::from(protocol) / BigUint::from(fee)
    };
    let mut next = state.clone();
    if zero {
        next.reserve0 = Amount::checked(&state.reserve0.0 + &input.0 - &protocol_amount)?;
        next.reserve1 = Amount::checked(&state.reserve1.0 - &output)?;
    } else {
        next.reserve1 = Amount::checked(&state.reserve1.0 + &input.0 - &protocol_amount)?;
        next.reserve0 = Amount::checked(&state.reserve0.0 - &output)?;
    }
    if next.reserve0.0.bits() > 128 || next.reserve1.0.bits() > 128 {
        return Err(err("INFINITY_BIN_RESERVE_WIDTH"));
    }
    let max_liquidity = BigUint::parse_bytes(
        b"65251743116719673010965625540244653191619923014385985379600384103134737",
        10,
    )
    .ok_or_else(|| err("INFINITY_BIN_CONSTANT"))?;
    if &next.reserve0.0 * &price + &next.reserve1.0 * &scale > max_liquidity {
        return Err(err("INFINITY_BIN_LIQUIDITY_LIMIT"));
    }
    Ok(InfinityBinSwap {
        amount_out: Amount::checked(output)?,
        protocol_amount: Amount::checked(protocol_amount)?,
        swap_fee: fee,
        price_x128: Amount::checked(price)?,
        next_state: next,
    })
}
