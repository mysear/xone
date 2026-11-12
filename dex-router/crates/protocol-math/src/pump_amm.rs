//! Output-driven `buy`, fixed input `sell` and `buy_exact_quote_in` on PumpSwap v1.
use crate::{ExactOutSwap, Swap};
use num_traits::ToPrimitive;
use xone_domain::{
    err,
    meteora::{key, u64_at},
    pump_amm::*,
    Amount, Pool, Result,
};
fn fail() -> xone_domain::Error {
    err("PUMP_AMM_ARITHMETIC")
}
fn narrow(n: u128) -> Result<u64> {
    n.try_into().map_err(|_| fail())
}
fn fee(n: u64, bps: u64) -> Result<u64> {
    narrow((u128::from(n) * u128::from(bps)).div_ceil(10000))
}
fn amount(a: &Amount) -> Result<u64> {
    a.0.to_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| err("PUMP_AMM_U64_AMOUNT"))
}
pub fn quote_exact_in(p: &Pool, input: &Amount, zero: bool) -> Result<Swap> {
    p.validate()?;
    let s = p
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_AMM_STATE_REQUIRED"))?;
    let (lp, protocol, creator) = fee_rates(p, s)?;
    let base = amount(&p.reserve0)?;
    let quote = amount(&p.reserve1)?
        .checked_add(virtual_quote(s, p)?)
        .ok_or_else(fail)?;
    let input = amount(input)?;
    if zero
        && input
            > u64_at(&s.data(&p.token0, 82)?, 36)
                .checked_sub(base)
                .ok_or_else(fail)?
    {
        return Err(err("PUMP_AMM_BASE_SUPPLY_BOUND"));
    }
    let (output, net, lf, pf, cf) = if zero {
        let net =
            narrow(u128::from(input) * u128::from(quote) / (u128::from(base) + u128::from(input)))?;
        let lf = fee(net, lp)?;
        let pf = fee(net, protocol)?;
        let cf = fee(net, creator)?;
        let output = net
            .checked_sub(lf)
            .and_then(|n| n.checked_sub(pf))
            .and_then(|n| n.checked_sub(cf))
            .ok_or_else(fail)?;
        (output, net, lf, pf, cf)
    } else {
        let estimate = narrow(
            u128::from(input) * 10000
                / (10000 + u128::from(lp) + u128::from(protocol) + u128::from(creator)),
        )?;
        let lf = fee(estimate, lp)?;
        let pf = fee(estimate, protocol)?;
        let cf = fee(estimate, creator)?;
        let net = input
            .checked_sub(lf)
            .and_then(|n| n.checked_sub(pf))
            .and_then(|n| n.checked_sub(cf))
            .ok_or_else(fail)?;
        let priced = net.checked_sub(1).ok_or_else(fail)?;
        let output = narrow(
            u128::from(base) * u128::from(priced) / (u128::from(quote) + u128::from(priced)),
        )?;
        (output, net, lf, pf, cf)
    };
    settle(p, input, output, net, lf, pf, cf, zero)
}
pub fn quote_exact_out(p: &Pool, target: &Amount, zero: bool) -> Result<ExactOutSwap> {
    p.validate()?;
    if zero {
        return quote_sell_target(p, target);
    }
    let s = p
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_AMM_STATE_REQUIRED"))?;
    let output = amount(target)?;
    let base = amount(&p.reserve0)?;
    let quote = amount(&p.reserve1)?
        .checked_add(virtual_quote(s, p)?)
        .ok_or_else(fail)?;
    if output >= base {
        return Err(err("PUMP_AMM_LIQUIDITY"));
    }
    let net = narrow((u128::from(quote) * u128::from(output)).div_ceil(u128::from(base - output)))?;
    let (lp, protocol, creator) = fee_rates(p, s)?;
    let lf = fee(net, lp)?;
    let pf = fee(net, protocol)?;
    let cf = fee(net, creator)?;
    let input = net
        .checked_add(lf)
        .and_then(|n| n.checked_add(pf))
        .and_then(|n| n.checked_add(cf))
        .ok_or_else(fail)?;
    let q = settle(p, input, output, net, lf, pf, cf, false)?;
    Ok(ExactOutSwap {
        amount_in: q.consumed_in,
        amount_out: q.amount_out,
        next_pool: q.next_pool,
    })
}
/// Invert three separately rounded fees by residue, then invert the constant
/// product. A fee boundary may reduce net output, so ExactIn binary search is
/// invalid. Each skipped gross interval has no feasible CP input image.
fn quote_sell_target(p: &Pool, target: &Amount) -> Result<ExactOutSwap> {
    let target = amount(target)?;
    let s = p
        .pump
        .as_ref()
        .ok_or_else(|| err("PUMP_AMM_STATE_REQUIRED"))?;
    let base = amount(&p.reserve0)?;
    let quote = amount(&p.reserve1)?
        .checked_add(virtual_quote(s, p)?)
        .ok_or_else(fail)?;
    let (lp, protocol, creator) = fee_rates(p, s)?;
    let slope = 10000u128
        .checked_sub(u128::from(lp) + u128::from(protocol) + u128::from(creator))
        .filter(|n| *n > 0)
        .ok_or_else(fail)?;
    let mut lower = 1u64;
    for _ in 0..32 {
        let mut gross = None;
        for residue in 0..10000u64 {
            let charges = u128::from(fee(residue, lp)?)
                + u128::from(fee(residue, protocol)?)
                + u128::from(fee(residue, creator)?);
            let needed = (u128::from(target) + charges).saturating_sub(u128::from(residue));
            let periods = needed
                .div_ceil(slope)
                .max(u128::from(lower.saturating_sub(residue)).div_ceil(10000));
            let candidate = periods * 10000 + u128::from(residue);
            if candidate >= u128::from(lower)
                && candidate < u128::from(quote)
                && gross.is_none_or(|g| candidate < g)
            {
                gross = Some(candidate);
            }
        }
        let gross = gross.ok_or_else(|| err("PUMP_AMM_EXACT_OUT_LIQUIDITY"))?;
        let input = narrow((gross * u128::from(base)).div_ceil(u128::from(quote) - gross))?;
        let actual =
            narrow(u128::from(input) * u128::from(quote) / (u128::from(base) + u128::from(input)))?;
        let lf = fee(actual, lp)?;
        let pf = fee(actual, protocol)?;
        let cf = fee(actual, creator)?;
        let delivered = actual
            .checked_sub(lf)
            .and_then(|n| n.checked_sub(pf))
            .and_then(|n| n.checked_sub(cf))
            .unwrap_or(0);
        if delivered >= target {
            // Reuse the normal forward settlement for supply and real-vault
            // payout constraints, fee accounts, native backing and full state.
            let q = quote_exact_in(p, &Amount(input.into()), true)?;
            if q.amount_out.0 < target.into() {
                return Err(err("PUMP_AMM_EXACT_OUT_UNDERDELIVERY"));
            }
            return Ok(ExactOutSwap {
                amount_in: q.consumed_in,
                amount_out: q.amount_out,
                next_pool: q.next_pool,
            });
        }
        lower = actual.checked_add(1).ok_or_else(fail)?;
    }
    Err(err("PUMP_AMM_EXACT_OUT_INVERSION_BUDGET"))
}

