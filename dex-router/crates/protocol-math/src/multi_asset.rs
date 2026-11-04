use crate::Swap;
use num_bigint::BigUint;
use xone_domain::{err, Amount, Pool, Result};
/// Token-selected multi-asset swaps mutate a single full physical balance vector.
/// Callers may not manufacture independently mutable pair projections.
pub fn quote_exact_in_tokens(
    pool: &Pool,
    input: &Amount,
    token_in: &str,
    token_out: &str,
) -> Result<Swap> {
    if pool.protocol == xone_domain::managed_v2_state::PROTOCOL {
        return crate::managed_v2_pool::quote_exact_in_tokens(pool, input, token_in, token_out);
    }
    pool.validate()?;
    let s = pool
        .multi_asset
        .as_ref()
        .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
    let i = s
        .assets
        .iter()
        .position(|a| a.token == token_in)
        .ok_or_else(|| err("MULTI_ASSET_INPUT_TOKEN"))?;
    let j = s
        .assets
        .iter()
        .position(|a| a.token == token_out)
        .ok_or_else(|| err("MULTI_ASSET_OUTPUT_TOKEN"))?;
    if i == j {
        return Err(err("MULTI_ASSET_IDENTICAL_TOKENS"));
    }
    let balances = s
        .assets
        .iter()
        .map(|a| a.balance.0.clone())
        .collect::<Vec<_>>();
    let scales = s
        .assets
        .iter()
        .map(|a| a.scaling.0.clone())
        .collect::<Vec<_>>();
    let fee = BigUint::from(pool.fee_numerator);
    let (out, next) = match pool.protocol.as_str() {
        "balancer-v2-weighted-multi" => {
            let weights = s.weights.iter().map(|w| w.0.clone()).collect::<Vec<_>>();
            let q = crate::balancer_multi::quote_exact_in(
                &balances, &weights, &scales, &fee, &input.0, i, j,
            )?;
            (q.amount_out, q.next_balances)
        }
        "balancer-v2-managed-snapshot" => {
            let weights = s.weights.iter().map(|w| w.0.clone()).collect::<Vec<_>>();
            let guards = crate::balancer_multi::ManagedSwapGuards {
                swaps_enabled: true,
                protocol_swap_fee: BigUint::from(0u8),
                management_swap_fee: BigUint::from(0u8),
            };
            let q = crate::balancer_multi::quote_managed_snapshot(
                &balances, &weights, &scales, &fee, &input.0, i, j, &guards,
            )?;
            (q.amount_out, q.next_balances)
        }
        "balancer-v2-composable-regular" => {
            let bpt = s
                .assets
                .iter()
                .position(|a| a.token == pool.pool_id)
                .ok_or_else(|| err("COMPOSABLE_BPT_REQUIRED"))?;
            let q = crate::balancer_composable::quote_exact_in(
                &balances,
                &s.amplification.0,
                &scales,
                &fee,
                &input.0,
                i,
                j,
                bpt,
            )?;
            (q.amount_out, q.next_balances)
        }
        "curve-crypto2-full" | "curve-crypto2-ramp" => {
            use crate::curve_crypto::{
                quote_full_exact_in, CryptoFullLedger, CryptoParams, CryptoRamp, CryptoState,
            };
            let c = s
                .curve_crypto
                .as_ref()
                .ok_or_else(|| err("CURVE_CRYPTO_STATE_REQUIRED"))?;
            let params = CryptoParams {
                amp: s.amplification.0.clone(),
                gamma: c.gamma.0.clone(),
                mid_fee: fee.clone(),
                out_fee: c.out_fee.0.clone(),
                fee_gamma: c.fee_gamma.0.clone(),
                allowed_extra_profit: c.allowed_extra_profit.0.clone(),
                adjustment_step: c.adjustment_step.0.clone(),
                admin_fee: BigUint::from(s.admin_fee),
                ma_half_time: BigUint::from(c.ma_half_time),
            };
            let state = CryptoState {
                invariant: c.invariant.0.clone(),
                price_scale: c.price_scale.0.clone(),
                price_oracle: c.price_oracle.0.clone(),
                last_prices: c.last_prices.0.clone(),
                last_prices_timestamp: c.last_prices_timestamp,
                xcp_profit: c.xcp_profit.0.clone(),
                xcp_profit_a: c.xcp_profit_a.0.clone(),
                virtual_price: c.virtual_price.0.clone(),
                not_adjusted: c.not_adjusted,
                future_a_gamma_time: c.future_a_gamma_time,
            };
            let ledger = CryptoFullLedger {
                total_supply: c.total_supply.0.clone(),
                receiver_balance: c.admin_receiver_balance.0.clone(),
                raw_balances: c.raw_balances.iter().map(|a| a.0.clone()).collect(),
                admin_receiver_enabled: c.admin_receiver != format!("0x{}", "0".repeat(40)),
                ramp: c.ramp.as_ref().map(|r| CryptoRamp {
                    initial_amp: r.initial_amp.0.clone(),
                    initial_gamma: r.initial_gamma.0.clone(),
                    initial_time: r.initial_time,
                }),
            };
            let precisions = scales
                .iter()
                .map(|scale| scale / BigUint::from(1_000_000_000_000_000_000u64))
                .collect::<Vec<_>>();
            let q = quote_full_exact_in(
                &balances,
                &precisions,
                &params,
                &state,
                &ledger,
                c.timestamp,
                &input.0,
                i,
                j,
            )?;
            if q.amount_out == BigUint::from(0u8) {
                return Err(err("INSUFFICIENT_OUTPUT"));
            }
            if q.next_balances.len() != 2 || q.next_raw_balances.len() != 2 {
                return Err(err("CURVE_CRYPTO_POSTSTATE_LENGTH"));
            }
            let mut next_pool = pool.clone();
            let next = next_pool
                .multi_asset
                .as_mut()
                .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
            for (asset, balance) in next.assets.iter_mut().zip(q.next_balances) {
                asset.balance = Amount::checked(balance)?;
            }
            let c = next
                .curve_crypto
                .as_mut()
                .ok_or_else(|| err("CURVE_CRYPTO_STATE_REQUIRED"))?;
            let state = q.next_pricing_state;
            c.invariant = Amount::checked(state.invariant)?;
            c.price_scale = Amount::checked(state.price_scale)?;
            c.price_oracle = Amount::checked(state.price_oracle)?;
            c.last_prices = Amount::checked(state.last_prices)?;
            c.last_prices_timestamp = state.last_prices_timestamp;
            c.xcp_profit = Amount::checked(state.xcp_profit)?;
            c.xcp_profit_a = Amount::checked(state.xcp_profit_a)?;
            c.virtual_price = Amount::checked(state.virtual_price)?;
            c.not_adjusted = state.not_adjusted;
            c.future_a_gamma_time = state.future_a_gamma_time;
            c.total_supply = Amount::checked(q.next_total_supply)?;
            c.admin_receiver_balance = Amount::checked(q.next_receiver_balance)?;
            c.raw_balances = q
                .next_raw_balances
                .into_iter()
                .map(Amount::checked)
                .collect::<Result<_>>()?;
            next_pool.reserve0 = next.assets[0].balance.clone();
            next_pool.reserve1 = next.assets[1].balance.clone();
            next_pool.validate()?;
            return Ok(Swap {
                amount_out: Amount::checked(q.amount_out)?,
                consumed_in: input.clone(),
                remaining_in: Amount::zero(),
                next_pool,
            });
        }
        "curve-metausd-direct" => {
            let m = s
                .curve_meta
                .as_ref()
                .ok_or_else(|| err("CURVE_META_STATE_REQUIRED"))?;
            let q = crate::curve_multi::quote_meta_exact_in(
                &balances,
                &s.amplification.0,
                &scales[0],
                &m.base_virtual_price.0,
                &fee,
                &BigUint::from(s.admin_fee),
                &input.0,
                i,
                j,
            )?;
            if q.next_balances.len() != 2 {
                return Err(err("MULTI_ASSET_POST_STATE_LENGTH"));
            }
            if q.amount_out == BigUint::from(0u8) {
                return Err(err("INSUFFICIENT_OUTPUT"));
            }
            let mut next_pool = pool.clone();
            let next = next_pool
                .multi_asset
                .as_mut()
                .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
            for (asset, balance) in next.assets.iter_mut().zip(q.next_balances) {
                asset.balance = Amount::checked(balance)?;
            }
            let m = next
                .curve_meta
                .as_mut()
                .ok_or_else(|| err("CURVE_META_STATE_REQUIRED"))?;
            m.stored_balances[i] = Amount::checked(&m.stored_balances[i].0 + &input.0)?;
            if m.stored_balances[j].0 < q.amount_out {
                return Err(err("CURVE_META_BALANCE_ACCOUNTING"));
            }
            m.stored_balances[j] = Amount::checked(&m.stored_balances[j].0 - &q.amount_out)?;
            m.admin_balances[j] = Amount::checked(&m.admin_balances[j].0 + q.admin_fee_amount)?;
            next_pool.reserve0 = next.assets[0].balance.clone();
            next_pool.reserve1 = next.assets[1].balance.clone();
            next_pool.validate()?;
            return Ok(Swap {
                amount_out: Amount::checked(q.amount_out)?,
                consumed_in: input.clone(),
                remaining_in: Amount::zero(),
                next_pool,
            });
        }
        "curve-stableswap-ng" => {
            let ng = s
                .curve_ng
                .as_ref()
                .ok_or_else(|| err("CURVE_NG_STATE_REQUIRED"))?;
            let native = crate::curve_ng::NgOracleState {
                stored_balances: ng.stored_balances.iter().map(|a| a.0.clone()).collect(),
                admin_balances: ng.admin_balances.iter().map(|a| a.0.clone()).collect(),
                last_prices_packed: ng.last_prices_packed.iter().map(|a| a.0.clone()).collect(),
                last_d_packed: ng.last_d_packed.0.clone(),
                ma_last_time: ng.ma_last_time.0.clone(),
                ma_exp_time: ng.ma_exp_time,
                d_ma_time: ng.d_ma_time,
                timestamp: ng.timestamp,
            };
            let q = crate::curve_ng::quote_full_exact_in(
                &balances,
                &s.amplification.0,
                &scales,
                &fee,
                &BigUint::from(s.admin_fee),
                &ng.offpeg_fee_multiplier.0,
                &input.0,
                i,
                j,
                &native,
            )?;
            if q.next_balances.len() != s.assets.len() {
                return Err(err("MULTI_ASSET_POST_STATE_LENGTH"));
            }
            if q.amount_out == BigUint::from(0u8) {
                return Err(err("INSUFFICIENT_OUTPUT"));
            }
            let mut next_pool = pool.clone();
            let next = next_pool
                .multi_asset
                .as_mut()
                .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
            for (a, b) in next.assets.iter_mut().zip(q.next_balances) {
                a.balance = Amount::checked(b)?;
            }
            let state = next
                .curve_ng
                .as_mut()
                .ok_or_else(|| err("CURVE_NG_STATE_REQUIRED"))?;
            state.stored_balances = q
                .next_state
                .stored_balances
                .into_iter()
                .map(Amount::checked)
                .collect::<Result<_>>()?;
            state.admin_balances = q
                .next_state
                .admin_balances
                .into_iter()
                .map(Amount::checked)
                .collect::<Result<_>>()?;
            state.last_prices_packed = q
                .next_state
                .last_prices_packed
                .into_iter()
                .map(Amount::checked)
                .collect::<Result<_>>()?;
            state.last_d_packed = Amount::checked(q.next_state.last_d_packed)?;
            state.ma_last_time = Amount::checked(q.next_state.ma_last_time)?;
            next_pool.reserve0 = next.assets[0].balance.clone();
            next_pool.reserve1 = next.assets[1].balance.clone();
            next_pool.validate()?;
            return Ok(Swap {
                amount_out: Amount::checked(q.amount_out)?,
                consumed_in: input.clone(),
                remaining_in: Amount::zero(),
                next_pool,
            });
        }
        "curve-plain3-basic" | "curve-plain4-basic" => {
            let q = crate::curve_multi::quote_exact_in(
                &balances,
                &s.amplification.0,
                &scales,
                &fee,
                &BigUint::from(s.admin_fee),
                &input.0,
                i,
                j,
            )?;
            (q.amount_out, q.next_balances)
        }
        _ => return Err(err("MULTI_ASSET_MATH_NOT_IMPLEMENTED")),
    };
    if out == BigUint::from(0u8) {
        return Err(err("INSUFFICIENT_OUTPUT"));
    }
    if next.len() != s.assets.len() {
        return Err(err("MULTI_ASSET_POSTSTATE_LENGTH"));
    }
    let mut next_pool = pool.clone();
    let state = next_pool
        .multi_asset
        .as_mut()
        .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
    for (asset, balance) in state.assets.iter_mut().zip(next) {
        asset.balance = Amount::checked(balance)?;
    }
    next_pool.reserve0 = state.assets[0].balance.clone();
    next_pool.reserve1 = state.assets[1].balance.clone();
    next_pool.validate()?;
    Ok(Swap {
        amount_out: Amount::checked(out)?,
        consumed_in: input.clone(),
        remaining_in: Amount::zero(),
        next_pool,
    })
}
