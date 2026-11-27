use std::collections::BTreeMap;
use xone_domain::{v2_swap, Pool};
use xone_routing::{exact_out, Budget, Funds, Graph, SearchExecution};

const SELL: &str = "0x0000000000000000000000000000000000000001";
const MID: &str = "0x0000000000000000000000000000000000000002";
const BUY: &str = "0x0000000000000000000000000000000000000003";
const FUNDS: Funds<'_> = Funds {
    sender: "0x000000000000000000000000000000000000dead",
    recipient: "0x000000000000000000000000000000000000beef",
};

fn pool(id: u8, token0: &str, token1: &str, reserve1: u64) -> Pool {
    serde_json::from_value(serde_json::json!({
        "chain_key": "evm:31337",
        "pool_id": format!("0x{id:040x}"),
        "protocol": v2_swap::PROFILE,
        "adapter_version": v2_swap::ADAPTER,
        "token0": token0,
        "token1": token1,
        "reserve0": "1000000",
        "reserve1": reserve1.to_string(),
        "fee_numerator": 3,
        "fee_denominator": 1000,
        "token_behavior": "standard",
        "v2": {
            "factory": "0x00000000000000000000000000000000000000fa",
            "code_sha256": v2_swap::PAIR_CODE_SHA256,
            "swap": {
                "raw_balance0": "1000000",
                "raw_balance1": reserve1.to_string(),
                "block_timestamp_last": 999,
                "price0_cumulative_last": "0",
                "price1_cumulative_last": "0",
                "unlocked": true,
                "factory_code_sha256": v2_swap::FACTORY_CODE_SHA256,
                "bank_block_number": 1,
                "bank_block_hash": format!("0x{:064x}", 1),
                "bank_unix_timestamp": 1000
            }
        }
    }))
    .unwrap()
}

#[test]
fn exact_out_beam_retains_parallel_suffixes_and_prioritizes_best_within_budget() {
    let mut pools = vec![pool(10, SELL, MID, 1_000_000)];
    for id in 11..=16 {
        pools.push(pool(id, MID, BUY, 1_000_000 + u64::from(id) * 50_000));
    }
    let graph = Graph::build(&pools).unwrap();
    let state: BTreeMap<_, _> = pools
        .iter()
        .map(|pool| (pool.pool_id.clone(), pool.clone()))
        .collect();
    let target = "1000".parse().unwrap();
    let prefix = &graph.adjacency[SELL][0];
    let mut oracle = graph.adjacency[MID]
        .iter()
        .filter(|edge| edge.to == BUY)
        .map(|suffix| {
            exact_out::evaluate_path(
                &mut state.clone(),
                &[prefix.clone(), suffix.clone()],
                &target,
                FUNDS,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    oracle.sort_by(|a, b| a.amount_in.cmp(&b.amount_in));
    let search = |max_expansions, max_candidates| {
        exact_out::search(
            &graph,
            &pools,
            SELL,
            BUY,
            &target,
            Budget {
                split_parts: 1,
                max_hops: 2,
                max_candidates,
                max_expansions,
            },
            SearchExecution {
                control: None,
                funds: Some(FUNDS),
            },
        )
        .unwrap()
    };
    let complete = search(100, 16);
    assert!(!complete.budget_exhausted);
    assert_eq!(complete.candidates.len(), oracle.len());
    assert_eq!(complete.evaluations, 2 * oracle.len());
    let truncated = search(14, 2);
    assert!(truncated.budget_exhausted);
    assert_eq!(truncated.max_labels_per_token, 4);
    assert_eq!(truncated.dropped_labels, 2);
    assert_eq!(truncated.expansions, 14);
    assert_eq!(truncated.candidates.len(), 2);
    for (path, expected) in truncated.candidates.iter().zip(oracle) {
        assert_eq!(path.amount_in, expected.amount_in);
        assert_eq!(path.amount_out, target);
        assert_eq!(path.edges, expected.edges);
    }
}
