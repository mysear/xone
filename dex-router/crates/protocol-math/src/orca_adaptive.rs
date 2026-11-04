//! Orca cc683ec92 adaptive fee state kernel (hundredths of one basis point).
//!
//! Call `begin` once per swap, `update_volatility_accumulator` before every swap
//! step, then use that group's fee and bounded target. Advance with the matching
//! skip/non-skip method after the step. After the loop, update the major-swap
//! timestamp with the *whole swap's* prices and consume `finish` to persist state.
//! This module does not quote swaps, decode Oracle accounts, or enable a profile.
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use xone_domain::{
    err,
    orca_math::{
        sqrt_price_from_tick, tick_from_sqrt_price, MAX_SQRT_PRICE_X64, MAX_TICK,
        MIN_SQRT_PRICE_X64, MIN_TICK,
    },
    Result,
};

pub const FEE_RATE_HARD_LIMIT: u32 = 100_000;
pub const VOLATILITY_ACCUMULATOR_SCALE_FACTOR: u32 = 10_000;
pub const REDUCTION_FACTOR_DENOMINATOR: u16 = 10_000;
pub const ADAPTIVE_FEE_CONTROL_FACTOR_DENOMINATOR: u32 = 100_000;
pub const MAX_REFERENCE_AGE: u64 = 3600;

