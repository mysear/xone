use crate::{err, protocols::validate_identity, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PROFILE: &str = "lfj-lb-v21";
pub const ADAPTER: &str = "lfj-lb/v2.1.0-fee-oracle-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LfjBin {
    pub id: u32,
    pub reserve0: Amount,
    pub reserve1: Amount,
    pub total_shares: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LfjOracle {
    pub index: u32,
    pub data: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LfjState {
    pub code_sha256: String,
    pub implementation: String,
    pub implementation_code_sha256: String,
    pub factory: String,
    pub factory_code_sha256: String,
    pub token0_code_sha256: String,
    pub token1_code_sha256: String,
    pub block_hash: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub bin_step: u32,
    pub parameters: Amount,
    pub raw_reserve0: Amount,
    pub raw_reserve1: Amount,
    pub protocol_fees0: Amount,
    pub protocol_fees1: Amount,
    pub bins: Vec<LfjBin>,
    pub oracle_samples: Vec<LfjOracle>,
    pub max_bins: u32,
    pub max_steps: u32,
}
pub fn field(value: &Amount, offset: usize, bits: usize) -> u64 {
    ((&value.0 >> offset) & ((BigUint::from(1u8) << bits) - 1u8))
        .to_u64()
        .unwrap_or(0)
}
pub fn set_field(value: &mut Amount, offset: usize, bits: usize, field: u64) {
    let mask = ((BigUint::from(1u8) << bits) - 1u8) << offset;
    let all = (BigUint::from(1u8) << 256usize) - 1u8;
    value.0 = (&value.0 & (&all ^ mask)) | (BigUint::from(field) << offset);
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value == value.to_ascii_lowercase() && hex::decode(value).is_ok()
}
impl LfjState {
    pub fn validate(&self, p: &Pool) -> Result<()> {
        if p.protocol != PROFILE
            || p.adapter_version != ADAPTER
            || p.token_behavior != "standard"
            || !p.chain_key.starts_with("evm:")
            || crate::validate_chain(&p.chain_key).is_err()
            || p.fee_numerator != 0
            || p.fee_denominator != 1_000_000_000_000_000_000
            || !p.reserve0.0.is_zero()
            || !p.reserve1.0.is_zero()
        {
            return Err(err("LFJ_PROFILE"));
        }
        for address in [
            &p.pool_id,
            &p.token0,
            &p.token1,
            &self.implementation,
            &self.factory,
        ] {
            crate::protocols::validate_address(address)?;
            if *address == crate::singleton::ZERO_ADDRESS {
                return Err(err("LFJ_ZERO_IDENTITY"));
            }
        }
        let addresses = [
            &p.pool_id,
            &p.token0,
            &p.token1,
            &self.implementation,
            &self.factory,
        ];
        if addresses.into_iter().collect::<BTreeSet<_>>().len() != 5 {
            return Err(err("LFJ_IDENTITY_ALIAS"));
        }
        self.validate_wire()
    }
    pub fn validate_wire(&self) -> Result<()> {
        validate_identity(&self.implementation, &self.implementation_code_sha256)?;
        validate_identity(&self.factory, &self.factory_code_sha256)?;
        if self.factory == crate::singleton::ZERO_ADDRESS
            || self.implementation == crate::singleton::ZERO_ADDRESS
            || self.factory == self.implementation
        {
            return Err(err("LFJ_STATE_IDENTITY"));
        }
        if !digest(&self.code_sha256)
            || !digest(&self.token0_code_sha256)
            || !digest(&self.token1_code_sha256)
            || self.block_hash.len() != 66
            || !self.block_hash.starts_with("0x")
            || !digest(&self.block_hash[2..])
            || self.timestamp >= 1u64 << 40
            || self.bin_step == 0
            || self.bin_step > 65535
            || !(1..=4096).contains(&self.max_bins)
            || !(1..=8192).contains(&self.max_steps)
            || self.bins.len() > self.max_bins as usize
            || self.parameters.0.bits() > 256
        {
            return Err(err("LFJ_STATE_IDENTITY"));
        }
        let g = |o, b| field(&self.parameters, o, b);
        if g(16, 12) > g(28, 12)
            || g(40, 14) > 10000
            || g(78, 14) > 2500
            || g(176, 40) > self.timestamp
        {
            return Err(err("LFJ_FEE_PARAMETERS"));
        }
        let product = u128::from(g(92, 20)) * u128::from(self.bin_step);
        let maxfee = u128::from(g(0, 16)) * u128::from(self.bin_step) * 10_000_000_000u128
            + (product * product * u128::from(g(54, 24))).div_ceil(100);
        if maxfee > 100_000_000_000_000_000 {
            return Err(err("LFJ_MAX_FEE"));
        }
        let mut sum0 = BigUint::zero();
        let mut sum1 = BigUint::zero();
        let mut previous = None;
        for bin in &self.bins {
            if bin.id == 0
                || bin.id >= 0xffffff
                || previous.is_some_and(|id| id >= bin.id)
                || bin.reserve0.0.bits() > 128
                || bin.reserve1.0.bits() > 128
                || bin.total_shares.0.bits() > 256
                || bin.total_shares.0.is_zero()
            {
                return Err(err("LFJ_BIN_SET"));
            }
            previous = Some(bin.id);
            sum0 += &bin.reserve0.0;
            sum1 += &bin.reserve1.0;
        }
        for amount in [
            &self.raw_reserve0,
            &self.raw_reserve1,
            &self.protocol_fees0,
            &self.protocol_fees1,
        ] {
            if amount.0.bits() > 128 {
                return Err(err("LFJ_RESERVE_WIDTH"));
            }
        }
        if sum0 + &self.protocol_fees0.0 != self.raw_reserve0.0
            || sum1 + &self.protocol_fees1.0 != self.raw_reserve1.0
        {
            return Err(err("LFJ_RESERVE_ACCOUNTING"));
        }
        if self.oracle_samples.len() > 2 {
            return Err(err("LFJ_ORACLE_SET"));
        }
        let mut indices = BTreeSet::new();
        for sample in &self.oracle_samples {
            if sample.index == 0
                || sample.index > 65535
                || sample.data.0.bits() > 256
                || !indices.insert(sample.index)
            {
                return Err(err("LFJ_ORACLE_SET"));
            }
        }
        let id = g(216, 16) as u32;
        if id == 0 {
            if !self.oracle_samples.is_empty() {
                return Err(err("LFJ_DISABLED_ORACLE"));
            }
        } else {
            let sample = self
                .oracle_samples
                .iter()
                .find(|s| s.index == id)
                .ok_or_else(|| err("LFJ_ORACLE_MISSING"))?;
            let length = field(&sample.data, 0, 16);
            let created = field(&sample.data, 216, 40);
            let lifetime = field(&sample.data, 208, 8);
            if length == 0
                || u64::from(id) > length
                || created
                    .checked_add(lifetime)
                    .is_none_or(|last| last > self.timestamp)
            {
                return Err(err("LFJ_ORACLE_CLOCK"));
            }
            if self.timestamp - created > 120
                && !indices.contains(&((u64::from(id) % length + 1) as u32))
            {
                return Err(err("LFJ_ORACLE_NEXT_MISSING"));
            }
        }
        Ok(())
    }
}

impl LfjState {
    pub fn to_proto(&self) -> Result<wire::LfjState> {
        self.validate_wire()?;
        let digest = |value: &str| -> Result<Vec<u8>> {
            hex::decode(value.strip_prefix("0x").unwrap_or(value))
                .map_err(|_| err("LFJ_WIRE_DIGEST"))
        };
        let amount = |value: &Amount| -> Result<Option<wire::Amount>> {
            Ok(Some(wire::Amount {
                uint256_be: value.to_be_bytes()?.to_vec(),
            }))
        };
        Ok(wire::LfjState {
            code_sha256: digest(&self.code_sha256)?,
            implementation: self.implementation.clone(),
            implementation_code_sha256: digest(&self.implementation_code_sha256)?,
            factory: self.factory.clone(),
            factory_code_sha256: digest(&self.factory_code_sha256)?,
            token0_code_sha256: digest(&self.token0_code_sha256)?,
            token1_code_sha256: digest(&self.token1_code_sha256)?,
            block_hash: digest(&self.block_hash)?,
            block_number: Some(self.block_number),
            timestamp: Some(self.timestamp),
            bin_step: self.bin_step,
            parameters: amount(&self.parameters)?,
            raw_reserve0: amount(&self.raw_reserve0)?,
            raw_reserve1: amount(&self.raw_reserve1)?,
            protocol_fees0: amount(&self.protocol_fees0)?,
            protocol_fees1: amount(&self.protocol_fees1)?,
            bins: self
                .bins
                .iter()
                .map(|bin| {
                    Ok(wire::LfjBin {
                        id: bin.id,
                        reserve0: amount(&bin.reserve0)?,
                        reserve1: amount(&bin.reserve1)?,
                        total_shares: amount(&bin.total_shares)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            oracle_samples: self
                .oracle_samples
                .iter()
                .map(|sample| {
                    Ok(wire::LfjOracle {
                        index: sample.index,
                        data: amount(&sample.data)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            max_bins: self.max_bins,
            max_steps: self.max_steps,
        })
    }
    pub fn from_proto(s: wire::LfjState) -> Result<Self> {
        let digest = |value: Vec<u8>, prefix: bool| -> Result<String> {
            if value.len() != 32 {
                return Err(err("LFJ_WIRE_DIGEST_WIDTH"));
            }
            Ok(format!(
                "{}{}",
                if prefix { "0x" } else { "" },
                hex::encode(value)
            ))
        };
        let amount = |value: Option<wire::Amount>| -> Result<Amount> {
            Amount::from_be_bytes(
                &value
                    .ok_or_else(|| err("LFJ_WIRE_AMOUNT_REQUIRED"))?
                    .uint256_be,
            )
        };
        let out = Self {
            code_sha256: digest(s.code_sha256, false)?,
            implementation: s.implementation,
            implementation_code_sha256: digest(s.implementation_code_sha256, false)?,
            factory: s.factory,
            factory_code_sha256: digest(s.factory_code_sha256, false)?,
            token0_code_sha256: digest(s.token0_code_sha256, false)?,
            token1_code_sha256: digest(s.token1_code_sha256, false)?,
            block_hash: digest(s.block_hash, true)?,
            block_number: s
                .block_number
                .ok_or_else(|| err("LFJ_WIRE_BANK_REQUIRED"))?,
            timestamp: s.timestamp.ok_or_else(|| err("LFJ_WIRE_BANK_REQUIRED"))?,
            bin_step: s.bin_step,
            parameters: amount(s.parameters)?,
            raw_reserve0: amount(s.raw_reserve0)?,
            raw_reserve1: amount(s.raw_reserve1)?,
            protocol_fees0: amount(s.protocol_fees0)?,
            protocol_fees1: amount(s.protocol_fees1)?,
            bins: s
                .bins
                .into_iter()
                .map(|bin| {
                    Ok(LfjBin {
                        id: bin.id,
                        reserve0: amount(bin.reserve0)?,
                        reserve1: amount(bin.reserve1)?,
                        total_shares: amount(bin.total_shares)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            oracle_samples: s
                .oracle_samples
                .into_iter()
                .map(|sample| {
                    Ok(LfjOracle {
                        index: sample.index,
                        data: amount(sample.data)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            max_bins: s.max_bins,
            max_steps: s.max_steps,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
