//! Exact integer and spacing-boundary checks against pinned source semantics.
#[path = "../src/rayclmm_dynamic.rs"]
mod dynamic;
use dynamic::DynamicSwap;
use xone_domain::rayclmm_math::sqrt_price_from_tick;
use xone_domain::raydium_clmm::RaydiumCLMMDynamicFee;

fn info() -> RaydiumCLMMDynamicFee {
    serde_json::from_value(serde_json::json!({
        "filter_period": 10, "decay_period": 100, "reduction_factor": 5000,
        "dynamic_fee_control": 1000, "max_volatility_accumulator": 1000000,
        "tick_spacing_index_reference": 0, "volatility_reference": 0,
        "volatility_accumulator": 0, "last_update_timestamp": 1000
    }))
    .unwrap()
}
fn price(tick: i32) -> u128 {
    sqrt_price_from_tick(tick).unwrap().try_into().unwrap()
}

#[test]
fn negative_spacing_index_uses_floor_and_clamps_both_directions() {
    for (tick, index) in [
        (-65, -2),
        (-64, -1),
        (-63, -1),
        (-1, -1),
        (0, 0),
        (63, 0),
        (64, 1),
    ] {
        let mut i = info();
        i.volatility_accumulator = 40001;
        let d = DynamicSwap::new(i, tick, 64, 1010).unwrap();
        assert_eq!(d.info.tick_spacing_index_reference, index);
        assert_eq!(d.info.volatility_reference, 20000);
        assert_eq!(
            d.bounded_price(price(-1000), 100, true).unwrap(),
            (false, price(index * 64), Some(index * 64))
        );
        assert_eq!(
            d.bounded_price(price(1000), 100, false).unwrap(),
            (false, price((index + 1) * 64), Some((index + 1) * 64))
        );
    }
}

#[test]
fn fee_uses_ceiling_then_caps_dynamic_and_combined_rate() {
    let mut i = info();
    i.dynamic_fee_control = 1;
    i.volatility_accumulator = 1;
    let d = DynamicSwap::new(i, 0, 1, 1000).unwrap();
    assert_eq!(d.total_fee(3000).unwrap(), 3001);
    let mut i = info();
    i.dynamic_fee_control = 1000;
    i.volatility_accumulator = 10000;
    let d = DynamicSwap::new(i, 0, 10, 1000).unwrap();
    assert_eq!(d.total_fee(3000).unwrap(), 3001); // exact division
    let mut i = info();
    i.dynamic_fee_control = 99999;
    i.volatility_accumulator = 1000000;
    let d = DynamicSwap::new(i, 0, 64, 1000).unwrap();
    assert_eq!(d.total_fee(3000).unwrap(), 100000);
    assert_eq!(d.total_fee(0).unwrap(), 100000);
}

#[test]
fn total_fee_cap_applies_when_only_sum_exceeds_cap() {
    let mut i = info();
    i.dynamic_fee_control = 99999;
    i.volatility_accumulator = 1000000;
    let d = DynamicSwap::new(i, 0, 1, 1000).unwrap();
    assert_eq!(d.total_fee(0).unwrap(), 10000);
    assert_eq!(d.total_fee(95000).unwrap(), 100000);
}

#[test]
fn zero_liquidity_skips_groups_and_reanchors_at_reached_tick() {
    for left in [true, false] {
        let mut d = DynamicSwap::new(info(), 0, 64, 1000).unwrap();
        d.update_volatility().unwrap();
        let tick = if left { -256 } else { 256 };
        assert_eq!(
            d.bounded_price(price(tick), 0, left).unwrap(),
            (true, price(tick), None)
        );
        d.advance(
            left,
            true,
            price(tick),
            if left { tick - 1 } else { tick },
            tick,
            price(tick),
        )
        .unwrap();
        assert_eq!(
            d.info.volatility_accumulator,
            if left { 40000 } else { 30000 }
        );
        d.update_volatility().unwrap();
        assert_eq!(
            d.info.volatility_accumulator,
            if left { 50000 } else { 40000 }
        );
    }
}

