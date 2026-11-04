//! Original Uniswap V2 Pair.swap state, separate from legacy reserve-only models.
use crate::{err, protocols, wire, Amount, Pool, Result};
use serde::{Deserialize, Serialize};
pub const PROFILE: &str = "uniswap-v2-complete";
pub const ADAPTER: &str = "uniswap-v2/core-1.0.1-full-swap-1";
pub const PAIR_CODE_SHA256: &str =
    "8b5db55fa9ab3b9527508d4abe0b39eb588bf310270c8e04b3f38214e8ba63b4";
pub const FACTORY_CODE_SHA256: &str =
    "3abc53f12a9cb8ae37ebfada9efc261c1ab4c2759d161e341a49bf67df3f8321";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct V2SwapState {
    pub raw_balance0: Amount,
    pub raw_balance1: Amount,
    pub block_timestamp_last: u32,
    pub price0_cumulative_last: Amount,
    pub price1_cumulative_last: Amount,
    pub unlocked: bool,
    pub factory_code_sha256: String,
    pub bank_block_number: u64,
    pub bank_block_hash: String,
    pub bank_unix_timestamp: u64,
}
impl V2SwapState {
    pub fn validate(&self) -> Result<()> {
        for value in [
            &self.raw_balance0,
            &self.raw_balance1,
            &self.price0_cumulative_last,
            &self.price1_cumulative_last,
        ] {
            if value.0.bits() > 256 {
                return Err(err("V2_SWAP_AMOUNT_WIDTH"));
            }
        }
        if !self.unlocked {
            return Err(err("V2_SWAP_LOCKED"));
        }
        if self.factory_code_sha256 != FACTORY_CODE_SHA256 {
            return Err(err("V2_SWAP_FACTORY_CODE"));
        }
        let digest = self
            .bank_block_hash
            .strip_prefix("0x")
            .ok_or_else(|| err("V2_SWAP_BANK_HASH"))?;
        if digest.len() != 64
            || digest != digest.to_ascii_lowercase()
            || digest.bytes().all(|b| b == b'0')
            || hex::decode(digest).is_err()
        {
            return Err(err("V2_SWAP_BANK_HASH"));
        }
        if self.bank_unix_timestamp.checked_mul(1000).is_none() {
            return Err(err("V2_SWAP_BANK_TIME"));
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::V2SwapState> {
        self.validate()?;
        let amount = |a: &Amount| -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            }))
        };
        Ok(wire::V2SwapState {
            raw_balance0: amount(&self.raw_balance0)?,
            raw_balance1: amount(&self.raw_balance1)?,
            block_timestamp_last: Some(self.block_timestamp_last),
            price0_cumulative_last: amount(&self.price0_cumulative_last)?,
            price1_cumulative_last: amount(&self.price1_cumulative_last)?,
            unlocked: self.unlocked,
            factory_code_sha256: hex::decode(&self.factory_code_sha256)
                .map_err(|_| err("V2_SWAP_WIRE_WIDTH"))?,
            bank_block_number: Some(self.bank_block_number),
            bank_block_hash: hex::decode(&self.bank_block_hash[2..])
                .map_err(|_| err("V2_SWAP_WIRE_WIDTH"))?,
            bank_unix_timestamp: Some(self.bank_unix_timestamp),
        })
    }
    pub fn from_proto(s: wire::V2SwapState) -> Result<Self> {
        if s.factory_code_sha256.len() != 32 || s.bank_block_hash.len() != 32 {
            return Err(err("V2_SWAP_WIRE_WIDTH"));
        }
        let amount = |v: Option<wire::Amount>| -> Result<Amount> {
            Amount::from_be_bytes(&v.ok_or_else(|| err("V2_SWAP_AMOUNT_REQUIRED"))?.uint256_be)
        };
        let out = Self {
            raw_balance0: amount(s.raw_balance0)?,
            raw_balance1: amount(s.raw_balance1)?,
            block_timestamp_last: s
                .block_timestamp_last
                .ok_or_else(|| err("V2_SWAP_FIELD_REQUIRED"))?,
            price0_cumulative_last: amount(s.price0_cumulative_last)?,
            price1_cumulative_last: amount(s.price1_cumulative_last)?,
            unlocked: s.unlocked,
            factory_code_sha256: hex::encode(s.factory_code_sha256),
            bank_block_number: s
                .bank_block_number
                .ok_or_else(|| err("V2_SWAP_FIELD_REQUIRED"))?,
            bank_block_hash: format!("0x{}", hex::encode(s.bank_block_hash)),
            bank_unix_timestamp: s
                .bank_unix_timestamp
                .ok_or_else(|| err("V2_SWAP_FIELD_REQUIRED"))?,
        };
        out.validate()?;
        Ok(out)
    }
}
impl Pool {
    pub(crate) fn validate_v2_complete(&self) -> Result<()> {
        if self.protocol != PROFILE {
            if self.v2.as_ref().is_some_and(|v| v.swap.is_some()) {
                return Err(err("V2_SWAP_PROFILE_MISMATCH"));
            }
            return Ok(());
        }
        let v = self
            .v2
            .as_ref()
            .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
        let s = v
            .swap
            .as_ref()
            .ok_or_else(|| err("V2_SWAP_STATE_REQUIRED"))?;
        if v.code_sha256 != PAIR_CODE_SHA256 {
            return Err(err("V2_SWAP_PAIR_CODE"));
        }
        if self.token_behavior != "standard" {
            return Err(err("V2_SWAP_TOKEN_BEHAVIOR"));
        }
        for a in [&self.pool_id, &self.token0, &self.token1, &v.factory] {
            if protocols::validate_address(a).is_err() || a[2..].bytes().all(|b| b == b'0') {
                return Err(err("V2_SWAP_ADDRESS"));
            }
        }
        if self.token0 >= self.token1 {
            return Err(err("V2_SWAP_TOKEN_ORDER"));
        }
        if self.reserve0.0.bits() > 112 || self.reserve1.0.bits() > 112 {
            return Err(err("V2_SWAP_RESERVE_WIDTH"));
        }
        s.validate()
    }
}
