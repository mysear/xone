use xone_domain::{singleton::SingletonAdvancedState, Amount};
use xone_protocol_math::singleton::quote_advanced_exact_in;
#[test]
fn original_contract_advanced_quotes_and_manager_fee_match() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-advanced-quotes.json"
    ))
    .unwrap();
    assert!(f["cases"].as_array().unwrap().len() >= 240);
    for case in f["cases"].as_array().unwrap() {
        let s: SingletonAdvancedState = serde_json::from_value(case["snapshot"].clone()).unwrap();
        let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
        let e = &case["expected"];
        let q = quote_advanced_exact_in(&s, &input, case["zero_for_one"].as_bool().unwrap())
            .unwrap_or_else(|e| panic!("{}: {}", case["name"], e.0));
        assert_eq!(
            q.amount_out.to_string(),
            e["amount_out"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            q.protocol_amount.to_string(),
            e["protocol_amount"].as_str().unwrap(),
            "{} protocol fee",
            case["name"]
        );
        assert_eq!(q.swap_fee, e["swap_fee"].as_u64().unwrap() as u32);
        assert_eq!(
            q.next_state.snapshot.core.sqrt_price_x96.to_string(),
            e["sqrt_price_x96"].as_str().unwrap()
        );
        assert_eq!(
            q.next_state.snapshot.core.tick,
            e["tick"].as_i64().unwrap() as i32
        );
        assert_eq!(
            q.next_state.snapshot.core.liquidity.to_string(),
            e["liquidity"].as_str().unwrap()
        );
        assert_eq!(
            q.consumed_in.to_string(),
            e["consumed_in"].as_str().unwrap()
        );
        assert_eq!(
            q.remaining_in.to_string(),
            e["remaining_in"].as_str().unwrap()
        );
        assert_eq!(q.next_state.key_fee, s.key_fee);
        assert_eq!(q.next_state.snapshot.pool_id, s.snapshot.pool_id);
    }
}

#[test]
fn advanced_core_budget_and_unproven_state_are_rejected() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-advanced-quotes.json"
    ))
    .unwrap();
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["input"] == "6000000000"
                && c["snapshot"]["snapshot"]["lp_fee"] == 3000
                && c["zero_for_one"] == false
        })
        .unwrap();
    let mut s: SingletonAdvancedState = serde_json::from_value(case["snapshot"].clone()).unwrap();
    s.snapshot.core.max_steps = 1;
    let input: Amount = "6000000000".parse().unwrap();
    assert!(quote_advanced_exact_in(&s, &input, false).is_err());
    s.snapshot.core.max_steps = 512;
    s.snapshot.core.tick_words.pop();
    assert!(quote_advanced_exact_in(&s, &input, false).is_err());
}

#[test]
fn central_advanced_profile_dispatch_preserves_snapshot_identity() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-advanced-quotes.json"
    ))
    .unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let state: SingletonAdvancedState = serde_json::from_value(c["snapshot"].clone()).unwrap();
        let protocol = format!("{}-advanced", state.protocol);
        let adapter = if state.protocol == "uniswap-v4" {
            "uniswap-v4/core-advanced-1"
        } else {
            "pancake-infinity-cl/cl-advanced-1"
        };
        let value = serde_json::json!({"chain_key":"evm:31337","pool_id":format!("{}/{}",state.snapshot.manager,state.snapshot.pool_id),
            "protocol":protocol,"token0":state.currency0,"token1":state.currency1,"reserve0":"0","reserve1":"0",
            "fee_numerator":state.snapshot.lp_fee,"fee_denominator":1000000,"adapter_version":adapter,"token_behavior":"standard",
            "singleton_advanced":state,"multi_asset":null});
        let pool: xone_domain::Pool = serde_json::from_value(value).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let q =
            xone_protocol_math::quote_exact_in(&pool, &input, c["zero_for_one"].as_bool().unwrap())
                .unwrap();
        assert_eq!(
            q.amount_out.to_string(),
            c["expected"]["amount_out"].as_str().unwrap()
        );
        let next = q.next_pool.singleton_advanced.unwrap();
        assert_eq!(
            next.key_fee,
            pool.singleton_advanced.as_ref().unwrap().key_fee
        );
        assert_eq!(
            next.snapshot.pool_id,
            pool.singleton_advanced.as_ref().unwrap().snapshot.pool_id
        );
    }
}
