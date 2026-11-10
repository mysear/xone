use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, optimize, AllocatedRoute};
use xone_routing::{
    evaluate_path, evaluate_path_with_funds, search, Budget, Edge, Funds, Graph, PathQuote,
};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../test/contracts/lfj-v21-state.json")).unwrap()
}
fn path(pool: &Pool, zero: bool, input: Amount) -> PathQuote {
    PathQuote {
        amount_in: input,
        amount_out: Amount::zero(),
        tokens: if zero {
            vec![pool.token0.clone(), pool.token1.clone()]
        } else {
            vec![pool.token1.clone(), pool.token0.clone()]
        },
        edges: vec![Edge {
            pool_id: pool.pool_id.clone(),
            protocol: pool.protocol.clone(),
            zero_for_one: zero,
            from_token: None,
            to: if zero {
                pool.token1.clone()
            } else {
                pool.token0.clone()
            },
        }],
    }
}
#[test]
fn lfj_shared_routes_keep_original_oracle_fees_and_all_bin_transitions() {
    let file = data();
    assert_eq!(file["sequences"].as_array().unwrap().len(), 2);
    for sequence in file["sequences"].as_array().unwrap() {
        let index = sequence["initial_case_index"].as_u64().unwrap() as usize;
        let pool: Pool = serde_json::from_value(file["cases"][index]["pool"].clone()).unwrap();
        let zero = sequence["zero_for_one"].as_bool().unwrap();
        let mut state = BTreeMap::from([(pool.pool_id.clone(), pool.clone())]);
        let mut routes = vec![];
        let mut total = Amount::zero();
        for step in sequence["steps"].as_array().unwrap() {
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let quote = path(&pool, zero, input.clone());
            let out = evaluate_path(&mut state, &quote.edges, &input).unwrap();
            assert_eq!(out.to_string(), step["amount_out"].as_str().unwrap());
            let actual = serde_json::to_value(state[&pool.pool_id].lfj.as_ref().unwrap()).unwrap();
            for key in [
                "parameters",
                "raw_reserve0",
                "raw_reserve1",
                "protocol_fees0",
                "protocol_fees1",
                "bins",
                "oracle_samples",
            ] {
                assert_eq!(actual[key], step["after"][key], "step state {key}");
            }
            total = Amount::checked(total.0 + &out.0).unwrap();
            routes.push(AllocatedRoute {
                path: quote,
                amount_in: input,
                amount_out: Amount::zero(),
            });
        }
        let allocation = evaluate_allocation(&[pool], &routes, &Amount::zero()).unwrap();
        assert_eq!(allocation.gross_buy_amount, total);
        assert_eq!(allocation.refund, Amount::zero());
    }
}
#[test]
fn lfj_split_capacity_and_best_grid_match_independent_original_getter_curve() {
    let file = data();
    for case in file["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !c["allocation_points"].as_array().unwrap().is_empty())
    {
        let first: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let mut second = first.clone();
        second.pool_id = "0x000000000000000000000000000000000000aacc".into();
        let pools = vec![first.clone(), second];
        let zero = case["zero_for_one"].as_bool().unwrap();
        let (sell, buy) = if zero {
            (&first.token0, &first.token1)
        } else {
            (&first.token1, &first.token0)
        };
        let points = case["allocation_points"].as_array().unwrap();
        for input in [2500000u64, 5000000] {
            let amount: Amount = input.to_string().parse().unwrap();
            let candidates = search(
                &Graph::build(&pools).unwrap(),
                &pools,
                sell,
                buy,
                &amount,
                Budget {
                    split_parts: 20,
                    max_hops: 1,
                    max_candidates: 8,
                    max_expansions: 100,
                },
            )
            .unwrap();
            let result = optimize(
                &pools,
                &candidates.candidates,
                &amount,
                20,
                2,
                &Amount::zero(),
            )
            .unwrap();
            let native = |n: u64| -> Option<u64> {
                let p = points
                    .iter()
                    .find(|p| p["input"].as_str().unwrap() == n.to_string())
                    .unwrap();
                if p["amount_in_left"] != "0" {
                    None
                } else {
                    Some(p["amount_out"].as_str().unwrap().parse().unwrap())
                }
            };
            let mut best = native(input).unwrap_or(0);
            for part in 1..20 {
                if let (Some(a), Some(b)) =
                    (native(input * part / 20), native(input - input * part / 20))
                {
                    best = best.max(a + b);
                }
            }
            assert_eq!(result.gross_buy_amount.to_string(), best.to_string());
            assert_eq!(result.sell_amount, amount);
            for route in &result.routes {
                assert_eq!(
                    route.amount_out.to_string(),
                    native(route.amount_in.to_string().parse().unwrap())
                        .unwrap()
                        .to_string()
                );
            }
        }
    }
}
#[test]
fn lfj_pool_funding_aliases_are_rejected_without_mutating_state() {
    let file = data();
    let pool: Pool = serde_json::from_value(file["cases"][0]["pool"].clone()).unwrap();
    let input: Amount = "10000".parse().unwrap();
    let route = path(&pool, true, input.clone());
    for address in [
        pool.pool_id.clone(),
        pool.pool_id.to_ascii_uppercase().replacen("0X", "0x", 1),
    ] {
        let mut state = BTreeMap::from([(pool.pool_id.clone(), pool.clone())]);
        let before = state.clone();
        let result = evaluate_path_with_funds(
            &mut state,
            &route.edges,
            &input,
            Funds {
                sender: "0x0000000000000000000000000000000000000001",
                recipient: &address,
            },
        );
        assert_eq!(result.err().unwrap().0, "LFJ_UNMODELED_SELF_FUNDING");
        assert_eq!(state, before);
    }
}

#[test]
fn external_funds_scope_also_rejects_an_unmodeled_other_protocol_pool_wallet() {
    let file = data();
    let pool: Pool = serde_json::from_value(file["cases"][0]["pool"].clone()).unwrap();
    let mut peer: Pool = serde_json::from_str::<xone_domain::Snapshot>(include_str!(
        "../../../test/fixtures/evm/baseline.json"
    ))
    .unwrap()
    .pools[0]
        .clone();
    peer.pool_id = "0x000000000000000000000000000000000000ccdd".into();
    peer.token0 = pool.token0.clone();
    peer.token1 = pool.token1.clone();
    let input: Amount = "10000".parse().unwrap();
    let route = path(&pool, true, input.clone());
    let recipient = peer.pool_id.clone();
    let mut state = BTreeMap::from([(pool.pool_id.clone(), pool), (peer.pool_id.clone(), peer)]);
    let before = state.clone();
    assert_eq!(
        evaluate_path_with_funds(
            &mut state,
            &route.edges,
            &input,
            Funds {
                sender: "0x0000000000000000000000000000000000000001",
                recipient: &recipient
            }
        )
        .err()
        .unwrap()
        .0,
        "LFJ_UNMODELED_SELF_FUNDING"
    );
    assert_eq!(state, before);
}