#[test]
fn capped_skip_retains_cap_until_next_iteration_and_can_return_below_cap() {
    let mut i = info();
    i.max_volatility_accumulator = 10000;
    i.tick_spacing_index_reference = 3;
    let mut d = DynamicSwap::new(i, 0, 64, 1000).unwrap();
    d.update_volatility().unwrap();
    assert_eq!(d.info.volatility_accumulator, 10000);
    assert!(d.bounded_price(price(192), 100, false).unwrap().0);
    d.advance(false, true, price(192), 192, 192, price(192))
        .unwrap();
    assert_eq!(d.info.volatility_accumulator, 10000);
    d.update_volatility().unwrap();
    assert_eq!(d.info.volatility_accumulator, 0);
    assert!(!d.bounded_price(price(256), 100, false).unwrap().0);
}

#[test]
fn target_inside_group_is_not_extended_to_spacing_boundary() {
    let d = DynamicSwap::new(info(), 1, 64, 1000).unwrap();
    assert_eq!(
        d.bounded_price(price(32), 100, false).unwrap(),
        (false, price(32), None)
    );
    let d = DynamicSwap::new(info(), 63, 64, 1000).unwrap();
    assert_eq!(
        d.bounded_price(price(32), 100, true).unwrap(),
        (false, price(32), None)
    );
}

#[test]
fn exact_spacing_target_retains_boundary_tick_hint() {
    let d = DynamicSwap::new(info(), 1, 64, 1000).unwrap();
    assert_eq!(
        d.bounded_price(price(64), 100, false).unwrap(),
        (false, price(64), Some(64))
    );
    assert_eq!(
        d.bounded_price(price(0), 100, true).unwrap(),
        (false, price(0), Some(0))
    );
}

#[test]
fn skip_partial_price_uses_current_tick_and_right_boundary_adjustment() {
    let mut d = DynamicSwap::new(info(), 0, 64, 1000).unwrap();
    d.advance(false, true, price(128), 128, 256, price(256))
        .unwrap();
    assert_eq!(d.info.volatility_accumulator, 10000);
    d.update_volatility().unwrap();
    assert_eq!(d.info.volatility_accumulator, 20000);
}

#[test]
fn invalid_arithmetic_returns_errors() {
    assert!(DynamicSwap::new(info(), 0, 0, 1000).is_err());
    assert!(DynamicSwap::new(info(), 0, u32::MAX, 1000).is_err());
    let mut i = info();
    i.volatility_accumulator = u32::MAX;
    let d = DynamicSwap::new(i, 0, 64, 1000).unwrap();
    assert_eq!(d.total_fee(3000).unwrap_err().0, "RAYCLMM_ARITHMETIC");
    let mut i = info();
    i.tick_spacing_index_reference = i32::MAX;
    let mut d = DynamicSwap::new(i, -1, 1, 1000).unwrap();
    assert_eq!(d.update_volatility().unwrap_err().0, "RAYCLMM_ARITHMETIC");
}

#[test]
fn global_price_limits_win_over_clamped_spacing_boundaries() {
    use xone_domain::rayclmm_math::{MAX_SQRT_PRICE_X64, MIN_SQRT_PRICE_X64};
    use xone_domain::raydium_clmm::{MAX_TICK, MIN_TICK};
    let mut i = info();
    i.max_volatility_accumulator = 0;
    let d = DynamicSwap::new(i, 0, 64, 1000).unwrap();
    assert_eq!(
        d.bounded_price(price(-128), 100, true).unwrap(),
        (true, price(-128), None)
    );
    for (tick, left, limit) in [
        (MIN_TICK, true, MIN_SQRT_PRICE_X64 + 1),
        (MAX_TICK, false, MAX_SQRT_PRICE_X64 - 1),
    ] {
        let d = DynamicSwap::new(info(), tick, 1000, 1000).unwrap();
        assert_eq!(
            d.bounded_price(limit, 100, left).unwrap(),
            (false, limit, None)
        );
    }
}
