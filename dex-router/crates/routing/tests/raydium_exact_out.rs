use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::exact_out::evaluate_path;
use xone_routing::{Edge, Funds};
const CALLER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
const RECIPIENT: &str = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
#[test]
fn raydium_exact_out_routes_preserve_native_state_and_atomic_rejection() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/raydium-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let (mut success, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let zero = c["zero_for_one"].as_bool().unwrap();
        let edge = Edge {
            from_token: None,
            pool_id: p.pool_id.clone(),
            protocol: p.protocol.clone(),
            zero_for_one: zero,
            to: if zero {
                p.token1.clone()
            } else {
                p.token0.clone()
            },
        };
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let before = state.clone();
        let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let q = evaluate_path(
            &mut state,
            std::slice::from_ref(&edge),
            &target,
            Funds {
                sender: CALLER,
                recipient: RECIPIENT,
            },
        );
        if c["expected"]["reverted"] == true {
            assert!(q.is_err());
            assert_eq!(state, before);
            rejected += 1;
        } else {
            let q = q.unwrap();
            assert_eq!(q.amount_out, target);
            assert_eq!(q.factory, xone_domain::raydium::PROGRAM_ID);
            assert_eq!(
                q.amount_in.to_string(),
                c["expected"]["amount_in"].as_str().unwrap()
            );
            assert_eq!(
                state[&p.pool_id],
                serde_json::from_value::<Pool>(c["expected"]["pool"].clone()).unwrap()
            );
            success += 1;
            let mut alias = before.clone();
            assert!(evaluate_path(
                &mut alias,
                std::slice::from_ref(&edge),
                &target,
                Funds {
                    sender: &p.raydium_cpmm.as_ref().unwrap().vault0,
                    recipient: RECIPIENT
                }
            )
            .is_err());
            assert_eq!(alias, before);
        }
    }
    assert!(success > 200 && rejected > 200);
}

#[test]
fn raydium_sequential_both_direction_post_states_match_original_helpers() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/raydium-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut count = 0;
    for sequence in f["sequences"].as_array().unwrap() {
        let initial: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        let mut state = BTreeMap::from([(initial.pool_id.clone(), initial)]);
        for step in sequence["steps"].as_array().unwrap() {
            let pool = state.values().next().unwrap();
            let zero = step["zero_for_one"].as_bool().unwrap();
            let edge = Edge {
                from_token: None,
                pool_id: pool.pool_id.clone(),
                protocol: pool.protocol.clone(),
                zero_for_one: zero,
                to: if zero {
                    pool.token1.clone()
                } else {
                    pool.token0.clone()
                },
            };
            let before = state.clone();
            let q = evaluate_path(
                &mut state,
                &[edge],
                &step["target"].as_str().unwrap().parse().unwrap(),
                Funds {
                    sender: CALLER,
                    recipient: RECIPIENT,
                },
            );
            if step["expected"]["reverted"] == true {
                assert!(q.is_err());
                assert_eq!(state, before);
            } else {
                let q = q.unwrap();
                assert_eq!(
                    q.amount_in.to_string(),
                    step["expected"]["amount_in"].as_str().unwrap()
                );
                assert_eq!(
                    state.values().next().unwrap(),
                    &serde_json::from_value::<Pool>(step["expected"]["pool"].clone()).unwrap()
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 54);
}
