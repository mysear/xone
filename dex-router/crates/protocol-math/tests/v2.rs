use xone_domain::*;
use xone_protocol_math::*;
fn pool() -> Pool {
    serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
        .unwrap()
        .pools[0]
        .clone()
}
#[test]
fn fee_regression_and_widened_uint256() {
    let mut p = pool();
    let q = quote_exact_in(&p, &"10000".parse().unwrap(), true).unwrap();
    assert_eq!(q.amount_out.to_string(), "9871");
    assert_eq!(q.consumed_in.to_string(), "10000");
    assert_eq!(q.remaining_in.to_string(), "0");
    p.reserve0 = "1267650600228229401496703205376".parse().unwrap();
    p.reserve1 = p.reserve0.clone();
    let q = quote_exact_in(&p, &"18446744073709551616000".parse().unwrap(), true).unwrap();
    assert!(q.amount_out > Amount::zero());
    p.token_behavior = "unknown-tax".into();
    assert!(quote_exact_in(&p, &"1".parse().unwrap(), true).is_err());
}
#[test]
fn golden_fee_and_two_directions() {
    let p = pool();
    for direction in [true, false] {
        assert_eq!(
            quote_exact_in(&p, &"10000".parse().unwrap(), direction)
                .unwrap()
                .amount_out
                .to_string(),
            "9871"
        );
    }
    let mut zero = p;
    zero.fee_numerator = 0;
    assert_eq!(
        quote_exact_in(&zero, &"10000".parse().unwrap(), true)
            .unwrap()
            .amount_out
            .to_string(),
        "9900"
    );
}
#[test]
fn uniswap_v2_rejects_uint112_reserve_overflow() {
    let p = pool();
    let too_large: Amount = "10384593717069655257060992658440192".parse().unwrap();
    assert_eq!(
        quote_exact_in(&p, &too_large, true).err().unwrap().0,
        "V2_RESERVE_OVERFLOW"
    );
}
