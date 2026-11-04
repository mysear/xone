//! The fixture is produced by the separate pinned original-manager runner.
//! Expected output and complete post-state never come from this quote module.
pub use xone_protocol_math::orca_adaptive_online;
#[path = "../src/orca.rs"]
mod orca;
use serde_json::Value;
use xone_domain::{Amount, Pool};
pub use xone_protocol_math::Swap;

fn assert_case(pool: &Pool, case: &Value) -> Option<Pool> {
    let name = case["name"].as_str().unwrap_or("sequence step");
    let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
    let result = orca::quote_exact_in(pool, &input, case["zero_for_one"].as_bool().unwrap());
    let expected = &case["expected"];
    if expected["reverted"].as_bool() == Some(true) {
        assert!(result.is_err(), "{name}: expected rejection");
        return None;
    }
    let q = result.unwrap_or_else(|e| panic!("{name}: {}", e.0));
    assert_eq!(
        q.amount_out.to_string(),
        expected["amount_out"].as_str().unwrap(),
        "{name}: output"
    );
    assert_eq!(q.consumed_in, input, "{name}: full input");
    assert_eq!(q.remaining_in, Amount::zero(), "{name}: remainder");
    let expected_pool: Pool = serde_json::from_value(expected["next_pool"].clone()).unwrap();
    assert_eq!(q.next_pool, expected_pool, "{name}: complete next pool");
    Some(q.next_pool)
}

#[test]
fn original_pinned_manager_matches_every_quote_and_shared_pool_sequence() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/orca-quotes.json");
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("independent cc683ec9 swap-manager fixture is required"),
    )
    .unwrap();
    assert_eq!(
        fixture["reference"]["revision"],
        "cc683ec92acb5e7606caa3d8e514d9383e9c7c11"
    );
    assert_eq!(fixture["reference"]["production_math_imports"], false);
    assert_eq!(fixture["reference"]["primary_array_cap"], 3);
    let cases = fixture["cases"].as_array().unwrap();
    assert!(
        cases.len() >= 300,
        "at least 300 original-manager cases required"
    );
    for case in cases {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        assert_case(&p, case);
    }
    let sequences = fixture["sequences"].as_array().unwrap();
    assert!(!sequences.is_empty(), "shared-pool sequences required");
    for sequence in sequences {
        let mut p: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        for step in sequence["steps"].as_array().unwrap() {
            if let Some(next) = assert_case(&p, step) {
                p = next;
            }
        }
        let expected: Pool =
            serde_json::from_value(sequence["expected_final_pool"].clone()).unwrap();
        assert_eq!(p, expected, "{}: final pool", sequence["name"]);
    }
}
