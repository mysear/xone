use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
const SENDER: &str = "0x000000000000000000000000000000000000dead";
const RECIPIENT: &str = "0x000000000000000000000000000000000000beef";
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/v3-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn edge(p: &Pool, from: &str) -> Edge {
    let zero = p.token0 == from;
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
fn original_recursive_callback_two_hop_targets_match_reverse_and_forward_replay() {
    let f = corpus();
    assert_eq!(f["paths"].as_array().unwrap().len(), 4);
    for c in f["paths"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(c["routePools"].clone()).unwrap();
        let tokens: Vec<String> = serde_json::from_value(c["routeTokens"].clone()).unwrap();
        let edges = pools
            .iter()
            .zip(&tokens)
            .map(|(p, t)| edge(p, t))
            .collect::<Vec<_>>();
        let mut map = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let q = evaluate_path(
            &mut map,
            &edges,
            &c["target"].as_str().unwrap().parse().unwrap(),
            Funds {
                sender: SENDER,
                recipient: RECIPIENT,
            },
        )
        .unwrap();
        assert_eq!(q.amount_in.to_string(), c["requiredInput"]);
        assert_eq!(q.amount_out.to_string(), c["target"]);
        for expected in c["routeAfter"].as_array().unwrap() {
            let p: Pool = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(map[&p.pool_id], p);
        }
    }
}
#[test]
fn standard_v3_paths_reject_cycles_cross_factory_or_mixed_templates_without_mutation() {
    let f = corpus();
    for c in f["paths"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(c["routePools"].clone()).unwrap();
        let tokens: Vec<String> = serde_json::from_value(c["routeTokens"].clone()).unwrap();
        let edges = pools
            .iter()
            .zip(&tokens)
            .map(|(p, t)| edge(p, t))
            .collect::<Vec<_>>();
        let target: Amount = "1000".parse().unwrap();
        let mut map = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let before = map.clone();
        assert!(evaluate_path(
            &mut map,
            &[edges[0].clone(), edges[0].clone()],
            &target,
            Funds {
                sender: SENDER,
                recipient: RECIPIENT
            }
        )
        .is_err());
        assert_eq!(map, before);
        let id = pools[1].pool_id.clone();
        map.get_mut(&id).unwrap().v3.as_mut().unwrap().factory =
            "0x0000000000000000000000000000000000000001".into();
        let changed = map.clone();
        assert!(evaluate_path(
            &mut map,
            &edges,
            &target,
            Funds {
                sender: SENDER,
                recipient: RECIPIENT
            }
        )
        .is_err());
        assert_eq!(map, changed);
        for internal in [&pools[0].pool_id, &pools[0].v3.as_ref().unwrap().factory] {
            let mut m = before.clone();
            assert!(evaluate_path(
                &mut m,
                &edges,
                &target,
                Funds {
                    sender: internal,
                    recipient: RECIPIENT
                }
            )
            .is_err());
            assert_eq!(m, before);
        }
    }
}
