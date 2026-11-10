use xone_domain::{Amount, Pool};
use xone_protocol_math::slipstream_context::quote_exact_in;
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-context-state.json"
    ))
    .unwrap()
}
#[test]
fn contextual_original_swaps_match_full_curve_fee_gauge_reward_and_oracle_state() {
    let data = data();
    assert_eq!(data["cases"].as_array().unwrap().len(), 108);
    let mut full = 0;
    let mut partial = 0;
    for case in data["cases"].as_array().unwrap() {
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let state = pool.slipstream_context.as_ref().unwrap();
        let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
        let result = quote_exact_in(
            &pool,
            &input,
            case["zero_for_one"].as_bool().unwrap(),
            &state.view_origin,
        );
        if case["full_consumption"] != true {
            assert!(result.is_err(), "partial original case {}", case["index"]);
            partial += 1;
        } else {
            let q = result.unwrap_or_else(|e| panic!("original case {}: {e}", case["index"]));
            assert_eq!(
                q.amount_out.to_string(),
                case["original_expected"]["amount_out"].as_str().unwrap()
            );
            assert_eq!(q.consumed_in, input);
            assert_eq!(q.remaining_in, Amount::zero());
            assert_eq!(
                serde_json::to_value(q.next_pool).unwrap(),
                case["expected_pool"],
                "full original state {}",
                case["index"]
            );
            full += 1;
        }
    }
    assert_eq!((full, partial), (90, 18));
}
#[test]
fn same_block_initial_fee_changes_and_twelve_original_carried_steps_are_exact() {
    let data = data();
    assert_eq!(data["sequences"].as_array().unwrap().len(), 4);
    let mut changed = 0;
    for sequence in data["sequences"].as_array().unwrap() {
        let index = sequence["initial_case_index"].as_u64().unwrap() as usize;
        let mut pool: Pool = serde_json::from_value(data["cases"][index]["pool"].clone()).unwrap();
        let zero = data["cases"][index]["zero_for_one"].as_bool().unwrap();
        let origin = pool
            .slipstream_context
            .as_ref()
            .unwrap()
            .view_origin
            .clone();
        let first_fee = pool.slipstream_context.as_ref().unwrap().view_swap_fee;
        for step in sequence["steps"].as_array().unwrap() {
            let fee = xone_domain::slipstream_context::policy::swap_fee(
                pool.slipstream_context.as_ref().unwrap(),
                &origin,
            )
            .unwrap();
            assert_eq!(fee.to_string(), step["fee"].as_str().unwrap());
            if fee != first_fee {
                changed += 1;
            }
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let q = quote_exact_in(&pool, &input, zero, &origin).unwrap();
            assert_eq!(
                q.amount_out.to_string(),
                step["amount_out"].as_str().unwrap()
            );
            assert_eq!(
                serde_json::to_value(&q.next_pool).unwrap(),
                step["expected_pool"]
            );
            pool = q.next_pool;
        }
    }
    assert_eq!(changed, 8);
}
#[test]
fn generic_context_free_entry_rejects_while_explicit_unknown_dynamic_origins_stay_closed() {
    let data = data();
    let case = data["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["mode"] == "dynamic")
        .unwrap();
    let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
    let input: Amount = "1000000".parse().unwrap();
    assert_eq!(
        xone_protocol_math::quote_exact_in(&pool, &input, true)
            .err()
            .unwrap()
            .0,
        "SLIPSTREAM_FUNDS_REQUIRED"
    );
    assert_eq!(
        quote_exact_in(
            &pool,
            &input,
            true,
            "0x000000000000000000000000000000000000beef"
        )
        .err()
        .unwrap()
        .0,
        "SLIPSTREAM_ORIGIN_SCOPE_UNAVAILABLE"
    );
}
