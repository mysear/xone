use xone_domain::{raydium_clmm::RaydiumCLMMState, Pool};
fn pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap()
}
#[test]
fn rayclmm_bitmap_and_observation_roundtrip_without_other_model() {
    let p = pool();
    p.validate().unwrap();
    let s = p.raydium_clmm.unwrap();
    assert_eq!(
        RaydiumCLMMState::from_proto(s.to_proto().unwrap()).unwrap(),
        s
    );
    let mut p = pool();
    p.raydium_clmm.as_mut().unwrap().bitmap.pop();
    assert!(p.validate().is_err());
    let mut p = pool();
    p.raydium_clmm.as_mut().unwrap().tick_arrays[0]
        .ticks
        .clear();
    assert!(p.validate().is_err());
    let mut p = pool();
    p.protocol = "orca-whirlpools-static".into();
    assert!(p.validate().is_err());
}

#[test]
fn independent_original_rayclmm_tickmath_vectors() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-tickmath.json"
    ))
    .unwrap();
    let vectors = f["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 2053);
    for v in vectors {
        let tick = i32::try_from(v["tick"].as_i64().unwrap()).unwrap();
        let price = xone_domain::rayclmm_math::sqrt_price_from_tick(tick).unwrap();
        assert_eq!(price.to_string(), v["sqrt_price_x64"].as_str().unwrap());
        for (key, value) in [
            ("tick_from_price", price.clone()),
            ("tick_below", &price - 1u8),
            ("tick_above", &price + 1u8),
        ] {
            assert_eq!(
                xone_domain::rayclmm_math::tick_from_sqrt_price(&value)
                    .ok()
                    .map(i64::from),
                v[key].as_i64(),
                "{tick}: {key}"
            );
        }
    }
}

#[test]
fn rayclmm_rejects_forged_pda_bindings() {
    for mode in ["bump", "pool", "vault0", "vault1", "observation", "array"] {
        let mut p = pool();
        let s = p.raydium_clmm.as_mut().unwrap();
        let fake = xone_domain::orca::PROGRAM_DATA;
        match mode {
            "bump" => s.pool_bump ^= 1,
            "pool" => p.pool_id = fake.into(),
            "vault0" => s.vault0 = fake.into(),
            "vault1" => s.vault1 = fake.into(),
            "observation" => s.observation = fake.into(),
            _ => s.tick_arrays[0].address = fake.into(),
        }
        assert!(p.validate().is_err(), "forged PDA accepted: {mode}");
    }
}
