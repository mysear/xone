use crate::{
    err,
    orca::{OrcaWhirlpoolState, MAX_TICK, MIN_TICK},
    wire, Result,
};
use serde::{Deserialize, Serialize};
pub const PROFILE: &str = "orca-whirlpools-adaptive";
pub const DYNAMIC_PROFILE: &str = "orca-whirlpools-adaptive-dynamic";
pub const DYNAMIC_ADAPTER: &str = "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1";
pub const REWARDS_PROFILE: &str = "orca-whirlpools-adaptive-rewards";
pub const REWARDS_ADAPTER: &str = "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1";
pub const ADAPTER: &str = "orca-whirlpools/cc683ec9-spl-adaptive-1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrcaAdaptiveState {
    pub whirlpool: String,
    pub trade_enable_timestamp: u64,
    pub filter_period: u32,
    pub decay_period: u32,
    pub reduction_factor: u32,
    pub adaptive_fee_control_factor: u32,
    pub max_volatility_accumulator: u32,
    pub tick_group_size: u32,
    pub major_swap_threshold_ticks: u32,
    pub constants_reserved: Vec<u8>,
    pub last_reference_update_timestamp: u64,
    pub last_major_swap_timestamp: u64,
    pub volatility_reference: u32,
    pub tick_group_index_reference: i32,
    pub volatility_accumulator: u32,
    pub variables_reserved: Vec<u8>,
    pub reserved: Vec<u8>,
}
impl OrcaAdaptiveState {
    pub fn validate(&self, s: &OrcaWhirlpoolState) -> Result<()> {
        if self.constants_reserved.len() != 16
            || self.variables_reserved.len() != 16
            || self.reserved.len() != 128
        {
            return Err(err("ORCA_ADAPTIVE_RESERVED_WIDTH"));
        }
        if self.filter_period == 0
            || self.filter_period > u16::MAX.into()
            || self.decay_period <= self.filter_period
            || self.decay_period > u16::MAX.into()
            || self.reduction_factor >= 10000
            || self.adaptive_fee_control_factor >= 100000
            || self.tick_group_size == 0
            || self.tick_group_size > s.tick_spacing
            || self.tick_group_size > u16::MAX.into()
            || !s.tick_spacing.is_multiple_of(self.tick_group_size)
            || self.major_swap_threshold_ticks == 0
            || self.major_swap_threshold_ticks > u16::MAX.into()
            || u64::from(self.major_swap_threshold_ticks) > u64::from(s.tick_spacing) * 88
            || u64::from(self.max_volatility_accumulator) * u64::from(self.tick_group_size)
                > u64::from(u32::MAX)
        {
            return Err(err("ORCA_ADAPTIVE_CONSTANTS"));
        }
        let group = self.tick_group_size as i32;
        if self.volatility_reference > self.max_volatility_accumulator
            || self.volatility_accumulator > self.max_volatility_accumulator
            || !((MIN_TICK - 1).div_euclid(group)..=MAX_TICK.div_euclid(group))
                .contains(&self.tick_group_index_reference)
        {
            return Err(err("ORCA_ADAPTIVE_VARIABLES"));
        }
        if self.trade_enable_timestamp > s.bank_unix_timestamp
            || self.last_reference_update_timestamp > s.bank_unix_timestamp
            || self.last_major_swap_timestamp > s.bank_unix_timestamp
        {
            return Err(err("ORCA_ADAPTIVE_TIMESTAMP"));
        }
        Ok(())
    }
    pub fn to_proto(&self) -> wire::OrcaAdaptiveState {
        wire::OrcaAdaptiveState {
            whirlpool: self.whirlpool.clone(),
            trade_enable_timestamp: self.trade_enable_timestamp,
            filter_period: self.filter_period,
            decay_period: self.decay_period,
            reduction_factor: self.reduction_factor,
            adaptive_fee_control_factor: self.adaptive_fee_control_factor,
            max_volatility_accumulator: self.max_volatility_accumulator,
            tick_group_size: self.tick_group_size,
            major_swap_threshold_ticks: self.major_swap_threshold_ticks,
            constants_reserved: self.constants_reserved.clone(),
            last_reference_update_timestamp: self.last_reference_update_timestamp,
            last_major_swap_timestamp: self.last_major_swap_timestamp,
            volatility_reference: self.volatility_reference,
            tick_group_index_reference: self.tick_group_index_reference,
            volatility_accumulator: self.volatility_accumulator,
            variables_reserved: self.variables_reserved.clone(),
            reserved: self.reserved.clone(),
        }
    }
    pub fn from_proto(s: wire::OrcaAdaptiveState) -> Self {
        Self {
            whirlpool: s.whirlpool,
            trade_enable_timestamp: s.trade_enable_timestamp,
            filter_period: s.filter_period,
            decay_period: s.decay_period,
            reduction_factor: s.reduction_factor,
            adaptive_fee_control_factor: s.adaptive_fee_control_factor,
            max_volatility_accumulator: s.max_volatility_accumulator,
            tick_group_size: s.tick_group_size,
            major_swap_threshold_ticks: s.major_swap_threshold_ticks,
            constants_reserved: s.constants_reserved,
            last_reference_update_timestamp: s.last_reference_update_timestamp,
            last_major_swap_timestamp: s.last_major_swap_timestamp,
            volatility_reference: s.volatility_reference,
            tick_group_index_reference: s.tick_group_index_reference,
            volatility_accumulator: s.volatility_accumulator,
            variables_reserved: s.variables_reserved,
            reserved: s.reserved,
        }
    }
}
