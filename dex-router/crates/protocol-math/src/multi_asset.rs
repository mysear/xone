use crate::Swap;
use num_bigint::BigUint;
use num_traits::Zero;
use std::borrow::Cow;
use xone_domain::multi_asset::COMPOSABLE_V6_PROTOCOL;
use xone_domain::{err, Amount, Pool, Result};

fn composable_v6_callback_state(pool: &Pool) -> Result<Cow<'_, Pool>> {
    use crate::balancer_composable::{refresh_token_rate_caches, CachedRate, TokenRateCache};
    let Some(v6) = pool
        .multi_asset
        .as_ref()
        .and_then(|s| s.composable_v6.as_ref())
    else {
        return Ok(Cow::Borrowed(pool));
    };
    if !v6
        .rate_caches
        .iter()
        .any(|cache| cache.observed_refresh_rate.is_some())
    {
        return Ok(Cow::Borrowed(pool));
    }
    let mut prepared = pool.clone();
    let state = prepared
        .multi_asset
        .as_mut()
        .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
    let v6 = state
        .composable_v6
        .as_mut()
        .ok_or_else(|| err("COMPOSABLE_V6_RATE_STATE"))?;
    let timestamp = v6
        .rate_snapshot_timestamp
        .ok_or_else(|| err("COMPOSABLE_V6_RATE_STATE"))?;
    let bpt = state
        .assets
        .iter()
        .position(|asset| asset.token == pool.pool_id)
        .ok_or_else(|| err("COMPOSABLE_BPT_REQUIRED"))?;
    let mut decimal_scales: Vec<_> = state
        .assets
        .iter()
        .map(|asset| asset.scaling.0.clone())
        .collect();
    let mut caches = vec![None; state.assets.len()];
    let mut observed = vec![None; state.assets.len()];
    let one = BigUint::from(1_000_000_000_000_000_000u64);
    for cache in &mut v6.rate_caches {
        let i = cache.token_index as usize;
        decimal_scales[i] = &decimal_scales[i] / &cache.current.0 * &one;
        caches[i] = Some(TokenRateCache {
            rate: CachedRate {
                current: cache.current.0.clone(),
                old: cache.old.0.clone(),
            },
            duration: cache.duration,
            expires: cache.expires,
        });
        observed[i] = cache.observed_refresh_rate.take().map(|rate| rate.0);
    }
    let (updated, effective) =
        refresh_token_rate_caches(&decimal_scales, &caches, bpt, timestamp, |i| {
            observed[i]
                .take()
                .ok_or_else(|| err("COMPOSABLE_V6_RATE_STATE"))
        })?;
    for cache in &mut v6.rate_caches {
        let i = cache.token_index as usize;
        let updated = updated[i]
            .as_ref()
            .ok_or_else(|| err("COMPOSABLE_V6_RATE_STATE"))?;
        cache.current = Amount(updated.rate.current.clone());
        cache.expires = updated.expires;
    }
    for (asset, scale) in state.assets.iter_mut().zip(effective) {
        asset.scaling = Amount(scale);
    }
    Ok(Cow::Owned(prepared))
}

