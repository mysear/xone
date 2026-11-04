use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_routing::{evaluate_path, Graph, PathQuote};
fn native() -> &'static Value {
    static NATIVE: OnceLock<Value> = OnceLock::new();
    NATIVE.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/v2-complete.json"
            ))
            .unwrap(),
        )
        .unwrap()
    })
}
#[test]
fn original_same_order_chunks_reuse_complete_donation_state_in_shared_allocation() {
    let sequence = &native()["sequences"][0];
    let steps = sequence["steps"].as_array().unwrap();
    assert_eq!(sequence["name"], "donation-forward-reverse");
    for start in [0, 2] {
        let original: Pool = serde_json::from_value(steps[start]["pool"].clone()).unwrap();
        let sell = steps[start]["token_in"].as_str().unwrap();
        let buy = steps[start]["token_out"].as_str().unwrap();
        let graph = Graph::build(std::slice::from_ref(&original)).unwrap();
        let edge = graph.adjacency[sell]
            .iter()
            .find(|e| e.to == buy)
            .unwrap()
            .clone();
        let mut state = BTreeMap::from([(original.pool_id.clone(), original.clone())]);
        let mut routes = Vec::new();
        let mut total = Amount::zero();
        for step in &steps[start..start + 2] {
            assert_eq!(
                step["execution_timestamp"].as_u64().unwrap(),
                original
                    .v2
                    .as_ref()
                    .unwrap()
                    .swap
                    .as_ref()
                    .unwrap()
                    .bank_unix_timestamp
            );
            assert_eq!(
                serde_json::to_value(&state[&original.pool_id]).unwrap(),
                step["pool"]
            );
            let input: Amount = step["amount_in"].as_str().unwrap().parse().unwrap();
            let out = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
            assert_eq!(
                out.to_string(),
                step["expected"]["amount_out"].as_str().unwrap()
            );
            let expected: Pool =
                serde_json::from_value(step["expected"]["pool_after"].clone()).unwrap();
            assert_eq!(state[&original.pool_id], expected);
            total.0 += &input.0;
            routes.push(AllocatedRoute {
                path: PathQuote {
                    amount_in: Amount::zero(),
                    amount_out: Amount::zero(),
                    edges: vec![edge.clone()],
                    tokens: vec![sell.into(), buy.into()],
                },
                amount_in: input,
                amount_out: out,
            });
        }
        let allocation =
            evaluate_allocation(std::slice::from_ref(&original), &routes, &Amount::zero()).unwrap();
        assert_eq!(allocation.sell_amount, total);
        assert_eq!(allocation.refund, Amount::zero());
        assert_eq!(
            allocation.gross_buy_amount.0,
            &routes[0].amount_out.0 + &routes[1].amount_out.0
        );
        assert_eq!(allocation.routes[1].amount_out, routes[1].amount_out);
        assert_eq!(
            original,
            serde_json::from_value::<Pool>(steps[start]["pool"].clone()).unwrap()
        );
    }
}
#[test]
fn original_eight_fixed_clock_path_steps_preserve_all_pool_writes() {
    let mut count = 0;
    for sequence in native()["sequences"].as_array().unwrap().iter().take(3) {
        let original: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        let graph = Graph::build(std::slice::from_ref(&original)).unwrap();
        let mut state = BTreeMap::from([(original.pool_id.clone(), original.clone())]);
        for step in sequence["steps"].as_array().unwrap() {
            let sell = step["token_in"].as_str().unwrap();
            let buy = step["token_out"].as_str().unwrap();
            let edge = graph.adjacency[sell].iter().find(|e| e.to == buy).unwrap();
            let input: Amount = step["amount_in"].as_str().unwrap().parse().unwrap();
            let output = evaluate_path(&mut state, std::slice::from_ref(edge), &input).unwrap();
            assert_eq!(
                output.to_string(),
                step["expected"]["amount_out"].as_str().unwrap()
            );
            assert_eq!(
                state[&original.pool_id],
                serde_json::from_value::<Pool>(step["expected"]["pool_after"].clone()).unwrap()
            );
            count += 1;
        }
    }
    assert_eq!(count, 8);
}
