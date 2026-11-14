use serde_json::Value;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/v3-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn standard_v3_negative_amount_swaps_match_original_price_projection_and_actual_token_debit() {
    let f = corpus();
    assert_eq!(f["cases"].as_array().unwrap().len(), 96);
    let (mut full, mut bad) = (0, 0);
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let before = p.clone();
        let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::exact_out::quote_exact_out(
            &p,
            &target,
            c["zeroForOne"].as_bool().unwrap(),
        );
        assert_eq!(p, before);
        if c["passed"] == false {
            assert!(q.is_err(), "{i}: expected original full-output revert");
            bad += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{i} {} {}: {}", c["protocol"], c["name"], e.0));
        assert_eq!(
            q.amount_in.to_string(),
            c["requiredInput"],
            "{i}: actual callback payment"
        );
        assert_eq!(q.amount_out, target);
        assert_eq!(
            q.next_pool,
            serde_json::from_value::<Pool>(c["after"].clone()).unwrap(),
            "{i}: full pricing projection"
        );
        assert_eq!(c["receipt"]["status"], 1);
        assert_eq!(c["control"]["receipt"]["status"], 0);
        full += 1;
    }
    assert_eq!((full, bad), (80, 16));
}
#[test]
fn standard_v3_output_quotes_replay_each_evolving_onchain_pool_price_state() {
    let f = corpus();
    for seq in f["sequences"].as_array().unwrap() {
        let steps = seq["steps"].as_array().unwrap();
        let mut p: Pool = serde_json::from_value(steps[0]["pool"].clone()).unwrap();
        for c in steps {
            assert_eq!(
                p,
                serde_json::from_value::<Pool>(c["pool"].clone()).unwrap()
            );
            let q = xone_protocol_math::exact_out::quote_exact_out(
                &p,
                &c["target"].as_str().unwrap().parse().unwrap(),
                c["zeroForOne"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(q.amount_in.to_string(), c["requiredInput"]);
            p = q.next_pool;
            assert_eq!(
                p,
                serde_json::from_value::<Pool>(c["after"].clone()).unwrap()
            );
        }
    }
}
