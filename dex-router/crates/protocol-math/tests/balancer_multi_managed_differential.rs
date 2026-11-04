use num_bigint::BigUint;
use serde_json::Value;
use xone_protocol_math::balancer_multi;
fn values(v: &Value) -> Vec<BigUint> {
    v.as_array().unwrap().iter().map(amount).collect()
}
fn amount(v: &Value) -> BigUint {
    v.as_str().unwrap().parse().unwrap()
}
#[test]
fn original_managed_callback_matches_compressed_scheduled_snapshot_weights() {
    let data: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/balancer-multi-managed.json"
    ))
    .unwrap();
    assert_eq!(
        data["reference"]["revision"],
        "e281ff40583c39bf1d46c322334b2b13936e0e33"
    );
    let guards = balancer_multi::ManagedSwapGuards {
        swaps_enabled: true,
        protocol_swap_fee: BigUint::from(0u8),
        management_swap_fee: BigUint::from(0u8),
    };
    let (mut good, mut bad, mut drift) = (0, 0, 0);
    for (id, c) in data["cases"].as_array().unwrap().iter().enumerate() {
        let b = values(&c["balances"]);
        let w = values(&c["weights"]);
        let s = values(&c["scales"]);
        if w.iter().sum::<BigUint>() != "1000000000000000000".parse().unwrap() {
            drift += 1;
        }
        let got = balancer_multi::quote_managed_snapshot(
            &b,
            &w,
            &s,
            &amount(&c["fee"]),
            &amount(&c["input"]),
            c["index_in"].as_u64().unwrap() as usize,
            c["index_out"].as_u64().unwrap() as usize,
            &guards,
        );
        let e = &c["expected"];
        if e["reverted"] == true || e["amount_out"] == "0" {
            assert!(got.is_err(), "case {id}");
            bad += 1;
        } else {
            let q = got.unwrap_or_else(|e| panic!("case {id}: {e}: {c}"));
            assert_eq!(q.amount_out, amount(&e["amount_out"]), "case {id}");
            assert_eq!(q.next_balances, values(&e["next_balances"]), "case {id}");
            good += 1;
        }
    }
    assert!(
        good > 150 && bad > 150 && drift > 100,
        "{good}/{bad}/{drift}"
    );
    for s in data["sequences"].as_array().unwrap() {
        let mut b = values(&s["balances"]);
        for step in s["steps"].as_array().unwrap() {
            let q = balancer_multi::quote_managed_snapshot(
                &b,
                &values(&s["weights"]),
                &values(&s["scales"]),
                &amount(&s["fee"]),
                &amount(&step["input"]),
                step["index_in"].as_u64().unwrap() as usize,
                step["index_out"].as_u64().unwrap() as usize,
                &guards,
            )
            .unwrap();
            assert_eq!(q.amount_out, amount(&step["expected"]["amount_out"]));
            assert_eq!(q.next_balances, values(&step["expected"]["next_balances"]));
            b = q.next_balances;
        }
    }
    for c in data["weight_cases"].as_array().unwrap() {
        let starts: Vec<u64> = c["start_compressed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().parse().unwrap())
            .collect();
        let ends: Vec<u64> = c["end_compressed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().parse().unwrap())
            .collect();
        let got = balancer_multi::managed_weights_at(
            &starts,
            &ends,
            &amount(&c["denormalized_sum"]),
            c["start_time"].as_u64().unwrap() as u32,
            c["end_time"].as_u64().unwrap() as u32,
            c["timestamp"].as_u64().unwrap(),
        )
        .unwrap();
        assert_eq!(got, values(&c["expected"]));
    }
    assert_eq!(data["guards"].as_array().unwrap().len(), 4);
}
