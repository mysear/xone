use num_bigint::BigUint;
pub use xone_domain::{err, Result};
#[path = "../../domain/src/rayclmm_math.rs"]
mod rayclmm_math;
use rayclmm_math::*;
#[test]
fn pinned_original_tick_boundaries_and_inversion() {
    for (tick, expected) in [
        (MIN_TICK, MIN_SQRT_PRICE_X64),
        (0, 1u128 << 64),
        (MAX_TICK, MAX_SQRT_PRICE_X64),
        (-1, 0xfffcb933bd6fb800),
    ] {
        assert_eq!(sqrt_price_from_tick(tick).unwrap(), BigUint::from(expected));
    }
    assert_ne!(
        sqrt_price_from_tick(MAX_TICK).unwrap(),
        BigUint::from(79226673515401279992447579055u128)
    );
    for tick in [MIN_TICK, -28861, -60, -1, 0, 1, 60, 28861, MAX_TICK - 1] {
        let price = sqrt_price_from_tick(tick).unwrap();
        assert_eq!(tick_from_sqrt_price(&price).unwrap(), tick);
        if tick > MIN_TICK {
            assert_eq!(tick_from_sqrt_price(&(price - 1u32)).unwrap(), tick - 1);
        }
    }
    assert_eq!(
        tick_from_sqrt_price(&BigUint::from(MAX_SQRT_PRICE_X64 - 1)).unwrap(),
        MAX_TICK - 1
    );
}
#[test]
fn tick_and_price_domain_are_source_exact() {
    assert!(sqrt_price_from_tick(MIN_TICK - 1).is_err());
    assert!(sqrt_price_from_tick(MAX_TICK + 1).is_err());
    assert!(sqrt_price_from_tick(i32::MIN).is_err());
    assert!(tick_from_sqrt_price(&BigUint::from(MIN_SQRT_PRICE_X64 - 1)).is_err());
    assert!(tick_from_sqrt_price(&BigUint::from(MAX_SQRT_PRICE_X64)).is_err());
    assert!(tick_from_sqrt_price(&(BigUint::from(1u8) << 200usize)).is_err());
}

#[test]
fn native_original_tick_vectors_match_prices_and_both_sides_of_boundaries() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-tickmath.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["revision"],
        "6d7a0f82d2f293c553c69aaf943377367c6c4cc6"
    );
    let vectors = fixture["vectors"].as_array().unwrap();
    assert!(vectors.len() >= 2000);
    for vector in vectors {
        let tick = vector["tick"].as_i64().unwrap() as i32;
        let price = sqrt_price_from_tick(tick).unwrap();
        assert_eq!(
            price.to_string(),
            vector["sqrt_price_x64"].as_str().unwrap(),
            "tick {tick}: original price"
        );
        for (key, value) in [
            ("tick_from_price", price.clone()),
            ("tick_below", &price - 1u8),
            ("tick_above", &price + 1u8),
        ] {
            let resolved = tick_from_sqrt_price(&value).ok().map(i64::from);
            assert_eq!(resolved, vector[key].as_i64(), "tick {tick}: {key}");
        }
    }
}
