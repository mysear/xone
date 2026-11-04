#[allow(dead_code)]
#[path = "../src/curve_multi.rs"]
mod curve_multi;
use num_bigint::BigUint;
use xone_protocol_math::curve_ng;
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
#[test]
fn ng_offpeg_quote_requires_real_ng_math() {
    let b = vec![
        n("1000000000000000000000000"),
        n("2000000000000000000000000"),
        n("3000000000000000000000000"),
    ];
    let r = vec![n("1000000000000000000"); 3];
    let q = curve_ng::quote_exact_in(
        &b,
        &n("10000"),
        &r,
        &n("4000000"),
        &n("5000000000"),
        &n("20000000000"),
        &n("10000000000000000000000"),
        0,
        2,
    )
    .unwrap();
    assert!(q.fee_rate > n("4000000"));
    assert!(q.amount_out > n("1"));
}
#[test]
fn original_ng_full_exchange_and_erc4626_rates_522_cases() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/curve-ng-quotes.json")).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 522);
    for (k, c) in cases.iter().enumerate() {
        let b: Vec<BigUint> = c["balances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        let r: Vec<BigUint> = c["rates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        let val = |s: &str| n(c[s].as_str().unwrap());
        let (i, j) = (
            c["index_in"].as_u64().unwrap() as usize,
            c["index_out"].as_u64().unwrap() as usize,
        );
        let q = curve_ng::quote_exact_in(
            &b,
            &val("amp"),
            &r,
            &val("fee"),
            &val("admin_fee"),
            &val("offpeg_fee_multiplier"),
            &val("input"),
            i,
            j,
        );
        if c["expected"]["reverted"] == true {
            assert!(q.is_err(), "revert {k}");
            continue;
        }
        if let Some(d) = c["invariant"].as_str() {
            assert_eq!(
                curve_ng::invariant(&curve_multi::normalize(&b, &r).unwrap(), &val("amp")).unwrap(),
                n(d),
                "invariant {k}"
            );
        }
        let q = q.unwrap_or_else(|e| panic!("case {k}: {e:?}"));
        assert_eq!(
            q.amount_out,
            n(c["expected"]["amount_out"].as_str().unwrap()),
            "out {k}"
        );
        let want: Vec<BigUint> = c["expected"]["next_balances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        assert_eq!(q.next_balances, want, "balances {k}");
        assert_eq!(
            q.admin_fee_amount,
            n(c["expected"]["admin_fee_amount"].as_str().unwrap()),
            "admin {k}"
        );
    }
}
#[test]
fn original_erc4626_stored_rate_snapshot_and_typed_boundaries() {
    use curve_ng::RateSnapshot;
    let d: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/curve-ng-quotes.json")).unwrap();
    for c in d["rate_cases"].as_array().unwrap() {
        let r: Vec<BigUint> = c["multipliers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        let snapshots: Vec<RateSnapshot> = c["asset_types"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if t == 3 {
                    RateSnapshot::Erc4626 {
                        converted_assets: n(c["converted_assets"][i].as_str().unwrap()),
                        scale_factor: n(c["scale_factors"][i].as_str().unwrap()),
                    }
                } else {
                    RateSnapshot::Standard
                }
            })
            .collect();
        let expected: Vec<BigUint> = c["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        assert_eq!(curve_ng::stored_rates(&r, &snapshots).unwrap(), expected);
    }
    let m = vec![n("1000000000000000000"); 2];
    let s = vec![
        RateSnapshot::Oracle {
            fetched_rate: n("1234567890123456789"),
        },
        RateSnapshot::Rebasing,
    ];
    assert_eq!(
        curve_ng::stored_rates(&m, &s).unwrap(),
        vec![n("1234567890123456789"), m[1].clone()]
    );
    assert!(curve_ng::stored_rates(&m, &s[..1]).is_err());
}

#[test]
fn original_ng_small_balance_invariant_flooring() {
    let d: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/curve-ng-quotes.json")).unwrap();
    let mut differs = false;
    for c in d["math_cases"].as_array().unwrap() {
        let xp: Vec<BigUint> = c["xp"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        let a = n(c["amp"].as_str().unwrap());
        let result = curve_ng::invariant(&xp, &a);
        if c["reverted"] == true {
            assert!(result.is_err());
        } else {
            let result = result.unwrap();
            assert_eq!(result, n(c["expected"].as_str().unwrap()));
            differs |= curve_multi::invariant(&xp, &a)
                .map(|x| x != result)
                .unwrap_or(true);
        }
    }
    assert!(
        differs,
        "native low balances must distinguish NG rounding from generic old D"
    );
}
