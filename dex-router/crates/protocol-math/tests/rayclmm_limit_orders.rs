use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_protocol_math::{quote_exact_in, raydium_clmm};

#[test]
#[ignore = "requires the pinned native Raydium generate-orders corpus"]
fn original_fifo_matches_exact_in_out_and_complete_post_state() {
    let (filled, drained, rejected) = compare("XONE_RAYCLMM_ORDER_CORPUS");
    assert_eq!((filled, drained, rejected), (180, 34, 36));
}

#[test]
#[ignore = "requires the pinned native Raydium generate-advanced corpus"]
fn original_advanced_combines_orders_rewards_and_dynamic_fee() {
    let (filled, drained, rejected) = compare("XONE_RAYCLMM_ADVANCED_CORPUS");
    assert_eq!((filled, drained, rejected), (24, 6, 0));
}

fn compare(corpus_env: &str) -> (usize, usize, usize) {
    let path = std::env::var(corpus_env).unwrap();
    let corpus: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        corpus["revision"],
        "6d7a0f82d2f293c553c69aaf943377367c6c4cc6"
    );
    let mut filled = 0;
    let mut drained = 0;
    let mut rejected = 0;
    for (i, case) in corpus["cases"].as_array().unwrap().iter().enumerate() {
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let amount: Amount = case["amount"].as_str().unwrap().parse().unwrap();
        let left = case["left"].as_bool().unwrap();
        let result = if case["exact_in"] == true {
            quote_exact_in(&pool, &amount, left)
        } else {
            raydium_clmm::quote_exact_out(&pool, &amount, left)
        };
        let expected = &case["expected"];
        if expected["reverted"] == true {
            assert!(result.is_err(), "case {i}: expected rejection {expected}");
            rejected += 1;
            continue;
        }
        let result = result.unwrap_or_else(|error| panic!("case {i}: {error}"));
        let next_pool: Pool = serde_json::from_value(expected["next_pool"].clone()).unwrap();
        assert_eq!(
            result.consumed_in.to_string(),
            expected["consumed"],
            "case {i}"
        );
        assert_eq!(
            result.amount_out.to_string(),
            expected["output"],
            "case {i}"
        );
        assert_eq!(
            xone_domain::hash(&result.next_pool).unwrap(),
            xone_domain::hash(&next_pool).unwrap(),
            "case {i}: complete post pool differs"
        );
        let before = pool.raydium_clmm.as_ref().unwrap();
        let after = next_pool.raydium_clmm.as_ref().unwrap();
        if before.bitmap != after.bitmap || before.bitmap_extension != after.bitmap_extension {
            drained += 1;
        }
        filled += 1;
    }
    (filled, drained, rejected)
}
