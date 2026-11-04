use crate::{err, wire, Amount, Pool, Result};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
pub const PROGRAM_DATA: &str = "HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx";
pub const PROGRAM_ID: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
pub const MIN_TICK: i32 = -443636;
pub const MAX_TICK: i32 = 443636;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMRewardInfo {
    pub reward_state: u32,
    pub open_time: u64,
    pub end_time: u64,
    pub last_update_time: u64,
    pub emissions_per_second_x64: Amount,
    pub reward_total_emitted: u64,
    pub reward_claimed: u64,
    pub token_mint: String,
    pub token_vault: String,
    pub authority: String,
    pub reward_growth_global_x64: Amount,
}
impl RaydiumCLMMRewardInfo {
    pub fn initialized(&self) -> bool {
        self.token_mint != "11111111111111111111111111111111"
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMTick {
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
pub struct RaydiumCLMMArray {
    pub address: String,
    pub start_tick_index: i32,
    pub recent_epoch: u64,
    pub ticks: Vec<RaydiumCLMMTick>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMObservation {
    pub block_timestamp: u32,
    pub tick_cumulative: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMBitmapExtension {
    pub address: String,
    pub positive: Vec<u64>,
    pub negative: Vec<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMDynamicFee {
    pub filter_period: u32,
    pub decay_period: u32,
    pub reduction_factor: u32,
    pub dynamic_fee_control: u32,
    pub max_volatility_accumulator: u32,
    pub tick_spacing_index_reference: i32,
    pub volatility_reference: u32,
    pub volatility_accumulator: u32,
    pub last_update_timestamp: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMState {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reward_infos: Vec<RaydiumCLMMRewardInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_fee: Option<RaydiumCLMMDynamicFee>,
    #[serde(default, skip_serializing_if = "fee_mode_is_zero")]
    pub seed_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitmap_extension: Option<RaydiumCLMMBitmapExtension>,
    #[serde(default, skip_serializing_if = "fee_mode_is_zero")]
    pub collect_fee_on: u32,
    pub program_id: String,
    pub programdata: String,
    pub program_code_sha256: String,
    pub deployed_slot: u64,
    pub token_program_code_sha256: String,
    pub amm_config: String,
    pub pool_bump: u32,
    pub observation: String,
    pub vault0: String,
    pub vault1: String,
    pub raw_vault0: u64,
    pub raw_vault1: u64,
    pub mint0_decimals: u32,
    pub mint1_decimals: u32,
    pub bank_slot: u64,
    pub bank_unix_timestamp: u64,
    pub bank_epoch: u64,
    pub trade_fee_rate: u32,
    pub protocol_fee_rate: u32,
    pub fund_fee_rate: u32,
    pub tick_spacing: u32,
    pub sqrt_price_x64: Amount,
    pub tick_current: i32,
    pub liquidity: Amount,
    pub fee_growth_global0: Amount,
    pub fee_growth_global1: Amount,
    pub protocol_fees0: u64,
    pub protocol_fees1: u64,
    pub fund_fees0: u64,
    pub fund_fees1: u64,
    pub open_time: u64,
    pub recent_epoch: u64,
    pub bitmap: Vec<u64>,
    pub tick_arrays: Vec<RaydiumCLMMArray>,
    pub max_steps: u32,
    pub observation_initialized: bool,
    pub observation_index: u32,
    pub observation_recent_epoch: u64,
    pub observations: Vec<RaydiumCLMMObservation>,
}
fn wide(a: &Amount) -> Result<&BigUint> {
    if a.0.bits() > 128 {
        return Err(err("RAYCLMM_U128_WIDTH"));
    }
    Ok(&a.0)
}
fn fee_mode_is_zero(mode: &u32) -> bool {
    *mode == 0
}
impl RaydiumCLMMState {
    pub fn validate_reward_state(&self) -> Result<()> {
        if !self.reward_infos.is_empty() && self.reward_infos.len() != 3 {
            return Err(err("RAYCLMM_REWARD_COUNT"));
        }
        let zero = "11111111111111111111111111111111";
        for r in &self.reward_infos {
            for key in [&r.token_mint, &r.token_vault, &r.authority] {
                if key != zero {
                    crate::raydium::validate_pubkey(key)?;
                }
            }
            wide(&r.emissions_per_second_x64)?;
            wide(&r.reward_growth_global_x64)?;
            if r.initialized() {
                if !(1..=3).contains(&r.reward_state)
                    || r.token_vault == zero
                    || r.open_time >= r.end_time
                    || r.last_update_time < r.open_time
                    || r.last_update_time > r.end_time
                    || r.last_update_time > self.bank_unix_timestamp
                        && self.bank_unix_timestamp >= r.open_time
                    || r.reward_claimed > r.reward_total_emitted
                {
                    return Err(err("RAYCLMM_REWARD_PARAMETERS"));
                }
            } else if r.reward_state != 0
                || r.open_time != 0
                || r.end_time != 0
                || r.last_update_time != 0
                || r.reward_total_emitted != 0
                || r.reward_claimed != 0
                || r.token_vault != zero
                || !r.emissions_per_second_x64.0.is_zero()
                || !r.reward_growth_global_x64.0.is_zero()
            {
                return Err(err("RAYCLMM_UNINITIALIZED_REWARD"));
            }
        }
        for t in self.tick_arrays.iter().flat_map(|a| &a.ticks) {
            if t.reward_growths_outside.len() != self.reward_infos.len() {
                return Err(err("RAYCLMM_REWARD_TICK_WIDTH"));
            }
            for (i, growth) in t.reward_growths_outside.iter().enumerate() {
                wide(growth)?;
                if !self.reward_infos[i].initialized() && !growth.0.is_zero() {
                    return Err(err("RAYCLMM_UNINITIALIZED_REWARD_GROWTH"));
                }
            }
        }
        Ok(())
    }
    fn validate_wire(&self) -> Result<()> {
        self.validate_reward_state()?;
        if self.seed_index > 65535
            || self.collect_fee_on > 2
            || self.program_id != PROGRAM_ID
            || self.programdata != PROGRAM_DATA
            || self.pool_bump > 255
            || self.mint0_decimals > 255
            || self.mint1_decimals > 255
            || self.deployed_slot > self.bank_slot
            || self.bank_unix_timestamp > u32::MAX as u64
            || self.open_time >= self.bank_unix_timestamp
            || self.recent_epoch > self.bank_epoch
            || self.observation_recent_epoch > self.bank_epoch
            || self.tick_spacing == 0
            || self.tick_spacing > 1000
            || self.trade_fee_rate >= 1000000
            || self.protocol_fee_rate > 1000000
            || self.fund_fee_rate > 1000000
            || u64::from(self.protocol_fee_rate) + u64::from(self.fund_fee_rate) > 1000000
            || !(1..=8192).contains(&self.max_steps)
            || self.bitmap.len() != 16
            || !(3..=7).contains(&self.tick_arrays.len())
            || self.observations.len() != 100
            || self.observation_index >= 100
        {
            return Err(err("RAYCLMM_STATE_PARAMETERS"));
        }
        if let Some(d) = &self.dynamic_fee {
            let spacing = self.tick_spacing as i32;
            if d.filter_period == 0
                || d.filter_period > 65535
                || d.decay_period <= d.filter_period
                || d.decay_period > 65535
                || d.reduction_factor == 0
                || d.reduction_factor >= 10000
                || d.dynamic_fee_control == 0
                || d.dynamic_fee_control >= 100000
                || u64::from(d.max_volatility_accumulator) * u64::from(self.tick_spacing)
                    > u32::MAX as u64
                || d.volatility_reference > d.max_volatility_accumulator
                || d.volatility_accumulator > d.max_volatility_accumulator
                || d.tick_spacing_index_reference < MIN_TICK.div_euclid(spacing)
                || d.tick_spacing_index_reference > MAX_TICK.div_euclid(spacing)
                || d.last_update_timestamp > u32::MAX as u64
            {
                return Err(err("RAYCLMM_DYNAMIC_PARAMETERS"));
            }
        }
        if let Some(e) = &self.bitmap_extension {
            crate::raydium::validate_pubkey(&e.address)?;
            if e.positive.len() != 112 || e.negative.len() != 112 {
                return Err(err("RAYCLMM_EXTENSION_WIDTH"));
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in [
            &self.programdata,
            &self.amm_config,
            &self.observation,
            &self.vault0,
            &self.vault1,
        ] {
            crate::raydium::validate_pubkey(key)?;
            if !seen.insert(key.as_str()) {
                return Err(err("RAYCLMM_ACCOUNT_IDENTITY"));
            }
        }
        for code in [&self.program_code_sha256, &self.token_program_code_sha256] {
            if code.len() != 64 || *code != code.to_ascii_lowercase() || hex::decode(code).is_err()
            {
                return Err(err("RAYCLMM_CODE_IDENTITY"));
            }
        }
        for a in [
            &self.liquidity,
            &self.fee_growth_global0,
            &self.fee_growth_global1,
        ] {
            wide(a)?;
        }
        let price = wide(&self.sqrt_price_x64)?;
        if !(MIN_TICK..MAX_TICK).contains(&self.tick_current)
            || price < &crate::rayclmm_math::sqrt_price_from_tick(MIN_TICK)?
            || price >= &crate::rayclmm_math::sqrt_price_from_tick(MAX_TICK)?
        {
            return Err(err("RAYCLMM_PRICE_TICK"));
        }
        if price < &crate::rayclmm_math::sqrt_price_from_tick(self.tick_current)?
            || price > &crate::rayclmm_math::sqrt_price_from_tick(self.tick_current + 1)?
        {
            return Err(err("RAYCLMM_PRICE_TICK"));
        }
        let span = 60 * i64::from(self.tick_spacing);
        for (i, a) in self.tick_arrays.iter().enumerate() {
            let start = i64::from(a.start_tick_index);
            let index = start / span;
            crate::raydium::validate_pubkey(&a.address)?;
            if !seen.insert(a.address.as_str())
                || start % span != 0
                || (self.bitmap_extension.is_none() && !(-512..=511).contains(&index))
                || !(-7680..=7679).contains(&index)
                || start > i64::from(MAX_TICK)
                || start + span <= i64::from(MIN_TICK)
                || a.recent_epoch > self.bank_epoch
                || i > 0 && start != i64::from(self.tick_arrays[i - 1].start_tick_index) + span
            {
                return Err(err("RAYCLMM_ARRAY_SEQUENCE"));
            }
            let active = self.array_initialized(index as i32)?;
            if active == a.ticks.is_empty() {
                return Err(err("RAYCLMM_BITMAP_PROOF"));
            }
            if a.ticks.len() > 60 {
                return Err(err("RAYCLMM_TICK_LIMIT"));
            }
            for (j, t) in a.ticks.iter().enumerate() {
                if i64::from(t.tick) < start
                    || i64::from(t.tick) >= start + span
                    || t.tick % (self.tick_spacing as i32) != 0
                    || !(MIN_TICK..=MAX_TICK).contains(&t.tick)
                    || j > 0 && t.tick <= a.ticks[j - 1].tick
                {
                    return Err(err("RAYCLMM_TICK_INDEX"));
                }
                let gross = wide(&t.liquidity_gross)?;
                if gross.is_zero() {
                    return Err(err("RAYCLMM_TICK_GROSS"));
                }
                let net = crate::v3::parse_liquidity_net(&t.liquidity_net)?;
                if BigUint::from(net.unsigned_abs()) > *gross {
                    return Err(err("RAYCLMM_TICK_NET"));
                }
                wide(&t.fee_growth_outside0)?;
                wide(&t.fee_growth_outside1)?;
            }
        }
        let low = i64::from(self.tick_arrays[0].start_tick_index);
        let hi = i64::from(
            self.tick_arrays
                .last()
                .ok_or_else(|| err("RAYCLMM_ARRAY_SEQUENCE"))?
                .start_tick_index,
        ) + span;
        if i64::from(self.tick_current) < low - 1 || i64::from(self.tick_current) >= hi {
            return Err(err("RAYCLMM_CURRENT_TICK_UNPROVEN"));
        }
        Ok(())
    }
    pub fn validate(&self, p: &Pool) -> Result<()> {
        self.validate_wire()?;
        if (p.protocol == "raydium-clmm-rewards") != (self.reward_infos.len() == 3) {
            return Err(err("RAYCLMM_REWARD_PROFILE"));
        }
        if (p.protocol == "raydium-clmm-bitmap"
            || p.protocol == "raydium-clmm-custom-seed"
            || p.protocol == "raydium-clmm-dynamic")
            != self.bitmap_extension.is_some()
        {
            return Err(err("RAYCLMM_EXTENSION_PROFILE"));
        }
        if p.protocol != "raydium-clmm-dynamic"
            && (p.protocol == "raydium-clmm-custom-seed") != (self.seed_index > 0)
        {
            return Err(err("RAYCLMM_SEED_PROFILE"));
        }
        if (p.protocol == "raydium-clmm-dynamic") != self.dynamic_fee.is_some() {
            return Err(err("RAYCLMM_DYNAMIC_PROFILE"));
        }
        if crate::validate_chain(&p.chain_key).is_err()
            || !p.chain_key.starts_with("solana:")
            || !((p.protocol == "raydium-clmm-static"
                && p.adapter_version == "raydium-clmm/6d7a0f82-spl-static-1"
                && self.collect_fee_on == 0)
                || (p.protocol == "raydium-clmm-token-fee"
                    && p.adapter_version == "raydium-clmm/6d7a0f82-spl-token-fee-1"
                    && (self.collect_fee_on == 1 || self.collect_fee_on == 2))
                || (p.protocol == "raydium-clmm-bitmap"
                    && p.adapter_version == "raydium-clmm/6d7a0f82-spl-bitmap-1")
                || (p.protocol == "raydium-clmm-custom-seed"
                    && p.adapter_version == "raydium-clmm/6d7a0f82-spl-custom-seed-1")
                || (p.protocol == "raydium-clmm-dynamic"
                    && p.adapter_version == "raydium-clmm/6d7a0f82-spl-dynamic-1")
                || (p.protocol == "raydium-clmm-rewards"
                    && p.adapter_version == "raydium-clmm/6d7a0f82-spl-rewards-1"
                    && self.collect_fee_on == 0))
            || p.token_behavior != "standard"
            || p.fee_denominator != 1000000
            || p.fee_numerator != u64::from(self.trade_fee_rate)
            || p.reserve0 != Amount::zero()
            || p.reserve1 != Amount::zero()
            || p.v2.is_some()
            || p.v3.is_some()
            || p.stable.is_some()
            || p.singleton_cl.is_some()
            || p.raydium_cpmm.is_some()
            || p.orca_whirlpool.is_some()
        {
            return Err(err("RAYCLMM_POOL_IDENTITY"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for key in [
            &p.pool_id,
            &p.token0,
            &p.token1,
            &self.program_id,
            &self.programdata,
            &self.amm_config,
            &self.observation,
            &self.vault0,
            &self.vault1,
            crate::raydium::TOKEN_PROGRAM,
            crate::raydium::CLOCK,
        ] {
            crate::raydium::validate_pubkey(key)?;
            if !seen.insert(key) {
                return Err(err("RAYCLMM_ACCOUNT_ALIAS"));
            }
        }
        if crate::raydium::validate_pubkey(&p.token0)?
            >= crate::raydium::validate_pubkey(&p.token1)?
        {
            return Err(err("RAYCLMM_MINT_ORDER"));
        }
        for a in &self.tick_arrays {
            if seen.contains(a.address.as_str()) {
                return Err(err("RAYCLMM_ACCOUNT_ALIAS"));
            }
        }
        self.validate_pdas(p)
    }
    pub fn to_proto(&self) -> Result<wire::RaydiumClmmState> {
        self.validate_wire()?;
        let amount = |a: &Amount| -> Result<wire::Amount> {
            Ok(wire::Amount {
                uint256_be: a.to_be_bytes()?.to_vec(),
            })
        };
        Ok(wire::RaydiumClmmState {
            reward_infos: self
                .reward_infos
                .iter()
                .map(|r| {
                    Ok(wire::RaydiumClmmRewardInfo {
                        reward_state: r.reward_state,
                        open_time: r.open_time,
                        end_time: r.end_time,
                        last_update_time: r.last_update_time,
                        emissions_per_second_x64: Some(amount(&r.emissions_per_second_x64)?),
                        reward_total_emitted: r.reward_total_emitted,
                        reward_claimed: r.reward_claimed,
                        token_mint: r.token_mint.clone(),
                        token_vault: r.token_vault.clone(),
                        authority: r.authority.clone(),
                        reward_growth_global_x64: Some(amount(&r.reward_growth_global_x64)?),
                    })
                })
                .collect::<Result<Vec<_>>>()?,

            dynamic_fee: self
                .dynamic_fee
                .as_ref()
                .map(|d| wire::RaydiumClmmDynamicFee {
                    filter_period: d.filter_period,
                    decay_period: d.decay_period,
                    reduction_factor: d.reduction_factor,
                    dynamic_fee_control: d.dynamic_fee_control,
                    max_volatility_accumulator: d.max_volatility_accumulator,
                    tick_spacing_index_reference: d.tick_spacing_index_reference,
                    volatility_reference: d.volatility_reference,
                    volatility_accumulator: d.volatility_accumulator,
                    last_update_timestamp: d.last_update_timestamp,
                }),
            seed_index: self.seed_index,
            bitmap_extension: self.bitmap_extension.as_ref().map(|e| {
                wire::RaydiumClmmBitmapExtension {
                    address: e.address.clone(),
                    positive: e.positive.clone(),
                    negative: e.negative.clone(),
                }
            }),
            collect_fee_on: self.collect_fee_on,
            program_id: self.program_id.clone(),
            programdata: self.programdata.clone(),
            program_code_sha256: hex::decode(&self.program_code_sha256)
                .map_err(|_| err("RAYCLMM_DIGEST"))?,
            deployed_slot: self.deployed_slot,
            token_program_code_sha256: hex::decode(&self.token_program_code_sha256)
                .map_err(|_| err("RAYCLMM_DIGEST"))?,
            amm_config: self.amm_config.clone(),
            pool_bump: self.pool_bump,
            observation: self.observation.clone(),
            vault0: self.vault0.clone(),
            vault1: self.vault1.clone(),
            raw_vault0: self.raw_vault0,
            raw_vault1: self.raw_vault1,
            mint0_decimals: self.mint0_decimals,
            mint1_decimals: self.mint1_decimals,
            bank_slot: self.bank_slot,
            bank_unix_timestamp: self.bank_unix_timestamp,
            bank_epoch: self.bank_epoch,
            trade_fee_rate: self.trade_fee_rate,
            protocol_fee_rate: self.protocol_fee_rate,
            fund_fee_rate: self.fund_fee_rate,
            tick_spacing: self.tick_spacing,
            sqrt_price_x64: Some(amount(&self.sqrt_price_x64)?),
            tick_current: self.tick_current,
            liquidity: Some(amount(&self.liquidity)?),
            fee_growth_global0: Some(amount(&self.fee_growth_global0)?),
            fee_growth_global1: Some(amount(&self.fee_growth_global1)?),
            protocol_fees0: self.protocol_fees0,
            protocol_fees1: self.protocol_fees1,
            fund_fees0: self.fund_fees0,
            fund_fees1: self.fund_fees1,
            open_time: self.open_time,
            recent_epoch: self.recent_epoch,
            bitmap: self.bitmap.clone(),
            tick_arrays: self
                .tick_arrays
                .iter()
                .map(|a| {
                    Ok(wire::RaydiumClmmArray {
                        address: a.address.clone(),
                        start_tick_index: a.start_tick_index,
                        recent_epoch: a.recent_epoch,
                        ticks: a
                            .ticks
                            .iter()
                            .map(|t| {
                                Ok(wire::RaydiumClmmTick {
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
            observation_initialized: self.observation_initialized,
            observation_index: self.observation_index,
            observation_recent_epoch: self.observation_recent_epoch,
            observations: self
                .observations
                .iter()
                .map(|o| wire::RaydiumClmmObservation {
                    block_timestamp: o.block_timestamp,
                    tick_cumulative: o.tick_cumulative,
                })
                .collect(),
        })
    }
    pub fn from_proto(s: wire::RaydiumClmmState) -> Result<Self> {
        if s.program_code_sha256.len() != 32 || s.token_program_code_sha256.len() != 32 {
            return Err(err("RAYCLMM_DIGEST_WIDTH"));
        }
        let amount = |a: Option<wire::Amount>| -> Result<Amount> {
            Amount::from_be_bytes(&a.ok_or_else(|| err("RAYCLMM_AMOUNT_REQUIRED"))?.uint256_be)
        };
        let out = Self {
            reward_infos: s
                .reward_infos
                .into_iter()
                .map(|r| {
                    Ok(RaydiumCLMMRewardInfo {
                        reward_state: r.reward_state,
                        open_time: r.open_time,
                        end_time: r.end_time,
                        last_update_time: r.last_update_time,
                        emissions_per_second_x64: amount(r.emissions_per_second_x64)?,
                        reward_total_emitted: r.reward_total_emitted,
                        reward_claimed: r.reward_claimed,
                        token_mint: r.token_mint,
                        token_vault: r.token_vault,
                        authority: r.authority,
                        reward_growth_global_x64: amount(r.reward_growth_global_x64)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,

            dynamic_fee: s.dynamic_fee.map(|d| RaydiumCLMMDynamicFee {
                filter_period: d.filter_period,
                decay_period: d.decay_period,
                reduction_factor: d.reduction_factor,
                dynamic_fee_control: d.dynamic_fee_control,
                max_volatility_accumulator: d.max_volatility_accumulator,
                tick_spacing_index_reference: d.tick_spacing_index_reference,
                volatility_reference: d.volatility_reference,
                volatility_accumulator: d.volatility_accumulator,
                last_update_timestamp: d.last_update_timestamp,
            }),
            seed_index: s.seed_index,
            bitmap_extension: s.bitmap_extension.map(|e| RaydiumCLMMBitmapExtension {
                address: e.address,
                positive: e.positive,
                negative: e.negative,
            }),
            collect_fee_on: s.collect_fee_on,
            program_id: s.program_id,
            programdata: s.programdata,
            program_code_sha256: hex::encode(s.program_code_sha256),
            deployed_slot: s.deployed_slot,
            token_program_code_sha256: hex::encode(s.token_program_code_sha256),
            amm_config: s.amm_config,
            pool_bump: s.pool_bump,
            observation: s.observation,
            vault0: s.vault0,
            vault1: s.vault1,
            raw_vault0: s.raw_vault0,
            raw_vault1: s.raw_vault1,
            mint0_decimals: s.mint0_decimals,
            mint1_decimals: s.mint1_decimals,
            bank_slot: s.bank_slot,
            bank_unix_timestamp: s.bank_unix_timestamp,
            bank_epoch: s.bank_epoch,
            trade_fee_rate: s.trade_fee_rate,
            protocol_fee_rate: s.protocol_fee_rate,
            fund_fee_rate: s.fund_fee_rate,
            tick_spacing: s.tick_spacing,
            sqrt_price_x64: amount(s.sqrt_price_x64)?,
            tick_current: s.tick_current,
            liquidity: amount(s.liquidity)?,
            fee_growth_global0: amount(s.fee_growth_global0)?,
            fee_growth_global1: amount(s.fee_growth_global1)?,
            protocol_fees0: s.protocol_fees0,
            protocol_fees1: s.protocol_fees1,
            fund_fees0: s.fund_fees0,
            fund_fees1: s.fund_fees1,
            open_time: s.open_time,
            recent_epoch: s.recent_epoch,
            bitmap: s.bitmap,
            tick_arrays: s
                .tick_arrays
                .into_iter()
                .map(|a| {
                    Ok(RaydiumCLMMArray {
                        address: a.address,
                        start_tick_index: a.start_tick_index,
                        recent_epoch: a.recent_epoch,
                        ticks: a
                            .ticks
                            .into_iter()
                            .map(|t| {
                                let net: [u8; 16] = t
                                    .liquidity_net_i128_be
                                    .try_into()
                                    .map_err(|_| err("RAYCLMM_NET_WIDTH"))?;
                                Ok(RaydiumCLMMTick {
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
            observation_initialized: s.observation_initialized,
            observation_index: s.observation_index,
            observation_recent_epoch: s.observation_recent_epoch,
            observations: s
                .observations
                .into_iter()
                .map(|o| RaydiumCLMMObservation {
                    block_timestamp: o.block_timestamp,
                    tick_cumulative: o.tick_cumulative,
                })
                .collect(),
        };
        out.validate_wire()?;
        Ok(out)
    }
}

fn pda(seeds: &[&[u8]]) -> Result<(String, u8)> {
    use sha2::{Digest, Sha256};
    let program = crate::raydium::validate_pubkey(PROGRAM_ID)?;
    for bump in (0..=255u8).rev() {
        let mut hash = Sha256::new();
        for seed in seeds {
            if seed.len() > 32 {
                return Err(err("SOLANA_PDA_SEED"));
            }
            hash.update(seed);
        }
        hash.update([bump]);
        hash.update(program);
        hash.update(b"ProgramDerivedAddress");
        let raw: [u8; 32] = hash.finalize().into();
        if curve25519_dalek::edwards::CompressedEdwardsY(raw)
            .decompress()
            .is_none()
        {
            return Ok((bs58::encode(raw).into_string(), bump));
        }
    }
    Err(err("SOLANA_PDA_DERIVATION"))
}
impl RaydiumCLMMState {
    fn validate_pdas(&self, p: &Pool) -> Result<()> {
        let config = crate::raydium::validate_pubkey(&self.amm_config)?;
        let mint0 = crate::raydium::validate_pubkey(&p.token0)?;
        let mint1 = crate::raydium::validate_pubkey(&p.token1)?;
        let seed = (self.seed_index as u16).to_le_bytes();
        let mut seeds = vec![
            b"pool".as_slice(),
            config.as_slice(),
            mint0.as_slice(),
            mint1.as_slice(),
        ];
        if self.seed_index > 0 {
            seeds.push(&seed);
        }
        let (address, bump) = pda(&seeds)?;
        if address != p.pool_id || u32::from(bump) != self.pool_bump {
            return Err(err("RAYCLMM_POOL_PDA"));
        }
        let pool = crate::raydium::validate_pubkey(&p.pool_id)?;
        for (expected, seeds) in [
            (
                &self.vault0,
                vec![b"pool_vault".as_slice(), pool.as_slice(), mint0.as_slice()],
            ),
            (
                &self.vault1,
                vec![b"pool_vault".as_slice(), pool.as_slice(), mint1.as_slice()],
            ),
            (
                &self.observation,
                vec![b"observation".as_slice(), pool.as_slice()],
            ),
        ] {
            if pda(&seeds)?.0 != *expected {
                return Err(err("RAYCLMM_DEPENDENCY_PDA"));
            }
        }
        if let Some(e) = &self.bitmap_extension {
            if pda(&[b"pool_tick_array_bitmap_extension", &pool])?.0 != e.address {
                return Err(err("RAYCLMM_EXTENSION_PDA"));
            }
        }
        for a in &self.tick_arrays {
            if pda(&[b"tick_array", &pool, &a.start_tick_index.to_be_bytes()])?.0 != a.address {
                return Err(err("RAYCLMM_ARRAY_PDA"));
            }
        }
        Ok(())
    }
}

impl RaydiumCLMMState {
    /// Original 512-bit extension ordering, including negative whole-group boundaries.
    pub fn array_initialized(&self, index: i32) -> Result<bool> {
        if self.bitmap.len() != 16 || self.tick_spacing == 0 || self.tick_spacing > 1000 {
            return Err(err("RAYCLMM_BITMAP_WIDTH"));
        }
        if (-512..=511).contains(&index) {
            let bit = (index + 512) as usize;
            return Ok(self.bitmap[bit / 64] & (1u64 << (bit % 64)) != 0);
        }
        let e = self
            .bitmap_extension
            .as_ref()
            .ok_or_else(|| err("RAYCLMM_BITMAP_EXTENSION_REQUIRED"))?;
        if e.positive.len() != 112 || e.negative.len() != 112 {
            return Err(err("RAYCLMM_EXTENSION_WIDTH"));
        }
        if !(-7680..=7679).contains(&index) {
            return Err(err("RAYCLMM_ARRAY_INDEX"));
        }
        let (words, offset) = if index >= 512 {
            (&e.positive, index - 512)
        } else {
            let group = (-index - 1) / 512 - 1;
            let bit = index + (group + 2) * 512;
            (&e.negative, group * 512 + bit)
        };
        let offset = offset as usize;
        Ok(words[offset / 64] & (1u64 << (offset % 64)) != 0)
    }
}
