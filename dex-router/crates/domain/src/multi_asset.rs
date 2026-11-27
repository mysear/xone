use crate::{
    err,
    protocols::{protocol, validate_address, validate_identity},
    wire, Amount, Pool, Result,
};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
pub const COMPOSABLE_V6_PROTOCOL: &str = "balancer-v2-composable-v6";
pub const COMPOSABLE_V6_ADAPTER: &str = "balancer-v2-composable-v6/no-rates-1";
pub const COMPOSABLE_V6_CACHED_ADAPTER: &str = "balancer-v2-composable-v6/cached-rates-1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BalancerComposableV6RateCache {
    pub token_index: u32,
    pub provider: String,
    pub provider_code_sha256: String,
    pub current: Amount,
    pub old: Amount,
    pub duration: u32,
    pub expires: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_refresh_rate: Option<Amount>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BalancerComposableV6State {
    pub total_supply: Amount,
    pub last_join_exit_amp: Amount,
    pub last_join_exit_invariant: Amount,
    pub protocol_swap_fee: Amount,
    pub protocol_yield_fee: Amount,
    pub fee_collector: String,
    pub collector_bpt: Amount,
    pub yield_exempt: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rate_caches: Vec<BalancerComposableV6RateCache>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_snapshot_timestamp: Option<u64>,
}

impl BalancerComposableV6State {
    fn valid_rates(&self, assets: &[MultiAsset], pool: &str, adapter: &str) -> Result<()> {
        if adapter == COMPOSABLE_V6_ADAPTER
            && self.rate_caches.is_empty()
            && self.rate_snapshot_timestamp.is_none()
        {
            return Ok(());
        }
        let fail = || err("COMPOSABLE_V6_RATE_STATE");
        let timestamp = self
            .rate_snapshot_timestamp
            .filter(|&stamp| stamp > 0)
            .ok_or_else(fail)?;
        if adapter != COMPOSABLE_V6_CACHED_ADAPTER || self.rate_caches.is_empty() {
            return Err(fail());
        }
        let mut previous = None;
        for rate in &self.rate_caches {
            let asset = assets.get(rate.token_index as usize).ok_or_else(fail)?;
            validate_identity(&rate.provider, &rate.provider_code_sha256)?;
            let expired = u64::from(rate.expires) < timestamp;
            if expired != rate.observed_refresh_rate.is_some()
                || rate.observed_refresh_rate.as_ref().is_some_and(|value| {
                    value.0.is_zero()
                        || value.0.bits() > 96
                        || timestamp > u64::from(u32::MAX - rate.duration)
                })
                || previous.is_some_and(|i| i >= rate.token_index)
                || asset.token == pool
                || rate.provider == pool
                || rate.current.0.is_zero()
                || rate.old.0.is_zero()
                || rate.current.0.bits() > 96
                || rate.old.0.bits() > 96
                || !(0..=18)
                    .any(|n| asset.scaling.0 == &rate.current.0 * BigUint::from(10u8).pow(n))
            {
                return Err(fail());
            }
            previous = Some(rate.token_index);
        }
        Ok(())
    }

