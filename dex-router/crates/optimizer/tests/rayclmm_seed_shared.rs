use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
#[test]
fn shared_rayclmm_custom_seed_sequences_preserve_complete_native_post_pool() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-seed-quotes.json"
    ))
    .unwrap();
    let sequences = f["sequences"].as_array().unwrap();
    assert!(sequences.len() >= 8);
    for seq in sequences {
        let p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        let g = Graph::build(std::slice::from_ref(&p)).unwrap();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let steps = seq["steps"].as_array().unwrap();
        let mut routes = Vec::new();
        let same_direction = steps
            .iter()
            .all(|s| s["zero_for_one"] == steps[0]["zero_for_one"]);
        let mut total = Amount::zero();
        let mut expected = Amount::zero();
        for step in steps {
            let zero = step["zero_for_one"].as_bool().unwrap();
            let (sell, buy) = if zero {
                (&p.token0, &p.token1)
            } else {
                (&p.token1, &p.token0)
            };
            let edge = g.adjacency[sell][0].clone();
            assert_eq!(edge.protocol, p.protocol);
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let actual = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
            assert_eq!(
                actual.to_string(),
                step["expected"]["amount_out"].as_str().unwrap(),
                "{}",
                seq["name"]
            );
            assert_eq!(
                serde_json::to_value(&state[&p.pool_id].raydium_clmm).unwrap(),
                step["expected"]["next_pool"]["raydium_clmm"]
            );
            if same_direction {
                total = Amount::checked(total.0 + &input.0).unwrap();
                expected = Amount::checked(expected.0 + &actual.0).unwrap();
                routes.push(AllocatedRoute {
                    path: PathQuote {
                        amount_in: Amount::zero(),
                        edges: vec![edge],
                        tokens: vec![sell.clone(), buy.clone()],
                        amount_out: Amount::zero(),
                    },
                    amount_in: input,
                    amount_out: Amount::zero(),
                });
            }
        }
        let final_expected: Pool =
            serde_json::from_value(seq["expected_final_pool"].clone()).unwrap();
        assert_eq!(state[&p.pool_id], final_expected);
        if same_direction {
            let q = evaluate_allocation(&[p], &routes, &Amount::zero()).unwrap();
            assert_eq!(q.sell_amount, total);
            assert_eq!(q.gross_buy_amount, expected);
            assert_eq!(q.refund, Amount::zero());
            for r in q.routes {
                assert_eq!(r.path.amount_out, r.amount_out);
                assert_eq!(r.path.amount_in, r.amount_in);
            }
        }
    }
}

#[test]
fn distinct_nonzero_seeds_share_mints_but_never_share_physical_pool_state() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-seed-quotes.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    let select = |seed: u64| {
        cases
            .iter()
            .find(|c| {
                c["pool"]["raydium_clmm"]["seed_index"].as_u64() == Some(seed)
                    && c["input"] == "1000000"
                    && c["zero_for_one"] == true
                    && c["expected"]["amount_out"].is_string()
            })
            .expect("independentseedbasecase")
    };
    let refs = [select(1), select(256)];
    let pools: Vec<Pool> = refs
        .iter()
        .map(|c| serde_json::from_value(c["pool"].clone()).unwrap())
        .collect();
    assert_ne!(pools[0].pool_id, pools[1].pool_id);
    assert_eq!(pools[0].token0, pools[1].token0);
    assert_eq!(pools[0].token1, pools[1].token1);
    assert_eq!(
        pools[0].raydium_clmm.as_ref().unwrap().amm_config,
        pools[1].raydium_clmm.as_ref().unwrap().amm_config
    );
    let g = Graph::build(&pools).unwrap();
    assert_eq!(g.adjacency[&pools[0].token0].len(), 2);
    let mut state: BTreeMap<_, _> = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    let mut routes = Vec::new();
    let mut total = Amount::zero();
    let mut expected = Amount::zero();
    for (i, p) in pools.iter().enumerate() {
        let edge = g.adjacency[&p.token0]
            .iter()
            .find(|e| e.pool_id == p.pool_id)
            .unwrap()
            .clone();
        let input: Amount = "1000000".parse().unwrap();
        let output = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
        assert_eq!(
            output.to_string(),
            refs[i]["expected"]["amount_out"].as_str().unwrap()
        );
        let post: Pool = serde_json::from_value(refs[i]["expected"]["next_pool"].clone()).unwrap();
        assert_eq!(state[&p.pool_id], post);
        if i == 0 {
            assert_eq!(state[&pools[1].pool_id], pools[1]);
        }
        total = Amount::checked(total.0 + &input.0).unwrap();
        expected = Amount::checked(expected.0 + &output.0).unwrap();
        routes.push(AllocatedRoute {
            path: PathQuote {
                amount_in: Amount::zero(),
                amount_out: Amount::zero(),
                edges: vec![edge],
                tokens: vec![p.token0.clone(), p.token1.clone()],
            },
            amount_in: input,
            amount_out: Amount::zero(),
        });
    }
    let actual = evaluate_allocation(&pools, &routes, &Amount::zero()).unwrap();
    assert_eq!(actual.sell_amount, total);
    assert_eq!(actual.gross_buy_amount, expected);
    assert_eq!(actual.refund, Amount::zero());
}

#[test]
fn native_multi_seed_scenarios_preserve_untouched_pool_at_every_step() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-seed-quotes.json"
    ))
    .unwrap();
    let scenarios = f["independence_cases"].as_array().unwrap();
    assert_eq!(scenarios.len(), 3);
    let mut steps = 0;
    for scenario in scenarios {
        let pools: Vec<Pool> = serde_json::from_value(scenario["pools"].clone()).unwrap();
        let graph = Graph::build(&pools).unwrap();
        let mut state: BTreeMap<_, _> = pools
            .iter()
            .map(|p| (p.pool_id.clone(), p.clone()))
            .collect();
        assert_eq!(state.len(), 2);
        for step in scenario["steps"].as_array().unwrap() {
            let id = step["pool_id"].as_str().unwrap();
            let pool = state[id].clone();
            let before = state.clone();
            let left = step["zero_for_one"].as_bool().unwrap();
            let sell = if left { &pool.token0 } else { &pool.token1 };
            let edge = graph.adjacency[sell]
                .iter()
                .find(|e| e.pool_id == id)
                .unwrap()
                .clone();
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let output = evaluate_path(&mut state, &[edge], &input).unwrap();
            assert_eq!(
                output.to_string(),
                step["expected"]["amount_out"].as_str().unwrap()
            );
            let post: Pool = serde_json::from_value(step["expected"]["next_pool"].clone()).unwrap();
            assert_eq!(state[id], post);
            for (other, old) in &before {
                if other != id {
                    assert_eq!(&state[other], old, "unrelated seed pool changed")
                }
            }
            steps += 1;
        }
        let finals: Vec<Pool> =
            serde_json::from_value(scenario["expected_final_pools"].clone()).unwrap();
        for p in finals {
            assert_eq!(state[&p.pool_id], p);
        }
    }
    assert_eq!(steps, 12);
}
