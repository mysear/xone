#[path = "../src/curve_multi.rs"]
mod curve_multi;
use num_bigint::BigUint;
use num_traits::One;
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
#[test]
fn three_and_four_coin_exchange_accepts_current_precise_a() {
    for count in [3, 4] {
        let b = vec![n("1000000000000000000000000"); count];
        let rates = vec![n("1000000000000000000"); count];
        let q = curve_multi::quote_exact_in(
            &b,
            &n("10000"),
            &rates,
            &n("4000000"),
            &n("5000000000"),
            &n("10000000000000000000000"),
            0,
            count - 1,
        )
        .unwrap();
        assert!(q.amount_out > n("9900000000000000000000"));
        assert_eq!(q.next_balances[0], &b[0] + n("10000000000000000000000"));
        assert!(q.admin_fee_amount > BigUint::from(0u8));
    }
}
#[test]
fn checked_uint256_rejects_oversized_intermediate() {
    let b = vec![BigUint::one() << 255, BigUint::one(), BigUint::one()];
    let r = vec![n("1000000000000000000"); 3];
    assert!(curve_multi::quote_exact_in(
        &b,
        &n("10000"),
        &r,
        &n("4000000"),
        &n("5000000000"),
        &n("1"),
        0,
        1
    )
    .is_err());
}
#[test]
fn original_vyper_exchange_and_poststate_720_cases() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-multi-quotes.json"
    ))
    .unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 720);
    for (k, c) in cases.iter().enumerate() {
        let b: Vec<BigUint> = c["balances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| n(v.as_str().unwrap()))
            .collect();
        let r: Vec<BigUint> = c["rates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| n(v.as_str().unwrap()))
            .collect();
        let (a, f, af, dx) = (
            n(c["amp"].as_str().unwrap()),
            n(c["fee"].as_str().unwrap()),
            n(c["admin_fee"].as_str().unwrap()),
            n(c["input"].as_str().unwrap()),
        );
        let (i, j) = (
            c["index_in"].as_u64().unwrap() as usize,
            c["index_out"].as_u64().unwrap() as usize,
        );
        let q = if c["model"] == "MetaUSD" {
            curve_multi::quote_meta_exact_in(&b, &a, &r[0], &r[1], &f, &af, &dx, i, j)
        } else {
            curve_multi::quote_exact_in(&b, &a, &r, &f, &af, &dx, i, j)
        };
        if c["expected"]["reverted"] == true {
            assert!(q.is_err(), "native revert #{k}");
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("case #{k}: {e:?}"));
        assert_eq!(
            q.amount_out,
            n(c["expected"]["amount_out"].as_str().unwrap()),
            "amount #{k}"
        );
        let expected: Vec<BigUint> = c["expected"]["next_balances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| n(v.as_str().unwrap()))
            .collect();
        assert_eq!(q.next_balances, expected, "balances #{k}");
    }
}
#[test]
fn meta_underlying_requires_specific_base_action() {
    use curve_multi::{meta_underlying_boundary as b, MetaUnderlyingBoundary as B};
    assert_eq!(b(0, 3).unwrap(), B::MetaThenBaseWithdraw { base_index: 2 });
    assert_eq!(b(2, 0).unwrap(), B::BaseDepositThenMeta { base_index: 1 });
    assert_eq!(
        b(1, 3).unwrap(),
        B::BaseExchange {
            index_in: 0,
            index_out: 2
        }
    );
    assert!(b(1, 1).is_err());
    assert!(b(0, 4).is_err());
}
#[test]
fn malformed_shape_amp_fee_rate_and_zero_liquidity_revert() {
    let b = vec![n("1000000000000000000000000"); 3];
    let r = vec![n("1000000000000000000"); 3];
    let q = |b: &[BigUint], r: &[BigUint], a: &str, f: &str, af: &str, i, j| {
        curve_multi::quote_exact_in(b, &n(a), r, &n(f), &n(af), &n("1000000000000000000"), i, j)
    };
    assert!(q(&b, &r, "10000", "4000000", "5000000000", 0, 0).is_err());
    assert!(q(&b, &r[..2], "10000", "4000000", "5000000000", 0, 1).is_err());
    assert!(q(&b, &r, "99", "4000000", "5000000000", 0, 1).is_err());
    assert!(q(&b, &r, "100000000", "4000000", "5000000000", 0, 1).is_err());
    assert!(q(&b, &r, "10000", "10000000001", "5000000000", 0, 1).is_err());
    assert!(q(&b, &r, "10000", "4000000", "0", 0, 1).is_err());
    let mut zero = r.clone();
    zero[0] = n("0");
    assert!(q(&b, &zero, "10000", "4000000", "5000000000", 0, 1).is_err());
    let mut zero = b.clone();
    zero[2] = n("0");
    assert!(q(&zero, &r, "10000", "4000000", "5000000000", 0, 1).is_err());
}