    fn valid(&self, vault_bpt: &Amount) -> Result<()> {
        validate_address(&self.fee_collector)?;
        let one = BigUint::from(1_000_000_000_000_000_000u64);
        if self.total_supply.0.bits() > 256
            || self.total_supply.0 <= vault_bpt.0
            || self.collector_bpt.0 > &self.total_supply.0 - &vault_bpt.0
            || !(BigUint::from(1000u32)..=BigUint::from(5_000_000u32))
                .contains(&self.last_join_exit_amp.0)
            || self.last_join_exit_invariant.0.is_zero()
            || self.last_join_exit_invariant.0.bits() > 233
            || self.protocol_swap_fee.0 > one
            || self.protocol_yield_fee.0 > one
        {
            return Err(err("COMPOSABLE_V6_SUPPLY_STATE"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MultiAsset {
    pub token: String,
    pub balance: Amount,
    pub scaling: Amount,
    pub code_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MultiAssetState {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::managed_v2_state::deserialize_present"
    )]
    pub composable_v6: Option<BalancerComposableV6State>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::managed_v2_state::deserialize_present"
    )]
    pub managed_v2: Option<crate::managed_v2_state::BalancerManagedV2State>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_crypto: Option<crate::curve_crypto_state::CurveCryptoState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_meta: Option<crate::curve_meta_state::CurveMetaState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_fees: Option<crate::managed_fee_state::BalancerManagedState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve_ng: Option<crate::curve_ng_state::CurveNGState>,
    pub code_sha256: String,
    pub vault: String,
    pub vault_code_sha256: String,
    pub pool_id: String,
    pub assets: Vec<MultiAsset>,
    pub weights: Vec<Amount>,
    pub amplification: Amount,
    pub admin_fee: u64,
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
impl Pool {
    pub(crate) fn validate_multi_asset(&self) -> Result<()> {
        let s = self
            .multi_asset
            .as_ref()
            .ok_or_else(|| err("MULTI_ASSET_STATE_REQUIRED"))?;
        let profile = protocol(&self.protocol).ok_or_else(|| err("MULTI_ASSET_PROFILE"))?;
        if self.weighted.is_some()
            || self.stable.is_some()
            || self.v2.is_some()
            || self.v3.is_some()
            || self.singleton_cl.is_some()
            || self.raydium_cpmm.is_some()
            || self.orca_whirlpool.is_some()
            || self.raydium_clmm.is_some()
        {
            return Err(err("MULTI_ASSET_STATE_REQUIRED"));
        }
        let v6 = self.protocol == COMPOSABLE_V6_PROTOCOL;
        let composable = self.protocol == "balancer-v2-composable-regular" || v6;
        if v6 != s.composable_v6.is_some() {
            return Err(err("COMPOSABLE_V6_PROFILE_MISMATCH"));
        }
        if self.protocol == crate::managed_v2_state::PROTOCOL
            || self.protocol == crate::managed_v2_schedule::PROTOCOL
            || self.protocol == crate::managed_v2_bpt_state::PROTOCOL
            || self.protocol == crate::managed_v2_circuit_state::PROTOCOL
        {
            return self.validate_managed_v2();
        };
        if s.managed_v2.is_some() {
            return Err(err("MANAGED_V2_PROFILE_MISMATCH"));
        }
        crate::validate_chain(&self.chain_key)?;
        validate_address(&self.pool_id)?;
        if !self.chain_key.starts_with("evm:")
            || (self.adapter_version != profile.adapter
                && !(v6 && self.adapter_version == COMPOSABLE_V6_CACHED_ADAPTER))
            || self.token_behavior != "standard"
            || !digest(&s.code_sha256)
            || !(2..=38).contains(&s.assets.len())
            || self.fee_denominator != profile.fee_denominator
            || self.fee_numerator >= self.fee_denominator
        {
            return Err(err("MULTI_ASSET_IDENTITY"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for (index, a) in s.assets.iter().enumerate() {
            let has_provider = s.composable_v6.as_ref().is_some_and(|v6| {
                v6.rate_caches
                    .iter()
                    .any(|rate| rate.token_index as usize == index)
            });
            validate_address(&a.token)?;
            if a.balance.0.bits() > 256
                || !seen.insert(&a.token)
                || (a.token == self.pool_id && !composable)
                || !digest(&a.code_sha256)
                || (!has_provider && !(18..=36).any(|n| a.scaling.0 == BigUint::from(10u8).pow(n)))
            {
                return Err(err("MULTI_ASSET_TOKEN"));
            }
        }
        if self.token0 != s.assets[0].token
            || self.token1 != s.assets[1].token
            || self.reserve0 != s.assets[0].balance
            || self.reserve1 != s.assets[1].balance
        {
            return Err(err("MULTI_ASSET_DUPLICATE_FIELDS"));
        }
        if self.protocol != "curve-crypto2-full"
            && self.protocol != "curve-crypto2-ramp"
            && s.curve_crypto.is_some()
        {
            return Err(err("CURVE_CRYPTO_PROFILE_MISMATCH"));
        }
        if self.protocol == "curve-crypto2-full" || self.protocol == "curve-crypto2-ramp" {
            return self.validate_curve_crypto();
        }
        if self.protocol != "balancer-v2-managed-fees" && s.managed_fees.is_some() {
            return Err(err("MANAGED_FEE_PROFILE_MISMATCH"));
        }
        if self.protocol == "balancer-v2-managed-fees" {
            self.validate_managed_fees()?;
        }
        if self.protocol != "curve-metausd-direct" && s.curve_meta.is_some() {
            return Err(err("CURVE_META_PROFILE_MISMATCH"));
        }
        if self.protocol == "curve-metausd-direct" {
            return self.validate_curve_meta();
        }
        if self.protocol == "curve-stableswap-ng" {
            return self.validate_curve_ng();
        }
        if s.curve_ng.is_some() {
            return Err(err("CURVE_NG_PROFILE_MISMATCH"));
        }
        if self.protocol.starts_with("balancer-v2-") {
            let managed = self.protocol == "balancer-v2-managed-snapshot"
                || self.protocol == "balancer-v2-managed-fees";
            let max_fee = if self.protocol == "balancer-v2-managed-fees" {
                800_000_000_000_000_000
            } else {
                100_000_000_000_000_000
            };
            if (!managed && !composable && !(3..=20).contains(&s.assets.len()))
                || (composable && !(3..=6).contains(&s.assets.len()))
            {
                return Err(err("MULTI_WEIGHTED_ASSETS"));
            }
            validate_identity(&s.vault, &s.vault_code_sha256)?;
            if s.pool_id.len() != 66
                || !s.pool_id.starts_with(&self.pool_id)
                || !digest(&s.pool_id[2..])
                || (!composable && &s.pool_id[42..46] != "0001"
                    || composable && &s.pool_id[42..46] != "0000")
                || (!composable && !s.amplification.0.is_zero())
                || s.admin_fee != 0
                || (!composable && s.weights.len() != s.assets.len()
                    || composable && !s.weights.is_empty())
                || !(1_000_000_000_000..=max_fee).contains(&self.fee_numerator)
            {
                return Err(err("MULTI_WEIGHTED_PARAMETERS"));
            }
            let mut sum = BigUint::zero();
            for (i, w) in s.weights.iter().enumerate() {
                if (!managed && w.0 < BigUint::from(10_000_000_000_000_000u64))
                    || (managed
                        && (w.0.is_zero() || w.0 > BigUint::from(1_000_000_000_000_000_000u64)))
                {
                    return Err(err("MULTI_WEIGHTED_WEIGHTS"));
                }
                sum += &w.0;
                if s.assets[i].balance.0.bits() > 112
                    || s.assets[i].token == s.vault
                    || i > 0 && s.assets[i - 1].token >= s.assets[i].token
                {
                    return Err(err("MULTI_WEIGHTED_ASSETS"));
                }
            }
            if !managed && !composable && sum != BigUint::from(1_000_000_000_000_000_000u64) {
                return Err(err("MULTI_WEIGHTED_WEIGHTS"));
            }

            let mut bpt = 0;
            for (i, a) in s.assets.iter().enumerate() {
                if a.balance.0.bits() > 112
                    || a.token == s.vault
                    || i > 0 && s.assets[i - 1].token >= a.token
                {
                    return Err(err("MULTI_WEIGHTED_ASSETS"));
                }
                if a.token == self.pool_id {
                    if a.code_sha256 != s.code_sha256 {
                        return Err(err("COMPOSABLE_BPT_IDENTITY"));
                    }
                    bpt += 1;
                    if a.scaling.0 != BigUint::from(1_000_000_000_000_000_000u64) {
                        return Err(err("COMPOSABLE_BPT_SCALE"));
                    }
                }
            }
            if composable {
                if bpt != 1
                    || s.amplification.0 < BigUint::from(1000u32)
                    || s.amplification.0 > BigUint::from(5_000_000u32)
                {
                    return Err(err("COMPOSABLE_PARAMETERS"));
                }
                if let Some(v6) = &s.composable_v6 {
                    v6.valid_rates(&s.assets, &self.pool_id, &self.adapter_version)?;
                    let bpt = s
                        .assets
                        .iter()
                        .find(|asset| asset.token == self.pool_id)
                        .ok_or_else(|| err("COMPOSABLE_BPT_REQUIRED"))?;
                    v6.valid(&bpt.balance)?;
                }
            } else if bpt != 0 {
                return Err(err("MULTI_WEIGHTED_ASSETS"));
            }
        } else {
            let n = if self.protocol == "curve-plain4-basic" {
                4
            } else {
                3
            };
            if s.assets.len() != n
                || !s.weights.is_empty()
                || !s.vault.is_empty()
                || !s.vault_code_sha256.is_empty()
                || !s.pool_id.is_empty()
                || s.amplification.0 < BigUint::from(100u8)
                || s.amplification.0 >= BigUint::from(100_000_000u64)
                || s.admin_fee != 5_000_000_000
                || !(4_000_000..=100_000_000).contains(&self.fee_numerator)
            {
                return Err(err("MULTI_CURVE_PARAMETERS"));
            }
        }
        Ok(())
    }
}
impl MultiAssetState {
    pub fn to_proto(&self) -> Result<wire::MultiAssetState> {
        let raw = |s: &str, optional: bool| -> Result<Vec<u8>> {
            let b =
                hex::decode(s.trim_start_matches("0x")).map_err(|_| err("MULTI_ASSET_BYTES"))?;
            if b.len() != 32 && !(optional && b.is_empty()) {
                return Err(err("MULTI_ASSET_BYTES_WIDTH"));
            }
            Ok(b)
        };
        let amount = |a: &Amount| -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        };
        Ok(wire::MultiAssetState {
            composable_v6: self
                .composable_v6
                .as_ref()
                .map(|v| {
                    Ok::<_, crate::Error>(wire::BalancerComposableV6State {
                        total_supply: amount(&v.total_supply)?,
                        last_join_exit_amp: amount(&v.last_join_exit_amp)?,
                        last_join_exit_invariant: amount(&v.last_join_exit_invariant)?,
                        protocol_swap_fee: amount(&v.protocol_swap_fee)?,
                        protocol_yield_fee: amount(&v.protocol_yield_fee)?,
                        fee_collector: v.fee_collector.clone(),
                        collector_bpt: amount(&v.collector_bpt)?,
                        yield_exempt: Some(v.yield_exempt),
                        rate_snapshot_timestamp: v.rate_snapshot_timestamp,
                        rate_caches: v
                            .rate_caches
                            .iter()
                            .map(|cache| {
                                Ok::<_, crate::Error>(wire::BalancerComposableV6RateCache {
                                    token_index: cache.token_index,
                                    provider: cache.provider.clone(),
                                    provider_code_sha256: raw(&cache.provider_code_sha256, false)?,
                                    current: amount(&cache.current)?,
                                    old: amount(&cache.old)?,
                                    duration: Some(cache.duration),
                                    expires: Some(cache.expires),
                                    observed_refresh_rate: cache
                                        .observed_refresh_rate
                                        .as_ref()
                                        .map(amount)
                                        .transpose()?
                                        .flatten(),
                                })
                            })
                            .collect::<Result<_>>()?,
                    })
                })
                .transpose()?,
            managed_v2: self.managed_v2.as_ref().map(|m| m.to_proto()).transpose()?,
            curve_crypto: self
                .curve_crypto
                .as_ref()
                .map(|s| s.to_proto())
                .transpose()?,
            curve_meta: self.curve_meta.as_ref().map(|s| s.to_proto()).transpose()?,
            managed_fees: self
                .managed_fees
                .as_ref()
                .map(|s| s.to_proto())
                .transpose()?,
            curve_ng: self.curve_ng.as_ref().map(|s| s.to_proto()).transpose()?,
            code_sha256: raw(&self.code_sha256, false)?,
            vault: self.vault.clone(),
            vault_code_sha256: raw(&self.vault_code_sha256, true)?,
            pool_id: raw(&self.pool_id, true)?,
            assets: self
                .assets
                .iter()
                .map(|a| {
                    Ok(wire::MultiAsset {
                        token: a.token.clone(),
                        balance: amount(&a.balance)?,
                        scaling: amount(&a.scaling)?,
                        code_sha256: raw(&a.code_sha256, false)?,
                    })
                })
                .collect::<Result<_>>()?,
            weights: self
                .weights
                .iter()
                .map(amount)
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect(),
            amplification: amount(&self.amplification)?,
            admin_fee: self.admin_fee,
        })
    }
    pub fn from_proto(s: wire::MultiAssetState) -> Result<Self> {
        fn bytes(v: Vec<u8>, optional: bool) -> Result<String> {
            if v.len() != 32 && !(optional && v.is_empty()) {
                return Err(err("MULTI_ASSET_BYTES_WIDTH"));
            }
            Ok(hex::encode(v))
        }
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(
                &a.ok_or_else(|| err("MULTI_ASSET_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        }
        let id = bytes(s.pool_id, true)?;
        Ok(Self {
            composable_v6: s
                .composable_v6
                .map(|v| {
                    Ok::<_, crate::Error>(BalancerComposableV6State {
                        total_supply: amount(v.total_supply)?,
                        last_join_exit_amp: amount(v.last_join_exit_amp)?,
                        last_join_exit_invariant: amount(v.last_join_exit_invariant)?,
                        protocol_swap_fee: amount(v.protocol_swap_fee)?,
                        protocol_yield_fee: amount(v.protocol_yield_fee)?,
                        fee_collector: v.fee_collector,
                        collector_bpt: amount(v.collector_bpt)?,
                        yield_exempt: v
                            .yield_exempt
                            .ok_or_else(|| err("COMPOSABLE_V6_EXEMPTION_REQUIRED"))?,
                        rate_snapshot_timestamp: v.rate_snapshot_timestamp,
                        rate_caches: v
                            .rate_caches
                            .into_iter()
                            .map(|cache| {
                                let missing = || err("COMPOSABLE_V6_RATE_CACHE_REQUIRED");
                                Ok::<_, crate::Error>(BalancerComposableV6RateCache {
                                    token_index: cache.token_index,
                                    provider: cache.provider,
                                    provider_code_sha256: bytes(cache.provider_code_sha256, false)?,
                                    current: amount(cache.current)?,
                                    old: amount(cache.old)?,
                                    duration: cache.duration.ok_or_else(missing)?,
                                    expires: cache.expires.ok_or_else(missing)?,
                                    observed_refresh_rate: cache
                                        .observed_refresh_rate
                                        .map(|rate| amount(Some(rate)))
                                        .transpose()?,
                                })
                            })
                            .collect::<Result<_>>()?,
                    })
                })
                .transpose()?,
            managed_v2: s
                .managed_v2
                .map(crate::managed_v2_state::BalancerManagedV2State::from_proto)
                .transpose()?,
            curve_crypto: s
                .curve_crypto
                .map(crate::curve_crypto_state::CurveCryptoState::from_proto)
                .transpose()?,
            curve_meta: s
                .curve_meta
                .map(crate::curve_meta_state::CurveMetaState::from_proto)
                .transpose()?,
            managed_fees: s
                .managed_fees
                .map(crate::managed_fee_state::BalancerManagedState::from_proto)
                .transpose()?,
            curve_ng: s
                .curve_ng
                .map(crate::curve_ng_state::CurveNGState::from_proto)
                .transpose()?,
            code_sha256: bytes(s.code_sha256, false)?,
            vault: s.vault,
            vault_code_sha256: bytes(s.vault_code_sha256, true)?,
            pool_id: if id.is_empty() { id } else { format!("0x{id}") },
            assets: s
                .assets
                .into_iter()
                .map(|a| {
                    Ok(MultiAsset {
                        token: a.token,
                        balance: amount(a.balance)?,
                        scaling: amount(a.scaling)?,
                        code_sha256: bytes(a.code_sha256, false)?,
                    })
                })
                .collect::<Result<_>>()?,
            weights: s
                .weights
                .into_iter()
                .map(|a| amount(Some(a)))
                .collect::<Result<_>>()?,
            amplification: amount(s.amplification)?,
            admin_fee: s.admin_fee,
        })
    }
}
