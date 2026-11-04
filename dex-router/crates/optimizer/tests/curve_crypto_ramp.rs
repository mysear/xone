use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_protocol_math::multi_asset::quote_exact_in_tokens;
use xone_routing::{evaluate_path, Graph, PathQuote};

#[test]
fn active_ramp_shared_allocations_keep_complete_schedule_and_recomputed_state() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-ramp-service.json"
    ))
    .unwrap();
    for row in corpus["cases"].as_array().unwrap() {
        // All native source banks are exercised, including sentinel clearing.
        let pool: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
        let source = pool.clone();
        let input: Amount = row["input"].as_str().unwrap().parse().unwrap();
        let i = row["index_in"].as_u64().unwrap() as usize;
        let j = row["index_out"].as_u64().unwrap() as usize;
        let assets = &pool.multi_asset.as_ref().unwrap().assets;
        let (token_in, token_out) = (&assets[i].token, &assets[j].token);
        let graph = Graph::build(std::slice::from_ref(&pool)).unwrap();
        let edge = graph.adjacency[token_in]
            .iter()
            .find(|e| &e.to == token_out)
            .unwrap()
            .clone();
        let mut physical = BTreeMap::from([(pool.pool_id.clone(), pool.clone())]);
        let mut current = pool.clone();
        let mut expected = Vec::new();
        let mut routes = Vec::new();
        for _ in 0..3 {
            let native_kernel =
                quote_exact_in_tokens(&current, &input, token_in, token_out).unwrap();
            let routed = evaluate_path(&mut physical, std::slice::from_ref(&edge), &input).unwrap();
            assert_eq!(routed, native_kernel.amount_out);
            assert_eq!(physical[&pool.pool_id], native_kernel.next_pool);
            expected.push(routed);
            current = native_kernel.next_pool;
            routes.push(AllocatedRoute {
                path: PathQuote {
                    amount_in: Amount::zero(),
                    amount_out: Amount::zero(),
                    edges: vec![edge.clone()],
                    tokens: vec![token_in.clone(), token_out.clone()],
                },
                amount_in: input.clone(),
                amount_out: Amount::zero(),
            });
        }
        // Only the first trade has an independent original transaction at this
        // exact clock; subsequent swaps verify allocation's sequential reuse.
        assert_eq!(expected[0].to_string(), row["expected"]["amount_out"]);
        let allocation =
            evaluate_allocation(std::slice::from_ref(&pool), &routes, &Amount::zero()).unwrap();
        for (route, output) in allocation.routes.iter().zip(&expected) {
            assert_eq!(&route.amount_out, output);
        }
        assert_eq!(
            allocation.gross_buy_amount.0,
            expected.iter().map(|a| a.0.clone()).sum()
        );
        assert_eq!(allocation.refund, Amount::zero());
        let initial = pool
            .multi_asset
            .as_ref()
            .unwrap()
            .curve_crypto
            .as_ref()
            .unwrap();
        let final_state = current
            .multi_asset
            .as_ref()
            .unwrap()
            .curve_crypto
            .as_ref()
            .unwrap();
        assert_eq!(final_state.ramp, initial.ramp);
        assert_eq!(final_state.gamma, initial.gamma);
        assert_eq!(final_state.timestamp, initial.timestamp);
        assert_eq!(final_state.block_hash, initial.block_hash);
        assert_ne!(final_state.raw_balances, initial.raw_balances);
        assert_eq!(pool, source);
        current.validate().unwrap();
    }
}
