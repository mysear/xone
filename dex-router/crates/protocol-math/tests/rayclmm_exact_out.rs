use serde_json::Value;
use std::collections::BTreeSet;
use xone_domain::{Amount, Pool};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/rayclmm-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn six_rayclmm_output_profiles_match_original_manager_and_complete_pool_state() {
    let f = corpus();
    assert_eq!(f["revision"], "6d7a0f82d2f293c553c69aaf943377367c6c4cc6");
    let (mut full, mut rejected) = (0, 0);
    let mut profiles = BTreeSet::new();
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let before = p.clone();
        let amount: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::exact_out::quote_exact_out(
            &p,
            &amount,
            c["zeroForOne"].as_bool().unwrap(),
        );
        assert_eq!(p, before);
        if c["passed"] == false {
            assert!(
                q.is_err(),
                "{i} {} {} expected native rejection",
                c["profile"],
                c["name"]
            );
            rejected += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{i} {} {}: {}", c["profile"], c["name"], e.0));
        assert_eq!(
            q.amount_in.to_string(),
            c["requiredInput"],
            "{i}: native input"
        );
        assert_eq!(q.amount_out, amount, "{i}: exact net target");
        assert_eq!(
            q.next_pool,
            serde_json::from_value::<Pool>(c["after"].clone()).unwrap(),
            "{i}: full native post-state"
        );
        full += 1;
        profiles.insert(p.protocol);
    }
    assert_eq!(profiles.len(), 6);
    assert!(full >= 900 && rejected >= 300, "{full}/{rejected}");
}
#[test]
fn rayclmm_output_sequences_keep_prior_state_across_clock_and_direction_changes() {
    let f = corpus();
    assert_eq!(f["sequences"].as_array().unwrap().len(), 6);
    for sequence in f["sequences"].as_array().unwrap() {
        let steps = sequence["steps"].as_array().unwrap();
        let mut pool: Pool = serde_json::from_value(steps[0]["pool"].clone()).unwrap();
        for (i, c) in steps.iter().enumerate() {
            let expected: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
            if i > 0 {
                let old = pool.raydium_clmm.as_mut().unwrap();
                let new = expected.raydium_clmm.as_ref().unwrap();
                assert_eq!(new.bank_slot, old.bank_slot + 1);
                assert!(new.bank_unix_timestamp >= old.bank_unix_timestamp);
                old.bank_slot = new.bank_slot;
                old.bank_unix_timestamp = new.bank_unix_timestamp;
            }
            assert_eq!(pool, expected, "only observed Clock changes between swaps");
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
fn original_output_steps_cover_both_fee_tokens_and_zero_rate_rounding_dust() {
    let f = corpus();
    let (mut full, mut bad, mut dust) = (0, 0, 0);
    for (i, c) in f["steps"].as_array().unwrap().iter().enumerate() {
        let r = &c["request"];
        let e = &c["response"];
        let q = xone_protocol_math::raydium_clmm::step_exact_out_with_fee(
            r["output"].as_str().unwrap().parse().unwrap(),
            r["fee"].as_u64().unwrap() as u32,
            r["liquidity"].as_str().unwrap().parse().unwrap(),
            r["price"].as_str().unwrap().parse().unwrap(),
            r["target_price"].as_str().unwrap().parse().unwrap(),
            r["zero_for_one"].as_bool().unwrap(),
            r["fee_on_input"].as_bool().unwrap(),
        );
        if e["reverted"] == true {
            assert!(q.is_err(), "step {i}");
            bad += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("step {i}: {}", e.0));
        assert_eq!(q.amount_in.to_string(), e["amount_in"], "step {i}");
        assert_eq!(q.amount_out.to_string(), e["amount_out"], "step {i}");
        assert_eq!(q.next_price.to_string(), e["next_price"], "step {i}");
        assert_eq!(q.fee_amount.to_string(), e["fee_amount"], "step {i}");
        if r["fee"] == 0 && r["fee_on_input"] == false && q.fee_amount > 0 {
            dust += 1;
        }
        full += 1;
    }
    assert!(
        full > 100 && bad > 0 && dust > 0,
        "step coverage {full}/{bad}/{dust}"
    );
}
