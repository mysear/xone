use serde_json::Value;
use std::collections::BTreeSet;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/orca-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn five_orca_profiles_exact_output_match_original_manager_full_post_state() {
    let f = corpus();
    assert_eq!(f["revision"], "cc683ec92acb5e7606caa3d8e514d9383e9c7c11");
    let mut profiles = BTreeSet::new();
    let (mut full, mut rejected) = (0, 0);
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let original = p.clone();
        let amount: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let zero = c["zeroForOne"].as_bool().unwrap();
        let q = xone_protocol_math::exact_out::quote_exact_out(&p, &amount, zero);
        assert_eq!(p, original, "immutable source {i}");
        if c["passed"] == false {
            assert!(q.is_err(), "{i} {} expected native reject", c["name"]);
            rejected += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{i} {} {}: {}", c["profile"], c["name"], e.0));
        assert_eq!(
            q.amount_in.to_string(),
            c["requiredInput"].as_str().unwrap(),
            "{i}: input"
        );
        assert_eq!(q.amount_out, amount, "{i}: exact target");
        let expected: Pool = serde_json::from_value(c["after"].clone()).unwrap();
        assert_eq!(
            q.next_pool, expected,
            "{i}: full native pool, Oracle, rewards and arrays"
        );
        profiles.insert(p.protocol);
        full += 1;
    }
    assert_eq!(profiles.len(), 5);
    assert!(
        full >= 400 && rejected >= 100,
        "full={full},rejected={rejected}"
    );
}
#[test]
fn orca_exact_output_sequences_use_evolving_fee_reward_and_tick_state() {
    let f = corpus();
    assert_eq!(f["sequences"].as_array().unwrap().len(), 8);
    for sequence in f["sequences"].as_array().unwrap() {
        let steps = sequence["steps"].as_array().unwrap();
        let mut pool: Pool = serde_json::from_value(steps[0]["pool"].clone()).unwrap();
        for c in steps {
            assert_eq!(
                pool,
                serde_json::from_value::<Pool>(c["pool"].clone()).unwrap()
            );
            let q = xone_protocol_math::exact_out::quote_exact_out(
                &pool,
                &c["target"].as_str().unwrap().parse().unwrap(),
                c["zeroForOne"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(q.amount_in.to_string(), c["requiredInput"]);
            pool = q.next_pool;
            assert_eq!(
                pool,
                serde_json::from_value::<Pool>(c["after"].clone()).unwrap()
            );
        }
    }
}
#[test]
fn orca_output_width_zero_budget_and_pool_source_controls_fail_without_mutation() {
    let f = corpus();
    let mut seen = BTreeSet::new();
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["passed"] == true && c["target"] == "1000")
    {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        if !seen.insert(p.protocol.clone()) {
            continue;
        }
        for output in ["0", "18446744073709551616"] {
            assert!(xone_protocol_math::exact_out::quote_exact_out(
                &p,
                &output.parse().unwrap(),
                true
            )
            .is_err());
        }
        let mut bad = p.clone();
        bad.orca_whirlpool.as_mut().unwrap().max_steps = 0;
        assert!(xone_protocol_math::exact_out::quote_exact_out(
            &bad,
            &"1000".parse().unwrap(),
            true
        )
        .is_err());
        bad = p.clone();
        bad.orca_whirlpool.as_mut().unwrap().raw_vault1 = 0;
        assert!(xone_protocol_math::exact_out::quote_exact_out(
            &bad,
            &"1000".parse().unwrap(),
            true
        )
        .is_err());
        bad = p.clone();
        bad.orca_whirlpool.as_mut().unwrap().program_id = "11111111111111111111111111111111".into();
        assert!(xone_protocol_math::exact_out::quote_exact_out(
            &bad,
            &"1000".parse().unwrap(),
            true
        )
        .is_err());
    }
    assert_eq!(seen.len(), 5);
}
