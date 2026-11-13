use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
const SENDER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
const RECIPIENT: &str = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/orca-exact-out.json"
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
fn orca_routes_preserve_exact_target_and_full_native_poststate_or_rollback() {
    let f = corpus();
    let (mut full, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let mut map = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let before = map.clone();
        let q = evaluate_path(
            &mut map,
            &[edge(&p, c["zeroForOne"].as_bool().unwrap())],
            &c["target"].as_str().unwrap().parse().unwrap(),
            Funds {
                sender: SENDER,
                recipient: RECIPIENT,
            },
        );
        if c["passed"] == false {
            assert!(q.is_err(), "{}", c["name"]);
            assert_eq!(map, before);
            rejected += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{}: {}", c["name"], e.0));
        assert_eq!(q.amount_in.to_string(), c["requiredInput"]);
        assert_eq!(q.amount_out.to_string(), c["target"]);
        assert!(q.target_output.is_none());
        assert_eq!(q.factory, xone_domain::orca::PROGRAM_ID);
        assert_eq!(
            map[&p.pool_id],
            serde_json::from_value::<Pool>(c["after"].clone()).unwrap()
        );
        full += 1;
    }
    assert!(full >= 400 && rejected >= 100);
}
#[test]
fn orca_route_scope_and_internal_funds_are_explicit_and_atomic() {
    let f = corpus();
    for pair in f["pairs"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(pair["pools"].clone()).unwrap();
        let p = &pools[0];
        let mut map = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let before = map.clone();
        let amount: Amount = "1000".parse().unwrap();
        let e = evaluate_path(
            &mut map,
            &[edge(p, true), edge(&pools[1], false)],
            &amount,
            Funds {
                sender: SENDER,
                recipient: RECIPIENT,
            },
        )
        .unwrap_err();
        assert_eq!(e.0, "EXACT_OUT_ORCA_SINGLE_POOL_ONLY");
        assert_eq!(map, before);
        let state = p.orca_whirlpool.as_ref().unwrap();
        for internal in [
            &p.pool_id,
            &p.token0,
            &state.program_id,
            &state.vault0,
            &state.oracle,
            &state.whirlpools_config,
            &state.tick_arrays[0].address,
        ] {
            let e = evaluate_path(
                &mut map,
                &[edge(p, true)],
                &amount,
                Funds {
                    sender: internal,
                    recipient: RECIPIENT,
                },
            )
            .unwrap_err();
            assert_eq!(e.0, "EXACT_OUT_UNMODELED_SELF_FUNDING");
            assert_eq!(map, before);
        }
    }
}
