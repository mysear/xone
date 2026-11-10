use serde_json::Value;
use xone_domain::{Amount, Pool};
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-bsc4-state.json"
    ))
    .unwrap()
}
#[test]
fn actual_bsc4_original_swaps_and_lazy_migration_match_every_poststate() {
    let f = data();
    let (mut complete, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let amount: Amount = serde_json::from_value(c["input"].clone()).unwrap();
        let result = xone_protocol_math::pancake_bsc_lm::quote_exact_in(
            &p,
            &amount,
            c["zero_for_one"].as_bool().unwrap(),
        );
        if c["expected"]["partial"] == true || c["expected"]["reverted"] == true {
            assert!(result.is_err(), "case {}", c["index"]);
            rejected += 1;
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("case {}: {}", c["index"], e));
        assert_eq!(
            q.amount_out.to_string(),
            c["expected"]["amount_out"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::to_value(q.next_pool).unwrap(),
            c["expected"]["expected_pool"],
            "case {}",
            c["index"]
        );
        assert_eq!(q.consumed_in, amount);
        assert_eq!(q.remaining_in, Amount::zero());
        complete += 1;
    }
    assert_eq!((complete, rejected), (48, 20));
}
#[test]
fn actual_bsc4_migration_flags_and_local_slots_carry_over_fifteen_original_steps() {
    let f = data();
    let mut steps = 0;
    for sequence in f["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        for step in sequence["steps"].as_array().unwrap() {
            let input: Amount = serde_json::from_value(step["input"].clone()).unwrap();
            let q = xone_protocol_math::pancake_bsc_lm::quote_exact_in(
                &p,
                &input,
                step["zero_for_one"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(
                q.amount_out.to_string(),
                step["amount_out"].as_str().unwrap()
            );
            assert_eq!(
                serde_json::to_value(&q.next_pool).unwrap(),
                step["expected_pool"],
                "{} step {}",
                sequence["mode"],
                steps
            );
            p = q.next_pool;
            steps += 1;
        }
    }
    assert_eq!(steps, 15);
}

#[test]
fn bsc4_actual_api_amount_is_complete_and_materializes_an_inherited_tick() {
    let f = data();
    let seq = &f["sequences"][0];
    let input: Amount = "13000000000".parse().unwrap();
    for index in 0..3 {
        let id = seq["source_bank_ids"][index].as_u64().unwrap() as usize;
        let p: Pool = serde_json::from_value(f["source_banks"][id]["pool"].clone()).unwrap();
        let q = xone_protocol_math::pancake_bsc_lm::quote_exact_in(&p, &input, true).unwrap();
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        if index == 0 {
            assert!(p.pancake_bsc_lm.unwrap().flags.iter().all(|f| !f.migrated));
            assert!(q
                .next_pool
                .pancake_bsc_lm
                .unwrap()
                .flags
                .iter()
                .any(|f| f.migrated));
        }
    }
}
