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
    assert_eq!(models.len(), 61);
    assert_eq!(
        models
            .values()
            .filter(|m| m.quote_modes.iter().any(|m| m == "EXACT_OUT"))
            .count(),
        38
    );
    assert_eq!(
        models.values().filter(|m| m.execution_preparation).count(),
        10
    );
    for model in ["uniswap-v4", "pancake-infinity-cl"] {
        assert_eq!(
            models[model].exact_out_route_scope.as_deref(),
            Some("SINGLE_POOL_ORDERED_ALLOCATION")
        );
        assert!(!models[model].unsigned_transaction);
    }
    for model in ["uniswap-v4-advanced", "pancake-infinity-cl-advanced"] {
        assert!(!xone_protocol_math::exact_out::supports_protocol(model));
    }
}
