use num_bigint::BigUint;
use serde_json::Value;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/stable-exact-out.json"
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
    let q = xone_protocol_math::exact_out::quote_exact_out(&p, &output, i == 0)
        .map(|q| (q.amount_in, q.amount_out, q.next_pool));
    assert_eq!(j, 1 - i);
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
fn original_stable_given_out_solver_scaling_fee_and_cash_match() {
    let f = corpus();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 720);
    assert_eq!(cases.iter().filter(|c| check(c).is_some()).count(), 396);
}
#[test]
fn original_stable_output_sequences_match_complete_pool_state() {
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
fn stable_output_boundaries_are_independent_of_weighted_ratios() {
    use xone_protocol_math::balancer_stable::quote_exact_out;
    let e: BigUint = 10u64.pow(18).into();
    let b = [BigUint::from(1_000_000u32), BigUint::from(1_000_000u32)];
    let amp = BigUint::from(100_000u32);
    let zero = BigUint::from(0u8);
    let one = BigUint::from(1u8);
    let quote =
        |balances: [&BigUint; 2], a: &BigUint, s: [&BigUint; 2], fee: &BigUint, o: &BigUint| {
            quote_exact_out(balances, a, s, fee, o, true)
        };
    let q = quote(
        [&b[0], &b[1]],
        &amp,
        [&e, &e],
        &zero,
        &BigUint::from(400_000u32),
    )
    .unwrap();
    assert!(q.amount_in > BigUint::from(400_000u32));
    assert_eq!(q.next_balances[1], BigUint::from(600_000u32));
    for out in [zero.clone(), b[1].clone(), &b[1] + &one, &one << 256usize] {
        assert!(quote([&b[0], &b[1]], &amp, [&e, &e], &zero, &out).is_err());
    }
    for bad in [
        zero.clone(),
        999u32.into(),
        5_000_001u32.into(),
        &one << 256usize,
    ] {
        assert!(quote([&b[0], &b[1]], &bad, [&e, &e], &zero, &one).is_err());
    }
    for bad in [zero.clone(), &e + &one, &one << 256usize] {
        assert!(quote([&b[0], &b[1]], &amp, [&bad, &e], &zero, &one).is_err());
    }
    for fee in [e.clone(), &e + &one] {
        assert!(quote([&b[0], &b[1]], &amp, [&e, &e], &fee, &one).is_err());
    }
    for bad in [zero.clone(), &one << 112usize] {
        assert!(quote([&bad, &b[1]], &amp, [&e, &e], &zero, &one).is_err());
    }
    let m = xone_protocol_math::capabilities::catalog();
    assert_eq!(
        m["balancer-v2-stable2"].exact_out_route_scope.as_deref(),
        Some("SINGLE_POOL_ORDERED_ALLOCATION")
    );
}
