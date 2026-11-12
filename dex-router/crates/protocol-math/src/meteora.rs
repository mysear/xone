//! Independent integer settlement of the admitted pinned Meteora swap2 and swap_exact_out2 profiles.
//! It preserves complete pair/array/oracle bytes and fails if account coverage ends.
use crate::Swap;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use xone_domain::{err, meteora::*, Amount, Pool, Result};
const Q: u128 = 1u128 << 64;
const F: u128 = 1_000_000_000;
fn fail() -> xone_domain::Error {
    err("METEORA_ARITHMETIC")
}
fn narrow(v: u128) -> Result<u64> {
    v.try_into().map_err(|_| fail())
}
fn ceil(n: u128, d: u128) -> Result<u128> {
    if d == 0 {
        return Err(fail());
    }
    Ok(n / d + u128::from(!n.is_multiple_of(d)))
}
fn put64(d: &mut [u8], o: usize, n: u64) {
    d[o..o + 8].copy_from_slice(&n.to_le_bytes());
}
fn put32(d: &mut [u8], o: usize, n: u32) {
    d[o..o + 4].copy_from_slice(&n.to_le_bytes());
}
fn put128(d: &mut [u8], o: usize, n: u128) {
    d[o..o + 16].copy_from_slice(&n.to_le_bytes());
}
/// The original pow uses an inverted u128 base, 19 checked squarings and an
/// integer reciprocal. Floating point and generic rational exponentiation differ.
pub use xone_domain::meteora::bin_price as price;

