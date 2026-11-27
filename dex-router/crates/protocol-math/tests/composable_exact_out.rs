use num_bigint::BigUint;
use serde_json::Value;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/composable-exact-out.json"
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
fn composable_output_matches_original_stable_math_with_bpt_in_all_positions() {
    let f = corpus();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 288);
    assert_eq!(cases.iter().filter(|c| check(c).is_some()).count(), 216);
}
#[test]
fn composable_output_continuations_preserve_underlying_and_bpt_cash() {
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
fn composable_regular_output_rejects_bpt_trades_and_invalid_cash_scales() {
    use xone_protocol_math::balancer_composable::quote_exact_out;
    let f = corpus();
    let c = &f["cases"][0];
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let s = p.multi_asset.as_ref().unwrap();
    let bpt = c["bpt"].as_u64().unwrap() as usize;
    let i = c["i"].as_u64().unwrap() as usize;
    let j = c["j"].as_u64().unwrap() as usize;
    let b = s
        .assets
        .iter()
        .map(|a| a.balance.0.clone())
        .collect::<Vec<_>>();
    let scales = s
        .assets
        .iter()
        .map(|a| a.scaling.0.clone())
        .collect::<Vec<_>>();
    let fee = BigUint::from(p.fee_numerator);
    let target = BigUint::from(1000u32);
    let one = BigUint::from(1u8);
    for (a, z) in [(bpt, j), (i, bpt), (i, i), (b.len(), j)] {
        assert!(
            quote_exact_out(&b, &s.amplification.0, &scales, &fee, &target, a, z, bpt).is_err()
        );
    }
    for out in [BigUint::from(0u8), b[j].clone(), &one << 256usize] {
        assert!(quote_exact_out(&b, &s.amplification.0, &scales, &fee, &out, i, j, bpt).is_err());
    }
    let mut bad = scales.clone();
    bad[bpt] += &one;
    assert!(quote_exact_out(&b, &s.amplification.0, &bad, &fee, &target, i, j, bpt).is_err());
    let mut bad = b.clone();
    bad[bpt] = &one << 112usize;
    assert!(quote_exact_out(&bad, &s.amplification.0, &scales, &fee, &target, i, j, bpt).is_err());
    let zero = Amount::zero();
    assert!(xone_protocol_math::multi_asset::quote_exact_out_tokens(
        &p,
        &zero,
        &s.assets[i].token,
        &s.assets[j].token
    )
    .is_err());
    assert!(xone_protocol_math::multi_asset::quote_exact_out_tokens(
        &p,
        &Amount(target),
        &p.pool_id,
        &s.assets[j].token
    )
    .is_err());
    let catalog = xone_protocol_math::capabilities::catalog();
    assert_eq!(
        catalog["balancer-v2-composable-regular"]
            .exact_out_route_scope
            .as_deref(),
        Some("SINGLE_POOL_UNDERLYING_ORDERED_ALLOCATION")
    );
}
