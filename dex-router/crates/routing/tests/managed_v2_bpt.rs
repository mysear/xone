//! Original BPT edges share one physical Pool; funds permission is explicit.
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, sync::OnceLock};
use xone_domain::{Amount, Pool};
use xone_routing::{
    evaluate_path, evaluate_path_with_funds, search_bounded_with_funds, Budget, Edge, Funds, Graph,
    SearchExecution,
};
fn data() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../test/contracts/managed-v2-bpt-pools.json"),
            )
            .unwrap(),
        )
        .unwrap()
    })
}
fn bank(id: &Value) -> Vec<Pool> {
    data()["banks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == *id)
        .unwrap()["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| serde_json::from_value(e["pool"].clone()).unwrap())
        .collect()
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
fn original_bpt_logical_edges_and_funded_execution_match_every_supported_swap() {
    let mut count = 0;
    for c in data()["cases"].as_array().unwrap() {
        if c["operation"] == "token_swap"
            || c["expected_native"]["status"] != 1
            || c["expected_native"]["actual_amount_out"] == "0"
        {
            continue;
        }
        let before = bank(&c["before"]);
        let after = bank(&c["after"]);
        let Some(p) = before.iter().find(|p| {
            p.pool_id
                == if c["operation"] == "bpt_join" {
                    c["token_out"].as_str().unwrap()
                } else {
                    c["token_in"].as_str().unwrap()
                }
        }) else {
            continue;
        };
        if !after.iter().any(|q| q.pool_id == p.pool_id) {
            continue;
        }
        let sell = c["token_in"].as_str().unwrap();
        let buy = c["token_out"].as_str().unwrap();
        let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        let g = Graph::build(&before).unwrap();
        let edges = &g.adjacency[sell];
        assert!(edges.iter().any(|e| e.pool_id == p.pool_id && e.to == buy));
        let path = vec![edge(p, sell, buy)];
        let initial = before
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut state = initial.clone();
        assert!(
            evaluate_path(&mut state, &path, &input).is_err(),
            "BPT swaps require explicit funds context"
        );
        assert_eq!(state, initial);
        let funds = Funds {
            sender: c["actor"].as_str().unwrap(),
            recipient: c["actor"].as_str().unwrap(),
        };
        let out = evaluate_path_with_funds(&mut state, &path, &input, funds)
            .unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        assert_eq!(
            out,
            c["expected_native"]["actual_amount_out"]
                .as_str()
                .unwrap()
                .parse::<Amount>()
                .unwrap()
        );
        // Route quotes use source-block execution context; the original receipt
        // mines one later block at this same timestamp. Normalize only those
        // two touched registration block coordinates for this comparison.
        for actual in state.values() {
            actual.validate().unwrap();
            let mut expected = after
                .iter()
                .find(|q| q.pool_id == actual.pool_id)
                .unwrap()
                .clone();
            let source = before.iter().find(|q| q.pool_id == actual.pool_id).unwrap();
            let sm = source
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
            m.block_number = sm.block_number;
            m.timestamp = sm.timestamp;
            m.block_hash = sm.block_hash.clone();
            if actual.pool_id == p.pool_id {
                let block = sm.block_number & u64::from(u32::MAX);
                let underlying = if c["operation"] == "bpt_join" {
                    sell
                } else {
                    buy
                };
                let idx = source
                    .multi_asset
                    .as_ref()
                    .unwrap()
                    .assets
                    .iter()
                    .position(|a| a.token == underlying)
                    .unwrap();
                m.token_info[idx].last_change_block = block;
                m.bpt.as_mut().unwrap().bpt_last_change_block = block;
            }
            // Every affected sibling has a derived context too. An untouched
            // sibling remains canonical and keeps the same computation clock.
            m.execution_context = actual
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap()
                .execution_context
                .clone();
            assert_eq!(
                actual, &expected,
                "{} complete shared route poststate",
                c["id"]
            );
        }
        let result = search_bounded_with_funds(
            &g,
            &before,
            sell,
            buy,
            &input,
            Budget {
                split_parts: 4,
                max_hops: 2,
                max_candidates: 8,
                max_expansions: 128,
            },
            SearchExecution {
                control: None,
                funds: Some(funds),
            },
        )
        .unwrap();
        assert!(!result.candidates.is_empty(), "{}", c["id"]);
        count += 1;
    }
    assert_eq!(count, 96);
}
#[test]
fn repeated_physical_bpt_pool_recalculates_and_shared_vault_peers_keep_their_supply() {
    let c = &data()["cases"][0];
    let pools = bank(&c["before"]);
    let p = &pools[0];
    let input: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
    let u = &p.multi_asset.as_ref().unwrap().assets[0].token;
    let funds = Funds {
        sender: c["actor"].as_str().unwrap(),
        recipient: c["actor"].as_str().unwrap(),
    };
    let mut state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let original = state.clone();
    let out = evaluate_path_with_funds(
        &mut state,
        &[edge(p, u, &p.pool_id), edge(p, &p.pool_id, u)],
        &input,
        funds,
    )
    .unwrap();
    assert!(out.0 < input.0);
    let sibling = &pools[1];
    let old = sibling
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    let new = state[&sibling.pool_id]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert_eq!(new.total_supply, old.total_supply);
    assert_eq!(new.virtual_supply, old.virtual_supply);
    assert_eq!(new.manager_bpt_balance, old.manager_bpt_balance);
    assert_ne!(
        new.token_info[0].vault_actual_balance,
        old.token_info[0].vault_actual_balance
    );
    let mut bad = original.clone();
    let path = [
        edge(p, u, &p.pool_id),
        edge(p, &p.pool_id, "0x0000000000000000000000000000000000000001"),
    ];
    assert!(evaluate_path_with_funds(&mut bad, &path, &input, funds).is_err());
    assert_eq!(bad, original);
}

#[test]
fn regular_managed_legs_reject_unmodeled_vault_self_funding_aliases_atomically() {
    let c = &data()["cases"][0];
    let pools = bank(&c["before"]);
    let p = &pools[0];
    let multi = p.multi_asset.as_ref().unwrap();
    let path = [edge(p, &multi.assets[0].token, &multi.assets[1].token)];
    let input: Amount = "1000000000000000000".parse().unwrap();
    let original = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    for funds in [
        Funds {
            sender: c["actor"].as_str().unwrap(),
            recipient: &multi.vault,
        },
        Funds {
            sender: &multi.vault,
            recipient: c["actor"].as_str().unwrap(),
        },
    ] {
        let mut state = original.clone();
        let e = evaluate_path_with_funds(&mut state, &path, &input, funds).unwrap_err();
        assert_eq!(e.0, "MANAGED_V2_REGULAR_FUNDS_ALIAS_UNSUPPORTED");
        assert_eq!(state, original);
    }
}

#[test]
fn external_v2_bpt_credit_is_carried_into_manager_and_collector_exit_balances() {
    let c = &data()["cases"][0];
    let source = bank(&c["after"]);
    let managed = &source[0];
    let ms = managed.multi_asset.as_ref().unwrap();
    let m = ms.managed_v2.as_ref().unwrap();
    for actor in [&m.manager, &m.protocol_collector] {
        let template: Value =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        let mut external: Pool = serde_json::from_value(template["pools"][0].clone()).unwrap();
        external.pool_id = "0x00000000000000000000000000000000000000e1".into();
        external.token0 = "0x00000000000000000000000000000000000000e2".into();
        external.token1 = managed.pool_id.clone();
        external.reserve0 = "1000000000000000000000".parse().unwrap();
        external.reserve1 = "100000000000000000000".parse().unwrap();
        external.fee_numerator = 3;
        external.fee_denominator = 1000;
        external.validate().unwrap();
        let input: Amount = "1000000000000000000".parse().unwrap();
        let first = xone_protocol_math::quote_exact_in(&external, &input, true).unwrap();
        assert_eq!(first.amount_out.to_string(), "99600698103990321");
        let mut expected = managed.clone();
        let before = expected
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let block = before.block_number;
        let time = before.timestamp;
        let em = expected
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        if actor == &m.manager {
            em.manager_bpt_balance = Amount(&em.manager_bpt_balance.0 + &first.amount_out.0);
        } else {
            em.protocol_collector_bpt_balance =
                Amount(&em.protocol_collector_bpt_balance.0 + &first.amount_out.0);
        }
        let expected = xone_protocol_math::managed_v2_pool::quote_bpt_exact_in_at(
            &expected,
            &first.amount_out,
            &managed.pool_id,
            &ms.assets[0].token,
            xone_protocol_math::managed_v2_pool::BptExecution {
                sender: actor,
                recipient: actor,
                block_number: block,
                timestamp: time,
            },
        )
        .unwrap();
        let mut state = source
            .iter()
            .chain(std::iter::once(&external))
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect();
        let path = [
            Edge {
                from_token: None,
                protocol: external.protocol.clone(),
                pool_id: external.pool_id.clone(),
                zero_for_one: true,
                to: managed.pool_id.clone(),
            },
            edge(managed, &managed.pool_id, &ms.assets[0].token),
        ];
        let out = evaluate_path_with_funds(
            &mut state,
            &path,
            &input,
            Funds {
                sender: actor,
                recipient: actor,
            },
        )
        .unwrap();
        assert_eq!(out, expected.amount_out);
        assert_eq!(state[&managed.pool_id], expected.next_pool);
    }
}
#[test]
fn mixed_case_vault_alias_cannot_bypass_regular_funds_guard() {
    let c = &data()["cases"][0];
    let source = bank(&c["before"]);
    let p = &source[0];
    let s = p.multi_asset.as_ref().unwrap();
    let upper = format!("0x{}", s.vault[2..].to_ascii_uppercase());
    let original = source
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut state = original.clone();
    assert!(evaluate_path_with_funds(
        &mut state,
        &[edge(p, &s.assets[0].token, &s.assets[1].token)],
        &"1000000000000000000".parse().unwrap(),
        Funds {
            sender: c["actor"].as_str().unwrap(),
            recipient: &upper
        }
    )
    .is_err());
    assert_eq!(state, original);
}

#[test]
#[ignore = "requires explicitly authenticated actual selected API VM vectors"]
fn actual_api_selected_bpt_native_full_poststate() {
    let path =
        std::env::var("XONE_BPT_SELECTED_VECTORS").expect("explicit authenticated vector file");
    let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        v["authority"],
        "authenticated-actual-selected-BPT-API-original-execution"
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

#[test]
fn external_v2_self_funding_aliases_reject_without_installing_fake_reserves() {
    let c = &data()["cases"][0];
    let source = bank(&c["after"]);
    let issuer = &source[0];
    let fixture: Value =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let mut pair: Pool = serde_json::from_value(fixture["pools"][0].clone()).unwrap();
    pair.pool_id = "0x00000000000000000000000000000000000000e1".into();
    pair.token0 = "0x00000000000000000000000000000000000000e2".into();
    pair.token1 = issuer.pool_id.clone();
    pair.reserve0 = "1000000000000000000000".parse().unwrap();
    pair.reserve1 = "100000000000000000000".parse().unwrap();
    pair.fee_numerator = 3;
    pair.fee_denominator = 1000;
    pair.validate().unwrap();
    let initial = source
        .iter()
        .chain(std::iter::once(&pair))
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let e = Edge {
        from_token: None,
        protocol: pair.protocol.clone(),
        pool_id: pair.pool_id.clone(),
        zero_for_one: true,
        to: issuer.pool_id.clone(),
    };
    let actor = c["actor"].as_str().unwrap();
    for funds in [
        Funds {
            sender: actor,
            recipient: &pair.pool_id,
        },
        Funds {
            sender: &pair.pool_id,
            recipient: actor,
        },
    ] {
        let mut state = initial.clone();
        assert!(evaluate_path_with_funds(
            &mut state,
            std::slice::from_ref(&e),
            &"1000000000000000000".parse().unwrap(),
            funds
        )
        .is_err());
        assert_eq!(state, initial);
    }
    let mut state = initial.clone();
    let path = [
        e,
        edge(
            issuer,
            &issuer.pool_id,
            &issuer.multi_asset.as_ref().unwrap().assets[0].token,
        ),
    ];
    assert!(evaluate_path_with_funds(
        &mut state,
        &path,
        &"1000000000000000000".parse().unwrap(),
        Funds {
            sender: &pair.pool_id,
            recipient: actor
        }
    )
    .is_err());
    assert_eq!(state, initial);
}
