//! Compare the service-local corpus from unchanged pinned native swap_internal.
//! Every successful quote must match the entire Pool, including extension proof.
use serde_json::Value;
use std::collections::BTreeSet;
use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-bitmap-quotes.json"
    ))
    .unwrap()
}

fn assert_case(pool: &Pool, case: &Value, context: &str) -> Option<Pool> {
    let name = case["name"].as_str().unwrap_or(context);
    let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
    let before = pool.clone();
    let result = quote_exact_in(pool, &input, case["zero_for_one"].as_bool().unwrap());
    assert_eq!(pool, &before, "{name}: caller Pool changed");
    let expected = &case["expected"];
    if expected["reverted"].as_bool() == Some(true) {
        assert!(result.is_err(), "{name}: expected native/profile rejection");
        return None;
    }
    let quote = result.unwrap_or_else(|e| panic!("{name}: {}", e.0));
    assert_eq!(
        quote.amount_out.to_string(),
        expected["amount_out"].as_str().unwrap(),
        "{name}: output"
    );
    assert_eq!(quote.consumed_in, input, "{name}: full input");
    assert_eq!(quote.remaining_in, Amount::zero(), "{name}: remainder");
    let expected_pool: Pool = serde_json::from_value(expected["next_pool"].clone()).unwrap();
    assert_eq!(quote.next_pool, expected_pool, "{name}: complete post-Pool");
    quote.next_pool.validate().unwrap();
    Some(quote.next_pool)
}

#[test]
fn pinned_native_bitmap_corpus_identifies_independent_source() {
    let corpus = fixture();
    assert_eq!(
        corpus["reference"]["revision"],
        "6d7a0f82d2f293c553c69aaf943377367c6c4cc6"
    );
    let reference = &corpus["reference"];
    assert!(reference["source"]
        .as_str()
        .unwrap()
        .contains("unchanged native swap_internal"));
    if let Some(imported) = reference.get("business_math_imported") {
        assert_eq!(imported, false);
    }
    assert_eq!(reference["sbf_transaction_executed"], false);
    assert_eq!(corpus["reference"]["profile"], "raydium-clmm-bitmap");
    assert_eq!(
        corpus["reference"]["adapter_version"],
        "raydium-clmm/6d7a0f82-spl-bitmap-1"
    );
}

#[test]
fn pinned_native_bitmap_cases_match_complete_post_pool() {
    let corpus = fixture();
    let cases = corpus["cases"].as_array().unwrap();
    assert!(cases.len() >= 160, "native bitmap corpus needs >=160 cases");
    let mut cells = [[0; 2]; 3];
    let mut successes = [[0; 2]; 3];
    let mut names = BTreeSet::new();
    let mut failures = Vec::new();
    for case in cases {
        assert!(names.insert(case["name"].as_str().unwrap()));
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        assert_eq!(pool.protocol, "raydium-clmm-bitmap");
        assert_eq!(pool.adapter_version, "raydium-clmm/6d7a0f82-spl-bitmap-1");
        let mode = pool.raydium_clmm.as_ref().unwrap().collect_fee_on as usize;
        assert!(mode <= 2);
        let direction = usize::from(case["zero_for_one"].as_bool().unwrap());
        cells[mode][direction] += 1;
        match std::panic::catch_unwind(|| assert_case(&pool, case, "case")) {
            Ok(Some(_)) => successes[mode][direction] += 1,
            Ok(None) => {}
            Err(_) => failures.push(case["name"].as_str().unwrap().to_owned()),
        }
    }
    assert!(
        failures.is_empty(),
        "native bitmap: {} cases checked; mismatches: {failures:?}",
        cases.len()
    );
    assert!(successes.iter().flatten().all(|count| *count > 0));
    let success: usize = successes.iter().flatten().sum();
    assert!(success < cases.len(), "native corpus also needs rejections");
    eprintln!(
        "native bitmap: {} cases, {success} successes, {} rejections; mode/direction counts {cells:?}",
        cases.len(),
        cases.len() - success
    );
}

#[test]
fn pinned_native_bitmap_shared_sequences_match_each_and_final_pool() {
    let corpus = fixture();
    let sequences = corpus["sequences"].as_array().unwrap();
    assert!(
        sequences.len() >= 8,
        "native bitmap corpus needs >=8 sequences"
    );
    let mut shapes = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut count = 0;
    for sequence in sequences {
        let name = sequence["name"].as_str().unwrap();
        assert!(names.insert(name));
        let mut pool: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        assert_eq!(pool.protocol, "raydium-clmm-bitmap");
        let steps = sequence["steps"].as_array().unwrap();
        assert!(steps.len() >= 2);
        let mut directions = BTreeSet::new();
        for step in steps {
            directions.insert(step["zero_for_one"].as_bool().unwrap());
            pool = assert_case(&pool, step, name).expect("shared sequence must succeed");
            count += 1;
        }
        shapes.insert(directions.len());
        let expected: Pool =
            serde_json::from_value(sequence["expected_final_pool"].clone()).unwrap();
        assert_eq!(pool, expected, "{name}: complete final Pool");
    }
    assert!(shapes.contains(&1), "same-direction sequences");
    assert!(shapes.contains(&2), "reverse-direction sequences");
    eprintln!(
        "native bitmap: {} shared sequences, {count} steps",
        sequences.len()
    );
}
