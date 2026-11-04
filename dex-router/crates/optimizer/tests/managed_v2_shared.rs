use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation, AllocatedRoute};
use xone_protocol_math::managed_v2_pool::quote_at;
use xone_routing::{evaluate_path, Graph, PathQuote};
fn fixture() -> &'static Value {
    static BANK: OnceLock<Value> = OnceLock::new();
    BANK.get_or_init(|| {
        serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/managed-v2-aum-pools.json"
            ))
            .unwrap(),
        )
        .unwrap()
    })
}
#[test]
fn shared_managed_v2_allocations_carry_full_cash_managed_and_do_not_collect_bpt() {
    let mut count = 0;
    for c in fixture()["cases"].as_array().unwrap() {
        if c["operation"] != "swap" {
            continue;
        }
        let original: Pool = serde_json::from_value(c["source_pool"].clone()).unwrap();
        let s = original.multi_asset.as_ref().unwrap();
        let m = s.managed_v2.as_ref().unwrap();
        let block = m.block_number;
        let time = m.timestamp;
        let token_in = s.assets[c["params"]["index_in"].as_u64().unwrap() as usize]
            .token
            .clone();
        let token_out = s.assets[c["params"]["index_out"].as_u64().unwrap() as usize]
            .token
            .clone();
        let input: Amount = c["params"]["amount"].as_str().unwrap().parse().unwrap();
        let graph = Graph::build(std::slice::from_ref(&original)).unwrap();
        let edge = graph.adjacency[&token_in]
            .iter()
            .find(|e| e.to == token_out)
            .unwrap()
            .clone();
        let mut state = BTreeMap::from([(original.pool_id.clone(), original.clone())]);
        let mut routes = Vec::new();
        let mut expected = original.clone();
        for step in 0..2 {
            let q = quote_at(&expected, &input, &token_in, &token_out, block, time).unwrap();
            let output = evaluate_path(&mut state, std::slice::from_ref(&edge), &input).unwrap();
            assert_eq!(output, q.amount_out);
            assert_eq!(state[&original.pool_id], q.next_pool);
            if step == 0 {
                assert_eq!(
                    output.to_string(),
                    c["expected_native"]["quantity"].as_str().unwrap()
                );
            }
            expected = q.next_pool;
            routes.push(AllocatedRoute {
                path: PathQuote {
                    edges: vec![edge.clone()],
                    tokens: vec![token_in.clone(), token_out.clone()],
                    amount_in: input.clone(),
                    amount_out: output.clone(),
                },
                amount_in: input.clone(),
                amount_out: output,
            });
        }
        let allocation =
            evaluate_allocation(std::slice::from_ref(&original), &routes, &Amount::zero()).unwrap();
        assert_eq!(allocation.sell_amount.0, &input.0 * 2u8);
        assert_eq!(
            allocation.gross_buy_amount.0,
            &routes[0].amount_out.0 + &routes[1].amount_out.0
        );
        assert_eq!(allocation.routes[1].amount_out, routes[1].amount_out);
        assert_eq!(allocation.refund, Amount::zero());
        let after = expected
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(after.total_supply, m.total_supply);
        assert_eq!(after.virtual_supply, m.virtual_supply);
        assert_eq!(after.last_collection_timestamp, m.last_collection_timestamp);
        assert_eq!(after.manager_bpt_balance, m.manager_bpt_balance);
        assert_eq!(
            after.protocol_collector_bpt_balance,
            m.protocol_collector_bpt_balance
        );
        assert_eq!(
            original,
            serde_json::from_value::<Pool>(c["source_pool"].clone()).unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 8);
}
#[test]
fn bpt_is_not_silently_added_to_underlying_only_candidate_graph() {
    let c = &fixture()["cases"][0];
    let p: Pool = serde_json::from_value(c["source_pool"].clone()).unwrap();
    let g = Graph::build(std::slice::from_ref(&p)).unwrap();
    assert!(!g.adjacency.contains_key(&p.pool_id));
    assert!(g.adjacency.values().flatten().all(|e| e.to != p.pool_id));
}
