//! Negative amountSpecified through pinned 98684794 Pool/LM/MasterChef, complete ledgers.
use serde_json::Value;
use xone_domain::{Amount, Pool};
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-lm-exact-out.json"
    ))
    .unwrap()
}
#[test]
fn original_legacy_lm_exact_output_preserves_all_ledgers_and_rejects_partial_targets() {
    let f = data();
    let (mut complete, mut partial, mut rejected) = (0, 0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let saved = p.clone();
        let output: Amount = c["target_output"].as_str().unwrap().parse().unwrap();
        let result =
            xone_protocol_math::quote_exact_out(&p, &output, c["zero_for_one"].as_bool().unwrap());
        if c["expected"]["reverted"] == true || c["expected"]["partial"] == true {
            assert!(
                result.is_err(),
                "case {} must reject partial/revert",
                c["index"]
            );
            if c["expected"]["partial"] == true {
                partial += 1;
            } else {
                rejected += 1;
            }
        } else {
            let q = result.unwrap_or_else(|e| panic!("case {}: {e}", c["index"]));
            assert_eq!(q.amount_out, output);
            assert_eq!(
                q.amount_in.to_string(),
                c["expected"]["consumed_in"].as_str().unwrap()
            );
            assert_eq!(
                serde_json::to_value(&q.next_pool).unwrap(),
                c["expected"]["expected_pool"],
                "case {}",
                c["index"]
            );
            let limit = Amount(&q.amount_in.0 - 1u8);
            assert_eq!(
                xone_protocol_math::exact_out::quote_exact_out_with_limit(
                    &p,
                    &output,
                    &limit,
                    c["zero_for_one"].as_bool().unwrap()
                )
                .unwrap_err()
                .0,
                "EXACT_OUT_MAX_INPUT"
            );
            complete += 1;
        }
        assert_eq!(p, saved);
    }
    assert_eq!((complete, partial, rejected), (80, 20, 4));
}
#[test]
fn original_legacy_lm_exact_output_sequences_carry_fee_reward_and_oracle() {
    let f = data();
    let mut count = 0;
    for seq in f["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        for step in seq["steps"].as_array().unwrap() {
            let target: Amount = step["target_output"].as_str().unwrap().parse().unwrap();
            let q = xone_protocol_math::quote_exact_out(
                &p,
                &target,
                step["zero_for_one"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(
                q.amount_in.to_string(),
                step["consumed_in"].as_str().unwrap()
            );
            assert_eq!(q.amount_out, target);
            assert_eq!(
                serde_json::to_value(&q.next_pool).unwrap(),
                step["expected_pool"],
                "{} step {count}",
                seq["mode"]
            );
            p = q.next_pool;
            count += 1;
        }
    }
    assert_eq!(count, 12);
}
#[test]
fn legacy_lm_exact_output_rejects_zero_overflow_and_missing_curve_scope() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][0]["pool"].clone()).unwrap();
    for target in [
        Amount::zero(),
        Amount(num_bigint::BigUint::from(1u8) << 255usize),
    ] {
        assert!(xone_protocol_math::quote_exact_out(&p, &target, true).is_err());
    }
    let mut p = p;
    let ledger = &mut p.pancake_lm.as_mut().unwrap();
    ledger.core.max_steps = 1;
    assert!(
        xone_protocol_math::quote_exact_out(&p, &"20000000000".parse().unwrap(), true).is_err()
    );
}