#[allow(clippy::too_many_arguments)]
fn quote_composable_v6_bpt(
    pool: &Pool,
    amount: &Amount,
    i: usize,
    j: usize,
    bpt: usize,
    balances: &[BigUint],
    scales: &[BigUint],
    kind: crate::balancer_composable::BptSwapKind,
) -> Result<Swap> {
    use crate::balancer_composable::{quote_bpt_swap, CachedRate, SupplyInput, YieldFeePolicy};
    let mut next_pool = pool.clone();
    let state = next_pool
        .multi_asset
        .as_mut()
        .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
    let v6 = state
        .composable_v6
        .as_mut()
        .ok_or_else(|| err("COMPOSABLE_V6_SUPPLY_STATE"))?;
    let mut rates = vec![None; balances.len()];
    for cache in &v6.rate_caches {
        rates[cache.token_index as usize] = Some(CachedRate {
            current: cache.current.0.clone(),
            old: cache.old.0.clone(),
        });
    }
    let input = SupplyInput {
        balances,
        scales,
        bpt_index: bpt,
        rates: &rates,
        yield_fee_policy: YieldFeePolicy::PoolWide {
            exempt: v6.yield_exempt,
        },
        total_supply: &v6.total_supply.0,
        last_join_exit_amp: &v6.last_join_exit_amp.0,
        last_post_join_exit_invariant: &v6.last_join_exit_invariant.0,
        protocol_swap_fee: &v6.protocol_swap_fee.0,
        protocol_yield_fee: &v6.protocol_yield_fee.0,
    };
    let out = quote_bpt_swap(
        &input,
        &state.amplification.0,
        &BigUint::from(pool.fee_numerator),
        &amount.0,
        i,
        j,
        kind,
    )?;
    for (asset, balance) in state.assets.iter_mut().zip(out.next_balances) {
        asset.balance = Amount::checked(balance)?;
    }
    v6.total_supply = Amount::checked(out.next_total_supply)?;
    v6.last_join_exit_amp = Amount::checked(out.last_join_exit_amp)?;
    v6.last_join_exit_invariant = Amount::checked(out.last_join_exit_invariant)?;
    for cache in &mut v6.rate_caches {
        let next = out.next_rates[cache.token_index as usize]
            .as_ref()
            .ok_or_else(|| err("COMPOSABLE_V6_RATE_STATE"))?;
        cache.old = Amount(next.old.clone());
    }
    v6.collector_bpt = Amount::checked(
        &v6.collector_bpt.0 + out.protocol_bpt_before_swap + out.protocol_bpt_after_swap,
    )?;
    next_pool.reserve0 = state.assets[0].balance.clone();
    next_pool.reserve1 = state.assets[1].balance.clone();
    next_pool.validate()?;
    Ok(Swap {
        amount_out: Amount::checked(out.amount_out)?,
        consumed_in: Amount::checked(out.amount_in)?,
        remaining_in: Amount::zero(),
        next_pool,
    })
}

fn meta_underlying_states(
    pool: &Pool,
) -> Result<(
    Vec<BigUint>,
    Vec<BigUint>,
    Vec<BigUint>,
    BigUint,
    BigUint,
    BigUint,
    crate::curve_multi::BaseLiquidityState,
)> {
    pool.validate()?;
    if pool.protocol != "curve-metausd-direct" {
        return Err(err("CURVE_META_UNDERLYING_UNSUPPORTED"));
    }
    let s = pool
        .multi_asset
        .as_ref()
        .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
    let m = s
        .curve_meta
        .as_ref()
        .ok_or_else(|| err("CURVE_META_STATE_REQUIRED"))?;
    let base = m
        .base_pool_state
        .as_ref()
        .ok_or_else(|| err("CURVE_META_BASE_POOL_STATE_REQUIRED"))?;
    Ok((
        s.assets.iter().map(|a| a.balance.0.clone()).collect(),
        m.admin_balances.iter().map(|a| a.0.clone()).collect(),
        s.assets.iter().map(|a| a.scaling.0.clone()).collect(),
        s.amplification.0.clone(),
        BigUint::from(pool.fee_numerator),
        BigUint::from(s.admin_fee),
        crate::curve_multi::BaseLiquidityState {
            balances: base.balances.iter().map(|a| a.0.clone()).collect(),
            admin_balances: base.admin_balances.iter().map(|a| a.0.clone()).collect(),
            rates: base.rates.iter().map(|a| a.0.clone()).collect(),
            amplification: base.amplification.0.clone(),
            fee: BigUint::from(base.fee),
            admin_fee: BigUint::from(base.admin_fee),
            total_supply: base.total_supply.0.clone(),
        },
    ))
}

pub fn quote_meta_underlying_exact_in(
    pool: &Pool,
    input: &Amount,
    index_in: usize,
    index_out: usize,
) -> Result<crate::curve_multi::MetaUnderlyingQuote> {
    let (balances, admin, rates, amp, fee, admin_fee, base) = meta_underlying_states(pool)?;
    crate::curve_multi::quote_meta_underlying_exact_in(
        &balances, &admin, &rates, &amp, &fee, &admin_fee, &base, &input.0, index_in, index_out,
    )
}

