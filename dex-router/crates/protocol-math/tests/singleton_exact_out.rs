use serde_json::Value;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/singleton-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn check(c: &Value) {
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let output: Amount = c["target"].as_str().unwrap().parse().unwrap();
    let before = p.clone();
    let q = xone_protocol_math::singleton::quote_exact_out(
        &p,
        &output,
        c["zeroForOne"].as_bool().unwrap(),
    );
    assert_eq!(p, before);
    if c["passed"] == false {
        assert!(
            q.is_err(),
            "{} {} {}",
            p.protocol,
            c["target"],
            c["zeroForOne"]
        );
        return;
    }
    let q = q.unwrap_or_else(|e| {
        panic!(
            "{} {} {}: {}",
            p.protocol, c["target"], c["zeroForOne"], e.0
        )
    });
    assert_eq!(q.amount_in.to_string(), c["requiredInput"]);
    assert_eq!(q.amount_out, output);
    assert_eq!(
        q.next_pool,
        serde_json::from_value::<Pool>(c["after"].clone()).unwrap()
    );
}
#[test]
fn original_singleton_positive_output_matches_pricing_projection() {
    let c = corpus();
    assert_eq!(c["cases"].as_array().unwrap().len(), 644);
    for row in c["cases"].as_array().unwrap() {
        check(row);
    }
}
#[test]
fn original_singleton_output_sequences_match_pricing_projection() {
    for seq in corpus()["sequences"].as_array().unwrap() {
        for step in seq["steps"].as_array().unwrap() {
            check(step);
        }
    }
}

#[test]
fn singleton_output_limits_and_versioned_capabilities_remain_explicit() {
    let f = corpus();
    let row = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["passed"] == true && r["zeroForOne"] == true && r["target"] == "1000")
        .unwrap();
    let p: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
    let before = p.clone();
    for output in [
        Amount::zero(),
        Amount(num_bigint::BigUint::from(1u8) << 127usize),
    ] {
        assert!(xone_protocol_math::singleton::quote_exact_out(&p, &output, true).is_err());
        assert_eq!(p, before);
    }
    let mut limited = p.clone();
    limited.singleton_cl.as_mut().unwrap().core.max_steps = 1;
    assert!(xone_protocol_math::singleton::quote_exact_out(
        &limited,
        &"1000".parse().unwrap(),
        true
    )
    .is_err());
    let mut hooks = p.clone();
    hooks.singleton_cl.as_mut().unwrap().hooks =
        "0x0000000000000000000000000000000000000001".into();
    assert!(
        xone_protocol_math::singleton::quote_exact_out(&hooks, &"1000".parse().unwrap(), true)
            .is_err()
    );
    let models = xone_protocol_math::capabilities::catalog();
    for model in [
        "uniswap-v4",
        "pancake-infinity-cl",
        "uniswap-v4-advanced",
        "pancake-infinity-cl-advanced",
    ] {
        assert_eq!(
            models[model].exact_out_route_scope.as_deref(),
            Some("SINGLE_POOL_ORDERED_ALLOCATION")
        );
        assert!(!models[model].unsigned_transaction);
    }
}

#[test]
fn advanced_singleton_uses_live_fee_and_preserves_native_pool_key() {
    use xone_domain::singleton::{SingletonAdvancedState, DYNAMIC_FEE_FLAG, ZERO_ADDRESS};

    for case in corpus()["cases"].as_array().unwrap() {
        let mut pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let Some(mut snapshot) = pool.singleton_cl.take() else {
            continue;
        };
        snapshot.hooks = "0x0000000000000000000000000000000000010000".into();
        let mut advanced = SingletonAdvancedState {
            protocol: pool.protocol.clone(),
            currency0: ZERO_ADDRESS.into(),
            currency1: pool.token1.clone(),
            key_fee: DYNAMIC_FEE_FLAG,
            snapshot_block_hash: format!("0x{}", "11".repeat(32)),
            snapshot,
        };
        advanced.snapshot.pool_id = advanced.key_id().unwrap();
        pool.pool_id = format!(
            "{}/{}",
            advanced.snapshot.manager, advanced.snapshot.pool_id
        );
        pool.protocol.push_str("-advanced");
        pool.adapter_version = xone_domain::protocols::protocol(&pool.protocol)
            .unwrap()
            .adapter
            .into();
        pool.token0 = advanced.currency0.clone();
        pool.singleton_advanced = Some(advanced.clone());
        let output: Amount = case["target"].as_str().unwrap().parse().unwrap();
        let quote = xone_protocol_math::exact_out::quote_exact_out(
            &pool,
            &output,
            case["zeroForOne"].as_bool().unwrap(),
        );
        if case["passed"] == false {
            assert!(quote.is_err());
            continue;
        }
        let quote = quote.unwrap();
        assert_eq!(quote.amount_in.to_string(), case["requiredInput"]);
        let expected: Pool = serde_json::from_value(case["after"].clone()).unwrap();
        advanced.snapshot.core = expected.singleton_cl.unwrap().core;
        assert_eq!(quote.next_pool.singleton_advanced, Some(advanced));
    }
}
