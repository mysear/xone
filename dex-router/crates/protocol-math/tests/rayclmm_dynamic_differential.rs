//! The local pinned native-program corpus is independent of router math.
use serde_json::Value;
use std::collections::BTreeSet;
use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-dynamic-quotes.json"
    ))
    .unwrap()
}

fn assert_case(pool: &Pool, case: &Value, context: &str) -> Option<Pool> {
    let name = case["name"].as_str().unwrap_or(context);
    let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
    let before = pool.clone();
    let result = quote_exact_in(pool, &input, case["zero_for_one"].as_bool().unwrap());
    assert_eq!(pool, &before, "{name}: caller state changed");
    let expected = &case["expected"];
    if expected["reverted"].as_bool() == Some(true) {
        assert!(result.is_err(), "{name}: expected native/profile rejection");
        return None;
    }
    let q = result.unwrap_or_else(|e| panic!("{name}: {}", e.0));
    assert_eq!(
        q.amount_out.to_string(),
        expected["amount_out"].as_str().unwrap(),
        "{name}: output"
    );
    assert_eq!(q.consumed_in, input, "{name}: input");
    assert_eq!(q.remaining_in, Amount::zero(), "{name}: remaining input");
    let expected_pool: Pool = serde_json::from_value(expected["next_pool"].clone()).unwrap();
    assert_eq!(q.next_pool, expected_pool, "{name}: complete post-pool");
    // Typed equality also checks every integer amount, identity and proof field.
    q.next_pool.validate().unwrap();
    Some(q.next_pool)
}

#[test]
fn pinned_native_dynamic_cases_match_complete_post_pool() {
    let corpus = fixture();
    assert_eq!(
        corpus["reference"]["revision"],
        "6d7a0f82d2f293c553c69aaf943377367c6c4cc6"
    );
    assert_eq!(corpus["reference"]["business_math_imported"], false);
    assert_eq!(corpus["reference"]["profile"], "raydium-clmm-dynamic");
    assert_eq!(
        corpus["reference"]["adapter_version"],
        "raydium-clmm/6d7a0f82-spl-dynamic-1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() >= 300);
    let mut cells = [[0; 2]; 3];
    let mut success = 0;
    let mut seeds = BTreeSet::new();
    let mut names = BTreeSet::new();
    for case in cases {
        assert!(names.insert(case["name"].as_str().unwrap()));
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        assert_eq!(pool.protocol, "raydium-clmm-dynamic");
        assert_eq!(pool.adapter_version, "raydium-clmm/6d7a0f82-spl-dynamic-1");
        let mode = pool.raydium_clmm.as_ref().unwrap().collect_fee_on;
        assert!(mode <= 2);
        let seed = pool.raydium_clmm.as_ref().unwrap().seed_index;
        assert!(seed <= 65535);
        seeds.insert(seed);
        assert!(pool.raydium_clmm.as_ref().unwrap().dynamic_fee.is_some());
        assert!(pool
            .raydium_clmm
            .as_ref()
            .unwrap()
            .bitmap_extension
            .is_some());
        let left = case["zero_for_one"].as_bool().unwrap();
        cells[mode as usize][usize::from(left)] += 1;
        if assert_case(&pool, case, "case").is_some() {
            success += 1;
        }
    }
    assert!(cells.iter().flatten().all(|count| *count > 0));
    assert!(seeds.contains(&0) && seeds.iter().any(|seed| *seed > 0));
    assert!(
        success > 0 && success < cases.len(),
        "cover success and rejection"
    );
    eprintln!("native dynamic: {} cases, {success} successes, {} rejections; mode/direction counts {cells:?}", cases.len(), cases.len() - success);
}

#[test]
fn pinned_native_dynamic_shared_sequences_match_each_and_final_pool() {
    let corpus = fixture();
    let sequences = corpus["sequences"].as_array().unwrap();
    assert!(sequences.len() >= 8);
    let mut shapes = BTreeSet::new();
    let mut count = 0;
    for sequence in sequences {
        let name = sequence["name"].as_str().unwrap();
        let mut pool: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        let mode = pool.raydium_clmm.as_ref().unwrap().collect_fee_on;
        let steps = sequence["steps"].as_array().unwrap();
        assert!(steps.len() >= 2);
        let mut directions = BTreeSet::new();
        for step in steps {
            directions.insert(step["zero_for_one"].as_bool().unwrap());
            pool = assert_case(&pool, step, name).expect("shared sequence must succeed");
            count += 1;
        }
        shapes.insert((mode, directions.len(), *directions.first().unwrap()));
        let expected: Pool =
            serde_json::from_value(sequence["expected_final_pool"].clone()).unwrap();
        assert_eq!(pool, expected, "{name}: complete final pool");
    }
    for mode in [0, 1, 2] {
        assert!(
            shapes.contains(&(mode, 1, true)),
            "mode{mode}: left sequences"
        );
        assert!(
            shapes.contains(&(mode, 1, false)),
            "mode{mode}: right sequences"
        );
        assert!(
            shapes.contains(&(mode, 2, false)),
            "mode{mode}: mixed sequences"
        );
    }
    eprintln!(
        "native dynamic: {} shared sequences, {count} steps",
        sequences.len()
    );
}

#[test]
fn native_dynamic_conservative_admission_is_distinct_from_actual_step_budget() {
    let f = fixture();
    let probes = f["admission_probes"].as_array().unwrap();
    assert!(!probes.is_empty());
    let mut actual_successes = 0;
    let mut actual_rejections = 0;
    for c in probes {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let before = p.clone();
        let result = quote_exact_in(&p, &input, c["zero_for_one"].as_bool().unwrap());
        assert_eq!(p, before);
        match result {
            Ok(q) => {
                actual_successes += 1;
                let expected = &c["relaxed_expected"];
                assert_eq!(
                    q.amount_out.to_string(),
                    expected["amount_out"].as_str().unwrap()
                );
                assert_eq!(q.consumed_in, input);
                assert_eq!(q.remaining_in, Amount::zero());
                let mut post: Pool = serde_json::from_value(expected["next_pool"].clone()).unwrap();
                post.raydium_clmm.as_mut().unwrap().max_steps =
                    p.raydium_clmm.as_ref().unwrap().max_steps;
                assert_eq!(q.next_pool, post, "{}: admitted actual budget", c["name"]);
            }
            Err(e) => {
                actual_rejections += 1;
                assert_eq!(e.0, "RAYCLMM_STEP_BUDGET", "{}", c["name"]);
            }
        }
    }
    assert!(actual_successes > 0 && actual_rejections > 0);
    eprintln!("native dynamic admission: {} conservative probes; actual {actual_successes} successes / {actual_rejections} budget rejections",probes.len());
}
