//! Original fixed Pump legacy classic/SOL instructions, no implicit SOL wrapping.
use crate::Swap;
use num_traits::ToPrimitive;
use xone_domain::{err, meteora::u64_at, pump::*, Amount, Pool, Result};
fn fail() -> xone_domain::Error {
    err("PUMP_ARITHMETIC")
}
fn fee(n: u64, bps: u64) -> Result<u64> {
    u64::try_from((u128::from(n) * u128::from(bps)).div_ceil(10000)).map_err(|_| fail())
}
fn narrow(n: u128) -> Result<u64> {
    n.try_into().map_err(|_| fail())
}
fn set64(raw: &mut [u8], o: usize, value: u64) {
    raw[o..o + 8].copy_from_slice(&value.to_le_bytes());
}
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero: bool) -> Result<Swap> {
    pool.validate()?;
    let source = pool
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_STATE_REQUIRED"))?;
    let d = source.data(&pool.pool_id, 166)?;
    if d[48] != 0 {
        return Err(err("PUMP_CURVE_COMPLETE"));
    }
    let amount = input
        .0
        .to_u64()
        .filter(|a| *a > 0)
        .ok_or_else(|| err("PUMP_U64_INPUT"))?;
    let vt = u64_at(&d, 8);
    let vq = u64_at(&d, 16);
    let rt = u64_at(&d, 24);
    let (protocol, creator) = source.fee_rates(pool)?;
    let (net, output, protocol_fee, creator_fee) = if zero {
        let gross =
            narrow(u128::from(amount) * u128::from(vq) / (u128::from(vt) + u128::from(amount)))?;
        let pf = fee(gross, protocol)?;
        let cf = fee(gross, creator)?;
        (
            gross,
            gross
                .checked_sub(pf)
                .and_then(|n| n.checked_sub(cf))
                .ok_or_else(fail)?,
            pf,
            cf,
        )
    } else {
        // spendable_sol_in quotes the original inclusive hard input. The program
        // chooses its token output from the fee-exclusive integer budget.
        let estimate = narrow(
            u128::from(amount) * 10000 / (10000 + u128::from(protocol) + u128::from(creator)),
        )?;
        let pf = fee(estimate, protocol)?;
        let cf = fee(estimate, creator)?;
        let net = amount
            .checked_sub(pf)
            .and_then(|n| n.checked_sub(cf))
            .ok_or_else(fail)?;
        let priced = net.checked_sub(1).ok_or_else(fail)?;
        let tokens =
            narrow(u128::from(priced) * u128::from(vt) / (u128::from(vq) + u128::from(priced)))?;
        if tokens >= rt {
            return Err(err("PUMP_PARTIAL_OR_GRADUATION_REQUIRED"));
        }
        (net, tokens, pf, cf)
    };
    if output == 0 {
        return Err(err("PUMP_INSUFFICIENT_OUTPUT"));
    }
    settle(pool, amount, output, net, protocol_fee, creator_fee, zero)
}

/// The original `buy` instruction requests an exact token amount and bounds SOL
/// including both separately rounded fees. It is distinct from buy_exact_sol_in.
pub fn quote_exact_out(pool: &Pool, output: &Amount, zero: bool) -> Result<crate::ExactOutSwap> {
    pool.validate()?;
    if zero {
        return quote_sell_target(pool, output);
    }
    let source = pool
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_STATE_REQUIRED"))?;
    let d = source.data(&pool.pool_id, 166)?;
    if d[48] != 0 {
        return Err(err("PUMP_CURVE_COMPLETE"));
    }
    let target = output
        .0
        .to_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| err("PUMP_U64_OUTPUT"))?;
    let vt = u64_at(&d, 8);
    if target > u64_at(&d, 24) || target >= vt {
        return Err(err("PUMP_EXACT_OUT_LIQUIDITY"));
    }
    // Original buy uses floor + 1, including exactly divisible products.
    let net =
        narrow(u128::from(target) * u128::from(u64_at(&d, 16)) / u128::from(vt - target) + 1)?;
    let (protocol, creator) = source.fee_rates(pool)?;
    let pf = fee(net, protocol)?;
    let cf = fee(net, creator)?;
    let input = net
        .checked_add(pf)
        .and_then(|n| n.checked_add(cf))
        .ok_or_else(fail)?;
    let q = settle(pool, input, target, net, pf, cf, false)?;
    Ok(crate::ExactOutSwap {
        amount_in: q.consumed_in,
        amount_out: q.amount_out,
        next_pool: q.next_pool,
    })
}

