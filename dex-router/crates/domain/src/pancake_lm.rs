use crate::{err, v3::V3State, wire, Amount, Pool, Result};
use serde::{Deserialize, Serialize};
pub const PROFILE: &str = "pancake-v3-lm-context";
pub const ADAPTER: &str = "pancake-v3/98684794-lm-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeLmTick {
    pub tick: i32,
    pub liquidity_gross: Amount,
    pub liquidity_net: String,
    pub reward_growth_outside: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeLedgerTick {
    pub tick: i32,
    pub liquidity_gross: Amount,
    pub liquidity_net: String,
    pub fee_growth_outside0: Amount,
    pub fee_growth_outside1: Amount,
    pub tick_cumulative_outside: String,
    pub seconds_per_liquidity_outside_x128: Amount,
    pub seconds_outside: u32,
    pub initialized: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeObservation {
    pub block_timestamp: u32,
    pub tick_cumulative: String,
    pub seconds_per_liquidity_cumulative_x128: Amount,
    pub initialized: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeMasterChef {
    pub address: String,
    pub code_sha256: String,
    pub template_sha256: String,
    pub cake: String,
    pub weth: String,
    pub position_manager: String,
    pub pool_pid: Amount,
    pub pool_address: String,
    pub pool_token0: String,
    pub pool_token1: String,
    pub pool_fee: u32,
    pub pool_alloc_point: Amount,
    pub total_alloc_point: Amount,
    pub latest_period_cake_per_second: Amount,
    pub latest_period_end_time: Amount,
    pub view_reward_per_second: Amount,
    pub view_end_time: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeLm {
    pub address: String,
    pub code_sha256: String,
    pub template_sha256: String,
    pub pool: String,
    pub master_chef: String,
    pub reward_growth_global: Amount,
    pub liquidity: Amount,
    pub last_reward_timestamp: u32,
    pub ticks: Vec<PancakeLmTick>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PancakeLmState {
    pub core: V3State,
    pub tick: i32,
    pub fee_protocol: u32,
    pub pool_template_sha256: String,
    pub factory_template_sha256: String,
    pub token0_code_sha256: String,
    pub token1_code_sha256: String,
    pub factory_registered: bool,
    pub master_chef: PancakeMasterChef,
    pub lm: PancakeLm,
    pub raw_balance0: Amount,
    pub raw_balance1: Amount,
    pub fee_growth_global0: Amount,
    pub fee_growth_global1: Amount,
    pub protocol_fees0: Amount,
    pub protocol_fees1: Amount,
    pub max_liquidity_per_tick: Amount,
    pub observation_index: u32,
    pub observation_cardinality: u32,
    pub observation_cardinality_next: u32,
    pub unlocked: bool,
    pub ticks: Vec<PancakeLedgerTick>,
    pub observations: Vec<PancakeObservation>,
    pub block_hash: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub max_observations: u32,
    pub derived: bool,
}

use num_bigint::BigUint;
use num_traits::{One, Zero};
use std::collections::BTreeSet;
pub(crate) const POOL_TEMPLATE: &str =
    "7b39db168808f3bb63aac29a396464c672dd6509dade03f7078499fc3803684c";
pub(crate) const LM_TEMPLATE: &str =
    "3d6848ee2e6cf3d9c6857bab9a877f5bdd59a830d16a6638485745ebbcd9b197";
pub(crate) const CHEF_TEMPLATE: &str =
    "c79545617ca1a25eaf407e57b1231031b574a63332e23c409af64157f896ba24";
pub(crate) const FACTORY_TEMPLATE: &str =
    "ca9caada1721c9dffa86fa20f7cf8ccc3c199e9019646f42c2e85617f896bed5";
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
fn address(s: &str) -> Result<()> {
    crate::protocols::validate_address(s)?;
    if s == crate::singleton::ZERO_ADDRESS {
        return Err(err("PANCAKE_LM_ZERO_ADDRESS"));
    }
    Ok(())
}
pub fn signed56(s: &str) -> Result<i64> {
    let n = s.parse::<i64>().map_err(|_| err("PANCAKE_LM_SIGNED56"))?;
    if n.to_string() != s || !(-(1i64 << 55)..(1i64 << 55)).contains(&n) {
        return Err(err("PANCAKE_LM_SIGNED56"));
    }
    Ok(n)
}
fn amount(a: &Amount, bits: u64) -> Result<()> {
    a.to_be_bytes()?;
    if a.0.bits() > bits {
        return Err(err("PANCAKE_LM_AMOUNT_WIDTH"));
    }
    Ok(())
}
impl PancakeLmTick {
    pub fn validate_wire(&self) -> Result<()> {
        amount(&self.liquidity_gross, 128)?;
        amount(&self.reward_growth_outside, 256)?;
        let net = crate::v3::parse_liquidity_net(&self.liquidity_net)?;
        if self.tick < crate::v3::MIN_TICK
            || self.tick > crate::v3::MAX_TICK
            || BigUint::from(net.unsigned_abs()) > self.liquidity_gross.0
            || self.liquidity_gross.0.is_zero() && !self.reward_growth_outside.0.is_zero()
        {
            return Err(err("PANCAKE_LM_TICK_VALUES"));
        }
        Ok(())
    }
}
impl PancakeLedgerTick {
    pub fn validate_wire(&self) -> Result<()> {
        amount(&self.liquidity_gross, 128)?;
        amount(&self.fee_growth_outside0, 256)?;
        amount(&self.fee_growth_outside1, 256)?;
        amount(&self.seconds_per_liquidity_outside_x128, 160)?;
        signed56(&self.tick_cumulative_outside)?;
        let net = crate::v3::parse_liquidity_net(&self.liquidity_net)?;
        if !self.initialized
            || self.liquidity_gross.0.is_zero()
            || BigUint::from(net.unsigned_abs()) > self.liquidity_gross.0
        {
            return Err(err("PANCAKE_LM_CORE_TICK_VALUES"));
        }
        Ok(())
    }
}
impl PancakeObservation {
    pub fn validate_wire(&self) -> Result<()> {
        signed56(&self.tick_cumulative)?;
        amount(&self.seconds_per_liquidity_cumulative_x128, 160)?;
        if !self.initialized
            && (self.tick_cumulative != "0"
                || !self.seconds_per_liquidity_cumulative_x128.0.is_zero()
                || self.block_timestamp > 1)
        {
            return Err(err("PANCAKE_LM_UNUSED_OBSERVATION"));
        }
        Ok(())
    }
}
impl PancakeMasterChef {
    pub fn validate_wire(&self) -> Result<()> {
        for s in [
            &self.address,
            &self.cake,
            &self.weth,
            &self.position_manager,
            &self.pool_address,
            &self.pool_token0,
            &self.pool_token1,
        ] {
            address(s)?;
        }
        if !digest(&self.code_sha256)
            || self.template_sha256 != CHEF_TEMPLATE
            || self.pool_pid.0.is_zero()
            || self.pool_fee >= 1000000
        {
            return Err(err("PANCAKE_LM_MASTER_CHEF_IDENTITY"));
        }
        for a in [
            &self.pool_pid,
            &self.pool_alloc_point,
            &self.total_alloc_point,
            &self.latest_period_cake_per_second,
            &self.latest_period_end_time,
            &self.view_reward_per_second,
            &self.view_end_time,
        ] {
            amount(a, 256)?;
        }
        let rate = if self.total_alloc_point.0.is_zero() {
            BigUint::zero()
        } else {
            let product = &self.latest_period_cake_per_second.0 * &self.pool_alloc_point.0;
            if product.bits() > 256 {
                return Err(err("PANCAKE_LM_MASTER_CHEF_CHECKED_OVERFLOW"));
            }
            product / &self.total_alloc_point.0
        };
        if rate != self.view_reward_per_second.0
            || self.view_end_time != self.latest_period_end_time
        {
            return Err(err("PANCAKE_LM_REWARD_VIEW_MISMATCH"));
        }
        Ok(())
    }
}
impl PancakeLm {
    pub fn validate_wire(&self) -> Result<()> {
        for s in [&self.address, &self.pool, &self.master_chef] {
            address(s)?;
        }
        if !digest(&self.code_sha256)
            || self.template_sha256 != LM_TEMPLATE
            || self.ticks.len() > 4096
        {
            return Err(err("PANCAKE_LM_IMPLEMENTATION"));
        }
        amount(&self.reward_growth_global, 256)?;
        amount(&self.liquidity, 128)?;
        for (i, t) in self.ticks.iter().enumerate() {
            t.validate_wire()?;
            if i > 0 && t.tick <= self.ticks[i - 1].tick {
                return Err(err("PANCAKE_LM_TICK_ORDER"));
            }
        }
        Ok(())
    }
}
impl PancakeLmState {
    pub fn validate_pool(&self, p: &Pool) -> Result<()> {
        if p.protocol != PROFILE
            || p.adapter_version != ADAPTER
            || p.token_behavior != "standard"
            || !p.chain_key.starts_with("evm:")
            || crate::validate_chain(&p.chain_key).is_err()
            || !p.reserve0.0.is_zero()
            || !p.reserve1.0.is_zero()
            || p.fee_numerator >= 1000000
            || p.fee_denominator != 1000000
        {
            return Err(err("PANCAKE_LM_PROFILE"));
        }
        if p.token0 >= p.token1 {
            return Err(err("PANCAKE_LM_TOKEN_ORDER"));
        }
        let mut roles = BTreeSet::new();
        for a in [
            &p.pool_id,
            &p.token0,
            &p.token1,
            &self.core.factory,
            &self.lm.address,
            &self.master_chef.address,
        ] {
            address(a)?;
            if !roles.insert(a) {
                return Err(err("PANCAKE_LM_IDENTITY_ALIAS"));
            }
        }
        let chef = &self.master_chef;
        if self.lm.pool != p.pool_id
            || self.lm.master_chef != chef.address
            || chef.pool_address != p.pool_id
            || chef.pool_token0 != p.token0
            || chef.pool_token1 != p.token1
            || u64::from(chef.pool_fee) != p.fee_numerator
        {
            return Err(err("PANCAKE_LM_PARENT_BINDING"));
        }
        self.validate_wire()
    }
    pub fn validate_wire(&self) -> Result<()> {
        self.core.validate()?;
        if self.tick != self.core.tick || self.fee_protocol != self.core.fee_protocol {
            return Err(err("PANCAKE_LM_CORE_OBSERVATION_BINDING"));
        }
        self.master_chef.validate_wire()?;
        self.lm.validate_wire()?;
        if !self.core.implementation.is_empty()
            || !self.core.implementation_code_sha256.is_empty()
            || self.pool_template_sha256 != POOL_TEMPLATE
            || self.factory_template_sha256 != FACTORY_TEMPLATE
            || !self.factory_registered
            || !self.unlocked
            || self.core.fee_protocol & 65535 > 10000
            || self.core.fee_protocol >> 16 > 10000
        {
            return Err(err("PANCAKE_LM_CODE_OR_LOCK"));
        }
        for d in [
            &self.core.factory_code_sha256,
            &self.token0_code_sha256,
            &self.token1_code_sha256,
        ] {
            if !digest(d) {
                return Err(err("PANCAKE_LM_CODE_DIGEST"));
            }
        }
        if self.block_hash.len() != 66
            || !self.block_hash.starts_with("0x")
            || !digest(&self.block_hash[2..])
            || self.timestamp.checked_mul(1000).is_none()
        {
            return Err(err("PANCAKE_LM_BANK"));
        }
        for a in [
            &self.raw_balance0,
            &self.raw_balance1,
            &self.fee_growth_global0,
            &self.fee_growth_global1,
        ] {
            amount(a, 256)?;
        }
        for a in [
            &self.protocol_fees0,
            &self.protocol_fees1,
            &self.max_liquidity_per_tick,
        ] {
            amount(a, 128)?;
        }
        if self.protocol_fees0.0 > self.raw_balance0.0
            || self.protocol_fees1.0 > self.raw_balance1.0
        {
            return Err(err("PANCAKE_LM_PROTOCOL_RAW_BALANCE"));
        }
        let spacing = self.core.tick_spacing;
        let min = (crate::v3::MIN_TICK / spacing) * spacing;
        let max = (crate::v3::MAX_TICK / spacing) * spacing;
        let count = (max - min) / spacing + 1;
        if self.max_liquidity_per_tick.0
            != ((BigUint::one() << 128usize) - 1u8) / BigUint::from(count as u32)
        {
            return Err(err("PANCAKE_LM_MAX_LIQUIDITY"));
        }
        if self.ticks.len() != self.core.ticks.len() || self.lm.ticks.len() != self.core.ticks.len()
        {
            return Err(err("PANCAKE_LM_COMPLETE_TICK_SCOPE"));
        }
        for ((t, l), c) in self.ticks.iter().zip(&self.lm.ticks).zip(&self.core.ticks) {
            t.validate_wire()?;
            if t.tick != c.tick
                || t.liquidity_gross != c.liquidity_gross
                || t.liquidity_net != c.liquidity_net
                || l.tick != c.tick
                || t.liquidity_gross.0 > self.max_liquidity_per_tick.0
                || l.liquidity_gross.0 > self.max_liquidity_per_tick.0
            {
                return Err(err("PANCAKE_LM_TICK_CORRESPONDENCE"));
            }
        }
        if self.max_observations == 0
            || self.max_observations > 4096
            || self.observation_cardinality == 0
            || self.observation_cardinality > self.observation_cardinality_next
            || self.observation_cardinality_next > self.max_observations
            || self.observation_index >= self.observation_cardinality
            || self.observations.len() != self.observation_cardinality_next as usize
        {
            return Err(err("PANCAKE_LM_OBSERVATION_SCOPE"));
        }
        for observation in &self.observations {
            observation.validate_wire()?;
        }
        if !self.observations[self.observation_index as usize].initialized
            || !self.observations[0].initialized
        {
            return Err(err("PANCAKE_LM_LATEST_OBSERVATION"));
        }
        // Both directions must remain representable over the declared complete window.
        for zero in [false, true] {
            let mut liquidity = self.lm.liquidity.0.clone();
            let iter: Box<dyn Iterator<Item = &PancakeLmTick>> = if zero {
                Box::new(
                    self.lm
                        .ticks
                        .iter()
                        .rev()
                        .filter(|t| t.tick <= self.core.tick),
                )
            } else {
                Box::new(self.lm.ticks.iter().filter(|t| t.tick > self.core.tick))
            };
            for t in iter {
                let net = crate::v3::parse_liquidity_net(&t.liquidity_net)?;
                let m = BigUint::from(net.unsigned_abs());
                liquidity = if (net < 0) ^ zero {
                    if liquidity < m {
                        return Err(err("PANCAKE_LM_LIQUIDITY_CURVE"));
                    }
                    liquidity - m
                } else {
                    liquidity + m
                };
                if liquidity.bits() > 128 {
                    return Err(err("PANCAKE_LM_LIQUIDITY_CURVE"));
                }
            }
        }
        Ok(())
    }
}

fn wire_digest(value: &str, block: bool) -> Result<Vec<u8>> {
    hex::decode(if block {
        value
            .strip_prefix("0x")
            .ok_or_else(|| err("PANCAKE_LM_WIRE_BLOCK_HASH"))?
    } else {
        value
    })
    .map_err(|_| err("PANCAKE_LM_WIRE_DIGEST"))
}
fn wire_decode_digest(value: Vec<u8>, block: bool) -> Result<String> {
    if !matches!(value.len(), 0 | 32) {
        return Err(err("PANCAKE_LM_WIRE_DIGEST_WIDTH"));
    }
    Ok(format!(
        "{}{}",
        if block { "0x" } else { "" },
        hex::encode(value)
    ))
}
fn wire_amount(value: &Amount) -> Result<Option<wire::Amount>> {
    Ok(Some(wire::Amount {
        uint256_be: value.to_be_bytes()?.to_vec(),
    }))
}
fn wire_decode_amount(value: Option<wire::Amount>) -> Result<Amount> {
    Amount::from_be_bytes(
        &value
            .ok_or_else(|| err("PANCAKE_LM_WIRE_AMOUNT_REQUIRED"))?
            .uint256_be,
    )
}
fn wire_signed(value: &str, bits: usize) -> Result<Vec<u8>> {
    if bits == 128 {
        Ok(crate::v3::parse_liquidity_net(value)?
            .to_be_bytes()
            .to_vec())
    } else {
        Ok(signed56(value)?.to_be_bytes()[1..].to_vec())
    }
}
fn wire_decode_signed(value: Vec<u8>, bits: usize) -> Result<String> {
    if value.len() != bits / 8 {
        return Err(err("PANCAKE_LM_WIRE_SIGNED_WIDTH"));
    }
    if bits == 128 {
        Ok(i128::from_be_bytes(
            value
                .try_into()
                .map_err(|_| err("PANCAKE_LM_WIRE_SIGNED_WIDTH"))?,
        )
        .to_string())
    } else {
        let mut raw = [if value[0] & 128 != 0 { 255 } else { 0 }; 8];
        raw[1..].copy_from_slice(&value);
        Ok(i64::from_be_bytes(raw).to_string())
    }
}

impl PancakeLmTick {
    pub fn to_proto(&self) -> Result<wire::PancakeLmTick> {
        self.validate_wire()?;
        Ok(wire::PancakeLmTick {
            tick: Some(self.tick),
            liquidity_gross: wire_amount(&self.liquidity_gross)?,
            liquidity_net: wire_signed(&self.liquidity_net, 128)?,
            reward_growth_outside: wire_amount(&self.reward_growth_outside)?,
        })
    }
    pub fn from_proto(s: wire::PancakeLmTick) -> Result<Self> {
        let out = Self {
            tick: s.tick.ok_or_else(|| err("PANCAKE_LM_WIRE_TICK_REQUIRED"))?,
            liquidity_gross: wire_decode_amount(s.liquidity_gross)?,
            liquidity_net: wire_decode_signed(s.liquidity_net, 128)?,
            reward_growth_outside: wire_decode_amount(s.reward_growth_outside)?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl PancakeLedgerTick {
    pub fn to_proto(&self) -> Result<wire::PancakeLedgerTick> {
        self.validate_wire()?;
        Ok(wire::PancakeLedgerTick {
            tick: Some(self.tick),
            liquidity_gross: wire_amount(&self.liquidity_gross)?,
            liquidity_net: wire_signed(&self.liquidity_net, 128)?,
            fee_growth_outside0: wire_amount(&self.fee_growth_outside0)?,
            fee_growth_outside1: wire_amount(&self.fee_growth_outside1)?,
            tick_cumulative_outside: wire_signed(&self.tick_cumulative_outside, 56)?,
            seconds_per_liquidity_outside_x128: wire_amount(
                &self.seconds_per_liquidity_outside_x128,
            )?,
            seconds_outside: Some(self.seconds_outside),
            initialized: Some(self.initialized),
        })
    }
    pub fn from_proto(s: wire::PancakeLedgerTick) -> Result<Self> {
        let out = Self {
            tick: s.tick.ok_or_else(|| err("PANCAKE_LM_WIRE_TICK_REQUIRED"))?,
            liquidity_gross: wire_decode_amount(s.liquidity_gross)?,
            liquidity_net: wire_decode_signed(s.liquidity_net, 128)?,
            fee_growth_outside0: wire_decode_amount(s.fee_growth_outside0)?,
            fee_growth_outside1: wire_decode_amount(s.fee_growth_outside1)?,
            tick_cumulative_outside: wire_decode_signed(s.tick_cumulative_outside, 56)?,
            seconds_per_liquidity_outside_x128: wire_decode_amount(
                s.seconds_per_liquidity_outside_x128,
            )?,
            seconds_outside: s
                .seconds_outside
                .ok_or_else(|| err("PANCAKE_LM_WIRE_SECONDS_OUTSIDE_REQUIRED"))?,
            initialized: s
                .initialized
                .ok_or_else(|| err("PANCAKE_LM_WIRE_INITIALIZED_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl PancakeObservation {
    pub fn to_proto(&self) -> Result<wire::PancakeObservation> {
        self.validate_wire()?;
        Ok(wire::PancakeObservation {
            block_timestamp: Some(self.block_timestamp),
            tick_cumulative: wire_signed(&self.tick_cumulative, 56)?,
            seconds_per_liquidity_cumulative_x128: wire_amount(
                &self.seconds_per_liquidity_cumulative_x128,
            )?,
            initialized: Some(self.initialized),
        })
    }
    pub fn from_proto(s: wire::PancakeObservation) -> Result<Self> {
        let out = Self {
            block_timestamp: s
                .block_timestamp
                .ok_or_else(|| err("PANCAKE_LM_WIRE_BLOCK_TIMESTAMP_REQUIRED"))?,
            tick_cumulative: wire_decode_signed(s.tick_cumulative, 56)?,
            seconds_per_liquidity_cumulative_x128: wire_decode_amount(
                s.seconds_per_liquidity_cumulative_x128,
            )?,
            initialized: s
                .initialized
                .ok_or_else(|| err("PANCAKE_LM_WIRE_INITIALIZED_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl PancakeMasterChef {
    pub fn to_proto(&self) -> Result<wire::PancakeMasterChef> {
        self.validate_wire()?;
        Ok(wire::PancakeMasterChef {
            address: self.address.clone(),
            code_sha256: wire_digest(&self.code_sha256, false)?,
            template_sha256: wire_digest(&self.template_sha256, false)?,
            cake: self.cake.clone(),
            weth: self.weth.clone(),
            position_manager: self.position_manager.clone(),
            pool_pid: wire_amount(&self.pool_pid)?,
            pool_address: self.pool_address.clone(),
            pool_token0: self.pool_token0.clone(),
            pool_token1: self.pool_token1.clone(),
            pool_fee: Some(self.pool_fee),
            pool_alloc_point: wire_amount(&self.pool_alloc_point)?,
            total_alloc_point: wire_amount(&self.total_alloc_point)?,
            latest_period_cake_per_second: wire_amount(&self.latest_period_cake_per_second)?,
            latest_period_end_time: wire_amount(&self.latest_period_end_time)?,
            view_reward_per_second: wire_amount(&self.view_reward_per_second)?,
            view_end_time: wire_amount(&self.view_end_time)?,
        })
    }
    pub fn from_proto(s: wire::PancakeMasterChef) -> Result<Self> {
        let out = Self {
            address: s.address,
            code_sha256: wire_decode_digest(s.code_sha256, false)?,
            template_sha256: wire_decode_digest(s.template_sha256, false)?,
            cake: s.cake,
            weth: s.weth,
            position_manager: s.position_manager,
            pool_pid: wire_decode_amount(s.pool_pid)?,
            pool_address: s.pool_address,
            pool_token0: s.pool_token0,
            pool_token1: s.pool_token1,
            pool_fee: s
                .pool_fee
                .ok_or_else(|| err("PANCAKE_LM_WIRE_POOL_FEE_REQUIRED"))?,
            pool_alloc_point: wire_decode_amount(s.pool_alloc_point)?,
            total_alloc_point: wire_decode_amount(s.total_alloc_point)?,
            latest_period_cake_per_second: wire_decode_amount(s.latest_period_cake_per_second)?,
            latest_period_end_time: wire_decode_amount(s.latest_period_end_time)?,
            view_reward_per_second: wire_decode_amount(s.view_reward_per_second)?,
            view_end_time: wire_decode_amount(s.view_end_time)?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl PancakeLm {
    pub fn to_proto(&self) -> Result<wire::PancakeLm> {
        self.validate_wire()?;
        Ok(wire::PancakeLm {
            address: self.address.clone(),
            code_sha256: wire_digest(&self.code_sha256, false)?,
            template_sha256: wire_digest(&self.template_sha256, false)?,
            pool: self.pool.clone(),
            master_chef: self.master_chef.clone(),
            reward_growth_global: wire_amount(&self.reward_growth_global)?,
            liquidity: wire_amount(&self.liquidity)?,
            last_reward_timestamp: Some(self.last_reward_timestamp),
            ticks: self
                .ticks
                .iter()
                .map(PancakeLmTick::to_proto)
                .collect::<Result<Vec<_>>>()?,
        })
    }
    pub fn from_proto(s: wire::PancakeLm) -> Result<Self> {
        let out = Self {
            address: s.address,
            code_sha256: wire_decode_digest(s.code_sha256, false)?,
            template_sha256: wire_decode_digest(s.template_sha256, false)?,
            pool: s.pool,
            master_chef: s.master_chef,
            reward_growth_global: wire_decode_amount(s.reward_growth_global)?,
            liquidity: wire_decode_amount(s.liquidity)?,
            last_reward_timestamp: s
                .last_reward_timestamp
                .ok_or_else(|| err("PANCAKE_LM_WIRE_LAST_REWARD_TIMESTAMP_REQUIRED"))?,
            ticks: s
                .ticks
                .into_iter()
                .map(PancakeLmTick::from_proto)
                .collect::<Result<Vec<_>>>()?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl PancakeLmState {
    pub fn to_proto(&self) -> Result<wire::PancakeLmState> {
        self.validate_wire()?;
        Ok(wire::PancakeLmState {
            core: Some(self.core.to_proto()?),
            tick: Some(self.tick),
            fee_protocol: Some(self.fee_protocol),
            pool_template_sha256: wire_digest(&self.pool_template_sha256, false)?,
            factory_template_sha256: wire_digest(&self.factory_template_sha256, false)?,
            token0_code_sha256: wire_digest(&self.token0_code_sha256, false)?,
            token1_code_sha256: wire_digest(&self.token1_code_sha256, false)?,
            factory_registered: Some(self.factory_registered),
            master_chef: Some(self.master_chef.to_proto()?),
            lm: Some(self.lm.to_proto()?),
            raw_balance0: wire_amount(&self.raw_balance0)?,
            raw_balance1: wire_amount(&self.raw_balance1)?,
            fee_growth_global0: wire_amount(&self.fee_growth_global0)?,
            fee_growth_global1: wire_amount(&self.fee_growth_global1)?,
            protocol_fees0: wire_amount(&self.protocol_fees0)?,
            protocol_fees1: wire_amount(&self.protocol_fees1)?,
            max_liquidity_per_tick: wire_amount(&self.max_liquidity_per_tick)?,
            observation_index: Some(self.observation_index),
            observation_cardinality: Some(self.observation_cardinality),
            observation_cardinality_next: Some(self.observation_cardinality_next),
            unlocked: Some(self.unlocked),
            ticks: self
                .ticks
                .iter()
                .map(PancakeLedgerTick::to_proto)
                .collect::<Result<Vec<_>>>()?,
            observations: self
                .observations
                .iter()
                .map(PancakeObservation::to_proto)
                .collect::<Result<Vec<_>>>()?,
            block_hash: wire_digest(&self.block_hash, true)?,
            block_number: Some(self.block_number),
            timestamp: Some(self.timestamp),
            max_observations: Some(self.max_observations),
            derived: Some(self.derived),
        })
    }
    pub fn from_proto(s: wire::PancakeLmState) -> Result<Self> {
        let out = Self {
            core: V3State::from_proto(s.core.ok_or_else(|| err("PANCAKE_LM_WIRE_CORE_REQUIRED"))?)?,
            tick: s.tick.ok_or_else(|| err("PANCAKE_LM_WIRE_TICK_REQUIRED"))?,
            fee_protocol: s
                .fee_protocol
                .ok_or_else(|| err("PANCAKE_LM_WIRE_FEE_PROTOCOL_REQUIRED"))?,
            pool_template_sha256: wire_decode_digest(s.pool_template_sha256, false)?,
            factory_template_sha256: wire_decode_digest(s.factory_template_sha256, false)?,
            token0_code_sha256: wire_decode_digest(s.token0_code_sha256, false)?,
            token1_code_sha256: wire_decode_digest(s.token1_code_sha256, false)?,
            factory_registered: s
                .factory_registered
                .ok_or_else(|| err("PANCAKE_LM_WIRE_FACTORY_REGISTERED_REQUIRED"))?,
            master_chef: PancakeMasterChef::from_proto(
                s.master_chef
                    .ok_or_else(|| err("PANCAKE_LM_WIRE_MASTER_CHEF_REQUIRED"))?,
            )?,
            lm: PancakeLm::from_proto(s.lm.ok_or_else(|| err("PANCAKE_LM_WIRE_LM_REQUIRED"))?)?,
            raw_balance0: wire_decode_amount(s.raw_balance0)?,
            raw_balance1: wire_decode_amount(s.raw_balance1)?,
            fee_growth_global0: wire_decode_amount(s.fee_growth_global0)?,
            fee_growth_global1: wire_decode_amount(s.fee_growth_global1)?,
            protocol_fees0: wire_decode_amount(s.protocol_fees0)?,
            protocol_fees1: wire_decode_amount(s.protocol_fees1)?,
            max_liquidity_per_tick: wire_decode_amount(s.max_liquidity_per_tick)?,
            observation_index: s
                .observation_index
                .ok_or_else(|| err("PANCAKE_LM_WIRE_OBSERVATION_INDEX_REQUIRED"))?,
            observation_cardinality: s
                .observation_cardinality
                .ok_or_else(|| err("PANCAKE_LM_WIRE_OBSERVATION_CARDINALITY_REQUIRED"))?,
            observation_cardinality_next: s
                .observation_cardinality_next
                .ok_or_else(|| err("PANCAKE_LM_WIRE_OBSERVATION_CARDINALITY_NEXT_REQUIRED"))?,
            unlocked: s
                .unlocked
                .ok_or_else(|| err("PANCAKE_LM_WIRE_UNLOCKED_REQUIRED"))?,
            ticks: s
                .ticks
                .into_iter()
                .map(PancakeLedgerTick::from_proto)
                .collect::<Result<Vec<_>>>()?,
            observations: s
                .observations
                .into_iter()
                .map(PancakeObservation::from_proto)
                .collect::<Result<Vec<_>>>()?,
            block_hash: wire_decode_digest(s.block_hash, true)?,
            block_number: s
                .block_number
                .ok_or_else(|| err("PANCAKE_LM_WIRE_BLOCK_NUMBER_REQUIRED"))?,
            timestamp: s
                .timestamp
                .ok_or_else(|| err("PANCAKE_LM_WIRE_TIMESTAMP_REQUIRED"))?,
            max_observations: s
                .max_observations
                .ok_or_else(|| err("PANCAKE_LM_WIRE_MAX_OBSERVATIONS_REQUIRED"))?,
            derived: s
                .derived
                .ok_or_else(|| err("PANCAKE_LM_WIRE_DERIVED_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
