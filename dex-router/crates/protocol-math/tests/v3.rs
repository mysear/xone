use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
fn pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap()
}
#[test]
fn v3_uses_concentrated_liquidity_and_preserves_input() {
    for direction in [true, false] {
        let p = pool();
        let q = quote_exact_in(&p, &"10000".parse().unwrap(), direction).unwrap();
        assert_eq!(q.consumed_in.to_string(), "10000");
        assert_eq!(q.remaining_in, Amount::zero());
        assert!(q.amount_out > Amount::zero());
        assert_ne!(
            q.next_pool.v3.as_ref().unwrap().sqrt_price_x96,
            p.v3.unwrap().sqrt_price_x96
        );
    }
}
#[test]
fn v3_range_and_segment_limits_do_not_fake_full_input() {
    let p = pool();
    let mut limited = p.clone();
    limited.v3.as_mut().unwrap().max_steps = 1;
    assert!(quote_exact_in(&limited, &"100000000".parse().unwrap(), true).is_err());
    assert!(quote_exact_in(&p, &"1000000000000000".parse().unwrap(), false).is_err());
    let mut missing = p;
    missing.v3.as_mut().unwrap().ticks.pop();
    assert!(quote_exact_in(&missing, &"100".parse().unwrap(), true).is_err());
}

#[test]
fn official_sdk_integer_differential_all_cases() {
    let file: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/v3-quotes.json")).unwrap();
    let mut passed = 0;
    for case in file["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
        let result = quote_exact_in(&p, &input, case["zero_for_one"].as_bool().unwrap());
        let expected = &case["expected"];
        if let Some(error) = expected["error"].as_str() {
            assert_eq!(result.unwrap_err().0, error, "{}", case["name"]);
        } else {
            let q = result.unwrap_or_else(|e| panic!("{}: {}", case["name"], e.0));
            let v = q.next_pool.v3.unwrap();
            assert_eq!(
                q.amount_out.to_string(),
                expected["amount_out"].as_str().unwrap(),
                "{} output",
                case["name"]
            );
            assert_eq!(q.consumed_in, input);
            assert_eq!(q.remaining_in, Amount::zero());
            assert_eq!(
                v.sqrt_price_x96.to_string(),
                expected["sqrt_price_x96"].as_str().unwrap(),
                "{} sqrt",
                case["name"]
            );
            assert_eq!(
                v.liquidity.to_string(),
                expected["liquidity"].as_str().unwrap(),
                "{} liquidity",
                case["name"]
            );
            assert_eq!(
                v.tick,
                i32::try_from(expected["tick"].as_i64().unwrap()).unwrap(),
                "{} tick",
                case["name"]
            );
        };
        passed += 1;
    }
    assert_eq!(passed, 264);
}
