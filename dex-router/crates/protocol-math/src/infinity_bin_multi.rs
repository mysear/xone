//! Atomic sidecar for complete initialized Infinity bin trees; no manager dispatch.
use crate::infinity_bin::price_from_id;
use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};
use xone_domain::{err, Amount, Result};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BinReserves {
    pub reserve0: Amount,
    pub reserve1: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MultiBinState {
    pub active_id: u32,
    pub bin_step: u16,
    pub lp_fee: u32,
    pub protocol_fee0: u32,
    pub protocol_fee1: u32,
    pub max_steps: u32,
    pub tree_min_id: u32,
    pub tree_max_id: u32,
    pub bins: BTreeMap<u32, BinReserves>,
}
#[derive(Clone, Debug)]
pub struct BinFill {
    pub id: u32,
    pub amount_in: Amount,
    pub amount_out: Amount,
    pub total_fee: Amount,
    pub protocol_amount: Amount,
    pub price_x128: Amount,
}
#[derive(Clone, Debug)]
pub struct MultiBinSwap {
    pub amount_out: Amount,
    pub consumed_in: Amount,
    pub remaining_in: Amount,
    pub protocol_amount: Amount,
    pub swap_fee: u32,
    pub steps: u32,
    pub bin_fills: Vec<BinFill>,
    pub next_state: MultiBinState,
    pub detached_active: Option<BinReserves>,
}
/// Bounds are a caller attestation that `bins` enumerates the ENTIRE initialized
/// tree, including depleted bins. Only [0, 0xffffff] is supported. This kernel
/// cannot prove that attestation from a window; the manager/tree source must.
/// `max_steps` counts loop visits, including empty active/initialized bins.
/// No hook return deltas, fee overrides, settlement or public Pool profile.
pub fn quote_exact_in(state: &MultiBinState, input: &Amount, zero: bool) -> Result<MultiBinSwap> {
    quote_exact_in_with_active(state, input, zero, None)
}

/// Native burn may remove the active bin from the tree while leaving locked dust.
/// It is visited at the initial active ID; all subsequent traversal uses only real tree members.
pub fn quote_exact_in_with_active(
    state: &MultiBinState,
    input: &Amount,
    zero: bool,
    active: Option<&BinReserves>,
) -> Result<MultiBinSwap> {
    if let Some(b) = active {
        if state.bins.contains_key(&state.active_id)
            || b.reserve0.0.bits() > 128
            || b.reserve1.0.bits() > 128
        {
            return Err(err("INFINITY_BIN_DETACHED_ACTIVE"));
        }
    }
    let mut detached = active.cloned();
    if state.tree_min_id != 0 || state.tree_max_id != 0xffffff {
        return Err(err("INFINITY_BIN_COMPLETE_TREE_REQUIRED"));
    }
    if state.active_id == 0
        || state.active_id >= 0xffffff
        || state.bin_step == 0
        || state.lp_fee > 100000
        || state.protocol_fee0 > 4000
        || state.protocol_fee1 > 4000
        || state.bins.iter().any(|(id, b)| {
            *id == 0 || *id >= 0xffffff || b.reserve0.0.bits() > 128 || b.reserve1.0.bits() > 128
        })
    {
        return Err(err("INFINITY_BIN_STATE"));
    }
    let signed_max = (BigUint::one() << 127usize) - BigUint::one();
    // Native final amount.safeInt128() rejects 2^127, even for exact input.
    if input.0.is_zero() || input.0 > signed_max {
        return Err(err("INFINITY_BIN_INPUT"));
    }
    let mut next = state.clone();
    let mut remaining = input.0.clone();
    let mut output = BigUint::zero();
    let mut protocol_total = BigUint::zero();
    let mut bin_fills = Vec::new();
    let mut steps = 0;
    let scale = BigUint::one() << 128usize;
    let million = BigUint::from(1000000u32);
    let protocol = if zero {
        state.protocol_fee0
    } else {
        state.protocol_fee1
    };
    let fee = protocol + state.lp_fee - protocol * state.lp_fee / 1000000;
    let factor = &million - BigUint::from(fee);
    let max_liquidity: BigUint =
        "65251743116719673010965625540244653191619923014385985379600384103134737"
            .parse()
            .unwrap();
    loop {
        if steps >= state.max_steps {
            return Err(err("INFINITY_BIN_STEP_BUDGET"));
        }
        steps += 1;
        let id = next.active_id;
        let current = if id == state.active_id && detached.is_some() {
            detached.as_mut()
        } else {
            next.bins.get_mut(&id)
        };
        if let Some(bin) = current {
            let reserve_out = if zero {
                &bin.reserve1.0
            } else {
                &bin.reserve0.0
            };
            if !reserve_out.is_zero() {
                let price = price_from_id(id, state.bin_step)?;
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
                let (used, out, total_fee) = if remaining >= max_in {
                    (max_in, reserve_out.clone(), max_fee)
                } else {
                    let total_fee = ceil(&remaining * BigUint::from(fee), &million);
                    let net = &remaining - &total_fee;
                    let out = if zero {
                        net * &price / &scale
                    } else {
                        net * &scale / &price
                    };
                    if out.bits() > 128 {
                        return Err(err("INFINITY_BIN_AMOUNT_WIDTH"));
                    }
                    (remaining.clone(), out.min(reserve_out.clone()), total_fee)
                };
                let pfee = if fee == 0 {
                    BigUint::zero()
                } else {
                    &total_fee * BigUint::from(protocol) / BigUint::from(fee)
                };
                if zero {
                    bin.reserve0.0 += &used - &pfee;
                    bin.reserve1.0 -= &out;
                } else {
                    bin.reserve1.0 += &used - &pfee;
                    bin.reserve0.0 -= &out;
                }
                if bin.reserve0.0.bits() > 128 || bin.reserve1.0.bits() > 128 {
                    return Err(err("INFINITY_BIN_RESERVE_OVERFLOW"));
                }
                // Native PackedUint128Math.add and BinHelper.getLiquidity guards.
                let liquidity = &price * &bin.reserve0.0 + (&bin.reserve1.0 << 128usize);
                if liquidity.bits() > 256 {
                    return Err(err("INFINITY_BIN_LIQUIDITY_OVERFLOW"));
                }
                if liquidity > max_liquidity {
                    return Err(err("INFINITY_BIN_MAX_LIQUIDITY"));
                }
                remaining -= &used;
                output += &out;
                protocol_total += &pfee;
                if output.bits() > 128 || protocol_total.bits() > 128 {
                    return Err(err("INFINITY_BIN_AMOUNT_WIDTH"));
                }
                bin_fills.push(BinFill {
                    id,
                    amount_in: Amount(used),
                    amount_out: Amount(out),
                    total_fee: Amount(total_fee),
                    protocol_amount: Amount(pfee),
                    price_x128: Amount(price),
                });
            }
        }
        if remaining.is_zero() {
            break;
        }
        // TreeMath's helpers exclude the current id, despite their <=/>= docs.
        // Ordered complete membership is equivalent to its three bitmap levels.
        // swap never clears depleted entries; reverse swaps can refill them.
        let successor = if zero {
            next.bins.range(..id).next_back()
        } else {
            next.bins.range((Excluded(id), Unbounded)).next()
        };
        next.active_id = *successor
            .ok_or_else(|| err("INFINITY_BIN_OUT_OF_LIQUIDITY"))?
            .0;
    }
    if output.is_zero() {
        return Err(err("INFINITY_BIN_ZERO_OUTPUT"));
    }
    if output > signed_max {
        return Err(err("INFINITY_BIN_OUTPUT_WIDTH"));
    }
    Ok(MultiBinSwap {
        amount_out: Amount(output),
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        protocol_amount: Amount(protocol_total),
        swap_fee: fee,
        steps,
        bin_fills,
        detached_active: if next.active_id == state.active_id {
            detached
        } else {
            None
        },
        next_state: next,
    })
}
fn ceil(a: BigUint, b: &BigUint) -> BigUint {
    (a + b - BigUint::one()) / b
}
