//! Static, zero-hook Bin manager state; no unlock/settlement execution claim.
use crate::{
    infinity_bin_multi::{BinReserves, MultiBinState},
    Swap,
};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Amount, Pool, Result};
pub fn quote_exact_in(pool: &Pool, input: &Amount, left: bool) -> Result<Swap> {
    pool.validate()?;
    let s = pool
        .infinity_bin
        .as_ref()
        .ok_or_else(|| err("INFINITY_BIN_STATE_REQUIRED"))?;
    let native = MultiBinState {
        active_id: s.active_id,
        bin_step: s.bin_step as u16,
        lp_fee: s.lp_fee,
        protocol_fee0: s.protocol_fee0,
        protocol_fee1: s.protocol_fee1,
        max_steps: s.max_steps,
        tree_min_id: 0,
        tree_max_id: 0xffffff,
        bins: s
            .bins
            .iter()
            .map(|b| {
                (
                    b.id,
                    BinReserves {
                        reserve0: b.reserve0.clone(),
                        reserve1: b.reserve1.clone(),
                    },
                )
            })
            .collect(),
    };
    let detached = s.detached_active.as_ref().map(|b| BinReserves {
        reserve0: b.reserve0.clone(),
        reserve1: b.reserve1.clone(),
    });
    let q = crate::infinity_bin_multi::quote_exact_in_with_active(
        &native,
        input,
        left,
        detached.as_ref(),
    )?;
    if q.consumed_in != *input || !q.remaining_in.0.is_zero() {
        return Err(err("INFINITY_BIN_PARTIAL_FILL"));
    }
    if q.amount_out.0.is_zero() {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let mut next_pool = pool.clone();
    let next = next_pool
        .infinity_bin
        .as_mut()
        .ok_or_else(|| err("INFINITY_BIN_STATE_REQUIRED"))?;
    for b in &mut next.bins {
        let bin = q
            .next_state
            .bins
            .get(&b.id)
            .ok_or_else(|| err("INFINITY_BIN_POSTSTATE"))?;
        b.reserve0 = bin.reserve0.clone();
        b.reserve1 = bin.reserve1.clone();
    }
    next.active_id = q.next_state.active_id;
    next.detached_active =
        q.detached_active
            .as_ref()
            .map(|b| xone_domain::infinity_bin::InfinityBinEntry {
                id: next.active_id,
                reserve0: b.reserve0.clone(),
                reserve1: b.reserve1.clone(),
                total_shares: Amount::checked(BigUint::from(1000u32)).expect("bounded constant"),
            });
    let modulus = BigUint::one() << 256usize;
    if left {
        next.app_reserve0 = Amount::checked(&s.app_reserve0.0 + &input.0)?;
        if s.app_reserve1.0 < q.amount_out.0 {
            return Err(err("INFINITY_BIN_APP_RESERVE_UNDERFLOW"));
        }
        next.app_reserve1 = Amount::checked(&s.app_reserve1.0 - &q.amount_out.0)?;
        next.protocol_fees0 =
            Amount::checked((&s.protocol_fees0.0 + &q.protocol_amount.0) % &modulus)?;
    } else {
        next.app_reserve1 = Amount::checked(&s.app_reserve1.0 + &input.0)?;
        if s.app_reserve0.0 < q.amount_out.0 {
            return Err(err("INFINITY_BIN_APP_RESERVE_UNDERFLOW"));
        }
        next.app_reserve0 = Amount::checked(&s.app_reserve0.0 - &q.amount_out.0)?;
        next.protocol_fees1 =
            Amount::checked((&s.protocol_fees1.0 + &q.protocol_amount.0) % &modulus)?;
    }
    next_pool.validate()?;
    Ok(Swap {
        amount_out: q.amount_out,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool,
    })
}
