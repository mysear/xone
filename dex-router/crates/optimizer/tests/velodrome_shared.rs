use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{Edge, PathQuote};

#[test]
fn sequential_velodrome_allocations_match_original_post_state_in_both_directions() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/velodrome-v2-quotes.json"
    ))
    .unwrap();
    for case in data["sequences"].as_array().unwrap() {
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let zero = case["zero_for_one"].as_bool().unwrap();
        let (sell, buy) = if zero {
            (&pool.token0, &pool.token1)
        } else {
            (&pool.token1, &pool.token0)
        };
        let mut total = Amount::zero();
        let mut gross = Amount::zero();
        let mut routes = vec![];
        for step in case["steps"].as_array().unwrap() {
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let output: Amount = step["amount_out"].as_str().unwrap().parse().unwrap();
            total = Amount::checked(total.0 + &input.0).unwrap();
            gross = Amount::checked(gross.0 + &output.0).unwrap();
            let path = PathQuote {
                amount_in: input.clone(),
                amount_out: Amount::zero(),
                tokens: vec![sell.clone(), buy.clone()],
                edges: vec![Edge {
                    pool_id: pool.pool_id.clone(),
                    protocol: pool.protocol.clone(),
                    zero_for_one: zero,
                    from_token: None,
                    to: buy.clone(),
                }],
            };
            routes.push(AllocatedRoute {
                path,
                amount_in: input,
                amount_out: Amount::zero(),
            });
        }
        let result = evaluate_allocation(&[pool], &routes, &Amount::zero()).unwrap();
        assert_eq!(result.sell_amount, total);
        assert_eq!(result.gross_buy_amount, gross);
        assert_eq!(result.refund, Amount::zero());
        for (route, step) in result.routes.iter().zip(case["steps"].as_array().unwrap()) {
            assert_eq!(
                route.amount_out.to_string(),
                step["amount_out"].as_str().unwrap()
            );
        }
    }
}
