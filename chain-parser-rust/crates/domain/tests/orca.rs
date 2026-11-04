use xone_domain::{orca::OrcaWhirlpoolState, Pool};
fn pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap()
}
#[test]
fn orca_static_q64_and_arrays_roundtrip() {
    let p = pool();
    p.validate().unwrap();
    let s = p.orca_whirlpool.unwrap();
    assert_eq!(
        OrcaWhirlpoolState::from_proto(s.to_proto().unwrap()).unwrap(),
        s
    );
    let mut p = pool();
    p.orca_whirlpool.as_mut().unwrap().fee_tier_index += 1;
    assert!(p.validate().is_err());
    let mut p = pool();
    p.orca_whirlpool.as_mut().unwrap().tick_arrays[0]
        .ticks
        .clear();
    assert!(p.validate().is_err());
    let mut p = pool();
    p.protocol = "fixture-cpmm".into();
    assert!(p.validate().is_err());
}

#[test]
fn independent_original_orca_tickmath_vectors() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/orca-tickmath.json")).unwrap();
    let vectors = f["vectors"].as_array().unwrap();
    assert!(vectors.len() > 1000);
    for v in vectors {
        let tick = i32::try_from(v["tick"].as_i64().unwrap()).unwrap();
        let price = xone_domain::orca_math::sqrt_price_from_tick(tick).unwrap();
        assert_eq!(price.to_string(), v["sqrt_price_x64"].as_str().unwrap());
        assert_eq!(
            xone_domain::orca_math::tick_from_sqrt_price(&price).unwrap(),
            tick
        );
    }
}
