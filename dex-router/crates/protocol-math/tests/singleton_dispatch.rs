use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
fn pools() -> Vec<Pool> {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/singleton-state.json")).unwrap();
    serde_json::from_value(f["pools"].clone()).unwrap()
}
#[test]
fn singleton_exact_in_updates_own_state_and_directional_fee() {
    for p in pools() {
        let input: Amount = "10000".parse().unwrap();
        let a = quote_exact_in(&p, &input, true).unwrap();
        let b = quote_exact_in(&p, &input, false).unwrap();
        assert_eq!(a.consumed_in, input);
        assert_eq!(a.remaining_in, Amount::zero());
        assert!(a.next_pool.v3.is_none());
        assert_ne!(
            a.next_pool
                .singleton_cl
                .as_ref()
                .unwrap()
                .core
                .sqrt_price_x96,
            p.singleton_cl.as_ref().unwrap().core.sqrt_price_x96
        );
        assert!(a.amount_out.0 > 0u32.into());
        assert!(b.amount_out.0 > 0u32.into());
        let mut high = p.clone();
        high.singleton_cl.as_mut().unwrap().protocol_fee0 = if p.protocol == "uniswap-v4" {
            1000
        } else {
            4000
        };
        let high = quote_exact_in(&high, &input, true).unwrap();
        assert!(high.amount_out < a.amount_out);
    }
}
#[test]
fn singleton_never_reports_truncated_input_as_full_quote() {
    for mut p in pools() {
        p.singleton_cl.as_mut().unwrap().core.max_steps = 1;
        assert!(quote_exact_in(&p, &"100000000".parse().unwrap(), true).is_err());
        let huge = Amount::checked((num_bigint::BigUint::from(1u32) << 127usize) + 1u32).unwrap();
        assert!(quote_exact_in(&p, &huge, true).is_err());
    }
}
