use xone_domain::Pool;
use xone_protocol_math::quote_exact_in;
#[test]
fn orca_q64_profile_dispatches_without_pseudo_v3_state() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap();
    let q = quote_exact_in(&p, &"10000".parse().unwrap(), true).unwrap();
    assert_eq!(q.consumed_in.to_string(), "10000");
    assert_eq!(q.remaining_in.to_string(), "0");
    assert!(q.next_pool.v3.is_none());
    assert!(q.next_pool.singleton_cl.is_none());
    assert_ne!(
        q.next_pool.orca_whirlpool.as_ref().unwrap().sqrt_price_x64,
        p.orca_whirlpool.as_ref().unwrap().sqrt_price_x64
    );
}
