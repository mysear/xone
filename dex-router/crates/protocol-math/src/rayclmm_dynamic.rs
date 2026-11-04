//! Pinned Raydium spacing-group fee transitions over the shared domain state.
use xone_domain::rayclmm_math::sqrt_price_from_tick;
use xone_domain::raydium_clmm::{RaydiumCLMMDynamicFee, MAX_TICK, MIN_TICK};
use xone_domain::{err, Result};

fn arithmetic() -> xone_domain::Error {
    err("RAYCLMM_ARITHMETIC")
}

pub(super) struct DynamicSwap {
    pub info: RaydiumCLMMDynamicFee,
    index: i32,
    spacing: i32,
}
impl DynamicSwap {
    pub fn new(mut info: RaydiumCLMMDynamicFee, tick: i32, spacing: u32, now: u64) -> Result<Self> {
        let spacing = i32::try_from(spacing).map_err(|_| arithmetic())?;
        if spacing <= 0 {
            return Err(arithmetic());
        }
        let index = tick.div_euclid(spacing);
        let elapsed = now.saturating_sub(info.last_update_timestamp);
        if elapsed >= u64::from(info.filter_period) {
            info.tick_spacing_index_reference = index;
            info.volatility_reference = if elapsed < u64::from(info.decay_period) {
                (u64::from(info.volatility_accumulator) * u64::from(info.reduction_factor) / 10_000)
                    .try_into()
                    .map_err(|_| arithmetic())?
            } else {
                0
            };
            info.last_update_timestamp = now;
        }
        Ok(Self {
            info,
            index,
            spacing,
        })
    }
    pub fn update_volatility(&mut self) -> Result<()> {
        let delta = self
            .info
            .tick_spacing_index_reference
            .checked_sub(self.index)
            .ok_or_else(arithmetic)?
            .unsigned_abs();
        self.info.volatility_accumulator =
            (u64::from(self.info.volatility_reference) + u64::from(delta) * 10_000)
                .min(u64::from(self.info.max_volatility_accumulator)) as u32;
        Ok(())
    }
    pub fn total_fee(&self, base: u32) -> Result<u32> {
        let crossed = self
            .info
            .volatility_accumulator
            .checked_mul(self.spacing as u32)
            .ok_or_else(arithmetic)?;
        let squared = u128::from(crossed) * u128::from(crossed);
        let numerator = u128::from(self.info.dynamic_fee_control)
            .checked_mul(squared)
            .ok_or_else(arithmetic)?;
        let dynamic = numerator
            .div_ceil(100_000 * 10_000 * 10_000u128)
            .min(100_000) as u32;
        Ok(base
            .checked_add(dynamic)
            .ok_or_else(arithmetic)?
            .min(100_000))
    }
    pub fn bounded_price(
        &self,
        target: u128,
        liquidity: u128,
        left: bool,
    ) -> Result<(bool, u128, Option<i32>)> {
        if liquidity == 0
            || self.info.volatility_accumulator == self.info.max_volatility_accumulator
        {
            return Ok((true, target, None));
        }
        let tick = if left {
            self.index
        } else {
            self.index.saturating_add(1)
        }
        .saturating_mul(self.spacing)
        .clamp(MIN_TICK, MAX_TICK);
        let price = u128::try_from(sqrt_price_from_tick(tick)?).map_err(|_| arithmetic())?;
        if (left && target > price) || (!left && target < price) {
            Ok((false, target, None))
        } else {
            Ok((false, price, Some(tick)))
        }
    }
    pub fn advance(
        &mut self,
        left: bool,
        skipped: bool,
        price: u128,
        tick: i32,
        next_tick: i32,
        next_price: u128,
    ) -> Result<()> {
        if skipped {
            let tick = if price == next_price { next_tick } else { tick };
            self.index = tick.div_euclid(self.spacing);
            if !left && tick % self.spacing == 0 {
                self.index = self.index.checked_sub(1).ok_or_else(arithmetic)?;
            }
            if self.info.volatility_accumulator != self.info.max_volatility_accumulator {
                self.update_volatility()?;
            }
        }
        self.index = self
            .index
            .checked_add(if left { -1 } else { 1 })
            .ok_or_else(arithmetic)?;
        Ok(())
    }
}
