use std::{
    collections::BTreeMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use xone_domain::{Amount, Pool};
use xone_routing::{Budget, Edge, Funds, Graph, SearchControl, SearchExecution};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../test/contracts/v2-exact-out.json")).unwrap()
}
fn edges(pools: &[Pool], tokens: &[serde_json::Value]) -> Vec<Edge> {
    tokens
        .windows(2)
        .map(|p| {
            let from = p[0].as_str().unwrap();
            let to = p[1].as_str().unwrap();
            let pool = pools
                .iter()
                .find(|p| {
                    (p.token0 == from && p.token1 == to) || (p.token1 == from && p.token0 == to)
                })
                .unwrap();
            Edge {
                pool_id: pool.pool_id.clone(),
                protocol: pool.protocol.clone(),
                zero_for_one: pool.token0 == from,
                to: to.into(),
                from_token: None,
            }
        })
        .collect()
}
#[test]
fn reverse_targets_then_forward_state_replay_match_every_original_router_path() {
    let f = data();
    let sender = f["context"]["sender"].as_str().unwrap();
    let recipient = f["context"]["recipient"].as_str().unwrap();
    let funds = Funds { sender, recipient };
    let mut count = 0;
    for c in f["cases"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(c["pools"].clone()).unwrap();
        let path = edges(&pools, c["path"].as_array().unwrap());
        let input_state = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut state = input_state.clone();
        let target: Amount = c["amount_out"].as_str().unwrap().parse().unwrap();
        let q = xone_routing::exact_out::evaluate_path(&mut state, &path, &target, funds)
            .unwrap_or_else(|e| panic!("case {} {e}", c["id"]));
        assert_eq!(
            q.amount_in.to_string(),
            c["expected"]["required_input"].as_str().unwrap()
        );
        assert_eq!(q.amount_out, target);
        assert_eq!(
            serde_json::to_value(q.amounts).unwrap(),
            c["expected"]["amounts"]
        );
        let expected: Vec<Pool> = serde_json::from_value(c["expected_pools"].clone()).unwrap();
        assert_eq!(
            state,
            expected
                .into_iter()
                .map(|p| (p.pool_id.clone(), p))
                .collect(),
            "case {}",
            c["id"]
        );
        count += 1;
    }
    assert_eq!(count, 90);
}
#[test]
fn exact_out_paths_reject_token_mismatch_cycles_and_internal_wallets_atomically() {
    let f = data();
    let c = &f["cases"][0];
    let pools: Vec<Pool> = serde_json::from_value(c["pools"].clone()).unwrap();
    let path = edges(&pools, c["path"].as_array().unwrap());
    let base = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    let input: Amount = "10003".parse().unwrap();
    let funds = Funds {
        sender: f["context"]["sender"].as_str().unwrap(),
        recipient: f["context"]["recipient"].as_str().unwrap(),
    };
    for mode in 0..4 {
        let mut bad = path.clone();
        let mut context = funds;
        match mode {
            0 => {
                bad[0].to = if bad[0].zero_for_one {
                    pools[0].token0.clone()
                } else {
                    pools[0].token1.clone()
                }
            }
            1 => bad[0].from_token = Some(pools[0].token0.clone()),
            2 => bad.push(bad[0].clone()),
            _ => context.recipient = &pools[0].pool_id,
        }
        let mut state = base.clone();
        assert!(
            xone_routing::exact_out::evaluate_path(&mut state, &bad, &input, context).is_err(),
            "mode {mode}"
        );
        assert_eq!(state, base);
    }
}
#[test]
fn reverse_candidate_search_keeps_direct_and_shared_prefix_paths_and_full_fallback() {
    let f = data();
    let pools: Vec<Pool> = serde_json::from_value(f["cases"][0]["pools"].clone()).unwrap();
    let graph = Graph::build(&pools).unwrap();
    let sell = f["identities"]["token0"]["address"].as_str().unwrap();
    let buy = f["identities"]["token2"]["address"].as_str().unwrap();
    let target: Amount = "500000000000000000000000".parse().unwrap();
    let funds = Funds {
        sender: f["context"]["sender"].as_str().unwrap(),
        recipient: f["context"]["recipient"].as_str().unwrap(),
    };
    let result = xone_routing::exact_out::search(
        &graph,
        &pools,
        sell,
        buy,
        &target,
        Budget {
            split_parts: 8,
            max_hops: 3,
            max_candidates: 16,
            max_expansions: 1000,
        },
        SearchExecution {
            control: None,
            funds: Some(funds),
        },
    )
    .unwrap();
    assert!(!result.budget_exhausted);
    assert!(result.candidates.iter().any(|p| p.edges.len() == 1));
    assert!(result.candidates.iter().any(|p| p.edges.len() == 2));
    assert!(result.candidates.iter().any(|p| p.edges.len() == 3));
    assert!(result.candidates.len() >= 3);
    let limited = xone_routing::exact_out::search(
        &graph,
        &pools,
        sell,
        buy,
        &target,
        Budget {
            split_parts: 8,
            max_hops: 3,
            max_candidates: 1,
            max_expansions: 1000,
        },
        SearchExecution {
            control: None,
            funds: Some(funds),
        },
    )
    .unwrap();
    assert!(
        limited.budget_exhausted,
        "candidate truncation must be visible"
    );
    assert_eq!(limited.candidates.len(), 1);
    assert_eq!(limited.candidates[0].edges.len(), 1);
    assert_eq!(limited.candidates[0].amount_out, target);
    let mut state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    assert!(xone_routing::exact_out::evaluate_path(
        &mut state,
        &limited.candidates[0].edges,
        &target,
        funds
    )
    .is_ok());
    let cancel = AtomicBool::new(true);
    let error = xone_routing::exact_out::search(
        &graph,
        &pools,
        sell,
        buy,
        &target,
        Budget {
            split_parts: 8,
            max_hops: 3,
            max_candidates: 16,
            max_expansions: 1000,
        },
        SearchExecution {
            control: Some(SearchControl {
                cancel: &cancel,
                deadline: Instant::now() + Duration::from_secs(1),
            }),
            funds: Some(funds),
        },
    )
    .unwrap_err();
    assert_eq!(error.0, "BUDGET_EXHAUSTED");
}

