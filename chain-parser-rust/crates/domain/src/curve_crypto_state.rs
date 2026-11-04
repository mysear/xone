use crate::{err, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
pub const LP_RUNTIME_SHA256: &str =
    "d3f43d24527d6a8ed0fa9f24d110ad7cc9325f40da335ed327e8e0e002a8694d";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CurveCryptoRamp {
    pub initial_amp: Amount,
    pub initial_gamma: Amount,
    pub initial_time: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CurveCryptoState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp: Option<CurveCryptoRamp>,
    pub gamma: Amount,
    pub out_fee: Amount,
    pub fee_gamma: Amount,
    pub allowed_extra_profit: Amount,
    pub adjustment_step: Amount,
    pub invariant: Amount,
    pub price_scale: Amount,
    pub price_oracle: Amount,
    pub last_prices: Amount,
    pub xcp_profit: Amount,
    pub xcp_profit_a: Amount,
    pub virtual_price: Amount,
    pub total_supply: Amount,
    pub admin_receiver_balance: Amount,
    pub ma_half_time: u64,
    pub last_prices_timestamp: u64,
    pub not_adjusted: bool,
    pub future_a_gamma_time: u64,
    pub lp_token: String,
    pub admin_receiver: String,
    pub timestamp: u64,
    pub block_number: u64,
    pub lp_token_code_sha256: String,
    pub raw_balances: Vec<Amount>,
    pub block_hash: String,
}
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
impl Pool {
    pub(crate) fn validate_curve_crypto(&self) -> Result<()> {
        let s = self
            .multi_asset
            .as_ref()
            .ok_or_else(|| err("CURVE_CRYPTO_STATE_REQUIRED"))?;
        let c = s
            .curve_crypto
            .as_ref()
            .ok_or_else(|| err("CURVE_CRYPTO_STATE_REQUIRED"))?;
        if s.curve_ng.is_some()
            || s.curve_meta.is_some()
            || s.managed_fees.is_some()
            || s.assets.len() != 2
            || !s.weights.is_empty()
            || !s.vault.is_empty()
            || !s.vault_code_sha256.is_empty()
            || !s.pool_id.is_empty()
            || c.raw_balances.len() != 2
            || c.timestamp == 0
            || c.timestamp.checked_mul(1000).is_none()
            || c.last_prices_timestamp > c.timestamp
            || self.protocol == "curve-crypto2-full"
                && (c.future_a_gamma_time > 1 || c.ramp.is_some())
        {
            return Err(err("CURVE_CRYPTO_PARAMETERS"));
        }
        if self.protocol == "curve-crypto2-ramp" {
            let r = c
                .ramp
                .as_ref()
                .ok_or_else(|| err("CURVE_CRYPTO_RAMP_REQUIRED"))?;
            if r.initial_time > c.timestamp
                || r.initial_time.checked_mul(1000).is_none()
                || c.future_a_gamma_time > 1
                    && (r.initial_time > c.future_a_gamma_time
                        || c.future_a_gamma_time.checked_mul(1000).is_none())
            {
                return Err(err("CURVE_CRYPTO_RAMP_TIME"));
            }
            if r.initial_amp.0.bits() > 128
                || r.initial_gamma.0.bits() > 128
                || r.initial_amp.0 < BigUint::from(4000u64)
                || r.initial_amp.0 > BigUint::from(4_000_000_000u64)
                || r.initial_gamma.0 < BigUint::from(10_000_000_000u64)
                || r.initial_gamma.0 > BigUint::from(20_000_000_000_000_000u64)
            {
                return Err(err("CURVE_CRYPTO_RAMP_PARAMETER_DOMAIN"));
            }
        }
        if c.block_hash.len() != 66
            || !c.block_hash.starts_with("0x")
            || !digest(&c.block_hash[2..])
            || c.block_hash[2..].bytes().all(|b| b == b'0')
        {
            return Err(err("CURVE_CRYPTO_BLOCK_HASH"));
        }
        crate::protocols::validate_address(&c.lp_token)?;
        crate::protocols::validate_address(&c.admin_receiver)?;
        let zero = format!("0x{}", "0".repeat(40));
        if c.lp_token == zero
            || c.lp_token == self.pool_id
            || c.lp_token == s.assets[0].token
            || c.lp_token == s.assets[1].token
            || c.lp_token_code_sha256 != LP_RUNTIME_SHA256
            || c.admin_receiver == self.pool_id
            || c.admin_receiver == c.lp_token
        {
            return Err(err("CURVE_CRYPTO_LP_IDENTITY"));
        }
        for a in [
            &c.gamma,
            &c.out_fee,
            &c.fee_gamma,
            &c.allowed_extra_profit,
            &c.adjustment_step,
            &c.invariant,
            &c.price_scale,
            &c.price_oracle,
            &c.last_prices,
            &c.xcp_profit,
            &c.xcp_profit_a,
            &c.virtual_price,
            &c.total_supply,
            &c.admin_receiver_balance,
            &s.amplification,
        ] {
            if a.0.bits() > 256 {
                return Err(err("AMOUNT_OVERFLOW"));
            }
        }
        let u = |x: u64| BigUint::from(x);
        let wad = u(1_000_000_000_000_000_000);
        if s.amplification.0 < u(4000)
            || s.amplification.0 > u(4_000_000_000)
            || c.gamma.0 < u(10_000_000_000)
            || c.gamma.0 > u(20_000_000_000_000_000)
            || c.out_fee.0 < u(500_000)
            || c.out_fee.0 > u(10_000_000_000)
            || c.out_fee.0 < u(self.fee_numerator)
            || s.admin_fee > 10_000_000_000
            || c.fee_gamma.0.is_zero()
            || c.fee_gamma.0 >= wad
            || c.allowed_extra_profit.0 > wad
            || c.adjustment_step.0 > wad
            || c.ma_half_time == 0
            || c.ma_half_time >= 604800
        {
            return Err(err("CURVE_CRYPTO_PARAMETER_DOMAIN"));
        }
        for a in [
            &c.invariant,
            &c.price_scale,
            &c.price_oracle,
            &c.last_prices,
            &c.virtual_price,
            &c.total_supply,
        ] {
            if a.0.is_zero() {
                return Err(err("CURVE_CRYPTO_PRICING_DOMAIN"));
            }
        }
        if c.admin_receiver_balance.0 > c.total_supply.0
            || c.admin_receiver == zero && !c.admin_receiver_balance.0.is_zero()
        {
            return Err(err("CURVE_CRYPTO_LP_LEDGER"));
        }
        for (i, a) in c.raw_balances.iter().enumerate() {
            if a.0.bits() > 256 || a.0 < s.assets[i].balance.0 {
                return Err(err("CURVE_CRYPTO_RAW_BALANCE"));
            }
        }
        Ok(())
    }
}
impl CurveCryptoState {
    pub fn to_proto(&self) -> Result<wire::CurveCryptoState> {
        fn bytes(s: &str) -> Result<Vec<u8>> {
            let b = hex::decode(s.trim_start_matches("0x"))
                .map_err(|_| err("CURVE_CRYPTO_BYTES_WIDTH"))?;
            if b.len() != 32 {
                return Err(err("CURVE_CRYPTO_BYTES_WIDTH"));
            }
            Ok(b)
        }
        fn amount(a: &Amount) -> Result<wire::Amount> {
            Ok(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            })
        }
        Ok(wire::CurveCryptoState {
            ramp: self
                .ramp
                .as_ref()
                .map(|r| -> Result<_> {
                    Ok(wire::CurveCryptoRamp {
                        initial_amp: Some(amount(&r.initial_amp)?),
                        initial_gamma: Some(amount(&r.initial_gamma)?),
                        initial_time: r.initial_time,
                    })
                })
                .transpose()?,
            gamma: Some(amount(&self.gamma)?),
            out_fee: Some(amount(&self.out_fee)?),
            fee_gamma: Some(amount(&self.fee_gamma)?),
            allowed_extra_profit: Some(amount(&self.allowed_extra_profit)?),
            adjustment_step: Some(amount(&self.adjustment_step)?),
            invariant: Some(amount(&self.invariant)?),
            price_scale: Some(amount(&self.price_scale)?),
            price_oracle: Some(amount(&self.price_oracle)?),
            last_prices: Some(amount(&self.last_prices)?),
            xcp_profit: Some(amount(&self.xcp_profit)?),
            xcp_profit_a: Some(amount(&self.xcp_profit_a)?),
            virtual_price: Some(amount(&self.virtual_price)?),
            total_supply: Some(amount(&self.total_supply)?),
            admin_receiver_balance: Some(amount(&self.admin_receiver_balance)?),
            ma_half_time: self.ma_half_time,
            last_prices_timestamp: self.last_prices_timestamp,
            not_adjusted: self.not_adjusted,
            future_a_gamma_time: self.future_a_gamma_time,
            lp_token: self.lp_token.clone(),
            admin_receiver: self.admin_receiver.clone(),
            timestamp: self.timestamp,
            block_number: self.block_number,
            lp_token_code_sha256: bytes(&self.lp_token_code_sha256)?,
            block_hash: bytes(&self.block_hash)?,
            raw_balances: self
                .raw_balances
                .iter()
                .map(amount)
                .collect::<Result<_>>()?,
        })
    }
    pub fn from_proto(s: wire::CurveCryptoState) -> Result<Self> {
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(
                &a.ok_or_else(|| err("CURVE_CRYPTO_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        }
        if s.lp_token_code_sha256.len() != 32 || s.block_hash.len() != 32 {
            return Err(err("CURVE_CRYPTO_BYTES_WIDTH"));
        }
        Ok(Self {
            ramp: s
                .ramp
                .map(|r| -> Result<_> {
                    Ok(CurveCryptoRamp {
                        initial_amp: amount(r.initial_amp)?,
                        initial_gamma: amount(r.initial_gamma)?,
                        initial_time: r.initial_time,
                    })
                })
                .transpose()?,
            gamma: amount(s.gamma)?,
            out_fee: amount(s.out_fee)?,
            fee_gamma: amount(s.fee_gamma)?,
            allowed_extra_profit: amount(s.allowed_extra_profit)?,
            adjustment_step: amount(s.adjustment_step)?,
            invariant: amount(s.invariant)?,
            price_scale: amount(s.price_scale)?,
            price_oracle: amount(s.price_oracle)?,
            last_prices: amount(s.last_prices)?,
            xcp_profit: amount(s.xcp_profit)?,
            xcp_profit_a: amount(s.xcp_profit_a)?,
            virtual_price: amount(s.virtual_price)?,
            total_supply: amount(s.total_supply)?,
            admin_receiver_balance: amount(s.admin_receiver_balance)?,
            ma_half_time: s.ma_half_time,
            last_prices_timestamp: s.last_prices_timestamp,
            not_adjusted: s.not_adjusted,
            future_a_gamma_time: s.future_a_gamma_time,
            lp_token: s.lp_token,
            admin_receiver: s.admin_receiver,
            timestamp: s.timestamp,
            block_number: s.block_number,
            lp_token_code_sha256: hex::encode(s.lp_token_code_sha256),
            block_hash: format!("0x{}", hex::encode(s.block_hash)),
            raw_balances: s
                .raw_balances
                .into_iter()
                .map(|v| amount(Some(v)))
                .collect::<Result<_>>()?,
        })
    }
}

/// LP fees deposited into another candidate pool change that pool's actual
/// balance. Until the plan models this cross-pool mutation, reject the scope.
pub fn validate_crypto_dependencies<'a>(pools: impl IntoIterator<Item = &'a Pool>) -> Result<()> {
    let rows = pools.into_iter().collect::<Vec<_>>();
    if !rows.iter().any(|p| {
        p.multi_asset
            .as_ref()
            .is_some_and(|s| s.curve_crypto.is_some() && s.admin_fee != 0)
    }) {
        return Ok(());
    }
    let receivers = rows
        .iter()
        .map(|p| (p.pool_id.as_str(), *p))
        .collect::<std::collections::BTreeMap<_, _>>();
    for p in &rows {
        let Some(c) = p.multi_asset.as_ref().and_then(|s| s.curve_crypto.as_ref()) else {
            continue;
        };
        if p.multi_asset.as_ref().is_some_and(|s| s.admin_fee == 0)
            || c.admin_receiver == format!("0x{}", "0".repeat(40))
        {
            continue;
        }
        if let Some(receiver) = receivers.get(c.admin_receiver.as_str()) {
            if receiver.token0 == c.lp_token
                || receiver.token1 == c.lp_token
                || receiver
                    .multi_asset
                    .as_ref()
                    .is_some_and(|s| s.assets.iter().any(|a| a.token == c.lp_token))
            {
                return Err(err("CURVE_CRYPTO_MUTABLE_LP_DEPENDENCY"));
            }
        }
    }
    Ok(())
}
