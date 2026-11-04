use num_bigint::BigUint;
use serde_json::Value;
use xone_protocol_math::balancer_multi;
fn values(v: &Value) -> Vec<BigUint> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().parse().unwrap())
        .collect()
}
fn amount(v: &Value) -> BigUint {
    v.as_str().unwrap().parse().unwrap()
}
#[test]
fn original_multi_weighted_callbacks_match_outputs_reverts_and_entire_poststate() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/balancer-multi-quotes.json"
    ))
    .unwrap();
    assert_eq!(
        data["reference"]["revision"],
        "e281ff40583c39bf1d46c322334b2b13936e0e33"
    );
    let (mut positive, mut rejected) = (0, 0);
    for (id, c) in data["cases"].as_array().unwrap().iter().enumerate() {
        let got = balancer_multi::quote_exact_in(
            &values(&c["balances"]),
            &values(&c["weights"]),
            &values(&c["scales"]),
            &amount(&c["fee"]),
            &amount(&c["input"]),
            c["index_in"].as_u64().unwrap() as usize,
            c["index_out"].as_u64().unwrap() as usize,
        );
        let e = &c["expected"];
        if e["reverted"] == true || e["amount_out"] == "0" {
            assert!(got.is_err(), "case {id} {c}");
            rejected += 1;
        } else {
            let q = got.unwrap_or_else(|e| panic!("case {id}: {e}: {c}"));
            assert_eq!(q.amount_out, amount(&e["amount_out"]), "case {id}");
            assert_eq!(q.next_balances, values(&e["next_balances"]), "case {id}");
            positive += 1;
        }
    }
    assert!(positive > 200 && rejected > 100, "{positive}/{rejected}");
    for s in data["sequences"].as_array().unwrap() {
        let mut b = values(&s["balances"]);
        for step in s["steps"].as_array().unwrap() {
            let q = balancer_multi::quote_exact_in(
                &b,
                &values(&s["weights"]),
                &values(&s["scales"]),
                &amount(&s["fee"]),
                &amount(&step["input"]),
                step["index_in"].as_u64().unwrap() as usize,
                step["index_out"].as_u64().unwrap() as usize,
            )
            .unwrap();
            assert_eq!(q.amount_out, amount(&step["expected"]["amount_out"]));
            assert_eq!(q.next_balances, values(&step["expected"]["next_balances"]));
            b = q.next_balances;
        }
    }
}
