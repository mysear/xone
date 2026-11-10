use xone_domain::{Amount, Pool};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-lm-state.json"
    ))
    .unwrap()
}
#[test]
fn complete_original_pancake_lm_swaps_match_full_reward_fee_tick_oracle_state() {
    let f = data();
    let (mut full, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = serde_json::from_value(c["input"].clone()).unwrap();
        let result = xone_protocol_math::pancake_lm::quote_exact_in(
            &p,
            &input,
            c["zero_for_one"].as_bool().unwrap(),
        );
        if c["expected"]["reverted"] == true || c["expected"]["partial"] == true {
            assert!(result.is_err(), "case {}", c["index"]);
            rejected += 1;
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("case {}: {}", c["index"], e));
        assert_eq!(
            q.amount_out.to_string(),
            c["expected"]["amount_out"].as_str().unwrap(),
            "case {}",
            c["index"]
        );
        let expected: Pool =
            serde_json::from_value(c["expected"]["expected_pool"].clone()).unwrap();
        assert_eq!(q.next_pool, expected, "case {}", c["index"]);
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        full += 1;
    }
    assert_eq!((full, rejected), (80, 24));
}
#[test]
fn original_pancake_lm_same_block_sequences_carry_every_accounting_field() {
    let f = data();
    let mut count = 0;
    for sequence in f["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        for step in sequence["steps"].as_array().unwrap() {
            let input: Amount = serde_json::from_value(step["input"].clone()).unwrap();
            let q = xone_protocol_math::pancake_lm::quote_exact_in(
                &p,
                &input,
                step["zero_for_one"].as_bool().unwrap(),
            )
            .unwrap();
            let expected: Pool = serde_json::from_value(step["expected_pool"].clone()).unwrap();
            assert_eq!(q.next_pool, expected);
            assert_eq!(
                q.amount_out.to_string(),
                step["amount_out"].as_str().unwrap()
            );
            p = q.next_pool;
            count += 1;
        }
    }
    assert_eq!(count, 12);
}
#[test]
fn generic_context_free_pancake_lm_entry_does_not_claim_external_funds() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][2]["pool"].clone()).unwrap();
    let amount = Amount(10000u64.into());
    let error = xone_protocol_math::quote_exact_in(&p, &amount, true).unwrap_err();
    assert!(error.to_string().contains("PANCAKE_LM_FUNDS_REQUIRED"));
}

#[test]
fn ambiguous_parent_or_substituted_context_is_not_silently_discarded() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][2]["pool"].clone()).unwrap();
    let input: Amount = "10000".parse().unwrap();
    let mut bad = p.clone();
    bad.v3 = Some(p.pancake_lm.as_ref().unwrap().core.clone());
    assert!(xone_protocol_math::pancake_lm::quote_exact_in(&bad, &input, true).is_err());
    let mut other = p.pancake_lm.as_ref().unwrap().clone();
    other.fee_growth_global0 = Amount(1u64.into());
    assert!(xone_protocol_math::pancake_lm::quote_state(&p, &other, &input, true).is_err());
}
