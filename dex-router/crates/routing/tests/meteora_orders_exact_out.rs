use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{meteora, Amount, Pool};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
const SENDER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
const RECIPIENT: &str = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
fn data() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-orders-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn edge(p: &Pool, zero: bool) -> Edge {
    Edge {
        from_token: None,
        protocol: p.protocol.clone(),
        pool_id: p.pool_id.clone(),
        zero_for_one: zero,
        to: if zero {
            p.token1.clone()
        } else {
            p.token0.clone()
        },
    }
}
#[test]
fn orders_exact_out_routes_replay_full_ledgers_and_fail_atomically() {
    let f = data();
    let (mut full, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["steps"][0]["pool"].clone()).unwrap();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p)]);
        for s in c["steps"].as_array().unwrap() {
            let e = edge(
                state.values().next().unwrap(),
                c["zero_for_one"].as_bool().unwrap(),
            );
            let before = state.clone();
            let target: Amount = s["amount"].as_str().unwrap().parse().unwrap();
            let q = evaluate_path(
                &mut state,
                &[e],
                &target,
                Funds {
                    sender: SENDER,
                    recipient: RECIPIENT,
                },
            );
            if s["passed"] == false {
                assert!(q.is_err(), "case {}", c["index"]);
                assert_eq!(state, before);
                rejected += 1;
            } else {
                let q = q.unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
                assert_eq!(q.amount_out, target);
                assert!(q.target_output.is_none());
                assert_eq!(q.factory, meteora::PROGRAM);
                assert_eq!(q.amount_in.to_string(), s["actual_input"].as_str().unwrap());
                let mut expected: Pool = serde_json::from_value(s["after"].clone()).unwrap();
                expected.meteora.as_mut().unwrap().derived = true;
                assert!(
                    state[&expected.pool_id] == expected,
                    "case {} full state",
                    c["index"]
                );
                full += 1;
            }
        }
    }
    assert_eq!((full, rejected), (193, 31));
}
#[test]
fn orders_exact_out_excludes_internal_funds_and_composed_hops() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][60]["steps"][0]["pool"].clone()).unwrap();
    let e = edge(&p, true);
    let initial = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    for account in p.meteora.as_ref().unwrap().internal_accounts(&p).unwrap() {
        let mut state = initial.clone();
        assert!(evaluate_path(
            &mut state,
            std::slice::from_ref(&e),
            &"1".parse().unwrap(),
            Funds {
                sender: SENDER,
                recipient: &account
            }
        )
        .is_err());
        assert_eq!(state, initial);
    }
    let mut state = initial.clone();
    assert!(evaluate_path(
        &mut state,
        &[e.clone(), edge(&p, false)],
        &"1".parse().unwrap(),
        Funds {
            sender: SENDER,
            recipient: RECIPIENT
        }
    )
    .is_err());
    assert_eq!(state, initial);
}
