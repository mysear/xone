use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
#[test]
fn shared_rayclmm_bitmap_sequences_preserve_extension_and_full_native_post_pool() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-bitmap-quotes.json"
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