fn settle(
    pool: &Pool,
    amount: u64,
    output: u64,
    net: u64,
    protocol_fee: u64,
    creator_fee: u64,
    zero: bool,
) -> Result<Swap> {
    let source = pool
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_STATE_REQUIRED"))?;
    let mut d = source.data(&pool.pool_id, 166)?;
    let (vt, vq, rt, rq) = (
        u64_at(&d, 8),
        u64_at(&d, 16),
        u64_at(&d, 24),
        u64_at(&d, 32),
    );
    if zero {
        set64(&mut d, 8, vt.checked_add(amount).ok_or_else(fail)?);
        set64(&mut d, 16, vq.checked_sub(net).ok_or_else(fail)?);
        set64(&mut d, 24, rt.checked_add(amount).ok_or_else(fail)?);
        set64(&mut d, 32, rq.checked_sub(net).ok_or_else(fail)?);
    } else {
        set64(&mut d, 8, vt.checked_sub(output).ok_or_else(fail)?);
        set64(&mut d, 16, vq.checked_add(net).ok_or_else(fail)?);
        set64(&mut d, 24, rt.checked_sub(output).ok_or_else(fail)?);
        set64(&mut d, 32, rq.checked_add(net).ok_or_else(fail)?);
    }
    d[48] = u8::from(u64_at(&d, 24) == 0);
    let mut next = pool.clone();
    let n = next.pump.as_mut().unwrap();
    n.derived = true;
    let curve = n.account_mut(&pool.pool_id)?;
    curve.data = hex::encode(d);
    curve.lamports = if zero {
        curve.lamports.checked_sub(net)
    } else {
        curve.lamports.checked_add(net)
    }
    .ok_or_else(fail)?;
    let vault_id = n.vault.clone();
    let vault = n.account_mut(&vault_id)?;
    let mut raw = hex::decode(&vault.data).map_err(|_| fail())?;
    let balance = u64_at(&raw, 64);
    set64(
        &mut raw,
        64,
        if zero {
            balance.checked_add(amount)
        } else {
            balance.checked_sub(output)
        }
        .ok_or_else(fail)?,
    );
    vault.data = hex::encode(raw);
    let global = n.data(&PumpState::global_id()?, 1088)?;
    let buyback = narrow(u128::from(protocol_fee) * u128::from(u64_at(&global, 997)) / 10000)?;
    for (address, delta) in [
        (n.creator_vault.clone(), creator_fee),
        (n.fee_recipient.clone(), protocol_fee - buyback),
        (n.buyback_recipient.clone(), buyback),
    ] {
        let a = n.account_mut(&address)?;
        a.lamports = a.lamports.checked_add(delta).ok_or_else(fail)?;
    }
    let raw = n.data(&pool.pool_id, 166)?;
    next.reserve0 = Amount(u64_at(&raw, 24).into());
    next.reserve1 = Amount(u64_at(&raw, 32).into());
    next.validate()?;
    Ok(Swap {
        amount_out: Amount(output.into()),
        consumed_in: Amount(amount.into()),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}

/// Independent inversion of the two ceil-fee functions. Each gross residue has
/// a linear net return over periods of 10,000; enumerating them proves the
/// least feasible gross target without assuming fees make net monotone.
fn quote_sell_target(pool: &Pool, target: &Amount) -> Result<crate::ExactOutSwap> {
    let source = pool
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_STATE_REQUIRED"))?;
    let d = source.data(&pool.pool_id, 166)?;
    if d[48] != 0 {
        return Err(err("PUMP_CURVE_COMPLETE"));
    }
    let target = target
        .0
        .to_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| err("PUMP_U64_OUTPUT"))?;
    let vt = u64_at(&d, 8);
    let vq = u64_at(&d, 16);
    let real = u64_at(&d, 32);
    let (protocol, creator) = source.fee_rates(pool)?;
    let period = 10000u128;
    let slope = period - u128::from(protocol) - u128::from(creator);
    let mut lower = 1u64;
    for _ in 0..32 {
        let mut gross = None;
        for residue in 0..10000u64 {
            let charges = u128::from(fee(residue, protocol)?) + u128::from(fee(residue, creator)?);
            let needed = (u128::from(target) + charges).saturating_sub(u128::from(residue));
            let periods = needed
                .div_ceil(slope)
                .max(u128::from(lower.saturating_sub(residue)).div_ceil(period));
            let candidate = periods * period + u128::from(residue);
            if candidate >= u128::from(lower)
                && candidate <= u128::from(real)
                && candidate < u128::from(vq)
                && gross.is_none_or(|g| candidate < g)
            {
                gross = Some(candidate);
            }
        }
        let gross = gross.ok_or_else(|| err("PUMP_EXACT_OUT_LIQUIDITY"))?;
        let input = narrow((gross * u128::from(vt)).div_ceil(u128::from(vq) - gross))?;
        let actual =
            narrow(u128::from(input) * u128::from(vq) / (u128::from(vt) + u128::from(input)))?;
        let pf = fee(actual, protocol)?;
        let cf = fee(actual, creator)?;
        let net = actual
            .checked_sub(pf)
            .and_then(|n| n.checked_sub(cf))
            .ok_or_else(fail)?;
        if net >= target {
            let q = settle(pool, input, net, actual, pf, cf, true)?;
            return Ok(crate::ExactOutSwap {
                amount_in: q.consumed_in,
                amount_out: q.amount_out,
                next_pool: q.next_pool,
            });
        }
        lower = actual.checked_add(1).ok_or_else(fail)?;
    }
    Err(err("PUMP_EXACT_OUT_INVERSION_BUDGET"))
}
