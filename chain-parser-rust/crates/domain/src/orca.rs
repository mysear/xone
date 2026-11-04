use crate::{err, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
pub const PROGRAM_DATA: &str = "CtXfPzz36dH5Ws4UYKZvrQ1Xqzn42ecDW6y8NKuiN8nD";
pub const PROGRAM_ID: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
pub const MIN_TICK: i32 = -443636;
pub const MAX_TICK: i32 = 443636;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrcaRewardInfo {
    pub mint: String,
    pub vault: String,
    pub extension: Vec<u8>,
    pub emissions_per_second_x64: Amount,
    pub growth_global_x64: Amount,
}
impl OrcaRewardInfo {
    pub fn initialized(&self) -> bool {
        self.mint != "11111111111111111111111111111111"
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrcaTick {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reward_growths_outside: Vec<Amount>,
    pub tick: i32,
    pub liquidity_gross: Amount,
    pub liquidity_net: String,
    pub fee_growth_outside0: Amount,
    pub fee_growth_outside1: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrcaTickArray {
    pub address: String,
    pub start_tick_index: i32,
    pub layout: String,
    pub initialized_bitmap: Amount,
    pub ticks: Vec<OrcaTick>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrcaWhirlpoolState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive: Option<crate::orca_adaptive_state::OrcaAdaptiveState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reward_infos: Vec<OrcaRewardInfo>,
    pub program_id: String,
    pub programdata: String,
    pub program_code_sha256: String,
    pub deployed_slot: u64,
    pub token_program_code_sha256: String,
    pub whirlpools_config: String,
    pub whirlpool_bump: u32,
    pub oracle: String,
    pub vault0: String,
    pub vault1: String,
    pub raw_vault0: u64,
    pub raw_vault1: u64,
    pub mint0_decimals: u32,
    pub mint1_decimals: u32,
    pub bank_slot: u64,
    pub bank_unix_timestamp: u64,
    pub bank_epoch: u64,
    pub fee_tier_index: u32,
    pub fee_rate: u32,
    pub protocol_fee_rate: u32,
    pub sqrt_price_x64: Amount,
    pub tick_current_index: i32,
    pub liquidity: Amount,
    pub tick_spacing: u32,
    pub protocol_fee_owed0: u64,
    pub protocol_fee_owed1: u64,
    pub fee_growth_global0: Amount,
    pub fee_growth_global1: Amount,
    pub reward_last_updated_timestamp: u64,
    pub tick_arrays: Vec<OrcaTickArray>,
    pub max_steps: u32,
}
fn amount128(a: &Amount) -> Result<&BigUint> {
    if a.0.bits() > 128 {
        return Err(err("ORCA_U128_WIDTH"));
    }
    Ok(&a.0)
}
impl OrcaWhirlpoolState {
    pub fn validate_reward_state(&self) -> Result<()> {
        if !self.reward_infos.is_empty() && self.reward_infos.len() != 3 {
            return Err(err("ORCA_REWARD_COUNT"));
        }
        let zero = "11111111111111111111111111111111";
        for (i, r) in self.reward_infos.iter().enumerate() {
            // The all-zero Pubkey is the original uninitialized-reward sentinel.
            // Business-account validation intentionally rejects it elsewhere.
            if r.mint != zero {
                crate::raydium::validate_pubkey(&r.mint)?;
            }
            if r.vault != zero {
                crate::raydium::validate_pubkey(&r.vault)?;
            }
            amount128(&r.emissions_per_second_x64)?;
            amount128(&r.growth_global_x64)?;
            if r.extension.len() != 32 || i > 0 && r.extension.iter().any(|v| *v != 0) {
                return Err(err("ORCA_REWARD_CONTROL_UNSUPPORTED"));
            }
            if r.initialized() {
                if r.vault == zero {
                    return Err(err("ORCA_REWARD_VAULT"));
                }
            } else if r.vault != zero
                || !r.emissions_per_second_x64.0.is_zero()
                || !r.growth_global_x64.0.is_zero()
            {
                return Err(err("ORCA_UNINITIALIZED_REWARD"));
            }
        }
        for t in self.tick_arrays.iter().flat_map(|a| &a.ticks) {
            if t.reward_growths_outside.len() != self.reward_infos.len() {
                return Err(err("ORCA_REWARD_TICK_WIDTH"));
            }
            for (i, growth) in t.reward_growths_outside.iter().enumerate() {
                amount128(growth)?;
                if !self.reward_infos[i].initialized() && !growth.0.is_zero() {
                    return Err(err("ORCA_UNINITIALIZED_REWARD_GROWTH"));
                }
            }
        }
        Ok(())
    }
    fn validate_wire(&self) -> Result<()> {
        self.validate_reward_state()?;
        if self.program_id != PROGRAM_ID
            || self.programdata != PROGRAM_DATA
            || self.whirlpool_bump > 255
            || self.mint0_decimals > 255
            || self.mint1_decimals > 255
            || self.deployed_slot > self.bank_slot
            || self.bank_unix_timestamp > i64::MAX as u64
            || self.reward_last_updated_timestamp > self.bank_unix_timestamp
            || self.tick_spacing == 0
            || self.tick_spacing > 65535
            || self.fee_tier_index > u16::MAX.into()
            || self.adaptive.is_none() && self.fee_tier_index != self.tick_spacing
            || self.fee_rate > 60000
            || self.protocol_fee_rate > 2500
            || !(1..=8192).contains(&self.max_steps)
            || !(3..=7).contains(&self.tick_arrays.len())
        {
            return Err(err("ORCA_STATIC_PARAMETERS"));
        }
        if let Some(a) = &self.adaptive {
            a.validate(self)?;
            if self.fee_tier_index == self.tick_spacing
                || self.tick_arrays.len() != 3
                || self.reward_infos.len() != 3
            {
                return Err(err("ORCA_ADAPTIVE_COMBINATION_UNSUPPORTED"));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in [
            &self.programdata,
            &self.whirlpools_config,
            &self.oracle,
            &self.vault0,
            &self.vault1,
        ] {
            crate::raydium::validate_pubkey(key)?;
            if !seen.insert(key.as_str()) {
                return Err(err("ORCA_ACCOUNT_IDENTITY"));
            }
        }
        for code in [&self.program_code_sha256, &self.token_program_code_sha256] {
            if code.len() != 64 || *code != code.to_ascii_lowercase() || hex::decode(code).is_err()
            {
                return Err(err("ORCA_CODE_IDENTITY"));
            }
        }
        for a in [
            &self.liquidity,
            &self.fee_growth_global0,
            &self.fee_growth_global1,
        ] {
            amount128(a)?;
        }
        let price = amount128(&self.sqrt_price_x64)?;
        if !(MIN_TICK - 1..=MAX_TICK).contains(&self.tick_current_index) {
            return Err(err("ORCA_TICK_RANGE"));
        }
        if price < &crate::orca_math::sqrt_price_from_tick(MIN_TICK)?
            || price > &crate::orca_math::sqrt_price_from_tick(MAX_TICK)?
        {
            return Err(err("ORCA_PRICE_RANGE"));
        }
        if self.tick_current_index >= MIN_TICK
            && price < &crate::orca_math::sqrt_price_from_tick(self.tick_current_index)?
            || self.tick_current_index < MAX_TICK
                && price > &crate::orca_math::sqrt_price_from_tick(self.tick_current_index + 1)?
        {
            return Err(err("ORCA_PRICE_TICK"));
        }
        let span = 88 * i64::from(self.tick_spacing);
        let min_start = i64::from(MIN_TICK).div_euclid(span) * span;
        for (i, a) in self.tick_arrays.iter().enumerate() {
            let start = i64::from(a.start_tick_index);
            crate::raydium::validate_pubkey(&a.address)?;
            if !seen.insert(&a.address)
                || !["fixed", "dynamic"].contains(&a.layout.as_str())
                || start % span != 0
                || start < min_start
                || start > i64::from(MAX_TICK)
                || i > 0 && start != i64::from(self.tick_arrays[i - 1].start_tick_index) + span
            {
                return Err(err("ORCA_ARRAY_SEQUENCE"));
            }
            if a.initialized_bitmap.0.bits() > 88 {
                return Err(err("ORCA_ARRAY_BITMAP"));
            }
            let mut ticks = Vec::new();
            for bit in 0..88u64 {
                if a.initialized_bitmap.0.bit(bit) {
                    let tick = start + bit as i64 * i64::from(self.tick_spacing);
                    if !(i64::from(MIN_TICK)..=i64::from(MAX_TICK)).contains(&tick) {
                        return Err(err("ORCA_TICK_RANGE"));
                    }
                    ticks.push(tick as i32);
                }
            }
            if ticks.len() != a.ticks.len() {
                return Err(err("ORCA_INCOMPLETE_TICKS"));
            }
            for (index, t) in a.ticks.iter().enumerate() {
                let gross = amount128(&t.liquidity_gross)?;
                if gross.is_zero() || t.tick != ticks[index] {
                    return Err(err("ORCA_TICK_GROSS"));
                }
                let net = crate::v3::parse_liquidity_net(&t.liquidity_net)?;
                if BigUint::from(net.unsigned_abs()) > *gross {
                    return Err(err("ORCA_TICK_NET"));
                }
                amount128(&t.fee_growth_outside0)?;
                amount128(&t.fee_growth_outside1)?;
            }
        }
        let low = i64::from(self.tick_arrays[0].start_tick_index);
        let hi = i64::from(
            self.tick_arrays
                .last()
                .ok_or_else(|| err("ORCA_ARRAY_SEQUENCE"))?
                .start_tick_index,
        ) + span;
        if i64::from(self.tick_current_index) < low - 1 || i64::from(self.tick_current_index) >= hi
        {
            return Err(err("ORCA_CURRENT_TICK_UNPROVEN"));
        }
        Ok(())
    }
    pub fn validate(&self, p: &Pool) -> Result<()> {
        self.validate_wire()?;
        if matches!(
            p.protocol.as_str(),
            crate::orca_adaptive_state::PROFILE | crate::orca_adaptive_state::REWARDS_PROFILE
        ) && self.tick_arrays.iter().any(|a| a.layout != "fixed")
        {
            return Err(err("ORCA_ADAPTIVE_COMBINATION_UNSUPPORTED"));
        }
        if let Some(a) = &self.adaptive {
            if a.whirlpool != p.pool_id {
                return Err(err("ORCA_ADAPTIVE_WHIRLPOOL_BINDING"));
            }
        }
        if crate::validate_chain(&p.chain_key).is_err()
            || !p.chain_key.starts_with("solana:")
            || !((p.protocol == "orca-whirlpools-static"
                && p.adapter_version == "orca-whirlpools/cc683ec9-spl-static-1"
                && self.reward_infos.is_empty()
                && self.adaptive.is_none())
                || (p.protocol == "orca-whirlpools-rewards"
                    && p.adapter_version == "orca-whirlpools/cc683ec9-spl-rewards-1"
                    && self.reward_infos.len() == 3
                    && self.adaptive.is_none())
                || (p.protocol == crate::orca_adaptive_state::PROFILE
                    && p.adapter_version == crate::orca_adaptive_state::ADAPTER
                    && self.adaptive.is_some()
                    && !self.reward_infos.iter().any(|r| r.initialized()))
                || (p.protocol == crate::orca_adaptive_state::REWARDS_PROFILE
                    && p.adapter_version == crate::orca_adaptive_state::REWARDS_ADAPTER
                    && self.adaptive.is_some())
                || (p.protocol == crate::orca_adaptive_state::DYNAMIC_PROFILE
                    && p.adapter_version == crate::orca_adaptive_state::DYNAMIC_ADAPTER
                    && self.adaptive.is_some()))
            || p.token_behavior != "standard"
            || p.fee_denominator != 1000000
            || p.fee_numerator != u64::from(self.fee_rate)
            || p.reserve0 != Amount::zero()
            || p.reserve1 != Amount::zero()
            || p.v2.is_some()
            || p.v3.is_some()
            || p.stable.is_some()
            || p.singleton_cl.is_some()
            || p.raydium_cpmm.is_some()
        {
            return Err(err("ORCA_POOL_IDENTITY"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in [
            &p.pool_id,
            &p.token0,
            &p.token1,
            &self.program_id,
            &self.programdata,
            &self.whirlpools_config,
            &self.oracle,
            &self.vault0,
            &self.vault1,
            crate::raydium::TOKEN_PROGRAM,
            crate::raydium::CLOCK,
        ] {
            crate::raydium::validate_pubkey(key)?;
            if !seen.insert(key) {
                return Err(err("ORCA_ACCOUNT_ALIAS"));
            }
        }
        if crate::raydium::validate_pubkey(&p.token0)?
            >= crate::raydium::validate_pubkey(&p.token1)?
        {
            return Err(err("ORCA_MINT_ORDER"));
        }
        for a in &self.tick_arrays {
            if seen.contains(a.address.as_str()) {
                return Err(err("ORCA_ACCOUNT_ALIAS"));
            }
        }
        Ok(())
    }
    pub fn to_proto(&self) -> Result<wire::OrcaWhirlpoolState> {
        self.validate_wire()?;
        let amount = |a: &Amount| -> Result<wire::Amount> {
            Ok(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            })
        };
        Ok(wire::OrcaWhirlpoolState {
            adaptive: self.adaptive.as_ref().map(|a| a.to_proto()),
            reward_infos: self
                .reward_infos
                .iter()
                .map(|r| {
                    Ok(wire::OrcaRewardInfo {
                        mint: r.mint.clone(),
                        vault: r.vault.clone(),
                        extension: r.extension.clone(),
                        emissions_per_second_x64: Some(amount(&r.emissions_per_second_x64)?),
                        growth_global_x64: Some(amount(&r.growth_global_x64)?),
                    })
                })
                .collect::<Result<Vec<_>>>()?,

            program_id: self.program_id.clone(),
            programdata: self.programdata.clone(),
            program_code_sha256: hex::decode(&self.program_code_sha256)
                .map_err(|_| err("ORCA_DIGEST"))?,
            deployed_slot: self.deployed_slot,
            token_program_code_sha256: hex::decode(&self.token_program_code_sha256)
                .map_err(|_| err("ORCA_DIGEST"))?,
            whirlpools_config: self.whirlpools_config.clone(),
            whirlpool_bump: self.whirlpool_bump,
            oracle: self.oracle.clone(),
            vault0: self.vault0.clone(),
            vault1: self.vault1.clone(),
            raw_vault0: self.raw_vault0,
            raw_vault1: self.raw_vault1,
            mint0_decimals: self.mint0_decimals,
            mint1_decimals: self.mint1_decimals,
            bank_slot: self.bank_slot,
            bank_unix_timestamp: self.bank_unix_timestamp,
            bank_epoch: self.bank_epoch,
            fee_tier_index: self.fee_tier_index,
            fee_rate: self.fee_rate,
            protocol_fee_rate: self.protocol_fee_rate,
            sqrt_price_x64: Some(amount(&self.sqrt_price_x64)?),
            tick_current_index: self.tick_current_index,
            liquidity: Some(amount(&self.liquidity)?),
            tick_spacing: self.tick_spacing,
            protocol_fee_owed0: self.protocol_fee_owed0,
            protocol_fee_owed1: self.protocol_fee_owed1,
            fee_growth_global0: Some(amount(&self.fee_growth_global0)?),
            fee_growth_global1: Some(amount(&self.fee_growth_global1)?),
            reward_last_updated_timestamp: self.reward_last_updated_timestamp,
            tick_arrays: self
                .tick_arrays
                .iter()
                .map(|a| {
                    Ok(wire::OrcaTickArray {
                        address: a.address.clone(),
                        start_tick_index: a.start_tick_index,
                        layout: a.layout.clone(),
                        initialized_bitmap: Some(amount(&a.initialized_bitmap)?),
                        ticks: a
                            .ticks
                            .iter()
                            .map(|t| {
                                Ok(wire::OrcaTick {
                                    reward_growths_outside: t
                                        .reward_growths_outside
                                        .iter()
                                        .map(amount)
                                        .collect::<Result<Vec<_>>>()?,
                                    tick: t.tick,
                                    liquidity_gross: Some(amount(&t.liquidity_gross)?),
                                    liquidity_net_i128_be: crate::v3::parse_liquidity_net(
                                        &t.liquidity_net,
                                    )?
                                    .to_be_bytes()
                                    .to_vec(),
                                    fee_growth_outside0: Some(amount(&t.fee_growth_outside0)?),
                                    fee_growth_outside1: Some(amount(&t.fee_growth_outside1)?),
                                })
                            })
                            .collect::<Result<Vec<_>>>()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            max_steps: self.max_steps,
        })
    }
    pub fn from_proto(s: wire::OrcaWhirlpoolState) -> Result<Self> {
        if s.program_code_sha256.len() != 32 || s.token_program_code_sha256.len() != 32 {
            return Err(err("ORCA_DIGEST_WIDTH"));
        }
        let amount = |a: Option<wire::Amount>| -> Result<Amount> {
            Amount::from_be_bytes(&a.ok_or_else(|| err("ORCA_AMOUNT_REQUIRED"))?.uint256_be)
        };
        let out = Self {
            adaptive: s
                .adaptive
                .map(crate::orca_adaptive_state::OrcaAdaptiveState::from_proto),
            reward_infos: s
                .reward_infos
                .into_iter()
                .map(|r| {
                    Ok(OrcaRewardInfo {
                        mint: r.mint,
                        vault: r.vault,
                        extension: r.extension,
                        emissions_per_second_x64: amount(r.emissions_per_second_x64)?,
                        growth_global_x64: amount(r.growth_global_x64)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,

            program_id: s.program_id,
            programdata: s.programdata,
            program_code_sha256: hex::encode(s.program_code_sha256),
            deployed_slot: s.deployed_slot,
            token_program_code_sha256: hex::encode(s.token_program_code_sha256),
            whirlpools_config: s.whirlpools_config,
            whirlpool_bump: s.whirlpool_bump,
            oracle: s.oracle,
            vault0: s.vault0,
            vault1: s.vault1,
            raw_vault0: s.raw_vault0,
            raw_vault1: s.raw_vault1,
            mint0_decimals: s.mint0_decimals,
            mint1_decimals: s.mint1_decimals,
            bank_slot: s.bank_slot,
            bank_unix_timestamp: s.bank_unix_timestamp,
            bank_epoch: s.bank_epoch,
            fee_tier_index: s.fee_tier_index,
            fee_rate: s.fee_rate,
            protocol_fee_rate: s.protocol_fee_rate,
            sqrt_price_x64: amount(s.sqrt_price_x64)?,
            tick_current_index: s.tick_current_index,
            liquidity: amount(s.liquidity)?,
            tick_spacing: s.tick_spacing,
            protocol_fee_owed0: s.protocol_fee_owed0,
            protocol_fee_owed1: s.protocol_fee_owed1,
            fee_growth_global0: amount(s.fee_growth_global0)?,
            fee_growth_global1: amount(s.fee_growth_global1)?,
            reward_last_updated_timestamp: s.reward_last_updated_timestamp,
            tick_arrays: s
                .tick_arrays
                .into_iter()
                .map(|a| {
                    Ok(OrcaTickArray {
                        address: a.address,
                        start_tick_index: a.start_tick_index,
                        layout: a.layout,
                        initialized_bitmap: amount(a.initialized_bitmap)?,
                        ticks: a
                            .ticks
                            .into_iter()
                            .map(|t| {
                                let net: [u8; 16] = t
                                    .liquidity_net_i128_be
                                    .try_into()
                                    .map_err(|_| err("ORCA_NET_WIDTH"))?;
                                Ok(OrcaTick {
                                    reward_growths_outside: t
                                        .reward_growths_outside
                                        .into_iter()
                                        .map(|a| amount(Some(a)))
                                        .collect::<Result<Vec<_>>>()?,
                                    tick: t.tick,
                                    liquidity_gross: amount(t.liquidity_gross)?,
                                    liquidity_net: i128::from_be_bytes(net).to_string(),
                                    fee_growth_outside0: amount(t.fee_growth_outside0)?,
                                    fee_growth_outside1: amount(t.fee_growth_outside1)?,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            max_steps: s.max_steps,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
