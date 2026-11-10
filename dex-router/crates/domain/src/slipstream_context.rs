use crate::{err, v3::V3State, wire, Amount, Pool, Result};
use serde::{Deserialize, Serialize};
pub const PROFILE: &str = "aerodrome-slipstream-context";
pub const ADAPTER: &str = "aerodrome-slipstream/f8717faa-module-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamDynamicFee {
    pub base_fee: u32,
    pub fee_cap: u32,
    pub scaling_factor: u64,
    pub initial_fee_enabled: bool,
    pub initial_fee: u32,
    pub default_scaling_factor: u64,
    pub default_fee_cap: u32,
    pub seconds_ago: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamModule {
    pub kind: String,
    pub address: String,
    pub code_sha256: String,
    pub template_sha256: String,
    pub factory: String,
    pub custom_fee: u32,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::managed_v2_state::deserialize_present"
    )]
    pub dynamic: Option<SlipstreamDynamicFee>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamOrigin {
    pub origin: String,
    pub discount: u32,
    pub view_swap_fee: u32,
    pub view_unstaked_fee: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamTick {
    pub tick: i32,
    pub liquidity_gross: Amount,
    pub liquidity_net: String,
    pub staked_liquidity_net: String,
    pub fee_growth_outside0: Amount,
    pub fee_growth_outside1: Amount,
    pub reward_growth_outside: Amount,
    pub tick_cumulative_outside: String,
    pub seconds_per_liquidity_outside_x128: Amount,
    pub seconds_outside: u32,
    pub initialized: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamObservation {
    pub block_timestamp: u32,
    pub tick_cumulative: String,
    pub seconds_per_liquidity_cumulative_x128: Amount,
    pub initialized: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SlipstreamState {
    pub core: V3State,
    pub implementation_template_sha256: String,
    pub factory_template_sha256: String,
    pub token0_code_sha256: String,
    pub token1_code_sha256: String,
    pub factory_voter: String,
    pub voter_code_sha256: String,
    pub voter_gauge: String,
    pub voter_gauge_alive: bool,
    pub factory_default_fee: u32,
    pub factory_default_unstaked_fee: u32,
    pub factory_registered: bool,
    pub swap_fee_module: SlipstreamModule,
    pub unstaked_fee_module: SlipstreamModule,
    pub origins: Vec<SlipstreamOrigin>,
    pub view_origin: String,
    pub view_swap_fee: u32,
    pub view_unstaked_fee: u32,
    pub raw_balance0: Amount,
    pub raw_balance1: Amount,
    pub fee_growth_global0: Amount,
    pub fee_growth_global1: Amount,
    pub reward_growth_global: Amount,
    pub gauge_fees0: Amount,
    pub gauge_fees1: Amount,
    pub reward_rate: Amount,
    pub reward_reserve: Amount,
    pub period_finish: Amount,
    pub rollover: Amount,
    pub staked_liquidity: Amount,
    pub last_updated: u32,
    pub max_liquidity_per_tick: Amount,
    pub gauge: String,
    pub nft: String,
    pub factory_registry: String,
    pub observation_index: u32,
    pub observation_cardinality: u32,
    pub observation_cardinality_next: u32,
    pub unlocked: bool,
    pub ticks: Vec<SlipstreamTick>,
    pub observations: Vec<SlipstreamObservation>,
    pub block_hash: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub max_observations: u32,
    pub derived: bool,
}

pub mod policy;
use num_bigint::BigUint;
use num_traits::{One, Zero};
use std::collections::BTreeSet;
const IMPLEMENTATION_TEMPLATE: &str =
    "82642d5d25fe9bf09e9a7ef46673ca3922f4c47cbffddfc173ce4866aa65c2e4";
const FACTORY_TEMPLATE: &str = "0a403e2c20be57a38c85a3d298207b1b25d3ba849dcb36b43be50c7983bb7187";
const CUSTOM_SWAP_TEMPLATE: &str =
    "ddc9d6554ff8e4a364fb7965029dc7e7f000676c15650cbd336e6fe47209904c";
const CUSTOM_UNSTAKED_TEMPLATE: &str =
    "7f7aa9b5f067b30673550db0232bfec1c4c21344f1f1fa48bae97189e38fcae7";
const DYNAMIC_SWAP_TEMPLATE: &str =
    "187b7edb56da74a9674a63daac04c68e4e2f5d5d497e6ee8a8695f4fcec3bcc6";
pub const VOTER_RUNTIME_SHA256: &str =
    "621bb8e29a36ec7d8319e019a2ec47de06fee83e332465a893bd0b34955dff74";
fn digest(s: &str) -> bool {
    s.len() == 64 && s == s.to_ascii_lowercase() && hex::decode(s).is_ok()
}
fn address(s: &str, nonzero: bool) -> Result<()> {
    crate::protocols::validate_address(s)?;
    if nonzero && s == crate::singleton::ZERO_ADDRESS {
        return Err(err("SLIPSTREAM_ZERO_IDENTITY"));
    }
    Ok(())
}
pub fn signed56(s: &str) -> Result<i64> {
    let n = s.parse::<i64>().map_err(|_| err("SLIPSTREAM_SIGNED56"))?;
    if !(-(1i64 << 55)..1i64 << 55).contains(&n) || n.to_string() != s {
        return Err(err("SLIPSTREAM_SIGNED56"));
    }
    Ok(n)
}
impl SlipstreamDynamicFee {
    pub fn validate_wire(&self) -> Result<()> {
        if self.base_fee > 30000
            || self.fee_cap > 50000
            || self.scaling_factor > 1_000_000_000_000_000_000
            || self.initial_fee > 50000
            || self.default_scaling_factor > 1_000_000_000_000_000_000
            || self.default_fee_cap > 50000
            || !(2..131070).contains(&self.seconds_ago)
        {
            return Err(err("SLIPSTREAM_DYNAMIC_FEE_PARAMETERS"));
        }
        Ok(())
    }
}
impl SlipstreamModule {
    pub fn validate_wire(&self) -> Result<()> {
        address(&self.address, false)?;
        address(&self.factory, false)?;
        match self.kind.as_str() {
            "none" => {
                if self.address != crate::singleton::ZERO_ADDRESS
                    || self.factory != crate::singleton::ZERO_ADDRESS
                    || !self.code_sha256.is_empty()
                    || !self.template_sha256.is_empty()
                    || self.custom_fee != 0
                    || self.dynamic.is_some()
                {
                    return Err(err("SLIPSTREAM_ABSENT_MODULE"));
                }
            }
            "custom" | "dynamic" => {
                address(&self.address, true)?;
                address(&self.factory, true)?;
                if !digest(&self.code_sha256) || !digest(&self.template_sha256) {
                    return Err(err("SLIPSTREAM_MODULE_CODE"));
                }
                if self.kind == "custom" {
                    if self.dynamic.is_some() || self.custom_fee > 500000 {
                        return Err(err("SLIPSTREAM_CUSTOM_FEE"));
                    }
                } else {
                    if self.custom_fee != 0 {
                        return Err(err("SLIPSTREAM_DYNAMIC_CUSTOM_FIELD"));
                    }
                    self.dynamic
                        .as_ref()
                        .ok_or_else(|| err("SLIPSTREAM_DYNAMIC_STATE_REQUIRED"))?
                        .validate_wire()?;
                }
            }
            _ => return Err(err("SLIPSTREAM_UNKNOWN_MODULE")),
        }
        Ok(())
    }
    fn validate_role(&self, factory: &str, swap: bool) -> Result<()> {
        self.validate_wire()?;
        if self.kind == "none" {
            return Ok(());
        }
        if self.factory != factory {
            return Err(err("SLIPSTREAM_MODULE_FACTORY"));
        }
        let expected = if self.kind == "custom" {
            if swap {
                if self.custom_fee > 30000 {
                    return Err(err("SLIPSTREAM_CUSTOM_SWAP_RANGE"));
                }
                CUSTOM_SWAP_TEMPLATE
            } else {
                CUSTOM_UNSTAKED_TEMPLATE
            }
        } else {
            if !swap {
                return Err(err("SLIPSTREAM_UNSTAKED_MODULE_KIND"));
            }
            DYNAMIC_SWAP_TEMPLATE
        };
        if self.template_sha256 != expected {
            return Err(err("SLIPSTREAM_MODULE_TEMPLATE"));
        }
        Ok(())
    }
}
impl SlipstreamOrigin {
    pub fn validate_wire(&self) -> Result<()> {
        address(&self.origin, true)?;
        if self.discount > 500000 || self.view_swap_fee > 100000 || self.view_unstaked_fee > 1000000
        {
            return Err(err("SLIPSTREAM_ORIGIN_VALUES"));
        }
        Ok(())
    }
}
impl SlipstreamTick {
    pub fn validate_wire(&self) -> Result<()> {
        let net = crate::v3::parse_liquidity_net(&self.liquidity_net)?;
        let stake = crate::v3::parse_liquidity_net(&self.staked_liquidity_net)?;
        if !self.initialized
            || self.liquidity_gross.0.is_zero()
            || self.liquidity_gross.0.bits() > 128
            || BigUint::from(net.unsigned_abs()) > self.liquidity_gross.0
            || BigUint::from(stake.unsigned_abs()) > self.liquidity_gross.0
            || self.seconds_per_liquidity_outside_x128.0.bits() > 160
        {
            return Err(err("SLIPSTREAM_TICK_VALUES"));
        }
        signed56(&self.tick_cumulative_outside)?;
        for a in [
            &self.fee_growth_outside0,
            &self.fee_growth_outside1,
            &self.reward_growth_outside,
        ] {
            a.to_be_bytes()?;
        }
        Ok(())
    }
}
impl SlipstreamObservation {
    pub fn validate_wire(&self) -> Result<()> {
        signed56(&self.tick_cumulative)?;
        if self.seconds_per_liquidity_cumulative_x128.0.bits() > 160 {
            return Err(err("SLIPSTREAM_OBSERVATION_WIDTH"));
        }
        if !self.initialized
            && (self.tick_cumulative != "0"
                || !self.seconds_per_liquidity_cumulative_x128.0.is_zero()
                || self.block_timestamp > 1)
        {
            return Err(err("SLIPSTREAM_UNUSED_OBSERVATION"));
        }
        Ok(())
    }
}
impl SlipstreamState {
    pub fn validate_pool(&self, p: &Pool) -> Result<()> {
        if p.protocol != PROFILE
            || p.adapter_version != ADAPTER
            || p.token_behavior != "standard"
            || !p.chain_key.starts_with("evm:")
            || crate::validate_chain(&p.chain_key).is_err()
            || !p.reserve0.0.is_zero()
            || !p.reserve1.0.is_zero()
            || p.fee_numerator != 0
            || p.fee_denominator != 1000000
        {
            return Err(err("SLIPSTREAM_CONTEXT_PROFILE"));
        }
        let mut distinct = BTreeSet::new();
        let mut roles = vec![
            &p.pool_id,
            &p.token0,
            &p.token1,
            &self.core.factory,
            &self.core.implementation,
            &self.factory_voter,
        ];
        for module in [&self.swap_fee_module, &self.unstaked_fee_module] {
            if module.kind != "none" {
                roles.push(&module.address);
            }
        }
        for a in roles {
            address(a, true)?;
            if !distinct.insert(a) {
                return Err(err("SLIPSTREAM_IDENTITY_ALIAS"));
            }
        }
        if self.origins.iter().any(|o| distinct.contains(&o.origin)) {
            return Err(err("SLIPSTREAM_ORIGIN_CLASS"));
        }
        self.validate_wire()
    }
    pub fn validate_wire(&self) -> Result<()> {
        self.core.validate()?;
        if self.core.fee_protocol != 0
            || !self.unlocked
            || !self.factory_registered
            || self.implementation_template_sha256 != IMPLEMENTATION_TEMPLATE
            || self.factory_template_sha256 != FACTORY_TEMPLATE
            || self.voter_code_sha256 != VOTER_RUNTIME_SHA256
        {
            return Err(err("SLIPSTREAM_CONTEXT_CODE_OR_LOCK"));
        }
        address(&self.core.implementation, true)?;
        address(&self.core.factory, true)?;
        address(&self.factory_voter, true)?;
        for d in [
            &self.core.implementation_code_sha256,
            &self.core.factory_code_sha256,
            &self.token0_code_sha256,
            &self.token1_code_sha256,
        ] {
            if !digest(d) {
                return Err(err("SLIPSTREAM_CONTEXT_DIGEST"));
            }
        }
        if self.block_hash.len() != 66
            || !self.block_hash.starts_with("0x")
            || !digest(&self.block_hash[2..])
            || self.timestamp.checked_mul(1000).is_none()
        {
            return Err(err("SLIPSTREAM_CONTEXT_BANK"));
        }
        for a in [
            &self.gauge,
            &self.nft,
            &self.factory_registry,
            &self.voter_gauge,
            &self.view_origin,
        ] {
            address(a, false)?;
        }
        if !(1..=100000).contains(&self.factory_default_fee)
            || self.factory_default_unstaked_fee > 500000
            || self.view_swap_fee > 100000
            || self.view_unstaked_fee > 1000000
        {
            return Err(err("SLIPSTREAM_FACTORY_FEE_RANGE"));
        }
        self.swap_fee_module
            .validate_role(&self.core.factory, true)?;
        self.unstaked_fee_module
            .validate_role(&self.core.factory, false)?;
        if self.origins.len() > 16
            || self.swap_fee_module.kind == "dynamic" && self.origins.is_empty()
        {
            return Err(err("SLIPSTREAM_ORIGIN_SCOPE"));
        }
        let mut origins = BTreeSet::new();
        for o in &self.origins {
            o.validate_wire()?;
            if !origins.insert(&o.origin)
                || self.swap_fee_module.kind != "dynamic" && o.discount != 0
            {
                return Err(err("SLIPSTREAM_ORIGIN_SCOPE"));
            }
        }
        if self.swap_fee_module.kind == "dynamic" && !origins.contains(&self.view_origin) {
            return Err(err("SLIPSTREAM_VIEW_ORIGIN"));
        }
        if self.staked_liquidity.0.bits() > 128
            || self.staked_liquidity.0 > self.core.liquidity.0
            || self.gauge_fees0.0.bits() > 128
            || self.gauge_fees1.0.bits() > 128
        {
            return Err(err("SLIPSTREAM_STAKED_OR_GAUGE_RANGE"));
        }
        let min = (-887272 / self.core.tick_spacing) * self.core.tick_spacing;
        let max = (887272 / self.core.tick_spacing) * self.core.tick_spacing;
        let n = (max - min) / self.core.tick_spacing + 1;
        let expected = ((BigUint::one() << 128usize) - 1u8) / BigUint::from(n as u32);
        if self.max_liquidity_per_tick.0 != expected {
            return Err(err("SLIPSTREAM_MAX_LIQUIDITY_PER_TICK"));
        }
        if self.ticks.len() != self.core.ticks.len() {
            return Err(err("SLIPSTREAM_FULL_TICK_SET"));
        }
        for (a, b) in self.ticks.iter().zip(&self.core.ticks) {
            a.validate_wire()?;
            if a.tick != b.tick
                || a.liquidity_gross != b.liquidity_gross
                || a.liquidity_net != b.liquidity_net
                || a.liquidity_gross > self.max_liquidity_per_tick
            {
                return Err(err("SLIPSTREAM_FULL_TICK_PROJECTION"));
            }
        }
        for a in [
            &self.raw_balance0,
            &self.raw_balance1,
            &self.fee_growth_global0,
            &self.fee_growth_global1,
            &self.reward_growth_global,
            &self.reward_rate,
            &self.reward_reserve,
            &self.period_finish,
            &self.rollover,
        ] {
            a.to_be_bytes()?;
        }
        if !(1..=4096).contains(&self.max_observations)
            || self.observation_cardinality == 0
            || self.observation_cardinality > self.observation_cardinality_next
            || self.observation_cardinality_next > self.max_observations
            || self.observation_index >= self.observation_cardinality
            || self.observations.len() != self.observation_cardinality_next as usize
        {
            return Err(err("SLIPSTREAM_OBSERVATION_SCOPE"));
        }
        for o in &self.observations {
            o.validate_wire()?;
        }
        if !self.observations[self.observation_index as usize].initialized {
            return Err(err("SLIPSTREAM_LATEST_OBSERVATION_REQUIRED"));
        }
        for zero in [false, true] {
            let mut l = self.core.liquidity.0.clone();
            let mut s = self.staked_liquidity.0.clone();
            let ticks: Vec<_> = if zero {
                self.ticks
                    .iter()
                    .rev()
                    .filter(|t| t.tick <= self.core.tick)
                    .collect()
            } else {
                self.ticks
                    .iter()
                    .filter(|t| t.tick > self.core.tick)
                    .collect()
            };
            for tick in ticks {
                for (value, net) in [
                    (&mut l, crate::v3::parse_liquidity_net(&tick.liquidity_net)?),
                    (
                        &mut s,
                        crate::v3::parse_liquidity_net(&tick.staked_liquidity_net)?,
                    ),
                ] {
                    let magnitude = BigUint::from(net.unsigned_abs());
                    if (net < 0) ^ zero {
                        if *value < magnitude {
                            return Err(err("SLIPSTREAM_LIQUIDITY_CURVE"));
                        }
                        *value -= magnitude;
                    } else {
                        *value += magnitude;
                    }
                    if value.bits() > 128 {
                        return Err(err("SLIPSTREAM_LIQUIDITY_CURVE"));
                    }
                }
                if s > l {
                    return Err(err("SLIPSTREAM_STAKED_CURVE"));
                }
            }
        }
        if !self.observations[0].initialized {
            return Err(err("SLIPSTREAM_FIRST_OBSERVATION_REQUIRED"));
        }
        if self
            .observations
            .iter()
            .skip(self.observation_cardinality as usize)
            .any(|o| o.initialized)
        {
            return Err(err("SLIPSTREAM_OBSERVATION_PREFIX"));
        }
        let active = &self.observations[..self.observation_cardinality as usize];
        let full = active.iter().all(|o| o.initialized);
        if !full
            && active
                .iter()
                .enumerate()
                .any(|(i, o)| o.initialized != (i <= self.observation_index as usize))
        {
            return Err(err("SLIPSTREAM_OBSERVATION_PREFIX"));
        }
        let length = if full {
            active.len()
        } else {
            self.observation_index as usize + 1
        };
        let first = if full {
            (self.observation_index as usize + 1) % active.len()
        } else {
            0
        };
        let mut previous = None;
        for step in 0..length {
            let observation = &active[(first + step) % active.len()];
            let age = (self.timestamp as u32).wrapping_sub(observation.block_timestamp);
            if previous.is_some_and(|before| before <= age) {
                return Err(err("SLIPSTREAM_OBSERVATION_TIME_ORDER"));
            }
            previous = Some(age);
        }
        policy::validate_views(self)
    }
}

fn wire_digest(value: &str, block: bool) -> Result<Vec<u8>> {
    hex::decode(if block {
        value
            .strip_prefix("0x")
            .ok_or_else(|| err("SLIPSTREAM_WIRE_BLOCK_HASH"))?
    } else {
        value
    })
    .map_err(|_| err("SLIPSTREAM_WIRE_DIGEST"))
}
fn wire_decode_digest(value: Vec<u8>, block: bool) -> Result<String> {
    if !matches!(value.len(), 0 | 32) {
        return Err(err("SLIPSTREAM_WIRE_DIGEST_WIDTH"));
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
            .ok_or_else(|| err("SLIPSTREAM_WIRE_AMOUNT_REQUIRED"))?
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
        return Err(err("SLIPSTREAM_WIRE_SIGNED_WIDTH"));
    }
    if bits == 128 {
        Ok(i128::from_be_bytes(
            value
                .try_into()
                .map_err(|_| err("SLIPSTREAM_WIRE_SIGNED_WIDTH"))?,
        )
        .to_string())
    } else {
        let mut raw = [if value[0] & 128 != 0 { 255 } else { 0 }; 8];
        raw[1..].copy_from_slice(&value);
        Ok(i64::from_be_bytes(raw).to_string())
    }
}

impl SlipstreamDynamicFee {
    pub fn to_proto(&self) -> Result<wire::SlipstreamDynamicFee> {
        self.validate_wire()?;
        Ok(wire::SlipstreamDynamicFee {
            base_fee: Some(self.base_fee),
            fee_cap: Some(self.fee_cap),
            scaling_factor: Some(self.scaling_factor),
            initial_fee_enabled: Some(self.initial_fee_enabled),
            initial_fee: Some(self.initial_fee),
            default_scaling_factor: Some(self.default_scaling_factor),
            default_fee_cap: Some(self.default_fee_cap),
            seconds_ago: Some(self.seconds_ago),
        })
    }
    pub fn from_proto(s: wire::SlipstreamDynamicFee) -> Result<Self> {
        let out = Self {
            base_fee: s
                .base_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_BASE_FEE_REQUIRED"))?,
            fee_cap: s
                .fee_cap
                .ok_or_else(|| err("SLIPSTREAM_WIRE_FEE_CAP_REQUIRED"))?,
            scaling_factor: s
                .scaling_factor
                .ok_or_else(|| err("SLIPSTREAM_WIRE_SCALING_FACTOR_REQUIRED"))?,
            initial_fee_enabled: s
                .initial_fee_enabled
                .ok_or_else(|| err("SLIPSTREAM_WIRE_INITIAL_FEE_ENABLED_REQUIRED"))?,
            initial_fee: s
                .initial_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_INITIAL_FEE_REQUIRED"))?,
            default_scaling_factor: s
                .default_scaling_factor
                .ok_or_else(|| err("SLIPSTREAM_WIRE_DEFAULT_SCALING_FACTOR_REQUIRED"))?,
            default_fee_cap: s
                .default_fee_cap
                .ok_or_else(|| err("SLIPSTREAM_WIRE_DEFAULT_FEE_CAP_REQUIRED"))?,
            seconds_ago: s
                .seconds_ago
                .ok_or_else(|| err("SLIPSTREAM_WIRE_SECONDS_AGO_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl SlipstreamModule {
    pub fn to_proto(&self) -> Result<wire::SlipstreamModule> {
        self.validate_wire()?;
        Ok(wire::SlipstreamModule {
            kind: self.kind.clone(),
            address: self.address.clone(),
            code_sha256: wire_digest(&self.code_sha256, false)?,
            template_sha256: wire_digest(&self.template_sha256, false)?,
            factory: self.factory.clone(),
            custom_fee: Some(self.custom_fee),
            dynamic: self
                .dynamic
                .as_ref()
                .map(SlipstreamDynamicFee::to_proto)
                .transpose()?,
        })
    }
    pub fn from_proto(s: wire::SlipstreamModule) -> Result<Self> {
        let out = Self {
            kind: s.kind,
            address: s.address,
            code_sha256: wire_decode_digest(s.code_sha256, false)?,
            template_sha256: wire_decode_digest(s.template_sha256, false)?,
            factory: s.factory,
            custom_fee: s
                .custom_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_CUSTOM_FEE_REQUIRED"))?,
            dynamic: s
                .dynamic
                .map(SlipstreamDynamicFee::from_proto)
                .transpose()?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl SlipstreamOrigin {
    pub fn to_proto(&self) -> Result<wire::SlipstreamOrigin> {
        self.validate_wire()?;
        Ok(wire::SlipstreamOrigin {
            origin: self.origin.clone(),
            discount: Some(self.discount),
            view_swap_fee: Some(self.view_swap_fee),
            view_unstaked_fee: Some(self.view_unstaked_fee),
        })
    }
    pub fn from_proto(s: wire::SlipstreamOrigin) -> Result<Self> {
        let out = Self {
            origin: s.origin,
            discount: s
                .discount
                .ok_or_else(|| err("SLIPSTREAM_WIRE_DISCOUNT_REQUIRED"))?,
            view_swap_fee: s
                .view_swap_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_VIEW_SWAP_FEE_REQUIRED"))?,
            view_unstaked_fee: s
                .view_unstaked_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_VIEW_UNSTAKED_FEE_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl SlipstreamTick {
    pub fn to_proto(&self) -> Result<wire::SlipstreamTick> {
        self.validate_wire()?;
        Ok(wire::SlipstreamTick {
            tick: Some(self.tick),
            liquidity_gross: wire_amount(&self.liquidity_gross)?,
            liquidity_net: wire_signed(&self.liquidity_net, 128)?,
            staked_liquidity_net: wire_signed(&self.staked_liquidity_net, 128)?,
            fee_growth_outside0: wire_amount(&self.fee_growth_outside0)?,
            fee_growth_outside1: wire_amount(&self.fee_growth_outside1)?,
            reward_growth_outside: wire_amount(&self.reward_growth_outside)?,
            tick_cumulative_outside: wire_signed(&self.tick_cumulative_outside, 56)?,
            seconds_per_liquidity_outside_x128: wire_amount(
                &self.seconds_per_liquidity_outside_x128,
            )?,
            seconds_outside: Some(self.seconds_outside),
            initialized: Some(self.initialized),
        })
    }
    pub fn from_proto(s: wire::SlipstreamTick) -> Result<Self> {
        let out = Self {
            tick: s.tick.ok_or_else(|| err("SLIPSTREAM_WIRE_TICK_REQUIRED"))?,
            liquidity_gross: wire_decode_amount(s.liquidity_gross)?,
            liquidity_net: wire_decode_signed(s.liquidity_net, 128)?,
            staked_liquidity_net: wire_decode_signed(s.staked_liquidity_net, 128)?,
            fee_growth_outside0: wire_decode_amount(s.fee_growth_outside0)?,
            fee_growth_outside1: wire_decode_amount(s.fee_growth_outside1)?,
            reward_growth_outside: wire_decode_amount(s.reward_growth_outside)?,
            tick_cumulative_outside: wire_decode_signed(s.tick_cumulative_outside, 56)?,
            seconds_per_liquidity_outside_x128: wire_decode_amount(
                s.seconds_per_liquidity_outside_x128,
            )?,
            seconds_outside: s
                .seconds_outside
                .ok_or_else(|| err("SLIPSTREAM_WIRE_SECONDS_OUTSIDE_REQUIRED"))?,
            initialized: s
                .initialized
                .ok_or_else(|| err("SLIPSTREAM_WIRE_INITIALIZED_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl SlipstreamObservation {
    pub fn to_proto(&self) -> Result<wire::SlipstreamObservation> {
        self.validate_wire()?;
        Ok(wire::SlipstreamObservation {
            block_timestamp: Some(self.block_timestamp),
            tick_cumulative: wire_signed(&self.tick_cumulative, 56)?,
            seconds_per_liquidity_cumulative_x128: wire_amount(
                &self.seconds_per_liquidity_cumulative_x128,
            )?,
            initialized: Some(self.initialized),
        })
    }
    pub fn from_proto(s: wire::SlipstreamObservation) -> Result<Self> {
        let out = Self {
            block_timestamp: s
                .block_timestamp
                .ok_or_else(|| err("SLIPSTREAM_WIRE_BLOCK_TIMESTAMP_REQUIRED"))?,
            tick_cumulative: wire_decode_signed(s.tick_cumulative, 56)?,
            seconds_per_liquidity_cumulative_x128: wire_decode_amount(
                s.seconds_per_liquidity_cumulative_x128,
            )?,
            initialized: s
                .initialized
                .ok_or_else(|| err("SLIPSTREAM_WIRE_INITIALIZED_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
impl SlipstreamState {
    pub fn to_proto(&self) -> Result<wire::SlipstreamState> {
        self.validate_wire()?;
        Ok(wire::SlipstreamState {
            core: Some(self.core.to_proto()?),
            implementation_template_sha256: wire_digest(
                &self.implementation_template_sha256,
                false,
            )?,
            factory_template_sha256: wire_digest(&self.factory_template_sha256, false)?,
            token0_code_sha256: wire_digest(&self.token0_code_sha256, false)?,
            token1_code_sha256: wire_digest(&self.token1_code_sha256, false)?,
            factory_voter: self.factory_voter.clone(),
            voter_code_sha256: wire_digest(&self.voter_code_sha256, false)?,
            voter_gauge: self.voter_gauge.clone(),
            voter_gauge_alive: Some(self.voter_gauge_alive),
            factory_default_fee: Some(self.factory_default_fee),
            factory_default_unstaked_fee: Some(self.factory_default_unstaked_fee),
            factory_registered: Some(self.factory_registered),
            swap_fee_module: Some(self.swap_fee_module.to_proto()?),
            unstaked_fee_module: Some(self.unstaked_fee_module.to_proto()?),
            origins: self
                .origins
                .iter()
                .map(SlipstreamOrigin::to_proto)
                .collect::<Result<Vec<_>>>()?,
            view_origin: self.view_origin.clone(),
            view_swap_fee: Some(self.view_swap_fee),
            view_unstaked_fee: Some(self.view_unstaked_fee),
            raw_balance0: wire_amount(&self.raw_balance0)?,
            raw_balance1: wire_amount(&self.raw_balance1)?,
            fee_growth_global0: wire_amount(&self.fee_growth_global0)?,
            fee_growth_global1: wire_amount(&self.fee_growth_global1)?,
            reward_growth_global: wire_amount(&self.reward_growth_global)?,
            gauge_fees0: wire_amount(&self.gauge_fees0)?,
            gauge_fees1: wire_amount(&self.gauge_fees1)?,
            reward_rate: wire_amount(&self.reward_rate)?,
            reward_reserve: wire_amount(&self.reward_reserve)?,
            period_finish: wire_amount(&self.period_finish)?,
            rollover: wire_amount(&self.rollover)?,
            staked_liquidity: wire_amount(&self.staked_liquidity)?,
            last_updated: Some(self.last_updated),
            max_liquidity_per_tick: wire_amount(&self.max_liquidity_per_tick)?,
            gauge: self.gauge.clone(),
            nft: self.nft.clone(),
            factory_registry: self.factory_registry.clone(),
            observation_index: Some(self.observation_index),
            observation_cardinality: Some(self.observation_cardinality),
            observation_cardinality_next: Some(self.observation_cardinality_next),
            unlocked: Some(self.unlocked),
            ticks: self
                .ticks
                .iter()
                .map(SlipstreamTick::to_proto)
                .collect::<Result<Vec<_>>>()?,
            observations: self
                .observations
                .iter()
                .map(SlipstreamObservation::to_proto)
                .collect::<Result<Vec<_>>>()?,
            block_hash: wire_digest(&self.block_hash, true)?,
            block_number: Some(self.block_number),
            timestamp: Some(self.timestamp),
            max_observations: Some(self.max_observations),
            derived: Some(self.derived),
        })
    }
    pub fn from_proto(s: wire::SlipstreamState) -> Result<Self> {
        let out = Self {
            core: V3State::from_proto(s.core.ok_or_else(|| err("SLIPSTREAM_WIRE_CORE_REQUIRED"))?)?,
            implementation_template_sha256: wire_decode_digest(
                s.implementation_template_sha256,
                false,
            )?,
            factory_template_sha256: wire_decode_digest(s.factory_template_sha256, false)?,
            token0_code_sha256: wire_decode_digest(s.token0_code_sha256, false)?,
            token1_code_sha256: wire_decode_digest(s.token1_code_sha256, false)?,
            factory_voter: s.factory_voter,
            voter_code_sha256: wire_decode_digest(s.voter_code_sha256, false)?,
            voter_gauge: s.voter_gauge,
            voter_gauge_alive: s
                .voter_gauge_alive
                .ok_or_else(|| err("SLIPSTREAM_WIRE_VOTER_GAUGE_ALIVE_REQUIRED"))?,
            factory_default_fee: s
                .factory_default_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_FACTORY_DEFAULT_FEE_REQUIRED"))?,
            factory_default_unstaked_fee: s
                .factory_default_unstaked_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_FACTORY_DEFAULT_UNSTAKED_FEE_REQUIRED"))?,
            factory_registered: s
                .factory_registered
                .ok_or_else(|| err("SLIPSTREAM_WIRE_FACTORY_REGISTERED_REQUIRED"))?,
            swap_fee_module: SlipstreamModule::from_proto(
                s.swap_fee_module
                    .ok_or_else(|| err("SLIPSTREAM_WIRE_SWAP_FEE_MODULE_REQUIRED"))?,
            )?,
            unstaked_fee_module: SlipstreamModule::from_proto(
                s.unstaked_fee_module
                    .ok_or_else(|| err("SLIPSTREAM_WIRE_UNSTAKED_FEE_MODULE_REQUIRED"))?,
            )?,
            origins: s
                .origins
                .into_iter()
                .map(SlipstreamOrigin::from_proto)
                .collect::<Result<Vec<_>>>()?,
            view_origin: s.view_origin,
            view_swap_fee: s
                .view_swap_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_VIEW_SWAP_FEE_REQUIRED"))?,
            view_unstaked_fee: s
                .view_unstaked_fee
                .ok_or_else(|| err("SLIPSTREAM_WIRE_VIEW_UNSTAKED_FEE_REQUIRED"))?,
            raw_balance0: wire_decode_amount(s.raw_balance0)?,
            raw_balance1: wire_decode_amount(s.raw_balance1)?,
            fee_growth_global0: wire_decode_amount(s.fee_growth_global0)?,
            fee_growth_global1: wire_decode_amount(s.fee_growth_global1)?,
            reward_growth_global: wire_decode_amount(s.reward_growth_global)?,
            gauge_fees0: wire_decode_amount(s.gauge_fees0)?,
            gauge_fees1: wire_decode_amount(s.gauge_fees1)?,
            reward_rate: wire_decode_amount(s.reward_rate)?,
            reward_reserve: wire_decode_amount(s.reward_reserve)?,
            period_finish: wire_decode_amount(s.period_finish)?,
            rollover: wire_decode_amount(s.rollover)?,
            staked_liquidity: wire_decode_amount(s.staked_liquidity)?,
            last_updated: s
                .last_updated
                .ok_or_else(|| err("SLIPSTREAM_WIRE_LAST_UPDATED_REQUIRED"))?,
            max_liquidity_per_tick: wire_decode_amount(s.max_liquidity_per_tick)?,
            gauge: s.gauge,
            nft: s.nft,
            factory_registry: s.factory_registry,
            observation_index: s
                .observation_index
                .ok_or_else(|| err("SLIPSTREAM_WIRE_OBSERVATION_INDEX_REQUIRED"))?,
            observation_cardinality: s
                .observation_cardinality
                .ok_or_else(|| err("SLIPSTREAM_WIRE_OBSERVATION_CARDINALITY_REQUIRED"))?,
            observation_cardinality_next: s
                .observation_cardinality_next
                .ok_or_else(|| err("SLIPSTREAM_WIRE_OBSERVATION_CARDINALITY_NEXT_REQUIRED"))?,
            unlocked: s
                .unlocked
                .ok_or_else(|| err("SLIPSTREAM_WIRE_UNLOCKED_REQUIRED"))?,
            ticks: s
                .ticks
                .into_iter()
                .map(SlipstreamTick::from_proto)
                .collect::<Result<Vec<_>>>()?,
            observations: s
                .observations
                .into_iter()
                .map(SlipstreamObservation::from_proto)
                .collect::<Result<Vec<_>>>()?,
            block_hash: wire_decode_digest(s.block_hash, true)?,
            block_number: s
                .block_number
                .ok_or_else(|| err("SLIPSTREAM_WIRE_BLOCK_NUMBER_REQUIRED"))?,
            timestamp: s
                .timestamp
                .ok_or_else(|| err("SLIPSTREAM_WIRE_TIMESTAMP_REQUIRED"))?,
            max_observations: s
                .max_observations
                .ok_or_else(|| err("SLIPSTREAM_WIRE_MAX_OBSERVATIONS_REQUIRED"))?,
            derived: s
                .derived
                .ok_or_else(|| err("SLIPSTREAM_WIRE_DERIVED_REQUIRED"))?,
        };
        out.validate_wire()?;
        Ok(out)
    }
}
