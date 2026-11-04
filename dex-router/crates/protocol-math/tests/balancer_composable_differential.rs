#[path = "../src/balancer_composable.rs"]
mod balancer_composable;
use num_bigint::BigUint;
use serde_json::Value;
fn amount(v: &Value) -> BigUint {
    v.as_str().unwrap().parse().unwrap()
}
fn values(v: &Value) -> Vec<BigUint> {
    v.as_array().unwrap().iter().map(amount).collect()
}
fn index(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
#[test]
fn original_stablemath_regular_and_bpt_kernels_match() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/balancer-composable-math.json"
    ))
    .unwrap();
    assert_eq!(
        data["reference"]["revision"],
        "e91a2b643a49856f51a648d175667c1b48cf3377"
    );
    let (mut good, mut bad) = (0, 0);
    for (id, c) in data["regular"].as_array().unwrap().iter().enumerate() {
        let b = values(&c["balances"]);
        let input = amount(&c["input"]);
        let i = index(&c["index_in"]);
        let j = index(&c["index_out"]);
        let q = balancer_composable::quote_exact_in(
            &b,
            &amount(&c["amplification"]),
            &values(&c["scales"]),
            &amount(&c["fee"]),
            &input,
            i,
            j,
            index(&c["bpt_index"]),
        );
        let e = &c["expected"];
        if e["reverted"] == true || e["amount_out"] == "0" {
            assert!(q.is_err(), "regular {id}");
            bad += 1;
        } else {
            let q = q.unwrap_or_else(|e| panic!("regular {id}: {e}: {c}"));
            assert_eq!(q.amount_out, amount(&e["amount_out"]), "regular {id}");
            let mut expected = b;
            expected[i] += input;
            expected[j] -= &q.amount_out;
            assert_eq!(q.next_balances, expected);
            good += 1;
        }
    }
    assert!(good > 200 && bad > 100, "regular {good}/{bad}");
    let (mut joins, mut exits, mut rejected) = (0, 0, 0);
    for (id, c) in data["bpt"].as_array().unwrap().iter().enumerate() {
        let b = values(&c["balances"]);
        let input = amount(&c["input"]);
        let i = index(&c["token_index"]);
        let join = c["token_to_bpt"].as_bool().unwrap();
        let q = balancer_composable::quote_bpt_exact_in_after_fees(
            &b,
            &amount(&c["amplification"]),
            &values(&c["scales"]),
            &amount(&c["fee"]),
            &input,
            &amount(&c["actual_supply_after_fees"]),
            i,
            join,
        );
        let e = &c["expected"];
        if e["reverted"] == true || e["amount_out"] == "0" {
            assert!(q.is_err(), "bpt {id}");
            rejected += 1;
        } else {
            let q = q.unwrap_or_else(|e| panic!("bpt {id}: {e}: {c}"));
            assert_eq!(q.amount_out, amount(&e["amount_out"]), "bpt {id}");
            assert_eq!(q.next_actual_supply, amount(&e["next_actual_supply"]));
            let mut expected = b;
            if join {
                expected[i] += input;
                joins += 1;
            } else {
                expected[i] -= &q.amount_out;
                exits += 1;
            }
            assert_eq!(q.next_underlying_balances, expected);
        }
    }
    assert!(
        joins > 100 && exits > 100 && rejected > 100,
        "BPT {joins}/{exits}/{rejected}"
    );
    for s in data["sequences"].as_array().unwrap() {
        let mut b = values(&s["balances"]);
        for step in s["steps"].as_array().unwrap() {
            let q = balancer_composable::quote_exact_in(
                &b,
                &amount(&s["amplification"]),
                &values(&s["scales"]),
                &amount(&s["fee"]),
                &amount(&step["input"]),
                index(&step["index_in"]),
                index(&step["index_out"]),
                index(&s["bpt_index"]),
            )
            .unwrap();
            assert_eq!(q.amount_out, amount(&step["expected"]["amount_out"]));
            b = q.next_balances;
        }
    }

    for s in data["bpt_sequences"].as_array().unwrap() {
        let mut b = values(&s["balances"]);
        let mut supply = amount(&s["actual_supply_after_fees"]);
        for step in s["steps"].as_array().unwrap() {
            let q = balancer_composable::quote_bpt_exact_in_after_fees(
                &b,
                &amount(&s["amplification"]),
                &values(&s["scales"]),
                &amount(&s["fee"]),
                &amount(&step["input"]),
                &supply,
                index(&step["token_index"]),
                step["token_to_bpt"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(q.amount_out, amount(&step["expected"]["amount_out"]));
            assert_eq!(
                q.next_actual_supply,
                amount(&step["expected"]["next_actual_supply"])
            );
            b = q.next_underlying_balances;
            supply = q.next_actual_supply;
        }
    }
}

#[test]
fn no_rate_nonzero_fee_admission_vectors_match_original_kernel() {
    let d: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/balancer-composable-admitted.json"
    ))
    .unwrap();
    let cases = d["regular"].as_array().unwrap();
    assert_eq!(cases.len(), 48);
    for c in cases {
        let q = balancer_composable::quote_exact_in(
            &values(&c["balances"]),
            &amount(&c["amplification"]),
            &values(&c["scales"]),
            &amount(&c["fee"]),
            &amount(&c["input"]),
            index(&c["index_in"]),
            index(&c["index_out"]),
            index(&c["bpt_index"]),
        )
        .unwrap();
        assert_eq!(q.amount_out, amount(&c["expected"]["amount_out"]));
    }
}
