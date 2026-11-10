use serde_json::Value;
use xone_domain::{Amount, Pool};
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/lfj-exact-out.json")).unwrap()
}
#[test]
fn lfj_original_target_inputs_preserve_actual_surplus_and_every_declared_ledger() {
    let f = data();
    let (mut full, mut extra, mut partial) = (0, 0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let saved = p.clone();
        let target: Amount = c["target_output"].as_str().unwrap().parse().unwrap();
        let zero = c["zero_for_one"].as_bool().unwrap();
        let result = xone_protocol_math::quote_exact_out(&p, &target, zero);
        if c["full_target"] == false {
            assert!(result.is_err(), "case {}", c["index"]);
            partial += 1;
        } else {
            let (input, fee) =
                xone_protocol_math::lfj::input_for_target(&p, &target, zero).unwrap();
            assert_eq!(input.to_string(), c["required_input"].as_str().unwrap());
            assert_eq!(
                fee.to_string(),
                c["original"]["original_input_view"]["fee"]
                    .as_str()
                    .unwrap()
            );
            let q = result.unwrap_or_else(|e| panic!("case {}: {}", c["index"], e));
            assert_eq!(q.amount_in, input);
            assert_eq!(
                q.amount_out.to_string(),
                c["actual_output"].as_str().unwrap()
            );
            assert!(q.amount_out.0 >= target.0);
            assert_eq!(serde_json::to_value(q.next_pool).unwrap(), c["actual_pool"]);
            assert!(xone_protocol_math::exact_out::quote_exact_out_with_limit(
                &p,
                &target,
                &Amount(&input.0 - 1u8),
                zero
            )
            .is_err());
            full += 1;
            if c["output_surplus"] != "0" {
                extra += 1;
            }
        }
        assert_eq!(p, saved);
    }
    assert_eq!((full, extra, partial), (216, 60, 72));
}
#[test]
fn lfj_original_target_sequences_carry_real_output_and_oracle_state() {
    let f = data();
    let mut count = 0;
    for seq in f["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        for c in seq["steps"].as_array().unwrap() {
            let target: Amount = c["target_output"].as_str().unwrap().parse().unwrap();
            let q = xone_protocol_math::quote_exact_out(
                &p,
                &target,
                c["zero_for_one"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(
                q.amount_in.to_string(),
                c["required_input"].as_str().unwrap()
            );
            assert_eq!(
                q.amount_out.to_string(),
                c["actual_output"].as_str().unwrap()
            );
            assert_eq!(
                serde_json::to_value(&q.next_pool).unwrap(),
                c["actual_pool"]
            );
            p = q.next_pool;
            count += 1;
        }
    }
    assert_eq!(count, 18);
}
#[test]
fn lfj_output_targets_reject_zero_width_and_incomplete_step_budgets() {
    let f = data();
    let mut p: Pool = serde_json::from_value(f["cases"][0]["pool"].clone()).unwrap();
    for target in [
        Amount::zero(),
        Amount(num_bigint::BigUint::from(1u8) << 128usize),
    ] {
        assert!(xone_protocol_math::quote_exact_out(&p, &target, true).is_err());
    }
    p.lfj.as_mut().unwrap().max_steps = 1;
    assert!(xone_protocol_math::quote_exact_out(&p, &"2500000".parse().unwrap(), true).is_err());
}
