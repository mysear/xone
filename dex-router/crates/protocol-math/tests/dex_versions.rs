use xone_domain::{
    protocols::{protocol, V2State},
    Amount, Pool,
};
use xone_protocol_math::quote_exact_in;
#[test]
fn version_specific_fees_change_output_and_cannot_be_overridden() {
    let base: Pool = serde_json::from_str::<xone_domain::Snapshot>(include_str!(
        "../../../test/fixtures/evm/baseline.json"
    ))
    .unwrap()
    .pools[0]
        .clone();
    let mut outputs = Vec::new();
    for name in ["pancake-v1", "pancake-v2", "sushiswap-v2"] {
        let profile = protocol(name).unwrap();
        let mut p = base.clone();
        p.pool_id = format!("0x{}", "33".repeat(20));
        p.protocol = name.into();
        p.adapter_version = profile.adapter.into();
        p.fee_numerator = profile.fee_numerator;
        p.fee_denominator = profile.fee_denominator;
        p.v2 = Some(V2State {
            swap: None,
            factory: format!("0x{}", "44".repeat(20)),
            code_sha256: "aa".repeat(32),
        });
        let q = quote_exact_in(&p, &"10000".parse().unwrap(), true).unwrap();
        outputs.push(q.amount_out);
        p.fee_numerator += 1;
        assert!(quote_exact_in(&p, &"10000".parse().unwrap(), false).is_err());
    }
    assert_eq!(
        outputs.iter().map(ToString::to_string).collect::<Vec<_>>(),
        vec!["9881", "9876", "9871"]
    );
    let mut v3: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    v3.protocol = "pancake-v3".into();
    v3.adapter_version = "pancake-v3/core-1".into();
    v3.v3.as_mut().unwrap().fee_protocol = 209718400;
    let q = quote_exact_in(&v3, &"10000".parse().unwrap(), true).unwrap();
    assert!(q.amount_out > Amount::zero());
    v3.adapter_version = "uniswap-v3/core-1".into();
    assert!(quote_exact_in(&v3, &"10000".parse().unwrap(), true).is_err());
}

#[test]
fn pinned_dex_sdk_and_pair_invariant_differential() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/dex-versions.json")).unwrap();
    let mut count = 0;
    for case in golden["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let input = case["input"].as_str().unwrap().parse().unwrap();
        let result = quote_exact_in(&p, &input, case["zero_for_one"].as_bool().unwrap());
        let expected = &case["expected"];
        if let Some(error) = expected["error"].as_str() {
            assert_eq!(result.unwrap_err().0, error, "{}", case["name"]);
        } else {
            let q = result.unwrap_or_else(|e| panic!("{}: {}", case["name"], e.0));
            assert_eq!(
                q.amount_out.to_string(),
                expected["amount_out"].as_str().unwrap(),
                "{}",
                case["name"]
            );
            assert_eq!(q.consumed_in, input);
            assert_eq!(q.remaining_in, Amount::zero());
            if let Some(v) = q.next_pool.v3 {
                assert_eq!(
                    v.sqrt_price_x96.to_string(),
                    expected["sqrt_price_x96"].as_str().unwrap()
                );
                assert_eq!(v.tick, expected["tick"].as_i64().unwrap() as i32);
                assert_eq!(
                    v.liquidity.to_string(),
                    expected["liquidity"].as_str().unwrap()
                );
                assert_eq!(v.fee_protocol, 209718400);
            }
        }
        count += 1;
    }
    assert_eq!(count, 372);
}
