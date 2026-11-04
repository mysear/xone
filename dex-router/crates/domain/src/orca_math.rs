//! Orca Whirlpools cc683ec92acb5e7606caa3d8e514d9383e9c7c11 integer tick math.
//! Positive ticks multiply original Q96 coefficients (floor after each product),
//! then shift 32; negative ticks multiply the original Q64 coefficients directly.
//! These are not V3 Q96 prices shifted into Q64.
use crate::{err, Result};
use num_bigint::BigUint;

pub const MIN_TICK: i32 = -443636;
pub const MAX_TICK: i32 = 443636;
pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;
pub const MAX_SQRT_PRICE_X64: u128 = 79226673515401279992447579055;

const POSITIVE: [u128; 19] = [
    79232123823359799118286999567,
    79236085330515764027303304731,
    79244008939048815603706035061,
    79259858533276714757314932305,
    79291567232598584799939703904,
    79355022692464371645785046466,
    79482085999252804386437311141,
    79736823300114093921829183326,
    80248749790819932309965073892,
    81282483887344747381513967011,
    83390072131320151908154831281,
    87770609709833776024991924138,
    97234110755111693312479820773,
    119332217159966728226237229890,
    179736315981702064433883588727,
    407748233172238350107850275304,
    2098478828474011932436660412517,
    55581415166113811149459800483533,
    38992368544603139932233054999993551,
];
const NEGATIVE: [u128; 19] = [
    18445821805675392311,
    18444899583751176498,
    18443055278223354162,
    18439367220385604838,
    18431993317065449817,
    18417254355718160513,
    18387811781193591352,
    18329067761203520168,
    18212142134806087854,
    17980523815641551639,
    17526086738831147013,
    16651378430235024244,
    15030750278693429944,
    12247334978882834399,
    8131365268884726200,
    3584323654723342297,
    696457651847595233,
    26294789957452057,
    37481735321082,
];

pub fn sqrt_price_from_tick(tick: i32) -> Result<BigUint> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return Err(err("ORCA_TICK_RANGE"));
    }
    let positive = tick >= 0;
    let (coefficients, shift) = if positive {
        (&POSITIVE, 96usize)
    } else {
        (&NEGATIVE, 64usize)
    };
    let mut ratio = BigUint::from(1u32) << shift;
    let magnitude = tick.unsigned_abs();
    for (bit, coefficient) in coefficients.iter().enumerate() {
        if magnitude & (1 << bit) != 0 {
            let product = ratio * BigUint::from(*coefficient);
            if product.bits() > if positive { 256 } else { 128 } {
                return Err(err("ORCA_TICK_OVERFLOW"));
            }
            ratio = product >> shift;
            if ratio.bits() > 128 {
                return Err(err("ORCA_TICK_OVERFLOW"));
            }
        }
    }
    Ok(if positive { ratio >> 32usize } else { ratio })
}

/// Exact floor inverse of the original integer price map. Binary search avoids
/// floating point and returns the original tick at every representable boundary.
pub fn tick_from_sqrt_price(price: &BigUint) -> Result<i32> {
    if price < &BigUint::from(MIN_SQRT_PRICE_X64) || price > &BigUint::from(MAX_SQRT_PRICE_X64) {
        return Err(err("ORCA_SQRT_PRICE_RANGE"));
    }
    let (mut low, mut high) = (MIN_TICK, MAX_TICK);
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
