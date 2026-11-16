use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/stable-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn edge(p: &Pool, i: usize, j: usize) -> Edge {
    let tokens = [&p.token0, &p.token1];
    Edge {
        pool_id: p.pool_id.clone(),
        protocol: p.protocol.clone(),
        zero_for_one: i == 0 && j == 1,
        from_token: None,
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
fn stable_directions_replay_original_inputs_and_fail_atomically() {
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
        let mut expected = p.clone();
        expected.reserve0 = balances[0].as_str().unwrap().parse().unwrap();
        expected.reserve1 = balances[1].as_str().unwrap().parse().unwrap();
        assert_eq!(next, &expected);
    }
}

#[test]
fn stable_output_rejects_internal_vault_and_multihop_without_mutation() {
    let f = corpus();
    let c = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["expected"]["reverted"] != true)
        .unwrap();
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let e = edge(&p, 0, 1);
    let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
    let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let before = state.clone();
    let vault = &p.stable.as_ref().unwrap().vault;
    assert!(evaluate_path(
        &mut state,
        std::slice::from_ref(&e),
        &target,
        Funds {
            sender: vault,
            recipient: funds().recipient
        }
    )
    .is_err());
    assert_eq!(state, before);
    let mut second = p.clone();
    second.pool_id = "0x0000000000000000000000000000000000000065".into();
    second.token0 = p.token1.clone();
    second.token1 = "0x0000000000000000000000000000000000000003".into();
    state.insert(second.pool_id.clone(), second.clone());
    let before = state.clone();
    assert!(evaluate_path(&mut state, &[e, edge(&second, 0, 1)], &target, funds()).is_err());
    assert_eq!(state, before);
}