pub fn quote_meta_underlying_exact_out(
    pool: &Pool,
    output: &Amount,
    index_in: usize,
    index_out: usize,
) -> Result<crate::curve_multi::MetaUnderlyingQuote> {
    let (balances, admin, rates, amp, fee, admin_fee, base) = meta_underlying_states(pool)?;
    crate::curve_multi::quote_meta_underlying_exact_out(
        &balances, &admin, &rates, &amp, &fee, &admin_fee, &base, &output.0, index_in, index_out,
    )
}
/// Token-selected multi-asset swaps mutate a single full physical balance vector.
/// Callers may not manufacture independently mutable pair projections.
pub fn quote_exact_in_tokens(
    pool: &Pool,
    input: &Amount,
    token_in: &str,
    token_out: &str,
) -> Result<Swap> {
    if pool.protocol == xone_domain::managed_v2_state::PROTOCOL
        || pool.protocol == xone_domain::managed_v2_schedule::PROTOCOL
        || pool.protocol == xone_domain::managed_v2_bpt_state::PROTOCOL
        || pool.protocol == xone_domain::managed_v2_circuit_state::PROTOCOL
    {
        return crate::managed_v2_pool::quote_exact_in_tokens(pool, input, token_in, token_out);
    }
    pool.validate()?;
    let callback = composable_v6_callback_state(pool)?;
    let pool = callback.as_ref();
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
    let mut managed_fee_state = None;
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
        "balancer-v2-managed-fees" => {
            let weights = s.weights.iter().map(|w| w.0.clone()).collect::<Vec<_>>();
            let fees = s
                .managed_fees
                .as_ref()
                .ok_or_else(|| err("MANAGED_FEE_STATE_REQUIRED"))?;
            let q = crate::balancer_managed_fees::quote_exact_in(
                &balances,
                &weights,
                &scales,
                &fee,
                &input.0,
                i,
                j,
                &crate::balancer_managed_fees::ManagedFeeState {
                    total_supply: fees.total_supply.0.clone(),
                    protocol_swap_fee: fees.protocol_swap_fee.0.clone(),
                    management_swap_fee: fees.management_swap_fee.0.clone(),
                    owner_balance: fees.owner_balance.0.clone(),
                    protocol_collector_balance: fees.protocol_collector_balance.0.clone(),
                    same_recipient: fees.owner == fees.protocol_collector,
                },
            )?;
            managed_fee_state = Some(q.next_state);
            (q.amount_out, q.next_balances)
        }
        "balancer-v2-composable-regular" | COMPOSABLE_V6_PROTOCOL => {
            let bpt = s
                .assets
                .iter()
                .position(|a| a.token == pool.pool_id)
                .ok_or_else(|| err("COMPOSABLE_BPT_REQUIRED"))?;
            if pool.protocol == COMPOSABLE_V6_PROTOCOL && (i == bpt || j == bpt) {
                return quote_composable_v6_bpt(
                    pool,
                    input,
                    i,
                    j,
                    bpt,
                    &balances,
                    &scales,
                    crate::balancer_composable::BptSwapKind::GivenIn,
                );
            }
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
    if let Some(next_fee_state) = managed_fee_state {
        let fees = state
            .managed_fees
            .as_mut()
            .ok_or_else(|| err("MANAGED_FEE_STATE_REQUIRED"))?;
        fees.total_supply = Amount::checked(next_fee_state.total_supply)?;
        fees.owner_balance = Amount::checked(next_fee_state.owner_balance)?;
        fees.protocol_collector_balance =
            Amount::checked(next_fee_state.protocol_collector_balance)?;
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

/// Explicit-token immutable WeightedPool target. A single complete physical
/// vector is mutated; independently mutable pair projections are never created.
pub fn quote_exact_out_tokens(
    pool: &Pool,
    output: &Amount,
    token_in: &str,
    token_out: &str,
) -> Result<Swap> {
    if crate::managed_v2_pool::supports_regular_exact_out(&pool.protocol) {
        return crate::managed_v2_pool::quote_exact_out_tokens(pool, output, token_in, token_out);
    }
    pool.validate()?;
    if !matches!(
        pool.protocol.as_str(),
        "balancer-v2-weighted-multi"
            | "balancer-v2-managed-snapshot"
            | "balancer-v2-managed-fees"
            | "balancer-v2-composable-regular"
            | COMPOSABLE_V6_PROTOCOL
            | "curve-plain3-basic"
            | "curve-plain4-basic"
            | "curve-metausd-direct"
            | "curve-stableswap-ng"
    ) {
        return Err(err("EXACT_OUT_PROTOCOL_UNSUPPORTED"));
    }
    let callback = composable_v6_callback_state(pool)?;
    let pool = callback.as_ref();
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
    let balances = s
        .assets
        .iter()
        .map(|a| a.balance.0.clone())
        .collect::<Vec<_>>();
    let weights = s.weights.iter().map(|w| w.0.clone()).collect::<Vec<_>>();
    let scales = s
        .assets
        .iter()
        .map(|a| a.scaling.0.clone())
        .collect::<Vec<_>>();
    if pool.protocol == "curve-stableswap-ng" {
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
        let q = crate::curve_ng::quote_full_exact_out(
            &balances,
            &s.amplification.0,
            &scales,
            &BigUint::from(pool.fee_numerator),
            &BigUint::from(s.admin_fee),
            &ng.offpeg_fee_multiplier.0,
            &output.0,
            i,
            j,
            &native,
        )?;
        let mut next_pool = pool.clone();
        let next = next_pool
            .multi_asset
            .as_mut()
            .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
        for (asset, balance) in next.assets.iter_mut().zip(q.next_balances) {
            asset.balance = Amount::checked(balance)?;
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
            amount_out: output.clone(),
            consumed_in: Amount::checked(q.amount_in)?,
            remaining_in: Amount::zero(),
            next_pool,
        });
    }
    let mut meta_admin_fee_amount = None;
    let mut managed_fee_state = None;

    let q = if matches!(
        pool.protocol.as_str(),
        "curve-plain3-basic" | "curve-plain4-basic"
    ) {
        let q = crate::curve_multi::quote_exact_out(
            &balances,
            &s.amplification.0,
            &scales,
            &BigUint::from(pool.fee_numerator),
            &BigUint::from(s.admin_fee),
            &output.0,
            i,
            j,
        )?;
        crate::balancer_multi::MultiExactOutQuote {
            amount_in: q.amount_in,
            next_balances: q.next_balances,
        }
    } else if matches!(
        pool.protocol.as_str(),
        "balancer-v2-composable-regular" | COMPOSABLE_V6_PROTOCOL
    ) {
        let bpt = s
            .assets
            .iter()
            .position(|a| a.token == pool.pool_id)
            .ok_or_else(|| err("COMPOSABLE_BPT_REQUIRED"))?;
        if pool.protocol == COMPOSABLE_V6_PROTOCOL && (i == bpt || j == bpt) {
            return quote_composable_v6_bpt(
                pool,
                output,
                i,
                j,
                bpt,
                &balances,
                &scales,
                crate::balancer_composable::BptSwapKind::GivenOut,
            );
        }
        let q = crate::balancer_composable::quote_exact_out(
            &balances,
            &s.amplification.0,
            &scales,
            &BigUint::from(pool.fee_numerator),
            &output.0,
            i,
            j,
            bpt,
        )?;
        crate::balancer_multi::MultiExactOutQuote {
            amount_in: q.amount_in,
            next_balances: q.next_balances,
        }
    } else if pool.protocol == "balancer-v2-managed-snapshot" {
        crate::balancer_multi::quote_managed_snapshot_exact_out(
            &balances,
            &weights,
            &scales,
            &BigUint::from(pool.fee_numerator),
            &output.0,
            i,
            j,
            &crate::balancer_multi::ManagedSwapGuards {
                swaps_enabled: true,
                protocol_swap_fee: BigUint::zero(),
                management_swap_fee: BigUint::zero(),
            },
        )?
    } else if pool.protocol == "balancer-v2-managed-fees" {
        let fees = s
            .managed_fees
            .as_ref()
            .ok_or_else(|| err("MANAGED_FEE_STATE_REQUIRED"))?;
        let q = crate::balancer_managed_fees::quote_exact_out(
            &balances,
            &weights,
            &scales,
            &BigUint::from(pool.fee_numerator),
            &output.0,
            i,
            j,
            &crate::balancer_managed_fees::ManagedFeeState {
                total_supply: fees.total_supply.0.clone(),
                protocol_swap_fee: fees.protocol_swap_fee.0.clone(),
                management_swap_fee: fees.management_swap_fee.0.clone(),
                owner_balance: fees.owner_balance.0.clone(),
                protocol_collector_balance: fees.protocol_collector_balance.0.clone(),
                same_recipient: fees.owner == fees.protocol_collector,
            },
        )?;
        managed_fee_state = Some(q.next_state);
        crate::balancer_multi::MultiExactOutQuote {
            amount_in: q.amount_in,
            next_balances: q.next_balances,
        }
    } else if pool.protocol == "curve-metausd-direct" {
        let m = s
            .curve_meta
            .as_ref()
            .ok_or_else(|| err("CURVE_META_STATE_REQUIRED"))?;
        let q = crate::curve_multi::quote_meta_exact_out(
            &balances,
            &s.amplification.0,
            &scales[0],
            &m.base_virtual_price.0,
            &BigUint::from(pool.fee_numerator),
            &BigUint::from(s.admin_fee),
            &output.0,
            i,
            j,
        )?;
        meta_admin_fee_amount = Some(q.admin_fee_amount.clone());
        crate::balancer_multi::MultiExactOutQuote {
            amount_in: q.amount_in,
            next_balances: q.next_balances,
        }
    } else {
        crate::balancer_multi::quote_exact_out(
            &balances,
            &weights,
            &scales,
            &BigUint::from(pool.fee_numerator),
            &output.0,
            i,
            j,
        )?
    };
    let mut next_pool = pool.clone();
    let next = next_pool
        .multi_asset
        .as_mut()
        .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
    for (asset, balance) in next.assets.iter_mut().zip(q.next_balances) {
        asset.balance = Amount::checked(balance)?;
    }
    if let Some(next_fee_state) = managed_fee_state {
        let fees = next
            .managed_fees
            .as_mut()
            .ok_or_else(|| err("MANAGED_FEE_STATE_REQUIRED"))?;
        fees.total_supply = Amount::checked(next_fee_state.total_supply)?;
        fees.owner_balance = Amount::checked(next_fee_state.owner_balance)?;
        fees.protocol_collector_balance =
            Amount::checked(next_fee_state.protocol_collector_balance)?;
    }
    if let Some(admin_fee_amount) = meta_admin_fee_amount {
        let m = next
            .curve_meta
            .as_mut()
            .ok_or_else(|| err("CURVE_META_STATE_REQUIRED"))?;
        if m.stored_balances[j].0 < output.0 {
            return Err(err("CURVE_META_BALANCE_ACCOUNTING"));
        }
        m.stored_balances[i] = Amount::checked(&m.stored_balances[i].0 + &q.amount_in)?;
        m.stored_balances[j] = Amount::checked(&m.stored_balances[j].0 - &output.0)?;
        m.admin_balances[j] = Amount::checked(&m.admin_balances[j].0 + admin_fee_amount)?;
    }
    next_pool.reserve0 = next.assets[0].balance.clone();
    next_pool.reserve1 = next.assets[1].balance.clone();
    next_pool.validate()?;
    Ok(Swap {
        amount_out: output.clone(),
        consumed_in: Amount::checked(q.amount_in)?,
        remaining_in: Amount::zero(),
        next_pool,
    })
}
