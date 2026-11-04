use xone_domain::Pool;
use xone_protocol_math::quote_exact_in;
#[test]
fn rayclmm_profile_uses_independent_q64_bitmap_and_observation() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    let q = quote_exact_in(&p, &"10000".parse().unwrap(), true).unwrap();
    assert_eq!(q.consumed_in.to_string(), "10000");
    assert_eq!(q.remaining_in.to_string(), "0");
    assert!(q.next_pool.orca_whirlpool.is_none());
    assert!(q.next_pool.v3.is_none());
    let state = q.next_pool.raydium_clmm.unwrap();
    assert_ne!(state.sqrt_price_x64, p.raydium_clmm.unwrap().sqrt_price_x64);
    assert_eq!(state.observations.len(), 100);
}
