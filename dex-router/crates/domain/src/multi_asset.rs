use crate::{
    err,
    protocols::{protocol, validate_address, validate_identity},
    wire, Amount, Pool, Result,
};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
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
        if self.protocol == crate::managed_v2_state::PROTOCOL {
            return self.validate_managed_v2();
        };
        if s.managed_v2.is_some() {
            return Err(err("MANAGED_V2_PROFILE_MISMATCH"));
        }
        crate::validate_chain(&self.chain_key)?;
        validate_address(&self.pool_id)?;
        if !self.chain_key.starts_with("evm:")
            || self.adapter_version != profile.adapter
            || self.token_behavior != "standard"
            || !digest(&s.code_sha256)
            || !(2..=38).contains(&s.assets.len())
            || self.fee_denominator != profile.fee_denominator
            || self.fee_numerator >= self.fee_denominator
        {
            return Err(err("MULTI_ASSET_IDENTITY"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for a in &s.assets {
            validate_address(&a.token)?;
            if a.balance.0.bits() > 256
                || !seen.insert(&a.token)
                || (a.token == self.pool_id && self.protocol != "balancer-v2-composable-regular")
                || !digest(&a.code_sha256)
                || !(18..=36).any(|n| a.scaling.0 == BigUint::from(10u8).pow(n))
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
            let composable = self.protocol == "balancer-v2-composable-regular";
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
