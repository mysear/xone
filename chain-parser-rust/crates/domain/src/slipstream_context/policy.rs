//! Fixed-version fee and Oracle policy; called only after structural validation.
use super::*;
use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};
fn mask(bits: usize) -> BigUint {
    (BigUint::one() << bits) - 1u8
}
fn add(a: &BigUint, b: &BigUint, bits: usize) -> BigUint {
    (a + b) & mask(bits)
}
fn sub(a: &BigUint, b: &BigUint, bits: usize) -> BigUint {
    ((a + (BigUint::one() << bits)) - b) & mask(bits)
}
pub(crate) fn wrap_signed(value: i128, bits: u32) -> i64 {
    let n = value & ((1i128 << bits) - 1);
    (if n & (1i128 << (bits - 1)) != 0 {
        n - (1i128 << bits)
    } else {
        n
    }) as i64
}
pub fn wrapping_tick_cumulative(value: i128) -> i64 {
    wrap_signed(value, 56)
}
pub fn transform(
    observation: &SlipstreamObservation,
    time: u32,
    tick: i32,
    liquidity: &Amount,
) -> Result<SlipstreamObservation> {
    let dt = time.wrapping_sub(observation.block_timestamp);
    let cumulative = wrap_signed(
        i128::from(signed56(&observation.tick_cumulative)?) + i128::from(tick) * i128::from(dt),
        56,
    );
    let growth = (BigUint::from(dt) << 128usize) / liquidity.0.clone().max(BigUint::one());
    Ok(SlipstreamObservation {
        block_timestamp: time,
        tick_cumulative: cumulative.to_string(),
        seconds_per_liquidity_cumulative_x128: Amount(add(
            &observation.seconds_per_liquidity_cumulative_x128.0,
            &growth,
            160,
        )),
        initialized: true,
    })
}
pub fn observe_single(state: &SlipstreamState, ago: u32) -> Result<(i64, BigUint)> {
    let time = state.timestamp as u32;
    let last = state
        .observations
        .get(state.observation_index as usize)
        .ok_or_else(|| err("SLIPSTREAM_OBSERVATION_SCOPE"))?;
    let age = time.wrapping_sub(last.block_timestamp);
    if ago <= age {
        let value = transform(
            last,
            time.wrapping_sub(ago),
            state.core.tick,
            &state.core.liquidity,
        )?;
        return Ok((
            signed56(&value.tick_cumulative)?,
            value.seconds_per_liquidity_cumulative_x128.0,
        ));
    }
    let window = state
        .observations
        .get(..state.observation_cardinality as usize)
        .ok_or_else(|| err("SLIPSTREAM_OBSERVATION_SCOPE"))?;
    let mut initialized = window.iter().filter(|o| o.initialized).collect::<Vec<_>>();
    initialized.sort_by_key(|o| std::cmp::Reverse(time.wrapping_sub(o.block_timestamp)));
    if initialized.is_empty() || time.wrapping_sub(initialized[0].block_timestamp) < ago {
        return Err(err("SLIPSTREAM_OBSERVATION_TOO_OLD"));
    }
    for value in &initialized {
        if time.wrapping_sub(value.block_timestamp) == ago {
            return Ok((
                signed56(&value.tick_cumulative)?,
                value.seconds_per_liquidity_cumulative_x128.0.clone(),
            ));
        }
    }
    for pair in initialized.windows(2) {
        let older = time.wrapping_sub(pair[0].block_timestamp);
        let newer = time.wrapping_sub(pair[1].block_timestamp);
        if older > ago && ago > newer {
            let span = older - newer;
            let delta = older - ago;
            let a = signed56(&pair[0].tick_cumulative)?;
            let b = signed56(&pair[1].tick_cumulative)?;
            let difference = wrap_signed(i128::from(b) - i128::from(a), 56);
            let cumulative = wrap_signed(
                i128::from(a) + i128::from(difference / i64::from(span)) * i128::from(delta),
                56,
            );
            let growth = sub(
                &pair[1].seconds_per_liquidity_cumulative_x128.0,
                &pair[0].seconds_per_liquidity_cumulative_x128.0,
                160,
            ) * delta
                / span;
            return Ok((
                cumulative,
                add(
                    &pair[0].seconds_per_liquidity_cumulative_x128.0,
                    &growth,
                    160,
                ),
            ));
        }
    }
    Err(err("SLIPSTREAM_OBSERVATION_TOO_OLD"))
}
pub fn swap_fee(state: &SlipstreamState, origin: &str) -> Result<u32> {
    let module = &state.swap_fee_module;
    let base = state.factory_default_fee;
    match module.kind.as_str() {
        "none" => Ok(base),
        "custom" => Ok(if module.custom_fee == 420 {
            0
        } else if module.custom_fee == 0 {
            base
        } else {
            module.custom_fee
        }),
        "dynamic" => {
            let d = module
                .dynamic
                .as_ref()
                .ok_or_else(|| err("SLIPSTREAM_DYNAMIC_STATE_REQUIRED"))?;
            let origin = state
                .origins
                .iter()
                .find(|o| o.origin == origin)
                .ok_or_else(|| err("SLIPSTREAM_ORIGIN_SCOPE_UNAVAILABLE"))?;
            if d.base_fee == 420 {
                return Ok(0);
            }
            let base = if d.base_fee == 0 { base } else { d.base_fee };
            let latest = state
                .observations
                .get(state.observation_index as usize)
                .ok_or_else(|| err("SLIPSTREAM_OBSERVATION_SCOPE"))?;
            if d.initial_fee_enabled && latest.block_timestamp != state.timestamp as u32 {
                return Ok(if d.initial_fee == 0 {
                    base
                } else if d.initial_fee == 420 {
                    0
                } else {
                    d.initial_fee
                });
            }
            if !(2..131070).contains(&d.seconds_ago) {
                return Err(err("SLIPSTREAM_DYNAMIC_FEE_PARAMETERS"));
            }
            let (scale, cap) = if d.scaling_factor == 0 {
                (d.default_scaling_factor, d.default_fee_cap)
            } else {
                (d.scaling_factor, d.fee_cap)
            };
            let mut dynamic = BigUint::zero();
            if state.observation_cardinality >= d.seconds_ago / 2 {
                match (
                    observe_single(state, d.seconds_ago),
                    observe_single(state, 0),
                ) {
                    (Ok((past, _)), Ok((current, _))) => {
                        let difference = wrap_signed(i128::from(current) - i128::from(past), 56);
                        let average =
                            wrap_signed(i128::from(difference / i64::from(d.seconds_ago)), 24);
                        let delta =
                            wrap_signed(i128::from(state.core.tick) - i128::from(average), 24);
                        dynamic =
                            BigUint::from(delta.unsigned_abs()) * BigUint::from(scale) / 1000000u32;
                    }
                    (Err(e), _) | (_, Err(e)) if e.0 == "SLIPSTREAM_OBSERVATION_TOO_OLD" => {}
                    (Err(e), _) | (_, Err(e)) => return Err(e),
                }
            }
            let fee = (BigUint::from(base) + dynamic)
                .min(BigUint::from(cap))
                .to_u64()
                .ok_or_else(|| err("SLIPSTREAM_DYNAMIC_FEE_RANGE"))?;
            if origin.discount > 500000 {
                return Err(err("SLIPSTREAM_ORIGIN_VALUES"));
            }
            let discounted = fee - (fee * u64::from(origin.discount)).div_ceil(1000000);
            u32::try_from(discounted).map_err(|_| err("SLIPSTREAM_DYNAMIC_FEE_RANGE"))
        }
        _ => Err(err("SLIPSTREAM_UNKNOWN_MODULE")),
    }
}
pub fn unstaked_fee(state: &SlipstreamState) -> Result<u32> {
    if !state.voter_gauge_alive || state.voter_gauge == crate::singleton::ZERO_ADDRESS {
        return Ok(0);
    }
    let module = &state.unstaked_fee_module;
    match module.kind.as_str() {
        "none" => Ok(state.factory_default_unstaked_fee),
        "custom" => Ok(if module.custom_fee == 420 {
            0
        } else if module.custom_fee == 0 {
            100000
        } else {
            module.custom_fee
        }),
        _ => Err(err("SLIPSTREAM_UNSTAKED_MODULE_KIND")),
    }
}
pub fn validate_views(state: &SlipstreamState) -> Result<()> {
    let fee = swap_fee(state, &state.view_origin)?;
    let unstaked = unstaked_fee(state)?;
    if fee != state.view_swap_fee || unstaked != state.view_unstaked_fee {
        return Err(err("SLIPSTREAM_CURRENT_FEE_VIEW_MISMATCH"));
    }
    for origin in &state.origins {
        if swap_fee(state, &origin.origin)? != origin.view_swap_fee
            || unstaked != origin.view_unstaked_fee
        {
            return Err(err("SLIPSTREAM_ORIGIN_FEE_VIEW_MISMATCH"));
        }
    }
    Ok(())
}
