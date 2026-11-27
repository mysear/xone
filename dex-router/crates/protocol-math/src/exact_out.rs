//! Independent exact-output target and original Router02 integer rounding.
//! Pair settlement uses the chosen output, never the maximum ExactIn output.
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use xone_domain::{err, v2_swap, Amount, Pool, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactOutSwap {
    pub amount_in: Amount,
    pub amount_out: Amount,
    pub next_pool: Pool,
}

/// Each admitted profile has an independent exact-output kernel and original-program oracle.
/// Execution preparation is advertised separately by the capability catalog.
pub fn supports_protocol(protocol: &str) -> bool {
    if crate::managed_v2_pool::supports_regular_exact_out(protocol)
        || matches!(
            protocol,
            "pancake-infinity-bin"
                | "uniswap-v4"
                | "pancake-infinity-cl"
                | "balancer-v2-stable2"
                | "balancer-v2-weighted2-equal"
                | "balancer-v2-weighted2-static"
                | "balancer-v2-weighted-multi"
                | "balancer-v2-managed-snapshot"
                | "balancer-v2-managed-fees"
                | "balancer-v2-composable-regular"
                | "curve-plain3-basic"
                | "curve-plain4-basic"
                | "curve-metausd-direct"
                | "curve-stableswap-ng"
        )
    {
        return true;
    }
    if crate::orca::supports_profile(protocol)
        || crate::raydium_clmm::supports_profile(protocol)
        || crate::v3::supports_exact_out(protocol)
    {
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

pub fn supports_pair(pool: &Pool, token_in: &str, token_out: &str) -> bool {
    supports_protocol(&pool.protocol)
        && ((token_in != pool.pool_id && token_out != pool.pool_id)
            || (pool.protocol != "balancer-v2-composable-regular"
                && (!crate::managed_v2_pool::supports_regular_exact_out(&pool.protocol)
                    || crate::managed_v2_pool::supports_bpt_exact_out(&pool.protocol))))
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
    if pool.protocol == xone_domain::infinity_bin::PROFILE {
        let q = crate::infinity_bin_online::quote_exact_out(pool, output, zero)?;
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    if matches!(pool.protocol.as_str(), "uniswap-v4" | "pancake-infinity-cl") {
        return crate::singleton::quote_exact_out(pool, output, zero);
    }
    if crate::orca::supports_profile(&pool.protocol) {
        let q = crate::orca::quote_exact_out(pool, output, zero)?;
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    if crate::raydium_clmm::supports_profile(&pool.protocol) {
        let q = crate::raydium_clmm::quote_exact_out(pool, output, zero)?;
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    if crate::v3::supports_exact_out(&pool.protocol) {
        let q = crate::v3::quote_exact_out(pool, output, zero)?;
        return Ok(ExactOutSwap {
            amount_in: q.consumed_in,
            amount_out: q.amount_out,
            next_pool: q.next_pool,
        });
    }
    if pool.protocol == "balancer-v2-stable2" {
        pool.validate()?;
        let s = pool
            .stable
            .as_ref()
            .ok_or_else(|| err("STABLE_STATE_REQUIRED"))?;
        let q = crate::balancer_stable::quote_exact_out(
            [&pool.reserve0.0, &pool.reserve1.0],
            &s.amplification.0,
            [&s.rate0.0, &s.rate1.0],
            &BigUint::from(pool.fee_numerator),
            &output.0,
            zero,
        )?;
        let mut next_pool = pool.clone();
        next_pool.reserve0 = Amount::checked(q.next_balances[0].clone())?;
        next_pool.reserve1 = Amount::checked(q.next_balances[1].clone())?;
        next_pool.validate()?;
        return Ok(ExactOutSwap {
            amount_in: Amount::checked(q.amount_in)?,
            amount_out: output.clone(),
            next_pool,
        });
    }
    if matches!(
        pool.protocol.as_str(),
        "balancer-v2-weighted2-equal" | "balancer-v2-weighted2-static"
    ) {
        pool.validate()?;
        let s = pool
            .weighted
            .as_ref()
            .ok_or_else(|| err("WEIGHTED_STATE_REQUIRED"))?;
        let q = crate::balancer_weighted::quote_exact_out_weighted(
            [&pool.reserve0.0, &pool.reserve1.0],
            [&s.scaling0.0, &s.scaling1.0],
            [&s.weight0.0, &s.weight1.0],
            &BigUint::from(pool.fee_numerator),
            &output.0,
            zero,
        )?;
        let mut next_pool = pool.clone();
        next_pool.reserve0 = Amount::checked(q.next_balances[0].clone())?;
        next_pool.reserve1 = Amount::checked(q.next_balances[1].clone())?;
        next_pool.validate()?;
        return Ok(ExactOutSwap {
            amount_in: Amount::checked(q.amount_in)?,
            amount_out: output.clone(),
            next_pool,
        });
    }
    if crate::managed_v2_pool::supports_regular_exact_out(&pool.protocol)
        || pool.protocol == "balancer-v2-weighted-multi"
        || pool.protocol == "balancer-v2-managed-snapshot"
        || pool.protocol == "balancer-v2-managed-fees"
        || pool.protocol == "balancer-v2-composable-regular"
        || pool.protocol == "curve-plain3-basic"
        || pool.protocol == "curve-plain4-basic"
        || pool.protocol == "curve-metausd-direct"
        || pool.protocol == "curve-stableswap-ng"
    {
        let (input, output_token) = if zero {
            (&pool.token0, &pool.token1)
        } else {
            (&pool.token1, &pool.token0)
        };
        let q = crate::multi_asset::quote_exact_out_tokens(pool, output, input, output_token)?;
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
    let source = pool
        .v2
        .as_ref()
        .and_then(|s| s.swap.as_ref())
        .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
    let settled =
        crate::v2_complete::quote_exact_out(pool, output, zero, source.bank_unix_timestamp)?;
    Ok(ExactOutSwap {
        amount_in: settled.consumed_in,
        amount_out: settled.amount_out,
        next_pool: settled.next_pool,
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
