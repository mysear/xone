use sha2::{Digest, Sha256};
use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;

fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/velodrome-v2-quotes.json"
    ))
    .unwrap()
}

#[test]
fn velodrome_reference_is_bound_to_the_pinned_original_source() {
    let data = vectors();
    let source = include_bytes!("../../../test/fixtures/velodrome-v2/Pool.sol");
    let harness = include_bytes!("../../../test/fixtures/velodrome-v2/Reference.sol");
    assert_eq!(
        data["reference"]["commit"],
        "b3065d8b6702b14b094f9f6046b752cc9f78c43b"
    );
    assert_eq!(
        data["reference"]["sha256"],
        format!("{:x}", Sha256::digest(source))
    );
    assert_eq!(
        data["reference"]["harness_sha256"],
        format!("{:x}", Sha256::digest(harness))
    );
}

#[test]
fn velodrome_both_models_match_original_solidity_outputs_and_post_balances() {
    let data = vectors();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 216);
    let mut reverts = 0;
    for (index, case) in cases.iter().enumerate() {
        let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
        let quote = quote_exact_in(&p, &input, case["zero_for_one"].as_bool().unwrap());
        if case["expected"]["reverted"] == true {
            assert!(quote.is_err(), "case {index}");
            reverts += 1;
        } else {
            let quote = quote.unwrap_or_else(|e| panic!("case {index}: {e:?}"));
            assert_eq!(
                quote.amount_out.to_string(),
                case["expected"]["amount_out"].as_str().unwrap(),
                "case {index}"
            );
            assert_eq!(
                quote.next_pool.reserve0.to_string(),
                case["expected"]["next_balances"][0].as_str().unwrap(),
                "case {index}"
            );
            assert_eq!(
                quote.next_pool.reserve1.to_string(),
                case["expected"]["next_balances"][1].as_str().unwrap(),
                "case {index}"
            );
            assert_eq!(quote.consumed_in, input);
            assert_eq!(quote.remaining_in, Amount::zero());
            assert_eq!(quote.next_pool.adapter_version, p.adapter_version);
        }
    }
    assert_eq!(reverts, 42);
}

#[test]
fn velodrome_original_sequences_preserve_fees_and_shared_pool_state() {
    let data = vectors();
    let sequences = data["sequences"].as_array().unwrap();
    assert_eq!(sequences.len(), 8);
    for case in sequences {
        let mut pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        for step in case["steps"].as_array().unwrap() {
            let input = step["input"].as_str().unwrap().parse().unwrap();
            let quote =
                quote_exact_in(&pool, &input, case["zero_for_one"].as_bool().unwrap()).unwrap();
            assert_eq!(
                quote.amount_out.to_string(),
                step["amount_out"].as_str().unwrap()
            );
            assert_eq!(
                quote.next_pool.reserve0.to_string(),
                step["next_balances"][0].as_str().unwrap()
            );
            assert_eq!(
                quote.next_pool.reserve1.to_string(),
                step["next_balances"][1].as_str().unwrap()
            );
            pool = quote.next_pool;
        }
    }
}

#[test]
fn velodrome_adapter_cannot_be_relabelled_as_aerodrome_or_the_other_curve() {
    let data = vectors();
    for case in data["cases"].as_array().unwrap() {
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        pool.validate().unwrap();
        let mut changed = pool.clone();
        changed.adapter_version =
            format!("{}/pool-1", pool.protocol.replace("velodrome", "aerodrome"));
        assert!(changed.validate().is_err());
        changed = pool.clone();
        changed.protocol = if pool.protocol.ends_with("-stable") {
            "velodrome-v2-volatile"
        } else {
            "velodrome-v2-stable"
        }
        .into();
        assert!(changed.validate().is_err());
    }
}

#[test]
fn extreme_rounding_samples_match_both_pinned_original_controls() {
    let data = vectors();
    let cases = data["rounding_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30);
    for case in cases {
        let mut pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
        for expected in [&case["expected"], &case["aerodrome_control"]] {
            let result = quote_exact_in(&pool, &input, true);
            if expected["reverted"] == true {
                assert!(result.is_err());
            } else {
                let quote = result.unwrap();
                assert_eq!(
                    quote.amount_out.to_string(),
                    expected["amount_out"].as_str().unwrap()
                );
                assert_eq!(
                    quote.next_pool.reserve0.to_string(),
                    expected["next_balances"][0].as_str().unwrap()
                );
                assert_eq!(
                    quote.next_pool.reserve1.to_string(),
                    expected["next_balances"][1].as_str().unwrap()
                );
            }
            pool.protocol = "aerodrome-v2-stable".into();
            pool.adapter_version = "aerodrome-v2-stable/pool-1".into();
        }
    }
}
