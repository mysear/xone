use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/managed-snapshot-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn edge(p: &Pool, i: usize, j: usize) -> Edge {
    let tokens = p
        .multi_asset
        .as_ref()
        .map(|s| s.assets.iter().map(|a| a.token.clone()).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![p.token0.clone(), p.token1.clone()]);
    Edge {
        pool_id: p.pool_id.clone(),
        protocol: p.protocol.clone(),
        zero_for_one: i == 0 && j == 1,
        from_token: p.multi_asset.as_ref().map(|_| tokens[i].clone()),
        to: tokens[j].clone(),
    }
}
fn funds() -> Funds<'static> {
    Funds {
        sender: "0x000000000000000000000000000000000000dead",
        recipient: "0x000000000000000000000000000000000000beef",
    }
}
#[test]
fn all_managed_snapshot_token_pairs_replay_and_failures_are_atomic() {
    for c in corpus()["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let e = edge(
            &p,
            c["i"].as_u64().unwrap() as usize,
            c["j"].as_u64().unwrap() as usize,
        );
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let before = state.clone();
        let q = evaluate_path(
            &mut state,
            &[e],
            &c["target"].as_str().unwrap().parse().unwrap(),
            funds(),
        );
        if c["expected"]["reverted"] == true {
            assert!(q.is_err());
            assert_eq!(state, before);
            continue;
        }
        let q = q.unwrap();
        assert_eq!(q.amount_in.to_string(), c["expected"]["amount_in"]);
        assert_eq!(q.amount_out.to_string(), c["target"]);
        let balances = c["expected"]["next_balances"].as_array().unwrap();
        let next = &state[&p.pool_id];
        if let Some(s) = &next.multi_asset {
            for (a, b) in s.assets.iter().zip(balances) {
                assert_eq!(a.balance.to_string(), b.as_str().unwrap());
            }
        }
    }
}
#[test]
fn ambiguous_multi_edges_internal_funds_and_multihop_reject_without_mutation() {
    let f = corpus();
    let c = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["pool"]["protocol"] == "balancer-v2-managed-snapshot"
                && c["i"] == 1
                && c["j"] == 2
                && c["expected"]["reverted"] != true
        })
        .unwrap();
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let e = edge(&p, 1, 2);
    let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
    let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let before = state.clone();
    for broken in [
        {
            let mut b = e.clone();
            b.from_token = None;
            b
        },
        {
            let mut b = e.clone();
            b.zero_for_one = true;
            b
        },
        {
            let mut b = e.clone();
            b.from_token = Some(e.to.clone());
            b
        },
    ] {
        assert!(evaluate_path(&mut state, &[broken], &target, funds()).is_err());
        assert_eq!(state, before);
    }
    assert!(evaluate_path(&mut state, &[e.clone(), e.clone()], &target, funds()).is_err());
    assert_eq!(state, before);
    let vault = &p.multi_asset.as_ref().unwrap().vault;
    assert!(evaluate_path(
        &mut state,
        &[e],
        &target,
        Funds {
            sender: vault,
            recipient: funds().recipient
        }
    )
    .is_err());
    assert_eq!(state, before);
}
