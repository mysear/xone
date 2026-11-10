//! Original breaker amounts through one physical-pool graph with explicit funds.
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, sync::OnceLock};
use xone_domain::{Amount, Pool};
use xone_routing::{
    evaluate_path_with_funds, search_bounded_with_funds, Budget, Edge, Funds, Graph,
    SearchExecution,
};
fn bank() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../test/contracts/managed-v2-breaker-pools.json"),
            )
            .unwrap(),
        )
        .unwrap()
    })
}
fn pools(c: &Value) -> Vec<Pool> {
    bank()["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == c["before"])
        .unwrap()["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| serde_json::from_value(p["pool"].clone()).unwrap())
        .collect()
}
#[test]
fn every_positive_original_breaker_swap_is_a_funded_logical_edge() {
    let mut count = 0;
    for c in bank()["cases"].as_array().unwrap() {
        if c["expected_native"]["status"] != 1 || c["expected_native"]["actual_amount_out"] == "0" {
            continue;
        }
        let pools = pools(c);
        let b = bank()["banks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == c["before"])
            .unwrap();
        let id = b["pools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == c["pool_name"])
            .unwrap()["pool"]["pool_id"]
            .as_str()
            .unwrap();
        let graph = Graph::build(&pools).unwrap();
        let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        let sell = c["token_in"].as_str().unwrap();
        let buy = c["token_out"].as_str().unwrap();
        let edge = graph
            .adjacency
            .get(sell)
            .and_then(|e| e.iter().find(|e| e.pool_id == id && e.to == buy))
            .unwrap_or_else(|| panic!("{} missing logical edge", c["id"]));
        let mut state = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let actor = c["actor"].as_str().unwrap();
        let actual = evaluate_path_with_funds(
            &mut state,
            std::slice::from_ref(edge),
            &input,
            Funds {
                sender: actor,
                recipient: actor,
            },
        )
        .unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        assert_eq!(
            actual,
            c["expected_native"]["actual_amount_out"]
                .as_str()
                .unwrap()
                .parse::<Amount>()
                .unwrap()
        );
        count += 1;
    }
    assert!(count > 200);
}
#[test]
fn candidate_search_keeps_currently_tripped_pool_for_recovering_amount() {
    let c = bank()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"] == "subunit-upper-large-join-recovers")
        .unwrap();
    let pools = pools(c);
    let graph = Graph::build(&pools).unwrap();
    let actor = c["actor"].as_str().unwrap();
    let found = search_bounded_with_funds(
        &graph,
        &pools,
        c["token_in"].as_str().unwrap(),
        c["token_out"].as_str().unwrap(),
        &c["amount_in"].as_str().unwrap().parse().unwrap(),
        Budget {
            split_parts: 10,
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 128,
        },
        SearchExecution {
            control: None,
            funds: Some(Funds {
                sender: actor,
                recipient: actor,
            }),
        },
    )
    .unwrap();
    assert!(!found.candidates.is_empty());
}
#[test]
fn breaker_rejection_rolls_back_complete_ordered_path() {
    let c = bank()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["label"] == "cached-large-exit")
        .unwrap();
    let pools = pools(c);
    let p = &pools[0];
    let actor = c["actor"].as_str().unwrap();
    let initial = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut state = initial.clone();
    let edge = Edge {
        from_token: Some(c["token_in"].as_str().unwrap().into()),
        protocol: p.protocol.clone(),
        pool_id: p.pool_id.clone(),
        zero_for_one: false,
        to: c["token_out"].as_str().unwrap().into(),
    };
    let e = evaluate_path_with_funds(
        &mut state,
        &[edge],
        &c["amount_in"].as_str().unwrap().parse().unwrap(),
        Funds {
            sender: actor,
            recipient: actor,
        },
    )
    .unwrap_err();
    assert_eq!(e.0, "MANAGED_V2_CIRCUIT_BREAKER_TRIPPED");
    assert_eq!(state, initial);
}

#[test]
fn original_shared_order_reuses_both_complete_physical_pools_and_wallets() {
    let raw: Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/contracts/managed-v2-breaker.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let o = &raw["orders"][0];
    let before = bank()["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == o["source_bank"])
        .unwrap();
    let source: Vec<Pool> = before["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| serde_json::from_value(p["pool"].clone()).unwrap())
        .collect();
    let mut state = source
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let cases = bank()["cases"].as_array().unwrap();
    for route in o["reference_routes"].as_array().unwrap() {
        for id in route["cases"].as_array().unwrap() {
            let c = cases.iter().find(|c| c["id"] == *id).unwrap();
            let expected = bank()["banks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["id"] == c["after"])
                .unwrap();
            let model: Vec<Pool> = expected["pools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| serde_json::from_value(p["pool"].clone()).unwrap())
                .collect();
            let context = model[0]
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap();
            let block = context.block_number;
            let time = context.timestamp;
            for p in state.values_mut() {
                *p = xone_protocol_math::managed_v2_pool::reclock(p, block, time).unwrap();
            }
            let own = expected["pools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == c["pool_name"])
                .unwrap()["pool"]["pool_id"]
                .as_str()
                .unwrap();
            let p = &state[own];
            let e = Edge {
                from_token: Some(c["token_in"].as_str().unwrap().into()),
                protocol: p.protocol.clone(),
                pool_id: p.pool_id.clone(),
                zero_for_one: false,
                to: c["token_out"].as_str().unwrap().into(),
            };
            let actor = o["caller"].as_str().unwrap();
            let out = evaluate_path_with_funds(
                &mut state,
                &[e],
                &c["amount_in"].as_str().unwrap().parse().unwrap(),
                Funds {
                    sender: actor,
                    recipient: actor,
                },
            )
            .unwrap();
            assert_eq!(
                out,
                c["expected_native"]["actual_amount_out"]
                    .as_str()
                    .unwrap()
                    .parse::<Amount>()
                    .unwrap()
            );
            for mut p in model {
                let source = source
                    .iter()
                    .find(|q| q.pool_id == p.pool_id)
                    .unwrap()
                    .multi_asset
                    .as_ref()
                    .unwrap()
                    .managed_v2
                    .as_ref()
                    .unwrap();
                let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
                m.execution_context =
                    Some(xone_domain::managed_v2_state::ManagedV2ExecutionContext {
                        block_number: m.block_number,
                        timestamp: m.timestamp,
                    });
                m.block_number = source.block_number;
                m.timestamp = source.timestamp;
                m.block_hash = source.block_hash.clone();
                assert_eq!(
                    state[&p.pool_id], p,
                    "{} full shared original poststate",
                    c["id"]
                );
            }
        }
    }
}

fn edge(p: &Pool, sell: &str, buy: &str) -> Edge {
    Edge {
        from_token: Some(sell.into()),
        protocol: p.protocol.clone(),
        pool_id: p.pool_id.clone(),
        zero_for_one: false,
        to: buy.into(),
    }
}
#[test]
#[ignore = "requires explicitly authenticated actual selected API VM vectors"]
fn actual_api_selected_breaker_native_full_poststate() {
    let path =
        std::env::var("XONE_BREAKER_SELECTED_VECTORS").expect("explicit authenticated vector file");
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        v["authority"],
        "authenticated-actual-selected-breaker-API-original-execution"
    );
    let mut count = 0;
    for execution in v["executions"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(execution["source_pools"].clone()).unwrap();
        let mut state = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let sender = execution["request"]["caller"].as_str().unwrap();
        let recipient = execution["request"]["recipient"].as_str().unwrap();
        assert_eq!(sender, recipient, "finite replay scope");
        let source = state.clone();
        for step in execution["steps"].as_array().unwrap() {
            let block = step["execution_block"].as_u64().unwrap();
            let timestamp = step["execution_timestamp"].as_u64().unwrap();
            for p in state.values_mut() {
                *p = xone_protocol_math::managed_v2_pool::reclock(p, block, timestamp).unwrap();
            }
            let p = state[step["pool_id"].as_str().unwrap()].clone();
            let e = edge(
                &p,
                step["token_in"].as_str().unwrap(),
                step["token_out"].as_str().unwrap(),
            );
            let input: Amount = step["amount_in"].as_str().unwrap().parse().unwrap();
            let amount =
                evaluate_path_with_funds(&mut state, &[e], &input, Funds { sender, recipient })
                    .unwrap();
            assert_eq!(
                amount,
                step["amount_out"]
                    .as_str()
                    .unwrap()
                    .parse::<Amount>()
                    .unwrap()
            );
            let expected: Vec<Pool> = serde_json::from_value(step["after_pools"].clone()).unwrap();
            assert_eq!(expected.len(), state.len());
            for mut expected in expected {
                let origin = source[&expected.pool_id]
                    .multi_asset
                    .as_ref()
                    .unwrap()
                    .managed_v2
                    .as_ref()
                    .unwrap();
                let m = expected
                    .multi_asset
                    .as_mut()
                    .unwrap()
                    .managed_v2
                    .as_mut()
                    .unwrap();
                m.execution_context =
                    Some(xone_domain::managed_v2_state::ManagedV2ExecutionContext {
                        block_number: m.block_number,
                        timestamp: m.timestamp,
                    });
                m.block_number = origin.block_number;
                m.timestamp = origin.timestamp;
                m.block_hash = origin.block_hash.clone();
                assert_eq!(
                    state[&expected.pool_id], expected,
                    "{} full selected poststate",
                    execution["id"]
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, v["selected_transactions"].as_u64().unwrap());
    assert!(count > 0);
}
