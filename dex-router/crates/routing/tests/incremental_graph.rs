use std::sync::Arc;
use xone_domain::{Pool, Snapshot};
use xone_routing::{search, Budget, Graph};

fn pool(id: &str, from: &str, to: &str) -> Pool {
    serde_json::from_value(serde_json::json!({
        "chain_key":"evm:31337", "pool_id":id, "protocol":"uniswap-v2",
        "token0":from, "token1":to, "reserve0":"1000000", "reserve1":"1000000",
        "fee_numerator":3, "fee_denominator":1000,
        "adapter_version":"uniswap-v2/1", "token_behavior":"standard"
    }))
    .unwrap()
}
fn matches_full(graph: &Graph, pools: &[Pool]) {
    let full = Graph::build(pools).unwrap();
    assert_eq!(graph.chain_key, full.chain_key);
    assert_eq!(graph.excluded_pools, full.excluded_pools);
    assert_eq!(graph.adjacency, full.adjacency);
}
#[test]
fn reserve_and_fee_state_reuse_the_complete_graph() {
    let mut pools = vec![pool("ab", "A", "B"), pool("cd", "C", "D")];
    pools[1].protocol = "fixture-cpmm".into();
    pools[1].adapter_version = "fixture-cpmm/1".into();
    let graph = Graph::build(&pools).unwrap();
    pools[0].reserve0 = "2000000".parse().unwrap();
    pools[1].fee_numerator = 17;
    assert!(graph.update(&pools).unwrap().is_none());
    matches_full(&graph, &pools);
}
#[test]
fn changed_pool_only_copies_its_previous_and_current_token_buckets() {
    let mut pools = vec![pool("ab", "A", "B"), pool("cd", "C", "D")];
    let graph = Graph::build(&pools).unwrap();
    pools[0].token1 = "E".into();
    let next = graph.update(&pools).unwrap().unwrap();
    matches_full(&next, &pools);
    for token in ["C", "D"] {
        assert!(Arc::ptr_eq(&graph.adjacency[token], &next.adjacency[token]));
    }
    assert!(!Arc::ptr_eq(&graph.adjacency["A"], &next.adjacency["A"]));
    assert!(graph.adjacency.contains_key("B"));
    assert!(!graph.adjacency.contains_key("E"));
    assert!(!next.adjacency.contains_key("B"));
    assert_eq!(next.adjacency["E"][0].to, "A");
}
#[test]
fn additions_deletions_and_empty_bank_keep_old_snapshots_immutable() {
    let mut pools = vec![pool("ab", "A", "B"), pool("cd", "C", "D")];
    let old = Graph::build(&pools).unwrap();
    pools.insert(0, pool("ae", "A", "E"));
    let added = old.update(&pools).unwrap().unwrap();
    matches_full(&added, &pools);
    assert_eq!(added.adjacency["A"][0].pool_id, "ae");
    assert!(Arc::ptr_eq(&old.adjacency["C"], &added.adjacency["C"]));
    pools.remove(1);
    let removed = added.update(&pools).unwrap().unwrap();
    matches_full(&removed, &pools);
    assert!(!removed.adjacency.contains_key("B"));
    assert!(added.adjacency.contains_key("B"));
    let empty = removed.update(&[]).unwrap().unwrap();
    matches_full(&empty, &[]);
    assert!(empty.adjacency.is_empty());
    assert!(empty.update(&[]).unwrap().is_none());
    matches_full(&empty.update(&pools).unwrap().unwrap(), &pools);
}
#[test]
fn admission_exclusion_and_restoration_update_all_pool_edges() {
    let mut pools = vec![pool("ab", "A", "B"), pool("cd", "C", "D")];
    let old = Graph::build(&pools).unwrap();
    pools[0].token_behavior = "simulation-only".into();
    let blocked = old.update(&pools).unwrap().unwrap();
    matches_full(&blocked, &pools);
    assert_eq!(blocked.excluded_pools["ab"], "TOKEN_BEHAVIOR_UNSUPPORTED");
    assert!(!blocked.adjacency.contains_key("A"));
    assert!(Arc::ptr_eq(&old.adjacency["C"], &blocked.adjacency["C"]));
    pools[0].token_behavior = "standard".into();
    let restored = blocked.update(&pools).unwrap().unwrap();
    matches_full(&restored, &pools);
    assert_eq!(restored.adjacency, old.adjacency);
}
#[test]
fn failed_updates_leave_indexes_and_old_edges_usable() {
    let pools = vec![pool("ab", "A", "B"), pool("cd", "C", "D")];
    let graph = Graph::build(&pools).unwrap();
    let mut invalid = pools.clone();
    invalid.push(pools[0].clone());
    assert_eq!(
        graph.update(&invalid).unwrap_err().0,
        "DUPLICATE_PHYSICAL_POOL"
    );
    invalid = pools.clone();
    invalid[1].chain_key = "evm:1".into();
    assert_eq!(graph.update(&invalid).unwrap_err().0, "CROSS_CHAIN_GRAPH");
    invalid = pools.clone();
    invalid[0].token1 = invalid[0].token0.clone();
    assert!(graph.update(&invalid).is_err());
    assert!(graph.update(&pools).unwrap().is_none());
    matches_full(&graph, &pools);
}
#[test]
fn reordered_pools_preserve_full_build_candidate_order_under_tight_budget() {
    let mut pools = vec![
        pool("ab-1", "A", "B"),
        pool("ab-2", "A", "B"),
        pool("cd", "C", "D"),
    ];
    let old = Graph::build(&pools).unwrap();
    pools.swap(0, 1);
    let next = old.update(&pools).unwrap().unwrap();
    let full = Graph::build(&pools).unwrap();
    matches_full(&next, &pools);
    assert_eq!(next.adjacency["A"][0].pool_id, "ab-2");
    assert!(Arc::ptr_eq(&old.adjacency["C"], &next.adjacency["C"]));
    let budget = Budget {
        split_parts: 1,
        max_hops: 1,
        max_candidates: 1,
        max_expansions: 1,
    };
    let amount = "1000".parse().unwrap();
    let a = search(&next, &pools, "A", "B", &amount, budget).unwrap();
    let b = search(&full, &pools, "A", "B", &amount, budget).unwrap();
    assert_eq!(
        serde_json::to_value(&a.candidates).unwrap(),
        serde_json::to_value(&b.candidates).unwrap()
    );
    assert_eq!(a.expansions, b.expansions);
    assert_eq!(a.budget_exhausted, b.budget_exhausted);
}
#[test]
fn deterministic_edit_sequences_match_full_builds_and_share_untouched_edges() {
    let mut pools = vec![
        pool("ab", "A", "B"),
        pool("bc", "B", "C"),
        pool("xy", "X", "Y"),
    ];
    let mut graph = Graph::build(&pools).unwrap();
    for step in 0..128 {
        match step % 8 {
            0 => pools[0].token1 = "D".into(),
            1 => pools[1].token_behavior = "simulation-only".into(),
            2 => pools.push(pool("extra", "A", "C")),
            3 => pools.swap(0, 1),
            4 => pools.iter_mut().find(|p| p.pool_id == "ab").unwrap().token1 = "B".into(),
            5 => {
                pools
                    .iter_mut()
                    .find(|p| p.pool_id == "bc")
                    .unwrap()
                    .token_behavior = "standard".into()
            }
            6 => pools.retain(|p| p.pool_id != "extra"),
            _ => pools[0].reserve0 = "3000000".parse().unwrap(),
        }
        let next = graph
            .update(&pools)
            .unwrap()
            .unwrap_or_else(|| graph.clone());
        matches_full(&next, &pools);
        assert!(Arc::ptr_eq(&graph.adjacency["X"], &next.adjacency["X"]));
        graph = next;
    }
}
#[test]
fn changing_third_asset_updates_every_direction_of_a_multi_asset_pool() {
    let doc: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/five-family-evm-wire.json"
    ))
    .unwrap();
    let mut snapshot: Snapshot = serde_json::from_value(doc["snapshot"].clone()).unwrap();
    let graph = Graph::build(&snapshot.pools).unwrap();
    let previous = snapshot.pools[0].multi_asset.as_ref().unwrap().assets[2]
        .token
        .clone();
    snapshot.pools[0].multi_asset.as_mut().unwrap().assets[2].token =
        "0x000000000000000000000000000000000000ffff".into();
    let next = graph.update(&snapshot.pools).unwrap().unwrap();
    matches_full(&next, &snapshot.pools);
    assert!(!next.adjacency.contains_key(&previous));
    assert!(graph.adjacency.contains_key(&previous));
}

#[test]
fn local_edit_in_256_pool_bank_shares_510_untouched_buckets() {
    let mut pools: Vec<_> = (0..256)
        .map(|i| {
            pool(
                &format!("pool-{i}"),
                &format!("token-{i}-a"),
                &format!("token-{i}-b"),
            )
        })
        .collect();
    let old = Graph::build(&pools).unwrap();
    pools[0].token1 = "new-token".into();
    let next = old.update(&pools).unwrap().unwrap();
    matches_full(&next, &pools);
    assert_eq!(next.adjacency.len(), 512);
    let shared = next
        .adjacency
        .iter()
        .filter(|(token, edges)| {
            old.adjacency
                .get(*token)
                .is_some_and(|previous| Arc::ptr_eq(previous, edges))
        })
        .count();
    assert_eq!(shared, 510);
    assert!(old.adjacency.contains_key("token-0-b"));
    assert!(!old.adjacency.contains_key("new-token"));
}
