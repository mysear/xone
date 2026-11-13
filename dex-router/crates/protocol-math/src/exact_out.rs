//! Independent exact-output target and original Router02 integer rounding.
//! Pair settlement uses the chosen output, never the maximum ExactIn output.
use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use xone_domain::{err, v2_swap, Amount, Pool, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactOutSwap {
    pub amount_in: Amount,
    pub amount_out: Amount,
    pub next_pool: Pool,
}

/// Each admitted profile has an independent execution template and original-program oracle.
pub fn supports_protocol(protocol: &str) -> bool {
    if crate::orca::supports_profile(protocol) {
        return true;
    }
    matches!(
        protocol,
        v2_swap::PROFILE
            | xone_domain::raydium::PROFILE
            | xone_domain::pancake_bsc_lm::PROFILE
            | xone_domain::pancake_lm::PROFILE
            | xone_domain::slipstream_context::PROFILE
            | xone_domain::lfj::PROFILE
            | xone_domain::meteora::PROFILE
            | xone_domain::meteora_token2022::PROFILE
            | xone_domain::meteora::REWARDS_PROFILE
            | xone_domain::meteora::BITMAP_PROFILE
            | xone_domain::meteora::ORDERS_PROFILE
            | xone_domain::meteora::ONLY_Y_PROFILE
            | xone_domain::pump::PROFILE
            | xone_domain::pump::MIGRATION_PROFILE
            | xone_domain::pump_amm::PROFILE
            | xone_domain::pump_amm::BOOST_PROFILE
    )
}

/// Contextual profiles bind tx.origin to the explicitly admitted request caller.
pub fn quote_exact_out_for_origin(
    pool: &Pool,
    output: &Amount,
    zero: bool,
    origin: &str,
) -> Result<ExactOutSwap> {
    if pool.protocol == xone_domain::slipstream_context::PROFILE {
        let q = crate::slipstream_context::quote_exact_out(pool, output, zero, origin)?;
        if q.amount_out != *output || q.remaining_in != Amount::zero() {
            return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
        }
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    quote_exact_out(pool, output, zero)
}

/// Fixed original Router02 execution template with standard external funding.
/// Other protocols and Token behaviors need their own independent interface.
pub fn quote_exact_out(pool: &Pool, output: &Amount, zero: bool) -> Result<ExactOutSwap> {
    if crate::orca::supports_profile(&pool.protocol) {
        let q = crate::orca::quote_exact_out(pool, output, zero)?;
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    if pool.protocol == xone_domain::raydium::PROFILE {
        return crate::raydium::quote_exact_out(pool, output, zero);
    }
    if xone_domain::pump_amm::supports_profile(&pool.protocol) {
        return crate::pump_amm::quote_exact_out(pool, output, zero);
    }
    if xone_domain::pump::supports_curve_profile(&pool.protocol) {
        return crate::pump::quote_exact_out(pool, output, zero);
    }
    if xone_domain::meteora::supports_profile(&pool.protocol) {
        return crate::meteora::quote_exact_out(pool, output, zero);
    }
    if pool.protocol == xone_domain::slipstream_context::PROFILE {
        return Err(err("SLIPSTREAM_ORIGIN_REQUIRED"));
    }
    if pool.protocol == xone_domain::lfj::PROFILE {
        let q = crate::lfj::quote_exact_out(pool, output, zero)?;
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    if matches!(
        pool.protocol.as_str(),
        xone_domain::pancake_bsc_lm::PROFILE | xone_domain::pancake_lm::PROFILE
    ) {
        let q = if pool.protocol == xone_domain::pancake_bsc_lm::PROFILE {
            crate::pancake_bsc_lm::quote_exact_out(pool, output, zero)?
        } else {
            crate::pancake_lm::quote_exact_out(pool, output, zero)?
        };
        if q.amount_out != *output || q.remaining_in != Amount::zero() {
            return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
        }
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    if pool.protocol != v2_swap::PROFILE {
        return Err(err("EXACT_OUT_PROTOCOL_UNSUPPORTED"));
    }
    pool.validate()?;
    if output.0.is_zero() {
        return Err(err("EXACT_OUT_ZERO_OUTPUT"));
    }
    if output.0.bits() > 256 {
        return Err(err("EXACT_OUT_AMOUNT_WIDTH"));
    }
    let source = pool
        .v2
        .as_ref()
        .and_then(|s| s.swap.as_ref())
        .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
    let (reserve_in, reserve_out) = if zero {
        (&pool.reserve0, &pool.reserve1)
    } else {
        (&pool.reserve1, &pool.reserve0)
    };
    if reserve_in.0.is_zero() || output.0 >= reserve_out.0 {
        return Err(err("EXACT_OUT_LIQUIDITY_BOUND"));
    }
    // Match original getAmountIn's floor + 1, including exactly divisible cases.
    // This is the admitted Router execution constraint, not arbitrary Pair min.
    let numerator = &reserve_in.0 * &output.0 * BigUint::from(1000u16);
    let denominator = (&reserve_out.0 - &output.0) * BigUint::from(997u16);
    if numerator.bits() > 256 || denominator.bits() > 256 {
        return Err(err("EXACT_OUT_UINT256_OVERFLOW"));
    }
    let amount_in = Amount::checked(numerator / denominator + BigUint::one())?;
    let none = Amount::zero();
    let (out0, out1, balance0, balance1) = if zero {
        if source.raw_balance1.0 < output.0 {
            return Err(err("V2_SWAP_UINT256_UNDERFLOW"));
        }
        (
            &none,
            output,
            Amount::checked(&source.raw_balance0.0 + &amount_in.0)?,
            Amount::checked(&source.raw_balance1.0 - &output.0)?,
        )
    } else {
        if source.raw_balance0.0 < output.0 {
            return Err(err("V2_SWAP_UINT256_UNDERFLOW"));
        }
        (
            output,
            &none,
            Amount::checked(&source.raw_balance0.0 - &output.0)?,
            Amount::checked(&source.raw_balance1.0 + &amount_in.0)?,
        )
    };
    let settled = crate::v2_complete::settle_swap(
        [&pool.reserve0, &pool.reserve1],
        source,
        [out0, out1],
        [&balance0, &balance1],
        source.bank_unix_timestamp,
    )?;
    let mut next_pool = pool.clone();
    next_pool.reserve0 = settled.reserve0;
    next_pool.reserve1 = settled.reserve1;
    next_pool
        .v2
        .as_mut()
        .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?
        .swap = Some(settled.state);
    next_pool.validate()?;
    Ok(ExactOutSwap {
        amount_in,
        amount_out: output.clone(),
        next_pool,
    })
}

pub fn quote_exact_out_with_limit(
    pool: &Pool,
    output: &Amount,
    max_input: &Amount,
    zero: bool,
) -> Result<ExactOutSwap> {
    if max_input.0.bits() > 256 {
        return Err(err("EXACT_OUT_AMOUNT_WIDTH"));
    }
    let quote = quote_exact_out(pool, output, zero)?;
    if quote.amount_in.0 > max_input.0 {
        return Err(err("EXACT_OUT_MAX_INPUT"));
    }
    Ok(quote)
}
