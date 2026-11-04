//! The local pinned native-program corpus is independent of router math.
use serde_json::Value;
use std::collections::BTreeSet;
use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-seed-quotes.json"
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
fn pinned_native_custom_seed_cases_match_complete_post_pool() {
    let corpus = fixture();
    assert_eq!(
        corpus["reference"]["revision"],
        "6d7a0f82d2f293c553c69aaf943377367c6c4cc6"
    );
    assert_eq!(corpus["reference"]["business_math_imported"], false);
    assert_eq!(corpus["reference"]["profile"], "raydium-clmm-custom-seed");
    assert_eq!(
        corpus["reference"]["adapter_version"],
        "raydium-clmm/6d7a0f82-spl-custom-seed-1"
    );
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() >= 160);
    let mut cells = [[0; 2]; 3];
    let mut success = 0;
    let mut names = BTreeSet::new();
    for case in cases {
        assert!(names.insert(case["name"].as_str().unwrap()));
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        assert_eq!(pool.protocol, "raydium-clmm-custom-seed");
        assert_eq!(
            pool.adapter_version,
            "raydium-clmm/6d7a0f82-spl-custom-seed-1"
        );
        let mode = pool.raydium_clmm.as_ref().unwrap().collect_fee_on;
        assert!(mode <= 2);
        assert!((1..=65535).contains(&pool.raydium_clmm.as_ref().unwrap().seed_index));
        let left = case["zero_for_one"].as_bool().unwrap();
        cells[mode as usize][usize::from(left)] += 1;
        if assert_case(&pool, case, "case").is_some() {
            success += 1;
        }
    }
    assert!(cells.iter().flatten().all(|count| *count > 0));
    assert!(
        success > 0 && success < cases.len(),
        "cover success and rejection"
    );
    eprintln!("native token-fee: {} cases, {success} successes, {} rejections; mode/direction counts {cells:?}", cases.len(), cases.len() - success);
}

#[test]
fn pinned_native_custom_seed_shared_sequences_match_each_and_final_pool() {
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
    for mode in [1, 2] {
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
        "native token-fee: {} shared sequences, {count} steps",
        sequences.len()
    );
}

#[test]
fn native_custom_seed_identity_error_probes_are_rejected_without_mutation() {
    let f = fixture();
    for c in f["error_probes"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let before = p.clone();
        assert!(
            quote_exact_in(&p, &input, c["zero_for_one"].as_bool().unwrap()).is_err(),
            "{}",
            c["name"]
        );
        assert_eq!(p, before);
    }
}
