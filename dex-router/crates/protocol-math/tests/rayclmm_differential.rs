//! Independent pinned native swap_internal output is the oracle. Compare every
//! field, including untouched observation entries, identity and proof metadata.
use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_protocol_math::raydium_clmm;
pub use xone_protocol_math::Swap;

fn assert_case(pool: &Pool, case: &Value) -> Option<Pool> {
    let name = case["name"].as_str().unwrap_or("sequence step");
    let input: Amount = case["input"]
        .as_str()
        .expect("input decimal string")
        .parse()
        .unwrap();
    let before = pool.clone();
    let result =
        raydium_clmm::quote_exact_in(pool, &input, case["zero_for_one"].as_bool().unwrap());
    assert_eq!(pool, &before, "{name}: input mutation");
    let expected = &case["expected"];
    if expected["reverted"].as_bool() == Some(true) {
        assert!(
            result.is_err(),
            "{name}: expected original-program/profile rejection"
        );
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
    assert_eq!(q.next_pool, expected_pool, "{name}: complete post-pool");
    Some(q.next_pool)
}
#[test]
fn native_original_swap_internal_matches_full_post_state_and_sequences() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-quotes.json")).unwrap();
    assert_eq!(
        fixture["reference"]["revision"],
        "6d7a0f82d2f293c553c69aaf943377367c6c4cc6"
    );
    assert_eq!(fixture["reference"]["business_math_imported"], false);
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() >= 300);
    for case in cases {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        assert_case(&p, case);
    }
    let sequences = fixture["sequences"].as_array().unwrap();
    assert!(sequences.len() >= 8);
    for seq in sequences {
        let mut pool: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        for step in seq["steps"].as_array().unwrap() {
            if let Some(next) = assert_case(&pool, step) {
                pool = next;
            }
        }
        let expected: Pool = serde_json::from_value(seq["expected_final_pool"].clone()).unwrap();
        assert_eq!(pool, expected, "{}: final pool", seq["name"]);
    }
}
