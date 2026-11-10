//! Negative amountSpecified through fixed original Slipstream modules and complete ledgers.
use serde_json::Value;
use xone_domain::{Amount, Pool};
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-exact-out.json"
    ))
    .unwrap()
}
#[test]
fn original_slipstream_exact_output_preserves_all_ledgers_and_rejects_partial_targets() {
    let f = data();
    let (mut complete, mut partial, mut rejected) = (0, 0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let saved = p.clone();
        let output: Amount = c["target_output"].as_str().unwrap().parse().unwrap();
        let result = xone_protocol_math::exact_out::quote_exact_out_for_origin(
            &p,
            &output,
            c["zero_for_one"].as_bool().unwrap(),
            f["deployment"]["sender"].as_str().unwrap(),
        );
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
            complete += 1;
        }
        assert_eq!(p, saved);
    }
    assert_eq!((complete, partial, rejected), (90, 18, 0));
}
#[test]
fn original_slipstream_exact_output_sequences_carry_fee_reward_and_oracle() {
    let f = data();
    let mut count = 0;
    for seq in f["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        for step in seq["steps"].as_array().unwrap() {
            let target: Amount = step["target_output"].as_str().unwrap().parse().unwrap();
            let q = xone_protocol_math::exact_out::quote_exact_out_for_origin(
                &p,
                &target,
                step["zero_for_one"].as_bool().unwrap(),
                f["deployment"]["sender"].as_str().unwrap(),
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
fn slipstream_exact_output_requires_origin_scope_and_complete_curve_scope() {
    let f = data();
    let mut p: Pool = serde_json::from_value(f["cases"][86]["pool"].clone()).unwrap();
    let output: Amount = "1000000000".parse().unwrap();
    let origin = f["deployment"]["sender"].as_str().unwrap();
    assert_eq!(
        xone_protocol_math::quote_exact_out(&p, &output, true)
            .unwrap_err()
            .0,
        "SLIPSTREAM_ORIGIN_REQUIRED"
    );
    for bad_origin in [
        "0x0000000000000000000000000000000000000000",
        "0x0000000000000000000000000000000000000001",
    ] {
        assert!(xone_protocol_math::exact_out::quote_exact_out_for_origin(
            &p, &output, true, bad_origin
        )
        .is_err());
    }
    for target in [
        Amount::zero(),
        Amount(num_bigint::BigUint::from(1u8) << 255usize),
    ] {
        assert!(xone_protocol_math::exact_out::quote_exact_out_for_origin(
            &p, &target, true, origin
        )
        .is_err());
    }
    p.slipstream_context.as_mut().unwrap().core.max_steps = 1;
    assert!(xone_protocol_math::exact_out::quote_exact_out_for_origin(
        &p,
        &"6000000000".parse().unwrap(),
        true,
        origin
    )
    .is_err());
}