/// Group index and original Q64 price at an optional core range edge.
pub type CoreTickGroupBound = Option<(i32, u128)>;
pub type CoreTickGroupRange = (CoreTickGroupBound, CoreTickGroupBound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveFeeConstants {
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub adaptive_fee_control_factor: u32,
    pub max_volatility_accumulator: u32,
    pub tick_group_size: u16,
    pub major_swap_threshold_ticks: u16,
    #[serde(default)]
    pub reserved: [u8; 16],
}
impl AdaptiveFeeConstants {
    /// Exact upstream AdaptiveFeeConstants::validate_constants predicates.
    pub fn validate(&self, tick_spacing: u16) -> bool {
        self.filter_period != 0
            && self.decay_period > self.filter_period
            && self.adaptive_fee_control_factor < ADAPTIVE_FEE_CONTROL_FACTOR_DENOMINATOR
            && u64::from(self.max_volatility_accumulator) * u64::from(self.tick_group_size)
                <= u64::from(u32::MAX)
            && self.reduction_factor < REDUCTION_FACTOR_DENOMINATOR
            && self.tick_group_size != 0
            && self.tick_group_size <= tick_spacing
            && tick_spacing.is_multiple_of(self.tick_group_size)
            && self.major_swap_threshold_ticks != 0
            && i32::from(self.major_swap_threshold_ticks) <= i32::from(tick_spacing) * 88
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveFeeVariables {
    pub last_reference_update_timestamp: u64,
    pub last_major_swap_timestamp: u64,
    pub volatility_reference: u32,
    pub tick_group_index_reference: i32,
    pub volatility_accumulator: u32,
    #[serde(default)]
    pub reserved: [u8; 16],
}
impl AdaptiveFeeVariables {
    pub fn update_reference(
        &mut self,
        group: i32,
        timestamp: u64,
        constants: &AdaptiveFeeConstants,
    ) -> Result<()> {
        let latest = self
            .last_reference_update_timestamp
            .max(self.last_major_swap_timestamp);
        if timestamp < latest {
            return Err(err("ORCA_ADAPTIVE_INVALID_TIMESTAMP"));
        }
        let elapsed = timestamp - latest;
        if timestamp - self.last_reference_update_timestamp > MAX_REFERENCE_AGE {
            self.tick_group_index_reference = group;
            self.volatility_reference = 0;
            self.last_reference_update_timestamp = timestamp;
        } else if elapsed >= u64::from(constants.filter_period) {
            self.tick_group_index_reference = group;
            self.volatility_reference = if elapsed < u64::from(constants.decay_period) {
                (u64::from(self.volatility_accumulator) * u64::from(constants.reduction_factor)
                    / u64::from(REDUCTION_FACTOR_DENOMINATOR)) as u32
            } else {
                0
            };
            self.last_reference_update_timestamp = timestamp;
        }
        Ok(())
    }
    pub fn update_volatility_accumulator(
        &mut self,
        group: i32,
        constants: &AdaptiveFeeConstants,
    ) -> Result<()> {
        // Native uses signed i32 subtraction; reject its overflow domain explicitly.
        let delta = self
            .tick_group_index_reference
            .checked_sub(group)
            .ok_or_else(|| err("ORCA_ADAPTIVE_GROUP_RANGE"))?
            .unsigned_abs();
        self.volatility_accumulator = (u64::from(self.volatility_reference)
            + u64::from(delta) * u64::from(VOLATILITY_ACCUMULATOR_SCALE_FACTOR))
        .min(u64::from(constants.max_volatility_accumulator))
            as u32;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveFeeInfo {
    pub constants: AdaptiveFeeConstants,
    pub variables: AdaptiveFeeVariables,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveFeeContext {
    pub a_to_b: bool,
    pub current_tick_index: i32,
    pub timestamp: u64,
    pub static_fee_rate: u16,
    pub tick_spacing: u16,
    pub trade_enable_timestamp: u64,
}
#[derive(Debug, Clone)]
pub struct FeeRateManager {
    context: AdaptiveFeeContext,
    tick_group_index: i32,
    info: AdaptiveFeeInfo,
    lower: Option<(i32, u128)>,
    upper: Option<(i32, u128)>,
}
fn price(tick: i32) -> Result<u128> {
    u128::try_from(sqrt_price_from_tick(tick)?).map_err(|_| err("ORCA_ADAPTIVE_PRICE_RANGE"))
}
fn validate_price(p: u128) -> Result<()> {
    if !(MIN_SQRT_PRICE_X64..=MAX_SQRT_PRICE_X64).contains(&p) {
        return Err(err("ORCA_ADAPTIVE_PRICE_RANGE"));
    }
    Ok(())
}
impl FeeRateManager {
    /// Validates protocol constants plus the reachable serialized state envelope.
    /// Upstream's internal constructor assumes these account invariants already hold.
    pub fn begin(context: AdaptiveFeeContext, mut info: AdaptiveFeeInfo) -> Result<Self> {
        let c = info.constants;
        if !c.validate(context.tick_spacing) {
            return Err(err("ORCA_ADAPTIVE_INVALID_CONSTANTS"));
        }
        if !((MIN_TICK - 1)..=MAX_TICK).contains(&context.current_tick_index)
            || context.static_fee_rate > 60_000
        {
            return Err(err("ORCA_ADAPTIVE_INVALID_CONTEXT"));
        }
        if context.timestamp < context.trade_enable_timestamp {
            return Err(err("ORCA_ADAPTIVE_TRADE_NOT_ENABLED"));
        }
        let size = i32::from(c.tick_group_size);
        let v = info.variables;
        if v.volatility_reference > c.max_volatility_accumulator
            || v.volatility_accumulator > c.max_volatility_accumulator
            || !((MIN_TICK - 1).div_euclid(size)..=MAX_TICK.div_euclid(size))
                .contains(&v.tick_group_index_reference)
        {
            return Err(err("ORCA_ADAPTIVE_INVALID_STATE"));
        }
        let group = context.current_tick_index.div_euclid(size);
        info.variables
            .update_reference(group, context.timestamp, &c)?;
        let distance = (c.max_volatility_accumulator - info.variables.volatility_reference)
            .div_ceil(VOLATILITY_ACCUMULATOR_SCALE_FACTOR) as i32;
        let lower_index = info.variables.tick_group_index_reference - distance;
        let upper_index = info.variables.tick_group_index_reference + distance;
        let lower_tick = lower_index * size;
        let upper_tick = upper_index * size + size;
        let lower = if lower_tick > MIN_TICK {
            Some((lower_index, price(lower_tick)?))
        } else {
            None
        };
        let upper = if upper_tick < MAX_TICK {
            Some((upper_index, price(upper_tick)?))
        } else {
            None
        };
        Ok(Self {
            context,
            tick_group_index: group,
            info,
            lower,
            upper,
        })
    }
    pub fn tick_group_index(&self) -> i32 {
        self.tick_group_index
    }
    pub fn core_tick_group_range(&self) -> CoreTickGroupRange {
        (self.lower, self.upper)
    }
    pub fn get_next_adaptive_fee_info(&self) -> AdaptiveFeeInfo {
        self.info
    }
    pub fn finish(self) -> AdaptiveFeeInfo {
        self.info
    }
    pub fn update_volatility_accumulator(&mut self) -> Result<()> {
        self.info
            .variables
            .update_volatility_accumulator(self.tick_group_index, &self.info.constants)
    }
    pub fn advance_tick_group(&mut self) -> Result<()> {
        self.tick_group_index = self
            .tick_group_index
            .checked_add(if self.context.a_to_b { -1 } else { 1 })
            .ok_or_else(|| err("ORCA_ADAPTIVE_GROUP_RANGE"))?;
        Ok(())
    }
    pub fn get_total_fee_rate(&self) -> u32 {
        let c = self.info.constants;
        let crossed =
            u64::from(self.info.variables.volatility_accumulator) * u64::from(c.tick_group_size);
        let numerator = u128::from(c.adaptive_fee_control_factor) * u128::from(crossed * crossed);
        let denominator = u128::from(ADAPTIVE_FEE_CONTROL_FACTOR_DENOMINATOR)
            * u128::from(VOLATILITY_ACCUMULATOR_SCALE_FACTOR).pow(2);
        let adaptive = numerator
            .div_ceil(denominator)
            .min(u128::from(FEE_RATE_HARD_LIMIT)) as u32;
        (u32::from(self.context.static_fee_rate) + adaptive).min(FEE_RATE_HARD_LIMIT)
    }
    /// `target` is the original next-tick/price-limit target, not the current price.
    pub fn get_bounded_sqrt_price_target(
        &self,
        target: u128,
        liquidity: u128,
    ) -> Result<(u128, bool)> {
        validate_price(target)?;
        let c = self.info.constants;
        if c.adaptive_fee_control_factor == 0 || liquidity == 0 {
            return Ok((target, true));
        }
        if let Some((group, p)) = self.lower {
            if self.tick_group_index < group {
                return Ok((
                    if self.context.a_to_b {
                        target
                    } else {
                        target.min(p)
                    },
                    true,
                ));
            }
        }
        if let Some((group, p)) = self.upper {
            if self.tick_group_index > group {
                return Ok((
                    if self.context.a_to_b {
                        target.max(p)
                    } else {
                        target
                    },
                    true,
                ));
            }
        }
        let tick = i64::from(self.tick_group_index) * i64::from(c.tick_group_size)
            + if self.context.a_to_b {
                0
            } else {
                i64::from(c.tick_group_size)
            };
        let boundary = price(tick.clamp(i64::from(MIN_TICK), i64::from(MAX_TICK)) as i32)?;
        Ok((
            if self.context.a_to_b {
                target.max(boundary)
            } else {
                target.min(boundary)
            },
            false,
        ))
    }
    pub fn advance_tick_group_after_skip(
        &mut self,
        sqrt_price: u128,
        next_tick_sqrt_price: u128,
        next_tick_index: i32,
    ) -> Result<()> {
        validate_price(sqrt_price)?;
        validate_price(next_tick_sqrt_price)?;
        // Native trusts this cached tick/price pair. An external caller must prove it.
        if price(next_tick_index)? != next_tick_sqrt_price {
            return Err(err("ORCA_ADAPTIVE_TICK_PRICE_MISMATCH"));
        }
        let size = i32::from(self.info.constants.tick_group_size);
        let (tick, on_boundary) = if sqrt_price == next_tick_sqrt_price {
            (next_tick_index, next_tick_index % size == 0)
        } else {
            let tick = tick_from_sqrt_price(&BigUint::from(sqrt_price))?;
            (tick, tick % size == 0 && sqrt_price == price(tick)?)
        };
        let last = if on_boundary && !self.context.a_to_b {
            tick / size - 1
        } else {
            tick.div_euclid(size)
        };
        if (self.context.a_to_b && last < self.tick_group_index)
            || (!self.context.a_to_b && last > self.tick_group_index)
        {
            self.info
                .variables
                .update_volatility_accumulator(last, &self.info.constants)?;
            self.tick_group_index = last;
        }
        self.advance_tick_group()
    }
    /// Use pre/post prices for the complete swap, at the timestamp passed to begin.
    pub fn update_major_swap_timestamp(&mut self, pre: u128, post: u128) -> Result<()> {
        validate_price(pre)?;
        validate_price(post)?;
        let factor = price(i32::from(self.info.constants.major_swap_threshold_ticks))?;
        let target = (BigUint::from(pre.min(post)) * BigUint::from(factor)) >> 64usize;
        let target = u128::try_from(target).map_err(|_| err("ORCA_ADAPTIVE_PRICE_RANGE"))?;
        if pre.max(post) >= target {
            self.info.variables.last_major_swap_timestamp = self.context.timestamp;
        }
        Ok(())
    }
}
