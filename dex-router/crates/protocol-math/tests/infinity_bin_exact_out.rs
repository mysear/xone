use serde_json::Value;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/infinity-bin-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn check(c: &Value) -> Option<Pool> {
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let before = p.clone();
    let output: Amount = c["target"].as_str().unwrap().parse().unwrap();
    let q = xone_protocol_math::exact_out::quote_exact_out(
        &p,
        &output,
        c["zeroForOne"].as_bool().unwrap(),
    );
    assert_eq!(p, before);
    if c["passed"] == false {
        assert!(q.is_err(), "{} {}", c["name"], c["target"]);
        return None;
    }
    let q = q.unwrap_or_else(|e| panic!("{} {}: {}", c["name"], c["target"], e.0));
    assert_eq!(q.amount_in.to_string(), c["requiredInput"]);
    assert_eq!(q.amount_out, output);
    assert_eq!(
        q.next_pool,
        serde_json::from_value::<Pool>(c["after"].clone()).unwrap()
    );
    Some(q.next_pool)
}
#[test]
fn original_bin_output_targets_match_full_physical_projection_and_burn_dust() {
    let f = corpus();
    let c = f["cases"].as_array().unwrap();
    assert_eq!(c.len(), 264);
    let mut success = 0;
    for r in c {
        success += usize::from(check(r).is_some());
    }
    assert_eq!(success, 219);
}
#[test]
fn original_bin_output_sequences_and_ordered_targets_keep_complete_state() {
    let f = corpus();
    for seq in f["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .chain(f["ordered"].as_array().unwrap())
    {
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
fn bin_output_integer_tree_step_and_hook_bounds_fail_closed() {
    let f = corpus();
    let c = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["passed"] == true && c["target"] == "5000000" && c["zeroForOne"] == true)
        .unwrap();
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let output: Amount = c["target"].as_str().unwrap().parse().unwrap();
    for value in [
        Amount::zero(),
        Amount(num_bigint::BigUint::from(1u8) << 127usize),
    ] {
        assert!(xone_protocol_math::exact_out::quote_exact_out(&p, &value, true).is_err());
    }
    let mut limited = p.clone();
    limited.infinity_bin.as_mut().unwrap().max_steps = 1;
    assert!(xone_protocol_math::exact_out::quote_exact_out(&limited, &output, true).is_err());
    let mut bad = p.clone();
    bad.infinity_bin.as_mut().unwrap().tree_level0 = Amount::zero();
    assert!(xone_protocol_math::exact_out::quote_exact_out(&bad, &output, true).is_err());
    let mut bad = p.clone();
    bad.infinity_bin.as_mut().unwrap().hooks = "0x0000000000000000000000000000000000000001".into();
    assert!(xone_protocol_math::exact_out::quote_exact_out(&bad, &output, true).is_err());
    let models = xone_protocol_math::capabilities::catalog();
    assert_eq!(
        models["pancake-infinity-bin"]
            .exact_out_route_scope
            .as_deref(),
        Some("SINGLE_POOL_ORDERED_ALLOCATION")
    );
}
