use num_bigint::BigUint;
use serde_json::Value;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/weighted-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn check(c: &Value) -> Option<Pool> {
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let before = p.clone();
    let output: Amount = c["target"].as_str().unwrap().parse().unwrap();
    let i = c["i"].as_u64().unwrap() as usize;
    let j = c["j"].as_u64().unwrap() as usize;
    let q = if let Some(s) = &p.multi_asset {
        xone_protocol_math::multi_asset::quote_exact_out_tokens(
            &p,
            &output,
            &s.assets[i].token,
            &s.assets[j].token,
        )
        .map(|q| (q.consumed_in, q.amount_out, q.next_pool))
    } else {
        xone_protocol_math::exact_out::quote_exact_out(&p, &output, i == 0)
            .map(|q| (q.amount_in, q.amount_out, q.next_pool))
    };
    assert_eq!(p, before);
    if c["expected"]["reverted"] == true {
        assert!(q.is_err(), "original reverted: {c}");
        return None;
    }
    let (input, out, next) =
        q.unwrap_or_else(|e| panic!("{} {i}->{j} target={output}: {}", p.protocol, e.0));
    assert_eq!(input.to_string(), c["expected"]["amount_in"]);
    assert_eq!(out, output);
    let mut expected = p.clone();
    let b = c["expected"]["next_balances"].as_array().unwrap();
    expected.reserve0 = b[0].as_str().unwrap().parse().unwrap();
    expected.reserve1 = b[1].as_str().unwrap().parse().unwrap();
    if let Some(s) = &mut expected.multi_asset {
        for (a, b) in s.assets.iter_mut().zip(b) {
            a.balance = b.as_str().unwrap().parse().unwrap();
        }
    }
    assert_eq!(next, expected, "entire physical state");
    Some(next)
}
#[test]
fn original_weighted_given_out_callbacks_and_packed_cash_match() {
    let f = corpus();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 304);
    assert_eq!(cases.iter().filter(|c| check(c).is_some()).count(), 222);
}
#[test]
fn original_output_sequences_preserve_all_other_assets() {
    for seq in corpus()["sequences"].as_array().unwrap() {
        let mut previous = None;
        for c in seq["steps"].as_array().unwrap() {
            if let Some(p) = previous {
                assert_eq!(
                    p,
                    serde_json::from_value::<Pool>(c["pool"].clone()).unwrap()
                );
            }
            previous = check(c);
            assert!(previous.is_some());
        }
    }
}
#[test]
fn exact_output_bounds_and_unsupported_models_fail_closed() {
    use xone_protocol_math::balancer_multi::quote_exact_out;
    let e: BigUint = 10u64.pow(18).into();
    let b = vec![&e * 1000u32; 3];
    let w = vec![&e / 2u8, &e / 4u8, &e / 4u8];
    let scales = vec![e.clone(); 3];
    let zero = BigUint::from(0u8);
    let one = BigUint::from(1u8);
    for (i, j) in [(0, 0), (3, 0), (0, 3), (usize::MAX, 0)] {
        assert!(quote_exact_out(&b, &w, &scales, &zero, &one, i, j).is_err());
    }
    assert!(quote_exact_out(&b, &w, &scales, &zero, &zero, 0, 1).is_err());
    assert!(quote_exact_out(&b, &w, &scales, &e, &one, 0, 1).is_err());
    assert!(quote_exact_out(&b, &w[..2], &scales, &zero, &one, 0, 1).is_err());
    let mut bad = scales.clone();
    bad[2] += &one;
    assert!(quote_exact_out(&b, &w, &bad, &zero, &one, 0, 1).is_err());
    let mut bad = w.clone();
    bad[2] += &one;
    assert!(quote_exact_out(&b, &bad, &scales, &zero, &one, 0, 1).is_err());
    let mut bad = b.clone();
    bad[2] = &one << 112usize;
    assert!(quote_exact_out(&bad, &w, &scales, &zero, &one, 0, 1).is_err());
    assert!(quote_exact_out(&b, &w, &scales, &zero, &(&one << 256usize), 0, 1).is_err());
    assert!(quote_exact_out(&b, &w, &scales, &zero, &(&b[1] * 3u8 / 10u8 + &one), 0, 1).is_err());
    assert!(!xone_protocol_math::exact_out::supports_protocol(
        "curve-crypto2-full"
    ));
}