#[allow(clippy::too_many_arguments)]
fn settle(
    p: &Pool,
    input: u64,
    output: u64,
    net: u64,
    lp: u64,
    protocol: u64,
    creator: u64,
    zero: bool,
) -> Result<Swap> {
    if output == 0 {
        return Err(err("PUMP_AMM_INSUFFICIENT_OUTPUT"));
    }
    let mut next = p.clone();
    let s = next
        .pump
        .as_mut()
        .ok_or_else(|| err("PUMP_AMM_STATE_REQUIRED"))?;
    s.derived = true;
    let d = s.data(&p.pool_id, 287)?;
    let quote_vault = key(&d, 171);
    let g = s.data(&global_id()?, 949)?;
    let buyback = narrow(u128::from(protocol) * u128::from(u64_at(&g, 899)) / 10000)?;
    let net_vault = if zero {
        net.checked_sub(lp)
    } else {
        net.checked_add(lp)
    }
    .ok_or_else(fail)?;
    if zero && net_vault > u64_at(&s.data(&quote_vault, 165)?, 64) {
        return Err(err("PUMP_AMM_REAL_QUOTE_BOUND"));
    }
    let movements = [
        (s.vault.clone(), if zero { input } else { output }, zero),
        (quote_vault.clone(), net_vault, !zero),
        (s.creator_vault.clone(), creator, true),
        (ata(WSOL, &s.fee_recipient)?, protocol - buyback, true),
        (ata(WSOL, &s.buyback_recipient)?, buyback, true),
    ];
    for (id, amount, add) in movements {
        let a = s.account_mut(&id)?;
        let mut raw = hex::decode(&a.data).map_err(|_| fail())?;
        let old = u64_at(&raw, 64);
        let new = if add {
            old.checked_add(amount)
        } else {
            old.checked_sub(amount)
        }
        .ok_or_else(fail)?;
        raw[64..72].copy_from_slice(&new.to_le_bytes());
        a.data = hex::encode(raw);
        if id != s.vault {
            let a = s.account_mut(&id)?;
            a.lamports = if add {
                a.lamports.checked_add(amount)
            } else {
                a.lamports.checked_sub(amount)
            }
            .ok_or_else(fail)?;
        }
    }
    next.reserve0 = Amount(u64_at(&s.data(&s.vault, 165)?, 64).into());
    next.reserve1 = Amount(u64_at(&s.data(&quote_vault, 165)?, 64).into());
    next.validate()?;
    Ok(Swap {
        amount_out: Amount(output.into()),
        consumed_in: Amount(input.into()),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}
