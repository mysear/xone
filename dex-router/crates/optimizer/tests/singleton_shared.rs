use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-quotes.json"
    ))
    .unwrap()
}
#[test]
fn mixed_direction_shared_singleton_continuation_matches_original_pool_swap() {
    let file = fixture();
    let cases = file["cases"].as_array().unwrap();
    for name in ["uniswap-v4", "pancake-infinity-cl"] {
        for first_zero in [true, false] {
            let seq: Vec<_> = cases
                .iter()
                .filter(|c| {
                    c["name"]
                        .as_str()
                        .unwrap()
                        .starts_with(&format!("{name}/sequential/{first_zero}/"))
                })
                .collect();
            assert_eq!(seq.len(), 3);
            let p: Pool = serde_json::from_value(seq[0]["pool"].clone()).unwrap();
            let graph = Graph::build(std::slice::from_ref(&p)).unwrap();
            let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
            for case in seq {
                let zero = case["zero_for_one"].as_bool().unwrap();
                let sell = if zero { &p.token0 } else { &p.token1 };
                let edge = graph.adjacency[sell][0].clone();
                let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
                let expected: Amount = case["expected"]["amount_out"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert_eq!(
                    evaluate_path(&mut state, &[edge], &input).unwrap(),
                    expected
                );
                let c = &state[&p.pool_id].singleton_cl.as_ref().unwrap().core;
                assert_eq!(
                    c.sqrt_price_x96.to_string(),
                    case["expected"]["sqrt_price_x96"].as_str().unwrap()
                );
                assert_eq!(
                    i64::from(c.tick),
                    case["expected"]["tick"].as_i64().unwrap()
                );
            }
        }
    }
}
#[test]
fn shared_singleton_allocations_recompute_stale_candidate_amounts() {
    let file = fixture();
    let cases = file["cases"].as_array().unwrap();
    for name in ["uniswap-v4", "pancake-infinity-cl"] {
        for zero in [true, false] {
            let seq: Vec<_> = cases
                .iter()
                .filter(|c| {
                    c["name"]
                        .as_str()
                        .unwrap()
                        .starts_with(&format!("{name}/allocation/{zero}/"))
                })
                .collect();
            assert_eq!(seq.len(), 3);
            let p: Pool = serde_json::from_value(seq[0]["pool"].clone()).unwrap();
            let (sell, buy) = if zero {
                (p.token0.clone(), p.token1.clone())
            } else {
                (p.token1.clone(), p.token0.clone())
            };
            let g = Graph::build(std::slice::from_ref(&p)).unwrap();
            let edge = g.adjacency[&sell][0].clone();
            let mut routes = Vec::new();
            let mut expected = Amount::zero();
            let mut total = Amount::zero();
            for case in &seq {
                assert_eq!(case["zero_for_one"].as_bool().unwrap(), zero);
                let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
                let output: Amount = case["expected"]["amount_out"]
                    .as_str()
                    .unwrap()
                    .parse()
                    .unwrap();
                expected = Amount::checked(expected.0 + output.0).unwrap();
                total = Amount::checked(total.0 + &input.0).unwrap();
                routes.push(AllocatedRoute {
                    path: PathQuote {
                        amount_in: Amount::zero(),
                        edges: vec![edge.clone()],
                        tokens: vec![sell.clone(), buy.clone()],
                        amount_out: Amount::zero(),
                    },
                    amount_in: input,
                    amount_out: Amount::zero(),
                });
            }
            let q = evaluate_allocation(&[p], &routes, &Amount::zero()).unwrap();
            assert_eq!(q.sell_amount, total);
            assert_eq!(q.gross_buy_amount, expected);
            assert_eq!(q.refund, Amount::zero());
            for (actual, c) in q.routes.iter().zip(seq) {
                assert_eq!(
                    actual.amount_out.to_string(),
                    c["expected"]["amount_out"].as_str().unwrap()
                );
                assert_eq!(actual.path.amount_out, actual.amount_out);
                assert_eq!(actual.path.amount_in, actual.amount_in);
            }
        }
    }
}
#[test]
fn identical_v4_key_in_different_manager_is_a_distinct_physical_pool() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/singleton-state.json")).unwrap();
    let p: Pool = serde_json::from_value(f["pools"][0].clone()).unwrap();
    let mut other = p.clone();
    let s = other.singleton_cl.as_mut().unwrap();
    s.manager = format!("0x{}", "55".repeat(20));
    let key = xone_domain::singleton::pool_id(
        &other.protocol,
        &s.manager,
        &other.token0,
        &other.token1,
        s.lp_fee,
        s.core.tick_spacing,
        &s.parameters,
    )
    .unwrap();
    assert_eq!(key, s.pool_id);
    other.pool_id = format!("{}/{}", s.manager, key);
    let graph = Graph::build(&[p.clone(), other]).unwrap();
    assert_eq!(graph.adjacency[&p.token0].len(), 2);
}
