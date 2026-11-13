//! Raydium CP-Swap f4b9b263 creator fee layout, standard SPL Token ExactIn/ExactOut.
//! Raw SPL balances and accrued fees, rather than the curve's temporary balances,
//! define the next trade's net reserves. Token2022 and SBF execution are excluded.
use crate::Swap;
use xone_domain::{err, Amount, Pool, Result};
const M: u128 = 1_000_000;
fn fail() -> xone_domain::Error {
    err("RAYDIUM_ARITHMETIC")
}
fn u64_value(a: &Amount) -> Result<u64> {
    u64::try_from(&a.0).map_err(|_| err("RAYDIUM_U64_AMOUNT"))
}
fn floor(amount: u128, rate: u64) -> Result<u128> {
    amount
        .checked_mul(rate.into())
        .and_then(|n| n.checked_div(M))
        .ok_or_else(fail)
}
fn ceil(amount: u128, rate: u64) -> Result<u128> {
    amount
        .checked_mul(rate.into())
        .and_then(|n| n.checked_add(M))
        .and_then(|n| n.checked_sub(1))
        .and_then(|n| n.checked_div(M))
        .ok_or_else(fail)
}
fn narrow(n: u128) -> Result<u64> {
    n.try_into().map_err(|_| err("RAYDIUM_U64_OVERFLOW"))
}
fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or_else(|| err("RAYDIUM_U64_OVERFLOW"))
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero: bool) -> Result<Swap> {
    pool.validate()?;
    if pool.protocol != "raydium-cpmm-creator" {
        return Err(err("PROTOCOL_UNSUPPORTED"));
    }
    let s = pool
        .raydium_cpmm
        .as_ref()
        .ok_or_else(|| err("RAYDIUM_STATE_REQUIRED"))?;
    let dx = u64_value(input)?;
    if dx == 0 {
        return Err(err("ZERO_INPUT"));
    }
    let (r0, r1) = s.net_reserves()?;
    let (rin, rout) = if zero { (r0, r1) } else { (r1, r0) };
    if rin == 0 || rout == 0 {
        return Err(err("INSUFFICIENT_LIQUIDITY"));
    }
    let on_input = match s.creator_fee_on {
        0 => true,
        1 => zero,
        2 => !zero,
        _ => return Err(err("RAYDIUM_CREATOR_MODE")),
    };
    let creator_rate = if s.creator_fee_enabled {
        s.creator_fee_rate
    } else {
        0
    };
    let (net_input, trade_fee, mut creator_fee) = if on_input {
        let sum = pool
            .fee_numerator
            .checked_add(creator_rate)
            .ok_or_else(fail)?;
        let total = ceil(dx.into(), sum)?;
        // The pinned original splits even when creator fees are disabled; a zero
        // combined rate makes the original checked division return None.
        let creator = total
            .checked_mul(creator_rate.into())
            .and_then(|n| n.checked_div(sum.into()))
            .ok_or_else(fail)?;
        (
            u128::from(dx).checked_sub(total).ok_or_else(fail)?,
            total.checked_sub(creator).ok_or_else(fail)?,
            creator,
        )
    } else {
        let trade = ceil(dx.into(), pool.fee_numerator)?;
        (
            u128::from(dx).checked_sub(trade).ok_or_else(fail)?,
            trade,
            0,
        )
    };
    let curve_output = net_input
        .checked_mul(rout.into())
        .and_then(|n| n.checked_div(u128::from(rin) + net_input))
        .ok_or_else(fail)?;
    if !on_input {
        creator_fee = ceil(curve_output, creator_rate)?;
    }
    let out = narrow(
        curve_output
            .checked_sub(if on_input { 0 } else { creator_fee })
            .ok_or_else(fail)?,
    )?;
    if out == 0 {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    settle(
        pool,
        zero,
        Settlement {
            dx,
            out,
            net_input,
            curve_output,
            trade_fee,
            creator_fee,
            on_input,
        },
    )
}
struct Settlement {
    dx: u64,
    out: u64,
    net_input: u128,
    curve_output: u128,
    trade_fee: u128,
    creator_fee: u128,
    on_input: bool,
}
fn settle(pool: &Pool, zero: bool, values: Settlement) -> Result<Swap> {
    let Settlement {
        dx,
        out,
        net_input,
        curve_output,
        trade_fee,
        creator_fee,
        on_input,
    } = values;
    let s = pool
        .raydium_cpmm
        .as_ref()
        .ok_or_else(|| err("RAYDIUM_STATE_REQUIRED"))?;
    let (r0, r1) = s.net_reserves()?;
    let (rin, rout) = if zero { (r0, r1) } else { (r1, r0) };
    let protocol = narrow(floor(trade_fee, s.protocol_fee_rate)?)?;
    let fund = narrow(floor(trade_fee, s.fund_fee_rate)?)?;
    let creator = narrow(creator_fee)?;
    // Reproduce the program's pre-fee constant check as well as the token/ledger
    // transitions. The source curve temporary input excludes the LP fee too.
    let before = u128::from(rin) * u128::from(rout);
    let after = (u128::from(rin) + net_input)
        .checked_mul(
            u128::from(rout)
                .checked_sub(curve_output)
                .ok_or_else(fail)?,
        )
        .ok_or_else(fail)?;
    if after < before {
        return Err(err("RAYDIUM_INVARIANT"));
    }
    let mut next = pool.clone();
    let n = next
        .raydium_cpmm
        .as_mut()
        .ok_or_else(|| err("RAYDIUM_STATE_REQUIRED"))?;
    if zero {
        n.protocol_fees0 = add(n.protocol_fees0, protocol)?;
        n.fund_fees0 = add(n.fund_fees0, fund)?;
        if on_input {
            n.creator_fees0 = add(n.creator_fees0, creator)?
        } else {
            n.creator_fees1 = add(n.creator_fees1, creator)?
        }
        n.raw_vault0 = add(n.raw_vault0, dx)?;
        n.raw_vault1 = n.raw_vault1.checked_sub(out).ok_or_else(fail)?;
    } else {
        n.protocol_fees1 = add(n.protocol_fees1, protocol)?;
        n.fund_fees1 = add(n.fund_fees1, fund)?;
        if on_input {
            n.creator_fees1 = add(n.creator_fees1, creator)?
        } else {
            n.creator_fees0 = add(n.creator_fees0, creator)?
        }
        n.raw_vault1 = add(n.raw_vault1, dx)?;
        n.raw_vault0 = n.raw_vault0.checked_sub(out).ok_or_else(fail)?;
    }
    n.recent_epoch = n.bank_epoch;
    let (a, b) = n.net_reserves()?;
    next.reserve0 = Amount::checked(a.into())?;
    next.reserve1 = Amount::checked(b.into())?;
    next.validate()?;
    Ok(Swap {
        amount_out: Amount::checked(out.into())?,
        consumed_in: Amount::checked(dx.into())?,
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}

// Output-driven inverse of the pinned swap_base_output. The creator fee can
// apply on either side; its integer split differs from reusing an ExactIn quote.
pub fn quote_exact_out(pool: &Pool, output: &Amount, zero: bool) -> Result<crate::ExactOutSwap> {
    pool.validate()?;
    if pool.protocol != "raydium-cpmm-creator" {
        return Err(err("PROTOCOL_UNSUPPORTED"));
    }
    let out = u64_value(output)?;
    if out == 0 {
        return Err(err("EXACT_OUT_ZERO_OUTPUT"));
    }
    let s = pool
        .raydium_cpmm
        .as_ref()
        .ok_or_else(|| err("RAYDIUM_STATE_REQUIRED"))?;
    let (r0, r1) = s.net_reserves()?;
    let (rin, rout) = if zero { (r0, r1) } else { (r1, r0) };
    if rin == 0 || rout == 0 {
        return Err(err("INSUFFICIENT_LIQUIDITY"));
    }
    let on_input = match s.creator_fee_on {
        0 => true,
        1 => zero,
        2 => !zero,
        _ => return Err(err("RAYDIUM_CREATOR_MODE")),
    };
    let creator_rate = if s.creator_fee_enabled {
        s.creator_fee_rate
    } else {
        0
    };
    let pre_fee = |net: u128, rate: u64| -> Result<u128> {
        let denominator = M
            .checked_sub(rate.into())
            .filter(|d| *d > 0)
            .ok_or_else(fail)?;
        net.checked_mul(M)
            .and_then(|n| n.checked_add(denominator - 1))
            .and_then(|n| n.checked_div(denominator))
            .ok_or_else(fail)
    };
    let curve_output = if on_input {
        u128::from(out)
    } else {
        pre_fee(out.into(), creator_rate)?
    };
    let denominator = u128::from(rout)
        .checked_sub(curve_output)
        .filter(|d| *d > 0)
        .ok_or_else(|| err("EXACT_OUT_LIQUIDITY_BOUND"))?;
    let net_input = u128::from(rin)
        .checked_mul(curve_output)
        .and_then(|n| n.checked_add(denominator - 1))
        .and_then(|n| n.checked_div(denominator))
        .ok_or_else(fail)?;
    let (gross, trade_fee, creator_fee) = if on_input {
        let rate = pool
            .fee_numerator
            .checked_add(creator_rate)
            .ok_or_else(fail)?;
        let gross = pre_fee(net_input, rate)?;
        let total = gross.checked_sub(net_input).ok_or_else(fail)?;
        let creator = total
            .checked_mul(creator_rate.into())
            .and_then(|n| n.checked_div(rate.into()))
            .ok_or_else(fail)?;
        (gross, total.checked_sub(creator).ok_or_else(fail)?, creator)
    } else {
        let gross = pre_fee(net_input, pool.fee_numerator)?;
        (
            gross,
            gross.checked_sub(net_input).ok_or_else(fail)?,
            curve_output.checked_sub(out.into()).ok_or_else(fail)?,
        )
    };
    let q = settle(
        pool,
        zero,
        Settlement {
            dx: narrow(gross)?,
            out,
            net_input,
            curve_output,
            trade_fee,
            creator_fee,
            on_input,
        },
    )?;
    Ok(crate::ExactOutSwap {
        amount_in: q.consumed_in,
        amount_out: q.amount_out,
        next_pool: q.next_pool,
    })
}