fn rate(d: &[u8]) -> Result<u128> {
    let base = u128::from(u16_at(d, 8))
        .checked_mul(u128::from(u16_at(d, 80)))
        .and_then(|n| n.checked_mul(10))
        .and_then(|n| {
            10u128
                .checked_pow(d[34].into())
                .and_then(|p| n.checked_mul(p))
        })
        .ok_or_else(fail)?;
    let v = u128::from(u32_at(d, 40)) * u128::from(u16_at(d, 80));
    let variable = ceil(
        v.checked_mul(v)
            .and_then(|n| n.checked_mul(u128::from(u32_at(d, 16))))
            .ok_or_else(fail)?,
        100_000_000_000,
    )?;
    Ok(base
        .checked_add(variable)
        .ok_or_else(fail)?
        .min(100_000_000))
}
fn amount_in(out: u64, p: u128, zero: bool) -> Result<u64> {
    let (n, d) = if zero {
        (BigUint::from(out) << 64, BigUint::from(p))
    } else {
        (BigUint::from(out) * BigUint::from(p), BigUint::from(Q))
    };
    if d == BigUint::from(0u8) {
        return Err(fail());
    }
    ((n + &d - BigUint::from(1u8)) / d)
        .to_u64()
        .ok_or_else(fail)
}
fn amount_out(input: u64, p: u128, zero: bool) -> Result<u64> {
    let (n, d) = if zero {
        (BigUint::from(input) * BigUint::from(p), BigUint::from(Q))
    } else {
        (BigUint::from(input) << 64, BigUint::from(p))
    };
    if d == BigUint::from(0u8) {
        return Err(fail());
    }
    (n / d).to_u64().ok_or_else(fail)
}
fn update_oracle(raw: &mut [u8], timestamp: u64, active: i32) -> Result<()> {
    let index = u64_at(raw, 8) as usize;
    let len = u64_at(raw, 24) as usize;
    let offset = 32 + index * 32;
    let updated = i64_at(raw, offset + 24);
    let created = i64_at(raw, offset + 16);
    let elapsed = (timestamp as i64).checked_sub(updated).ok_or_else(fail)?;
    let cumulative = i128::from_le_bytes(raw[offset..offset + 16].try_into().unwrap())
        .checked_add(i128::from(active) * i128::from(elapsed))
        .ok_or_else(fail)?;
    let dst = if timestamp - created as u64 >= 120 {
        let next = (index + 1) % len;
        put64(raw, 8, next as u64);
        put64(raw, 16, u64_at(raw, 16).max((next + 1) as u64));
        put64(raw, 32 + next * 32 + 16, timestamp);
        32 + next * 32
    } else {
        offset
    };
    raw[dst..dst + 16].copy_from_slice(&cumulative.to_le_bytes());
    put64(raw, dst + 24, timestamp);
    Ok(())
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero: bool) -> Result<Swap> {
    quote(pool, input, zero, false)
}
/// Independent output-driven SwapExactOut2 settlement. Rounding can remove more
/// output from a bin than the exact recipient credit; it remains vault dust in
/// this pinned SBF and must not be silently credited to the protocol or user.
pub fn quote_exact_out(pool: &Pool, output: &Amount, zero: bool) -> Result<crate::ExactOutSwap> {
    let q = quote(pool, output, zero, true)?;
    Ok(crate::ExactOutSwap {
        amount_in: q.consumed_in,
        amount_out: q.amount_out,
        next_pool: q.next_pool,
    })
}
fn quote(pool: &Pool, input: &Amount, zero: bool, exact_out: bool) -> Result<Swap> {
    if pool.protocol == xone_domain::meteora_token2022::PROFILE {
        return quote_token2022(pool, input, zero, exact_out);
    }
    if pool.protocol == ORDERS_PROFILE {
        return quote_orders(pool, input, zero, exact_out);
    }
    pool.validate()?;
    let state = pool
        .meteora
        .as_ref()
        .ok_or_else(|| err("METEORA_STATE_REQUIRED"))?;
    let specified = input.0.to_u64().filter(|n| *n > 0).ok_or_else(|| {
        err(if exact_out {
            "METEORA_U64_TARGET"
        } else {
            "METEORA_U64_INPUT"
        })
    })?;
    let mut d = state.pair()?;
    let mut oracle = bytes(&state.oracle_data, state.oracle_data.len() / 2)?;
    let mut arrays = state
        .bin_arrays
        .iter()
        .map(|a| bytes(&a.data, ARRAY_SIZE))
        .collect::<Result<Vec<_>>>()?;
    let fee_on_input = d[36] == 0 || !zero;
    let start = i32_at(&d, 76);
    let mut active = start;
    let mut left = specified;
    let mut output = 0u64;
    let mut total_input = 0u64;
    let elapsed = state
        .timestamp
        .checked_sub(u64_at(&d, 56))
        .ok_or_else(fail)?;
    if elapsed >= u64::from(u16_at(&d, 10)) {
        d[48..52].copy_from_slice(&active.to_le_bytes());
        let reference = if elapsed < u64::from(u16_at(&d, 12)) {
            u32_at(&d, 40)
                .checked_mul(u32::from(u16_at(&d, 14)))
                .ok_or_else(fail)?
                / 10000
        } else {
            0
        };
        put32(&mut d, 44, reference);
    }
    let mut reward_bins = Vec::new();
    let mut steps = 0;
    while left > 0 {
        if steps >= state.max_steps {
            return Err(err("METEORA_STEP_BUDGET"));
        }
        steps += 1;
        if active < i32_at(&d, 24) || active > i32_at(&d, 28) {
            return Err(err("METEORA_LIQUIDITY_EXHAUSTED"));
        }
        let mut array_index = i64::from(active.div_euclid(70));
        let (min_array, max_array) = if state.bitmap_extension.is_some() {
            (-6656, 6655)
        } else {
            (-512, 511)
        };
        if !(min_array..=max_array).contains(&array_index) {
            return Err(err("METEORA_BITMAP_EXTENSION_REQUIRED"));
        }
        if !state.array_initialized(&d, array_index)? {
            let mut next = if zero {
                array_index - 1
            } else {
                array_index + 1
            };
            while (min_array..=max_array).contains(&next) && !state.array_initialized(&d, next)? {
                next += if zero { -1 } else { 1 };
            }
            if !(min_array..=max_array).contains(&next) {
                return Err(err("METEORA_LIQUIDITY_EXHAUSTED"));
            }
            array_index = next;
            active = (array_index * 70 + if zero { 69 } else { 0 }) as i32;
            if active < i32_at(&d, 24) || active > i32_at(&d, 28) {
                return Err(err("METEORA_LIQUIDITY_EXHAUSTED"));
            }
        }
        let Some(i) = state.bin_arrays.iter().position(|a| a.index == array_index) else {
            return Err(err("METEORA_ARRAY_COVERAGE"));
        };
        let o = 56 + active.rem_euclid(70) as usize * BIN_SIZE;
        let max_out = u64_at(&arrays[i], o + if zero { 8 } else { 0 });
        if max_out > 0 {
            if pool.protocol == REWARDS_PROFILE {
                reward_bins.push((i, o));
                if reward_bins.len() > 15 {
                    return Err(err("METEORA_REWARD_BIN_BUDGET"));
                }
            }
            let distance = i64::from(i32_at(&d, 48)).abs_diff(i64::from(active));
            let volatility =
                (u64::from(u32_at(&d, 44)) + distance * 10000).min(u64::from(u32_at(&d, 20)));
            put32(&mut d, 40, volatility as u32);
            let p = u128_at(&arrays[i], o + 16);
            let fee_rate = rate(&d)?;
            let (used, bin_out, delivered, fee) = if !fee_on_input {
                let capacity = amount_in(max_out, p, zero)?;
                if exact_out {
                    let output_fee = narrow(ceil(u128::from(max_out) * fee_rate, F)?)?;
                    let target = left.min(max_out - output_fee);
                    let gross = target
                        .checked_add(narrow(ceil(u128::from(target) * fee_rate, F - fee_rate)?)?)
                        .ok_or_else(fail)?;
                    let used = amount_in(gross, p, zero)?;
                    let physical = amount_out(used, p, zero)?.min(max_out);
                    let fee = narrow(ceil(u128::from(physical) * fee_rate, F)?)?;
                    if physical - fee < target {
                        return Err(err("METEORA_EXACT_OUT_UNDERDELIVERY"));
                    }
                    (used, physical, target, fee)
                } else {
                    let used = left.min(capacity);
                    let physical = amount_out(used, p, zero)?.min(max_out);
                    let fee = narrow(ceil(u128::from(physical) * fee_rate, F)?)?;
                    (used, physical, physical - fee, fee)
                }
            } else if exact_out {
                let target = left.min(max_out);
                let net = amount_in(target, p, zero)?;
                let fee = narrow(ceil(u128::from(net) * fee_rate, F - fee_rate)?)?;
                let used = net.checked_add(fee).ok_or_else(fail)?;
                let physical = amount_out(net, p, zero)?.min(max_out);
                if physical < target {
                    return Err(err("METEORA_EXACT_OUT_UNDERDELIVERY"));
                }
                (used, physical, target, fee)
            } else {
                let capacity = amount_in(max_out, p, zero)?;
                let max_fee = narrow(ceil(u128::from(capacity) * fee_rate, F - fee_rate)?)?;
                let cap = capacity.checked_add(max_fee).ok_or_else(fail)?;
                let (used, out, fee) = if left > cap {
                    (cap, max_out, max_fee)
                } else {
                    let fee = narrow(ceil(u128::from(left) * fee_rate, F)?)?;
                    (
                        left,
                        amount_out(left.checked_sub(fee).ok_or_else(fail)?, p, zero)?.min(max_out),
                        fee,
                    )
                };
                (used, out, out, fee)
            };
            let protocol = narrow(u128::from(fee) * u128::from(u16_at(&d, 32)) / 10000)?;
            let supply = u128_at(&arrays[i], o + 32);
            if supply >> 64 == 0 {
                return Err(fail());
            }
            let growth = ((BigUint::from(fee - protocol) << 64usize) / BigUint::from(supply >> 64))
                .to_u128()
                .ok_or_else(fail)?;
            let fee_token0 = fee_on_input && zero;
            let fee_offset = o + if fee_token0 { 80 } else { 96 };
            let next_growth = u128_at(&arrays[i], fee_offset)
                .checked_add(growth)
                .ok_or_else(fail)?;
            put128(&mut arrays[i], fee_offset, next_growth);
            let input_offset = o + if zero { 0 } else { 8 };
            let output_offset = o + if zero { 8 } else { 0 };
            let x = u64_at(&arrays[i], input_offset)
                .checked_add(used - if fee_on_input { fee } else { 0 })
                .ok_or_else(fail)?;
            let y = u64_at(&arrays[i], output_offset)
                .checked_sub(bin_out)
                .ok_or_else(fail)?;
            put64(&mut arrays[i], input_offset, x);
            put64(&mut arrays[i], output_offset, y);
            let rounding = if exact_out && !fee_on_input {
                bin_out - fee - delivered
            } else {
                0
            };
            let pf = 216 + if fee_token0 { 0 } else { 8 };
            let next_fee = u64_at(&d, pf)
                .checked_add(protocol)
                .and_then(|v| v.checked_add(rounding))
                .ok_or_else(fail)?;
            put64(&mut d, pf, next_fee);
            left -= if exact_out { delivered } else { used };
            total_input = total_input.checked_add(used).ok_or_else(fail)?;
            output = output.checked_add(delivered).ok_or_else(fail)?;
        }
        if left > 0 {
            active = if zero {
                active.checked_sub(1)
            } else {
                active.checked_add(1)
            }
            .ok_or_else(fail)?;
        }
    }
    if output == 0 {
        return Err(err("METEORA_INSUFFICIENT_OUTPUT"));
    }
    if pool.protocol == REWARDS_PROFILE && !reward_bins.is_empty() {
        for reward in 0..2 {
            let r = 264 + reward * 144;
            if d[r..r + 32].iter().all(|b| *b == 0) {
                continue;
            }
            let current = state.timestamp.min(u64_at(&d, r + 104));
            let elapsed = current.saturating_sub(u64_at(&d, r + 128));
            let total = u128_at(&d, r + 112)
                .checked_mul(u128::from(elapsed))
                .ok_or_else(fail)?
                / reward_bins.len() as u128;
            for (i, o) in &reward_bins {
                let supply = u128_at(&arrays[*i], *o + 32) >> 64;
                if supply == 0 {
                    return Err(err("METEORA_BIN_SUPPLY"));
                }
                let offset = *o + 48 + reward * 16;
                let growth = u128_at(&arrays[*i], offset)
                    .checked_add(total / supply)
                    .ok_or_else(fail)?;
                put128(&mut arrays[*i], offset, growth);
            }
            put64(&mut d, r + 128, current);
        }
    }
    d[76..80].copy_from_slice(&active.to_le_bytes());
    if active != start {
        put64(&mut d, 56, state.timestamp);
    }
    update_oracle(&mut oracle, state.timestamp, start)?;
    let mut next = pool.clone();
    let n = next.meteora.as_mut().unwrap();
    n.derived = true;
    n.pair_data = hex::encode(d);
    n.oracle_data = hex::encode(oracle);
    for (a, raw) in n.bin_arrays.iter_mut().zip(arrays) {
        a.data = hex::encode(raw);
    }
    if zero {
        n.raw_vault0 = n.raw_vault0.checked_add(total_input).ok_or_else(fail)?;
        n.raw_vault1 = n.raw_vault1.checked_sub(output).ok_or_else(fail)?;
    } else {
        n.raw_vault1 = n.raw_vault1.checked_add(total_input).ok_or_else(fail)?;
        n.raw_vault0 = n.raw_vault0.checked_sub(output).ok_or_else(fail)?;
    }
    next.reserve0 = Amount((n.raw_vault0).into());
    next.reserve1 = Amount((n.raw_vault1).into());
    next.validate()?;
    Ok(Swap {
        amount_out: Amount((output).into()),
        consumed_in: Amount(total_input.into()),
        remaining_in: Amount((0u64).into()),
        next_pool: next,
    })
}