#[test]
fn full_order_fallback_survives_an_infeasible_small_probe() {
    // A controlled state admitted by the complete Pair model. This does not
    // claim a public historical standard-Token deficit bank.
    let f = data();
    let base: Pool = serde_json::from_value(f["cases"][0]["pools"][0].clone()).unwrap();
    let funds = Funds {
        sender: f["context"]["sender"].as_str().unwrap(),
        recipient: f["context"]["recipient"].as_str().unwrap(),
    };
    for zero in [true, false] {
        let mut pool = base.clone();
        let (rin, rout) = ("997".parse().unwrap(), "8".parse().unwrap());
        (pool.reserve0, pool.reserve1) = if zero { (rin, rout) } else { (rout, rin) };
        let state = pool.v2.as_mut().unwrap().swap.as_mut().unwrap();
        (state.raw_balance0, state.raw_balance1) = if zero {
            ("996".parse().unwrap(), "8".parse().unwrap())
        } else {
            ("8".parse().unwrap(), "996".parse().unwrap())
        };
        pool.validate().unwrap();
        let target: Amount = "4".parse().unwrap();
        assert!(xone_protocol_math::quote_exact_out(&pool, &"1".parse().unwrap(), zero).is_err());
        let full = xone_protocol_math::quote_exact_out(&pool, &target, zero).unwrap();
        assert_eq!(full.amount_in.to_string(), "1001");
        let (sell, buy) = if zero {
            (&pool.token0, &pool.token1)
        } else {
            (&pool.token1, &pool.token0)
        };
        let result = xone_routing::exact_out::search(
            &Graph::build(std::slice::from_ref(&pool)).unwrap(),
            std::slice::from_ref(&pool),
            sell,
            buy,
            &target,
            Budget {
                split_parts: 4,
                max_hops: 1,
                max_candidates: 1,
                max_expansions: 16,
            },
            SearchExecution {
                control: None,
                funds: Some(funds),
            },
        )
        .unwrap();
        assert_eq!(
            result.candidates.len(),
            1,
            "a failed probe discarded a feasible whole order"
        );
        assert_eq!(result.candidates[0].amount_in, full.amount_in);
        assert_eq!(result.candidates[0].amount_out, target);
        let third = f["identities"]["token2"]["address"].as_str().unwrap();
        let mut prefix: Pool = f["cases"][0]["pools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| serde_json::from_value::<Pool>(p.clone()).unwrap())
            .find(|p| {
                (p.token0 == third && p.token1 == *sell) || (p.token1 == third && p.token0 == *sell)
            })
            .unwrap();
        prefix.reserve0 = "1000000".parse().unwrap();
        prefix.reserve1 = prefix.reserve0.clone();
        let before = prefix.v2.as_mut().unwrap().swap.as_mut().unwrap();
        before.raw_balance0 = prefix.reserve0.clone();
        before.raw_balance1 = prefix.reserve1.clone();
        let all = vec![pool.clone(), prefix];
        let graph = Graph::build(&all).unwrap();
        let token_path = serde_json::json!([third, sell, buy]);
        let path = edges(&all, token_path.as_array().unwrap());
        let mut state = all.iter().map(|p| (p.pool_id.clone(), p.clone())).collect();
        let expected =
            xone_routing::exact_out::evaluate_path(&mut state, &path, &target, funds).unwrap();
        let result = xone_routing::exact_out::search(
            &graph,
            &all,
            third,
            buy,
            &target,
            Budget {
                split_parts: 4,
                max_hops: 2,
                max_candidates: 1,
                max_expansions: 32,
            },
            SearchExecution {
                control: None,
                funds: Some(funds),
            },
        )
        .unwrap();
        assert_eq!(
            result.candidates.len(),
            1,
            "failed suffix probe hid a valid multi-hop order"
        );
        assert_eq!(result.candidates[0].amount_in, expected.amount_in);
        assert_eq!(result.candidates[0].amount_out, target);
        assert_eq!(result.candidates[0].edges.len(), 2);
    }
}
