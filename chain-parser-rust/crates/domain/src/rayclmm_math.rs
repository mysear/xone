//! Raydium CLMM 6d7a0f82d2f293c553c69aaf943377367c6c4cc6 Q64 tick math.
//! The original Q64 factors truncate after each multiply. Positive ticks invert
//! with u128::MAX; these prices differ from Orca and from shifted V3 prices.
use crate::{err, Result};
use num_bigint::BigUint;

pub const MIN_TICK: i32 = -443636;
pub const MAX_TICK: i32 = 443636;
pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;
pub const MAX_SQRT_PRICE_X64: u128 = 79226673521066979257578248091;

const FACTORS: [u64; 19] = [
    0xfffcb933bd6fb800,
    0xfff97272373d4000,
    0xfff2e50f5f657000,
    0xffe5caca7e10f000,
    0xffcb9843d60f7000,
    0xff973b41fa98e800,
    0xff2ea16466c9b000,
    0xfe5dee046a9a3800,
    0xfcbe86c7900bb000,
    0xf987a7253ac65800,
    0xf3392b0822bb6000,
    0xe7159475a2caf000,
    0xd097f3bdfd2f2000,
    0xa9f746462d9f8000,
    0x70d869a156f31c00,
    0x31be135f97ed3200,
    0x09aa508b5b85a500,
    0x005d6af8dedc582c,
    0x00002216e584f5fa,
];

pub fn sqrt_price_from_tick(tick: i32) -> Result<BigUint> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return Err(err("RAYCLMM_TICK_RANGE"));
    }
    let mut ratio = BigUint::from(1u8) << 64usize;
    for (bit, factor) in FACTORS.iter().enumerate() {
        if tick.unsigned_abs() & (1 << bit) != 0 {
            let product = ratio * BigUint::from(*factor);
            if product.bits() > 128 {
                return Err(err("RAYCLMM_TICK_OVERFLOW"));
            }
            ratio = product >> 64usize;
        }
    }
    Ok(if tick > 0 {
        BigUint::from(u128::MAX) / ratio
    } else {
        ratio
    })
}

/// Greatest original tick whose price is <= `price`. The source excludes the
/// maximum price from the inverse domain. Binary search resolves exactly the
/// same integer boundaries without a floating point logarithm.
pub fn tick_from_sqrt_price(price: &BigUint) -> Result<i32> {
    if price < &BigUint::from(MIN_SQRT_PRICE_X64) || price >= &BigUint::from(MAX_SQRT_PRICE_X64) {
        return Err(err("RAYCLMM_SQRT_PRICE_RANGE"));
    }
    let (mut low, mut high) = (MIN_TICK, MAX_TICK - 1);
    while low < high {
        let middle = low + (high - low + 1) / 2;
        if sqrt_price_from_tick(middle)? <= *price {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    Ok(low)
}