/// Three distinct integer liquidity layers; order balances and fees never become LP reserves.
fn quote_orders(pool: &Pool, input: &Amount, zero: bool, exact_out: bool) -> Result<Swap> {
    pool.validate()?;
    let specified = input
        .0
        .to_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| err("METEORA_U64_INPUT"))?;
    let state = pool
        .meteora
        .as_ref()
        .ok_or_else(|| err("METEORA_STATE_REQUIRED"))?;
    let mut d = state.pair()?;
    let mut oracle = bytes(&state.oracle_data, state.oracle_data.len() / 2)?;
    let mut arrays = state
        .bin_arrays
        .iter()
        .map(|a| bytes(&a.data, ARRAY_SIZE))
        .collect::<Result<Vec<_>>>()?;
    let start = i32_at(&d, 76);
    let mut active = start;
    let (mut left, mut output, mut consumed) = (specified, 0u64, 0u64);
    let elapsed = state
        .timestamp
        .checked_sub(u64_at(&d, 56))
        .ok_or_else(fail)?;
    if elapsed >= u64::from(u16_at(&d, 10)) {
        d[48..52].copy_from_slice(&active.to_le_bytes());
        let r = if elapsed < u64::from(u16_at(&d, 12)) {
            u32_at(&d, 40)
                .checked_mul(u32::from(u16_at(&d, 14)))
                .ok_or_else(fail)?
                / 10000
        } else {
            0
        };
        put32(&mut d, 44, r);
    }
    let mut steps = 0;
    while left > 0 {
        if steps >= state.max_steps {
            return Err(err("METEORA_STEP_BUDGET"));
        }
        steps += 1;
        if active < i32_at(&d, 24) || active > i32_at(&d, 28) {
            return Err(err("METEORA_LIQUIDITY_EXHAUSTED"));
        }
        let mut index = i64::from(active.div_euclid(70));
        if !(-512..=511).contains(&index) {
            return Err(err("METEORA_BITMAP_EXTENSION_REQUIRED"));
        }
        if !state.array_initialized(&d, index)? {
            let mut next = if zero { index - 1 } else { index + 1 };
            while (-512..=511).contains(&next) && !state.array_initialized(&d, next)? {
                next += if zero { -1 } else { 1 };
            }
            if !(-512..=511).contains(&next) {
                return Err(err("METEORA_LIQUIDITY_EXHAUSTED"));
            }
            index = next;
            active = (index * 70 + if zero { 69 } else { 0 }) as i32;
        }
        let i = state
            .bin_arrays
            .iter()
            .position(|a| a.index == index)
            .ok_or_else(|| err("METEORA_ARRAY_COVERAGE"))?;
        let o = 56 + active.rem_euclid(70) as usize * BIN_SIZE;
        let mm = u64_at(&arrays[i], o + if zero { 8 } else { 0 });
        let matches = arrays[i][o + 140] == u8::from(!zero);
        let processed = if matches {
            u64_at(&arrays[i], o + 128)
        } else {
            0
        };
        let open = if matches {
            u64_at(&arrays[i], o + 112)
        } else {
            0
        };
        if mm > 0 || processed > 0 || open > 0 {
            let distance = i64::from(i32_at(&d, 48)).abs_diff(i64::from(active));
            let volatility = ((u64::from(u32_at(&d, 44)) + distance * 10000)
                .min(u64::from(u32_at(&d, 20)))) as u32;
            put32(&mut d, 40, volatility);
            let p = u128_at(&arrays[i], o + 16);
            let r = rate(&d)?;
            // The pinned deployed SBF converts the bin's aggregate output once,
            // then fills MM / processed / open layers with that net input.
            // Independently rounding every layer up (as the SDK quote does)
            // changes execution at layer boundaries and can hide rejection.
            let target = if exact_out {
                left.min(
                    mm.checked_add(processed)
                        .and_then(|v| v.checked_add(open))
                        .ok_or_else(fail)?,
                )
            } else {
                0
            };
            let gross = if exact_out {
                let capacity = mm
                    .checked_add(processed)
                    .and_then(|v| v.checked_add(open))
                    .ok_or_else(fail)?;
                let net = if target == capacity {
                    // A drained bin follows the full ExactIn fill branch.
                    [mm, processed, open].into_iter().try_fold(0u64, |n, out| {
                        n.checked_add(amount_in(out, p, zero)?).ok_or_else(fail)
                    })?
                } else {
                    amount_in(target, p, zero)?
                };
                net.checked_add(narrow(ceil(u128::from(net) * r, F - r)?)?)
                    .ok_or_else(fail)?
            } else {
                left
            };
            let estimated_fee = narrow(ceil(u128::from(gross) * r, F)?)?;
            let available = gross.checked_sub(estimated_fee).ok_or_else(fail)?;
            let mut budget = available;
            let mut fills = [(0u64, 0u64); 3];
            for (layer, capacity) in [mm, processed, open].into_iter().enumerate() {
                if budget == 0 {
                    break;
                }
                let maximum = amount_in(capacity, p, zero)?;
                let used = budget.min(maximum);
                let out = amount_out(used, p, zero)?.min(capacity);
                fills[layer] = (used, out);
                budget -= used;
            }
            let net = fills
                .iter()
                .try_fold(0u64, |n, x| n.checked_add(x.0).ok_or_else(fail))?;
            let delivered = fills
                .iter()
                .try_fold(0u64, |n, x| n.checked_add(x.1).ok_or_else(fail))?;
            let fee = if budget > 0 {
                narrow(ceil(u128::from(net) * r, F - r)?)?
            } else {
                estimated_fee
            };
            let used = if budget > 0 {
                net.checked_add(fee).ok_or_else(fail)?
            } else {
                gross
            };
            if exact_out && delivered < target {
                return Err(err("METEORA_EXACT_OUT_UNDERDELIVERY"));
            }
            if net == 0 {
                return Err(err("METEORA_INSUFFICIENT_OUTPUT"));
            }
            let mm_fee = narrow(ceil(
                u128::from(fee) * u128::from(fills[0].0),
                u128::from(net),
            )?)?;
            let mm_protocol = narrow(u128::from(mm_fee) * u128::from(u16_at(&d, 32)) / 10000)?;
            let lp_fee = mm_fee - mm_protocol;
            let order_fee = (fee - mm_fee) / 2;
            let protocol = fee - lp_fee - order_fee;
            if fills[0].0 > 0 {
                let supply = u128_at(&arrays[i], o + 32) >> 64;
                if supply == 0 {
                    return Err(err("METEORA_BIN_SUPPLY"));
                }
                let offset = o + if zero { 80 } else { 96 };
                let growth = u128_at(&arrays[i], offset)
                    .checked_add((u128::from(lp_fee) << 64) / supply)
                    .ok_or_else(fail)?;
                put128(&mut arrays[i], offset, growth);
            }
            let in_o = o + if zero { 0 } else { 8 };
            let out_o = o + if zero { 8 } else { 0 };
            let new_in = u64_at(&arrays[i], in_o)
                .checked_add(fills[0].0)
                .ok_or_else(fail)?;
            let new_out = u64_at(&arrays[i], out_o)
                .checked_sub(fills[0].1)
                .ok_or_else(fail)?;
            put64(&mut arrays[i], in_o, new_in);
            put64(&mut arrays[i], out_o, new_out);
            let filled = fills[1].1.checked_add(fills[2].1).ok_or_else(fail)?;
            if filled > 0 {
                let fulfilled_o = o + if zero { 56 } else { 48 };
                let sum = u64_at(&arrays[i], fulfilled_o)
                    .checked_add(filled)
                    .ok_or_else(fail)?;
                put64(&mut arrays[i], fulfilled_o, sum);
                if fills[2].1 > 0 {
                    put64(&mut arrays[i], o + 112, 0);
                    put64(&mut arrays[i], o + 120, open);
                    put64(&mut arrays[i], o + 128, open - fills[2].1);
                    let age = u32_at(&arrays[i], o + 136)
                        .checked_add(1)
                        .ok_or_else(fail)?;
                    put32(&mut arrays[i], o + 136, age);
                } else {
                    put64(&mut arrays[i], o + 128, processed - fills[1].1);
                }
                let fee_o = o + if zero { 72 } else { 64 };
                let new_fee = u64_at(&arrays[i], fee_o)
                    .checked_add(order_fee)
                    .ok_or_else(fail)?;
                put64(&mut arrays[i], fee_o, new_fee);
            }
            let pf = 216 + if zero { 0 } else { 8 };
            let new_fee = u64_at(&d, pf).checked_add(protocol).ok_or_else(fail)?;
            put64(&mut d, pf, new_fee);
            let credited = if exact_out { target } else { delivered };
            left = left
                .checked_sub(if exact_out { credited } else { used })
                .ok_or_else(fail)?;
            consumed = consumed.checked_add(used).ok_or_else(fail)?;
            output = output.checked_add(credited).ok_or_else(fail)?;
        }
        if left > 0 {
            active = if zero {
                active.checked_sub(1)
            } else {
                active.checked_add(1)
            }
            .ok_or_else(fail)?;
        }
    }
    if output == 0 {
        return Err(err("METEORA_INSUFFICIENT_OUTPUT"));
    }
    d[76..80].copy_from_slice(&active.to_le_bytes());
    if active != start {
        put64(&mut d, 56, state.timestamp);
    }
    update_oracle(&mut oracle, state.timestamp, start)?;
    let mut next = pool.clone();
    let n = next.meteora.as_mut().unwrap();
    n.derived = true;
    n.pair_data = hex::encode(d);
    n.oracle_data = hex::encode(oracle);
    for (a, raw) in n.bin_arrays.iter_mut().zip(arrays) {
        a.data = hex::encode(raw);
    }
    if zero {
        n.raw_vault0 = n.raw_vault0.checked_add(consumed).ok_or_else(fail)?;
        n.raw_vault1 = n.raw_vault1.checked_sub(output).ok_or_else(fail)?;
    } else {
        n.raw_vault1 = n.raw_vault1.checked_add(consumed).ok_or_else(fail)?;
        n.raw_vault0 = n.raw_vault0.checked_sub(output).ok_or_else(fail)?;
    }
    next.reserve0 = Amount(n.raw_vault0.into());
    next.reserve1 = Amount(n.raw_vault1.into());
    next.validate()?;
    Ok(Swap {
        amount_out: Amount(output.into()),
        consumed_in: Amount(consumed.into()),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}

fn quote_token2022(pool: &Pool, input: &Amount, zero: bool, exact_out: bool) -> Result<Swap> {
    pool.validate()?;
    let t = pool
        .meteora
        .as_ref()
        .and_then(|m| m.token2022.as_ref())
        .ok_or_else(|| err("METEORA_TOKEN2022_STATE"))?;
    let specified = input
        .0
        .to_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| err("METEORA_U64_INPUT"))?;
    let inner_amount = if exact_out {
        if zero {
            specified
        } else {
            t.gross(specified)?
        }
    } else if zero {
        specified
            .checked_sub(t.fee(specified)?)
            .filter(|n| *n > 0)
            .ok_or_else(|| err("METEORA_TOKEN2022_INSUFFICIENT_INPUT"))?
    } else {
        specified
    };
    let mut base = pool.clone();
    base.protocol = PROFILE.into();
    base.adapter_version = ADAPTER.into();
    let state = base.meteora.as_mut().unwrap();
    state.token2022 = None;
    let mut pair = state.pair()?;
    pair[880] = 0;
    state.pair_data = hex::encode(pair);
    let mut q = quote(&base, &Amount(inner_amount.into()), zero, exact_out)?;
    let inner_in = q.consumed_in.0.to_u64().ok_or_else(fail)?;
    let inner_out = q.amount_out.0.to_u64().ok_or_else(fail)?;
    let gross_input = if zero {
        if exact_out {
            t.gross(inner_in)?
        } else {
            specified
        }
    } else {
        inner_in
    };
    let received = if zero {
        inner_out
    } else {
        inner_out.checked_sub(t.fee(inner_out)?).ok_or_else(fail)?
    };
    if received == 0 || (exact_out && received != specified) {
        return Err(err("METEORA_TOKEN2022_RECIPIENT_CREDIT"));
    }
    let mut token = t.clone();
    let mut raw = bytes(&token.vault_data, 178)?;
    let next = q.next_pool.meteora.as_mut().unwrap();
    put64(&mut raw, 64, next.raw_vault0);
    let withheld = u64_at(&raw, 170)
        .checked_add(if zero { t.fee(gross_input)? } else { 0 })
        .ok_or_else(fail)?;
    put64(&mut raw, 170, withheld);
    token.vault_data = hex::encode(raw);
    next.token2022 = Some(token);
    let mut d = next.pair()?;
    d[880] = 1;
    next.pair_data = hex::encode(d);
    q.next_pool.protocol = pool.protocol.clone();
    q.next_pool.adapter_version = pool.adapter_version.clone();
    q.next_pool.validate()?;
    q.consumed_in = Amount(gross_input.into());
    q.amount_out = Amount(received.into());
    Ok(q)
}
