use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{exact_out::evaluate_path, Edge, Funds};
fn corpus() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/infinity-bin-exact-out.json"
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
fn bin_full_targets_replay_native_projection_and_fail_atomically() {
    for c in corpus()["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let i = usize::from(!c["zeroForOne"].as_bool().unwrap());
        let e = edge(&p, i, 1 - i);
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let before = state.clone();
        let q = evaluate_path(
            &mut state,
            &[e],
            &c["target"].as_str().unwrap().parse().unwrap(),
            funds(),
        );
        if c["passed"] == false {
            assert!(q.is_err());
            assert_eq!(state, before);
            continue;
        }
        let q = q.unwrap();
        assert_eq!(q.amount_in.to_string(), c["requiredInput"]);
        assert_eq!(q.amount_out.to_string(), c["target"]);
        let expected: Pool = serde_json::from_value(c["after"].clone()).unwrap();
        assert_eq!(state[&p.pool_id], expected);
    }
}

#[test]
fn bin_output_rejects_internal_manager_and_multihop_without_mutation() {
    let f = corpus();
    let c = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["passed"] == true)
        .unwrap();
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let e = edge(&p, 0, 1);
    let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
    let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let before = state.clone();
    let vault = &p.infinity_bin.as_ref().unwrap().manager;
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
    let second: Pool = serde_json::from_value(f["grids"][0]["pools"][1].clone()).unwrap();
    state.insert(second.pool_id.clone(), second.clone());
    let before = state.clone();
    assert!(evaluate_path(&mut state, &[e, edge(&second, 0, 1)], &target, funds()).is_err());
    assert_eq!(state, before);
}

#[test]
fn output_updates_shared_currency_ledgers_without_mutating_sibling_bins() {
    let f = corpus();
    for grid in f["grids"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(grid["pools"].clone()).unwrap();
        let mut state = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let i = usize::from(!grid["zeroForOne"].as_bool().unwrap());
        let target: Amount = "1000".parse().unwrap();
        let sibling = pools[1].infinity_bin.as_ref().unwrap();
        let bins = sibling.bins.clone();
        let active = sibling.active_id;
        evaluate_path(&mut state, &[edge(&pools[0], i, 1 - i)], &target, funds()).unwrap();
        xone_domain::validate_pool_dependencies(state.values()).unwrap();
        let first = state[&pools[0].pool_id].infinity_bin.as_ref().unwrap();
        let second = state[&pools[1].pool_id].infinity_bin.as_ref().unwrap();
        assert_eq!(second.app_reserve0, first.app_reserve0);
        assert_eq!(second.app_reserve1, first.app_reserve1);
        assert_eq!(second.protocol_fees0, first.protocol_fees0);
        assert_eq!(second.protocol_fees1, first.protocol_fees1);
        assert_eq!(second.bins, bins);
        assert_eq!(second.active_id, active);
        evaluate_path(&mut state, &[edge(&pools[1], i, 1 - i)], &target, funds()).unwrap();
        xone_domain::validate_pool_dependencies(state.values()).unwrap();
    }
}
