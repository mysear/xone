//! Exact BSC V4 lazy migration, sharing only the verified curve/accounting kernel.
use std::collections::BTreeSet;
use xone_domain::{err, Amount, Pool, Result};

pub fn quote_exact_in(parent: &Pool, input: &Amount, zero: bool) -> Result<crate::Swap> {
    quote(parent, input, zero, false)
}
pub fn quote_exact_out(parent: &Pool, output: &Amount, zero: bool) -> Result<crate::Swap> {
    quote(parent, output, zero, true)
}
fn quote(parent: &Pool, input: &Amount, zero: bool, exact_out: bool) -> Result<crate::Swap> {
    let before = parent
        .pancake_bsc_lm
        .as_ref()
        .ok_or_else(|| err("BSC4_STATE_REQUIRED"))?;
    let arithmetic = before.arithmetic_pool(parent)?;
    let mut curve = arithmetic.clone();
    curve.protocol = "pancake-v3".into();
    curve.adapter_version = "pancake-v3/core-1".into();
    curve.v3 = Some(
        arithmetic
            .pancake_lm
            .as_ref()
            .ok_or_else(|| err("BSC4_ARITHMETIC_LEDGER"))?
            .core
            .clone(),
    );
    curve.pancake_lm = None;
    let (_, traces) = if exact_out {
        crate::v3::quote_exact_out_with_trace(&curve, input, zero)?
    } else {
        crate::v3::quote_exact_in_with_trace(&curve, input, zero)?
    };
    let crossed = traces
        .into_iter()
        .filter_map(|t| t.crossed_tick)
        .collect::<BTreeSet<_>>();
    let swap = if exact_out {
        crate::pancake_lm::quote_exact_out(&arithmetic, input, zero)?
    } else {
        crate::pancake_lm::quote_exact_in(&arithmetic, input, zero)?
    };
    let mut next = before.clone();
    next.ledger = swap
        .next_pool
        .pancake_lm
        .ok_or_else(|| err("BSC4_ARITHMETIC_LEDGER"))?;
    next.ledger.pool_template_sha256 = before.ledger.pool_template_sha256.clone();
    next.ledger.factory_template_sha256 = before.ledger.factory_template_sha256.clone();
    next.ledger.lm.template_sha256 = before.ledger.lm.template_sha256.clone();
    next.ledger.master_chef.template_sha256 = before.ledger.master_chef.template_sha256.clone();
    for (index, flag) in next.flags.iter_mut().enumerate() {
        if crossed.contains(&flag.tick) {
            // _updateLMTicks writes the flag even if the inherited gross is zero.
            flag.migrated = true;
        } else {
            // An unvisited predecessor slot must not be materialized locally.
            next.ledger.lm.ticks[index] = before.ledger.lm.ticks[index].clone();
        }
    }
    let mut next_pool = parent.clone();
    next_pool.pancake_bsc_lm = Some(next);
    next_pool.validate()?;
    Ok(crate::Swap {
        amount_out: swap.amount_out,
        consumed_in: swap.consumed_in,
        remaining_in: swap.remaining_in,
        next_pool,
    })
}
