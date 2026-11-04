use crate::{err, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::One;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CurveNGState {
    pub stored_balances: Vec<Amount>,
    pub admin_balances: Vec<Amount>,
    pub last_prices_packed: Vec<Amount>,
    pub last_d_packed: Amount,
    pub ma_last_time: Amount,
    pub ma_exp_time: u64,
    pub d_ma_time: u64,
    pub timestamp: u64,
    pub block_number: u64,
    pub block_hash: String,
    pub offpeg_fee_multiplier: Amount,
}
impl Pool {
    pub(crate) fn validate_curve_ng(&self) -> Result<()> {
        let s = self
            .multi_asset
            .as_ref()
            .ok_or_else(|| err("CURVE_NG_STATE_REQUIRED"))?;
        let g = s
            .curve_ng
            .as_ref()
            .ok_or_else(|| err("CURVE_NG_STATE_REQUIRED"))?;
        let n = s.assets.len();
        if !(2..=8).contains(&n)
            || !s.weights.is_empty()
            || !s.vault.is_empty()
            || !s.vault_code_sha256.is_empty()
            || !s.pool_id.is_empty()
            || g.stored_balances.len() != n
            || g.admin_balances.len() != n
            || g.last_prices_packed.len() != n - 1
            || g.ma_exp_time == 0
            || g.d_ma_time == 0
            || g.timestamp == 0
            || g.timestamp.checked_mul(1000).is_none()
            || g.block_hash.len() != 66
            || !g.block_hash.starts_with("0x")
            || g.block_hash != g.block_hash.to_ascii_lowercase()
            || hex::decode(&g.block_hash[2..]).is_err()
            || g.block_hash[2..].bytes().all(|b| b == b'0')
        {
            return Err(err("CURVE_NG_PARAMETERS"));
        }
        if s.amplification.0 < BigUint::from(100u32)
            || s.amplification.0 >= BigUint::from(100_000_000u32)
            || self.fee_numerator > 5_000_000_000
            || s.admin_fee != 5_000_000_000
        {
            return Err(err("CURVE_NG_FEE_OR_AMP"));
        }
        let product = &g.offpeg_fee_multiplier.0 * BigUint::from(self.fee_numerator);
        if g.offpeg_fee_multiplier.0.bits() > 256
            || product.bits() > 256
            || product > BigUint::from(5_000_000_000u64) * BigUint::from(10_000_000_000u64)
        {
            return Err(err("CURVE_NG_OFFPEG_FEE"));
        }
        for i in 0..n {
            let expected = &s.assets[i].balance.0 + &g.admin_balances[i].0;
            if expected.bits() > 256
                || g.stored_balances[i].0.bits() > 256
                || g.admin_balances[i].0.bits() > 256
                || g.stored_balances[i].0 != expected
            {
                return Err(err("CURVE_NG_BALANCE_ACCOUNTING"));
            }
        }
        let mask = (BigUint::one() << 128usize) - BigUint::one();
        let cap = BigUint::from(2_000_000_000_000_000_000u64);
        for a in &g.last_prices_packed {
            if a.0.bits() > 256 || (&a.0 & &mask) > cap || (&a.0 >> 128usize) > cap {
                return Err(err("CURVE_NG_ORACLE_PRICE"));
            }
        }
        if g.last_d_packed.0.bits() > 256 || g.ma_last_time.0.bits() > 256 {
            return Err(err("CURVE_NG_ORACLE_WIDTH"));
        }
        let now = BigUint::from(g.timestamp);
        if (&g.ma_last_time.0 & mask) > now || (&g.ma_last_time.0 >> 128usize) > now {
            return Err(err("CURVE_NG_ORACLE_TIME"));
        }
        Ok(())
    }
}
impl CurveNGState {
    pub fn to_proto(&self) -> Result<wire::CurveNgState> {
        fn amount(a: &Amount) -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        }
        fn values(v: &[Amount]) -> Result<Vec<wire::Amount>> {
            v.iter()
                .map(|a| {
                    Ok(wire::Amount {
                        uint256_be: a.to_be_bytes()?.to_vec(),
                    })
                })
                .collect()
        }
        let hash = hex::decode(
            self.block_hash
                .strip_prefix("0x")
                .ok_or_else(|| err("CURVE_NG_BLOCK_HASH"))?,
        )
        .map_err(|_| err("CURVE_NG_BLOCK_HASH"))?;
        if hash.len() != 32 {
            return Err(err("CURVE_NG_BLOCK_HASH"));
        }
        Ok(wire::CurveNgState {
            stored_balances: values(&self.stored_balances)?,
            admin_balances: values(&self.admin_balances)?,
            last_prices_packed: values(&self.last_prices_packed)?,
            last_d_packed: amount(&self.last_d_packed)?,
            ma_last_time: amount(&self.ma_last_time)?,
            ma_exp_time: self.ma_exp_time,
            d_ma_time: self.d_ma_time,
            timestamp: self.timestamp,
            block_number: self.block_number,
            block_hash: hash,
            offpeg_fee_multiplier: amount(&self.offpeg_fee_multiplier)?,
        })
    }
    pub fn from_proto(s: wire::CurveNgState) -> Result<Self> {
        fn amount(a: Option<wire::Amount>) -> Result<Amount> {
            Amount::from_be_bytes(&a.ok_or_else(|| err("CURVE_NG_AMOUNT_REQUIRED"))?.uint256_be)
        }
        fn values(v: Vec<wire::Amount>) -> Result<Vec<Amount>> {
            v.into_iter().map(|a| amount(Some(a))).collect()
        }
        if s.block_hash.len() != 32 {
            return Err(err("CURVE_NG_BLOCK_HASH"));
        }
        Ok(Self {
            stored_balances: values(s.stored_balances)?,
            admin_balances: values(s.admin_balances)?,
            last_prices_packed: values(s.last_prices_packed)?,
            last_d_packed: amount(s.last_d_packed)?,
            ma_last_time: amount(s.ma_last_time)?,
            ma_exp_time: s.ma_exp_time,
            d_ma_time: s.d_ma_time,
            timestamp: s.timestamp,
            block_number: s.block_number,
            block_hash: format!("0x{}", hex::encode(s.block_hash)),
            offpeg_fee_multiplier: amount(s.offpeg_fee_multiplier)?,
        })
    }
}
