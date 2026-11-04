use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
fn pool() -> Pool {
    let mut p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/raydium-state.json")).unwrap();
    p.reserve0 = "1000".parse().unwrap();
    p.reserve1 = "1000".parse().unwrap();
    let s = p.raydium_cpmm.as_mut().unwrap();
    s.raw_vault0 = 1000;
    s.raw_vault1 = 1000;
    s.protocol_fees0 = 0;
    s.protocol_fees1 = 0;
    s.fund_fees0 = 0;
    s.fund_fees1 = 0;
    s.creator_fees0 = 0;
    s.creator_fees1 = 0;
    s.creator_fee_enabled = false;
    p
}
#[test]
fn actual_raydium_ceil_fee_does_not_use_v2_output() {
    let p = pool();
    let q = quote_exact_in(&p, &"10".parse().unwrap(), true).unwrap();
    assert_eq!(q.amount_out.to_string(), "8");
    assert_eq!(q.next_pool.reserve0.to_string(), "1010");
    assert_eq!(q.next_pool.reserve1.to_string(), "992");
    assert_eq!(q.consumed_in.to_string(), "10");
    assert_eq!(q.remaining_in, Amount::zero());
    assert!(q.next_pool.v2.is_none());
}
#[test]
fn zero_combined_input_fee_preserves_original_none_branch() {
    let mut p = pool();
    p.fee_numerator = 0;
    p.raydium_cpmm.as_mut().unwrap().creator_fee_rate = 0;
    assert!(quote_exact_in(&p, &"10".parse().unwrap(), true).is_err());
    p.raydium_cpmm.as_mut().unwrap().creator_fee_on = 1;
    let q = quote_exact_in(&p, &"10".parse().unwrap(), false).unwrap();
    assert_eq!(q.amount_out.to_string(), "9");
}
#[test]
fn spl_token_vault_overflow_cannot_report_a_full_swap() {
    let mut p = pool();
    let s = p.raydium_cpmm.as_mut().unwrap();
    s.raw_vault0 = u64::MAX;
    s.protocol_fees0 = u64::MAX - 1000;
    assert!(quote_exact_in(&p, &"10".parse().unwrap(), true).is_err());
    let p = pool();
    assert!(quote_exact_in(&p, &"18446744073709551616".parse().unwrap(), true).is_err());
}
