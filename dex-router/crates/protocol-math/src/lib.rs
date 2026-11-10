pub mod aerodrome;
pub mod balancer_composable;
mod balancer_log_exp;
pub mod balancer_managed_fees;
pub mod balancer_multi;
pub mod balancer_stable;
pub mod balancer_weighted;
pub mod curve_crypto;
pub mod curve_multi;
pub mod curve_ng;
pub mod curve_plain;
pub mod exact_out;
pub mod lfj;
pub mod pancake_bsc_lm;
pub use exact_out::{quote_exact_out, ExactOutSwap};
pub mod pancake_lm;
pub mod slipstream_context;
pub use singleton::infinity_bin;
pub mod infinity_bin_multi;
pub mod infinity_bin_online;
pub mod managed_v2_aum;
pub mod managed_v2_bpt;
pub mod managed_v2_breaker;
pub mod managed_v2_pool;
pub mod managed_v2_swap;
pub mod multi_asset;
pub mod orca;
pub mod orca_adaptive;
pub mod orca_adaptive_online;
pub mod orca_adaptive_swap;
pub mod raydium;
pub mod raydium_clmm;
pub mod singleton;
pub mod v2_complete;
pub mod v3;
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use xone_domain::{err, Amount, Pool, Result};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Swap {
    pub amount_out: Amount,
    pub consumed_in: Amount,
    pub remaining_in: Amount,
    pub next_pool: Pool,
}
/// Exact V2 contract semantics: multiply before dividing; intermediate products are widened.
pub fn quote_exact_in(pool: &Pool, input: &Amount, zero_for_one: bool) -> Result<Swap> {
    let profile = xone_domain::protocols::protocol(&pool.protocol);
    if pool.protocol == xone_domain::v2_swap::PROFILE {
        return v2_complete::quote_exact_in(pool, input, zero_for_one);
    }
    if pool.protocol == xone_domain::pancake_bsc_lm::PROFILE {
        return Err(err("BSC4_FUNDS_REQUIRED"));
    }
    if pool.protocol == xone_domain::pancake_lm::PROFILE {
        return Err(err("PANCAKE_LM_FUNDS_REQUIRED"));
    }
    if pool.protocol == xone_domain::slipstream_context::PROFILE {
        return Err(err("SLIPSTREAM_FUNDS_REQUIRED"));
    }
    if pool.protocol == xone_domain::lfj::PROFILE {
        return lfj::quote_exact_in(pool, input, zero_for_one);
    }
    if pool.protocol == xone_domain::infinity_bin::PROFILE {
        return infinity_bin_online::quote_exact_in(pool, input, zero_for_one);
    }
    if profile.is_some_and(|p| p.family == "singleton-advanced") {
        pool.validate()?;
        let s = pool
            .singleton_advanced
            .as_ref()
            .ok_or_else(|| err("SINGLETON_ADVANCED_STATE_REQUIRED"))?;
        let q = singleton::quote_advanced_exact_in(s, input, zero_for_one)?;
        let mut next_pool = pool.clone();
        next_pool.singleton_advanced = Some(q.next_state);
        return Ok(Swap {
            amount_out: q.amount_out,
            consumed_in: q.consumed_in,
            remaining_in: q.remaining_in,
            next_pool,
        });
    }

    if pool.multi_asset.is_some() {
        return multi_asset::quote_exact_in_tokens(
            pool,
            input,
            if zero_for_one {
                &pool.token0
            } else {
                &pool.token1
            },
            if zero_for_one {
                &pool.token1
            } else {
                &pool.token0
            },
        );
    }
    if profile.is_some_and(|p| p.family == "weighted") {
        pool.validate()?;
        let s = pool
            .weighted
            .as_ref()
            .ok_or_else(|| err("WEIGHTED_STATE_REQUIRED"))?;
        let q = balancer_weighted::quote_exact_in_weighted(
            [&pool.reserve0.0, &pool.reserve1.0],
            [&s.scaling0.0, &s.scaling1.0],
            [&s.weight0.0, &s.weight1.0],
            &BigUint::from(pool.fee_numerator),
            &input.0,
            zero_for_one,
        )?;
        let mut next_pool = pool.clone();
        next_pool.reserve0 = Amount::checked(q.next_balances[0].clone())?;
        next_pool.reserve1 = Amount::checked(q.next_balances[1].clone())?;
        return Ok(Swap {
            amount_out: Amount::checked(q.amount_out)?,
            consumed_in: input.clone(),
            remaining_in: Amount::zero(),
            next_pool,
        });
    }
    if profile.is_some_and(|p| p.family == "rayclmm") {
        return raydium_clmm::quote_exact_in(pool, input, zero_for_one);
    }
    if profile.is_some_and(|p| p.family == "orca") {
        return orca::quote_exact_in(pool, input, zero_for_one);
    }
    if profile.is_some_and(|p| p.family == "raydium") {
        return raydium::quote_exact_in(pool, input, zero_for_one);
    }
    if profile.is_some_and(|p| p.family == "singleton") {
        return singleton::quote_exact_in(pool, input, zero_for_one);
    }
    if let Some(profile) = profile.filter(|p| ["aero", "curve", "balancer"].contains(&p.family)) {
        pool.validate()?;
        if pool.token_behavior != "standard" {
            return Err(err("TOKEN_BEHAVIOR_UNSUPPORTED"));
        }
        let s = pool
            .stable
            .as_ref()
            .ok_or_else(|| err("STABLE_STATE_REQUIRED"))?;
        let balances = [&pool.reserve0.0, &pool.reserve1.0];
        let rates = [&s.rate0.0, &s.rate1.0];
        let (out, next) = match profile.family {
            "aero" => {
                let quote = if pool.protocol.starts_with("velodrome-v2-") {
                    aerodrome::quote_exact_in_velodrome
                } else {
                    aerodrome::quote_exact_in
                };
                let q = quote(
                    balances,
                    rates,
                    pool.fee_numerator,
                    &input.0,
                    zero_for_one,
                    pool.protocol == "aerodrome-v2-stable"
                        || pool.protocol == "velodrome-v2-stable",
                )?;
                (q.amount_out, q.next_balances)
            }
            "curve" => {
                let q = curve_plain::quote_exact_in(
                    balances,
                    rates,
                    &s.amplification.0,
                    pool.fee_numerator,
                    s.admin_fee,
                    &input.0,
                    zero_for_one,
                )?;
                (q.amount_out, q.next_balances)
            }
            "balancer" => {
                let q = balancer_stable::quote_exact_in(
                    balances,
                    &s.amplification.0,
                    rates,
                    &BigUint::from(pool.fee_numerator),
                    &input.0,
                    zero_for_one,
                )?;
                (q.amount_out, q.next_balances)
            }
            _ => return Err(err("STABLE_PROTOCOL_UNSUPPORTED")),
        };
        if out.is_zero() {
            return Err(err("INSUFFICIENT_OUTPUT"));
        }
        let mut next_pool = pool.clone();
        next_pool.reserve0 = Amount::checked(next[0].clone())?;
        next_pool.reserve1 = Amount::checked(next[1].clone())?;
        return Ok(Swap {
            amount_out: Amount::checked(out)?,
            consumed_in: input.clone(),
            remaining_in: Amount::zero(),
            next_pool,
        });
    }
    if profile.is_some_and(|p| p.family == "v3") {
        return v3::quote_exact_in(pool, input, zero_for_one);
    }
    pool.validate()?;
    let evm_v2 = profile.is_some_and(|p| p.family == "v2");
    if !evm_v2 && pool.protocol != "fixture-cpmm" {
        return Err(err("PROTOCOL_UNSUPPORTED"));
    }
    if pool.token_behavior != "standard" {
        return Err(err("TOKEN_BEHAVIOR_UNSUPPORTED"));
    }
    if input.0.is_zero() {
        return Err(err("ZERO_INPUT"));
    }
    let (rin, rout) = if zero_for_one {
        (&pool.reserve0.0, &pool.reserve1.0)
    } else {
        (&pool.reserve1.0, &pool.reserve0.0)
    };
    if evm_v2 && (rin.bits() > 112 || rout.bits() > 112 || (rin + &input.0).bits() > 112) {
        return Err(err("V2_RESERVE_OVERFLOW"));
    }
    if rin.is_zero() || rout.is_zero() {
        return Err(err("INSUFFICIENT_LIQUIDITY"));
    }
    let denominator = BigUint::from(pool.fee_denominator);
    let after_fee = &input.0 * BigUint::from(pool.fee_denominator - pool.fee_numerator);
    let numerator = &after_fee * rout;
    let divisor = rin * &denominator + &after_fee;
    if evm_v2 && (after_fee.bits() > 256 || numerator.bits() > 256 || divisor.bits() > 256) {
        return Err(err("V2_INTERMEDIATE_OVERFLOW"));
    }
    let output = numerator / divisor;
    if output.is_zero() || &output >= rout {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    let amount_out = Amount::checked(output.clone())?;
    let new_in = Amount::checked(rin + &input.0)?;
    let new_out = Amount::checked(rout - output)?;
    let mut next = pool.clone();
    if zero_for_one {
        next.reserve0 = new_in;
        next.reserve1 = new_out;
    } else {
        next.reserve1 = new_in;
        next.reserve0 = new_out;
    }
    Ok(Swap {
        amount_out,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool: next,
    })
}
