use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_optimizer::exact_out::{evaluate_allocation, ExactOutProposal};
use xone_routing::{Edge, Funds};
#[test]
fn raydium_shared_pool_ordered_allocations_match_native_sequential_ledgers() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/raydium-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let funds = Funds {
        sender: "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso",
        recipient: "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE",
    };
    let mut cases = 0;
    for sequence in f["sequences"].as_array().unwrap() {
        let steps = sequence["steps"].as_array().unwrap();
        let zero = steps[0]["zero_for_one"].as_bool().unwrap();
        if !steps
            .iter()
            .all(|s| s["zero_for_one"] == zero && s["expected"]["reverted"] != true)
        {
            continue;
        }
        let pool: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        let proposals = steps
            .iter()
            .map(|s| ExactOutProposal {
                edges: vec![Edge {
                    from_token: None,
                    pool_id: pool.pool_id.clone(),
                    protocol: pool.protocol.clone(),
                    zero_for_one: zero,
                    to: if zero {
                        pool.token1.clone()
                    } else {
                        pool.token0.clone()
                    },
                }],
                amount_out: s["target"].as_str().unwrap().parse().unwrap(),
            })
            .collect::<Vec<_>>();
        let required = steps
            .iter()
            .map(|s| {
                s["expected"]["amount_in"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
            })
            .sum::<u64>();
        let target: Amount = "3100".parse().unwrap();
        let budget: Amount = required.to_string().parse().unwrap();
        let allocation = evaluate_allocation(
            std::slice::from_ref(&pool),
            &proposals,
            &target,
            &budget,
            funds,
        )
        .unwrap();
        assert_eq!(allocation.required_input, budget);
        assert_eq!(allocation.gross_output(), &target);
        assert!(evaluate_allocation(
            std::slice::from_ref(&pool),
            &proposals,
            &target,
            &(required - 1).to_string().parse().unwrap(),
            funds
        )
        .is_err());
        cases += 1;
    }
    assert!(cases >= 6, "{cases}");
}
