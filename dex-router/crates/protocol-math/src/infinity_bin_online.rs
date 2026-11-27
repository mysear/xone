//! Bin manager swaps with complete tree state and Vault app-reserve accounting.
use crate::{
    infinity_bin_multi::{BinReserves, MultiBinState},
    singleton::{SETTLEMENT_HOOK_FEE, SETTLEMENT_HOOK_SPECIFIED, SETTLEMENT_HOOK_UNSPECIFIED},
    Swap,
};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use xone_domain::{err, Amount, Pool, Result};
pub fn quote_exact_in(pool: &Pool, input: &Amount, left: bool) -> Result<Swap> {
    quote(pool, input, left, false)
}
pub fn quote_exact_out(pool: &Pool, target: &Amount, left: bool) -> Result<Swap> {
    quote(pool, target, left, true)
}
fn quote(pool: &Pool, amount: &Amount, left: bool, exact_out: bool) -> Result<Swap> {
    pool.validate()?;
    let s = pool
        .infinity_bin
        .as_ref()
        .ok_or_else(|| err("INFINITY_BIN_STATE_REQUIRED"))?;
    let hooked = !s.hook_model.is_empty();
    let signed_limit = BigUint::one() << 127usize;
    if hooked && !exact_out && amount.0 > signed_limit {
        return Err(err("INFINITY_BIN_INT128_RANGE"));
    }
    let core_amount = if !hooked {
        amount.clone()
    } else if exact_out {
        Amount::checked(&amount.0 + SETTLEMENT_HOOK_SPECIFIED)?
    } else {
        let fee = BigUint::from(SETTLEMENT_HOOK_SPECIFIED);
        if amount.0 <= fee {
            return Err(err("SINGLETON_HOOK_INPUT_TOO_SMALL"));
        }
        Amount::checked(&amount.0 - fee)?
    };
    let native = MultiBinState {
        active_id: s.active_id,
        bin_step: s.bin_step as u16,
        lp_fee: if hooked {
            SETTLEMENT_HOOK_FEE
        } else {
            s.lp_fee
        },
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
    let q = if exact_out {
        crate::infinity_bin_exact_out::quote_exact_out_with_active(
            &native,
            &core_amount,
            left,
            detached.as_ref(),
        )?
    } else {
        crate::infinity_bin_multi::quote_exact_in_with_active(
            &native,
            &core_amount,
            left,
            detached.as_ref(),
        )?
    };
    if (if exact_out {
        q.amount_out != core_amount
    } else {
        q.consumed_in != core_amount
    }) || !q.remaining_in.0.is_zero()
    {
        return Err(err("INFINITY_BIN_PARTIAL_FILL"));
    }
    let input = &q.consumed_in;
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
    let (consumed_in, amount_out) = if !hooked {
        (q.consumed_in, q.amount_out)
    } else if exact_out {
        let input = Amount::checked(&q.consumed_in.0 + SETTLEMENT_HOOK_UNSPECIFIED)?;
        if input.0 > signed_limit {
            return Err(err("INFINITY_BIN_INT128_RANGE"));
        }
        (input, amount.clone())
    } else {
        let delta = BigUint::from(SETTLEMENT_HOOK_UNSPECIFIED);
        if q.amount_out.0 <= delta {
            return Err(err("SINGLETON_HOOK_OUTPUT_TOO_SMALL"));
        }
        (amount.clone(), Amount::checked(q.amount_out.0 - delta)?)
    };
    Ok(Swap {
        amount_out,
        consumed_in,
        remaining_in: Amount::zero(),
        next_pool,
    })
}
