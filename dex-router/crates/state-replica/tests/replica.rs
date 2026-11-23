use xone_domain::*;
use xone_state_replica::*;
fn snapshot() -> Snapshot {
    serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap()
}
#[test]
fn immutable_handles_reorg_and_scope_barrier() {
    let mut s = snapshot();
    let replica = Replica::new(vec![(
        "evm:31337".to_owned(),
        "chain-parser-go".to_owned(),
        1,
    )]);
    replica.install_baseline(s.clone()).unwrap();
    let old = replica.view("evm:31337").unwrap();
    s.reference.block.number_or_slot += 1;
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = "next".into();
    s.reference.journal_seq += 1;
    s.pools[0].reserve0 = "1100000".parse().unwrap();
    let r = Record {
        reference: s.reference.clone(),
        kind: "BLOCK_COMMIT".into(),
        normalized_facts: None,
        record_id: "next".into(),
        payload_hash: hash(&s.pools).unwrap(),
        pools: s.pools.clone(),
    };
    replica.apply(r.clone()).unwrap();
    replica.apply(r).unwrap();
    assert_eq!(old.pools[0].reserve0.to_string(), "1000000");
    assert!(old.is_valid());
    s.reference.canonical_epoch += 1;
    s.reference.online_generation += 1;
    s.reference.journal_seq += 1;
    replica.install_baseline(s).unwrap();
    assert!(!old.is_valid());
}
#[test]
fn wrong_owner_missing_seq_and_history_rejected() {
    let s = snapshot();
    let replica = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    let mut bad = s.clone();
    bad.reference.owner_service = "chain-parser-rust".into();
    assert!(replica.install_baseline(bad).is_err());
    replica.install_baseline(s.clone()).unwrap();
    let r = Record {
        reference: s.reference,
        kind: "HISTORY_BLOCK".into(),
        normalized_facts: None,
        record_id: "history".into(),
        payload_hash: hash(&s.pools).unwrap(),
        pools: s.pools,
    };
    assert!(replica.apply(r).is_err());
}
#[test]
fn parser_scope_without_supported_math_does_not_make_quote_ready() {
    let mut s = snapshot();
    s.pools
        .iter_mut()
        .for_each(|p| p.protocol = "unknown-v4-hook".into());
    let replica = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    assert_eq!(
        replica.install_baseline(s).err().unwrap().0,
        "NO_QUOTE_CAPABLE_SCOPE"
    );
    assert!(replica.view("evm:31337").is_err());
}
#[test]
fn trusted_checkpoint_rebuilds_same_generation_without_replaying_history() {
    let s = snapshot();
    let r = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    r.install_baseline(s.clone()).unwrap();
    let old = r.view("evm:31337").unwrap();
    r.suspend("evm:31337");
    r.restore_owner_checkpoint(s.clone()).unwrap();
    assert!(!old.is_valid());
    assert!(r.view("evm:31337").unwrap().is_valid());
    let mut corrupted = s;
    corrupted.pools[0].reserve0 = "999".parse().unwrap();
    assert!(r.restore_owner_checkpoint(corrupted).is_err());
}

fn record(s: &Snapshot, kind: &str) -> Record {
    Record {
        reference: s.reference.clone(),
        kind: kind.into(),
        normalized_facts: None,
        record_id: String::new(),
        payload_hash: hash(&s.pools).unwrap(),
        pools: s.pools.clone(),
    }
}
#[test]
fn reorg_barrier_revokes_all_handles_and_requires_exact_commit() {
    let mut s = snapshot();
    let r = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    r.install_baseline(s.clone()).unwrap();
    let old = r.view("evm:31337").unwrap();
    s.reference.canonical_epoch += 1;
    s.reference.journal_seq += 1;
    let begin = record(&s, "REORG_BEGIN");
    r.apply(begin.clone()).unwrap();
    assert_eq!(
        r.last_reference("evm:31337").unwrap(),
        Some(s.reference.clone())
    );
    r.apply(begin).unwrap();
    assert!(!old.is_valid());
    assert!(r.view("evm:31337").is_err());
    assert!(r.restore_owner_checkpoint(snapshot()).is_err());
    assert!(r.restore_owner_checkpoint(s.clone()).is_err());
    assert!(r.apply(record(&s, "BLOCK_COMMIT")).is_err());
    s.reference.journal_seq += 1;
    s.reference.block.hash = "replacement".into();
    s.pools[0].reserve0 = "900000".parse().unwrap();
    let mut wrong = s.clone();
    wrong.reference.canonical_epoch += 1;
    assert!(r.apply(record(&wrong, "REORG_COMMIT")).is_err());
    assert!(r.view("evm:31337").is_err());
    r.apply(record(&s, "REORG_COMMIT")).unwrap();
    r.apply(record(&s, "REORG_COMMIT")).unwrap();
    assert!(!old.is_valid());
    assert_eq!(r.view("evm:31337").unwrap().reference, s.reference);
}
#[test]
fn malformed_begin_cannot_revoke_and_commit_cannot_bypass_begin() {
    let s = snapshot();
    let r = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    r.install_baseline(s.clone()).unwrap();
    let mut bad = s.clone();
    bad.reference.journal_seq += 1;
    assert!(r.apply(record(&bad, "REORG_BEGIN")).is_err());
    assert!(r.view("evm:31337").is_ok());
    bad.reference.canonical_epoch += 1;
    assert!(r.apply(record(&bad, "REORG_COMMIT")).is_err());
}

#[test]
fn scope_revocation_precedes_confirmed_reorg_without_allocating_epoch() {
    let mut s = snapshot();
    let r = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    r.install_baseline(s.clone()).unwrap();
    let old = r.view("evm:31337").unwrap();
    s.reference.journal_seq += 1;
    r.apply(record(&s, "ONLINE_SCOPE_REVOKED")).unwrap();
    assert!(!old.is_valid());
    assert!(r.restore_owner_checkpoint(s.clone()).is_err());
    s.reference.canonical_epoch += 1;
    s.reference.journal_seq += 1;
    r.apply(record(&s, "REORG_BEGIN")).unwrap();
    s.reference.journal_seq += 1;
    s.reference.block.hash = "confirmed-fork".into();
    r.apply(record(&s, "REORG_COMMIT")).unwrap();
    assert!(r.view("evm:31337").is_ok());
    assert!(!old.is_valid());
}

#[test]
fn token_capability_exclusion_is_visible_and_restores_without_deleting_source() {
    let mut s = snapshot();
    let replica = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    replica.install_baseline(s.clone()).unwrap();
    let id = s.pools[0].pool_id.clone();
    s.reference.block.number_or_slot += 1;
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = "blocked-token".into();
    s.reference.journal_seq += 1;
    s.pools[0].token_behavior = "simulation-only".into();
    replica.apply(record(&s, "BLOCK_COMMIT")).unwrap();
    let view = replica.view("evm:31337").unwrap();
    assert_eq!(view.pools.len(), s.pools.len());
    assert_eq!(view.graph.excluded_pools[&id], "TOKEN_BEHAVIOR_UNSUPPORTED");
    assert!(view
        .graph
        .adjacency
        .values()
        .flat_map(|edges| edges.iter())
        .all(|e| e.pool_id != id));
    s.reference.block.number_or_slot += 1;
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = "restored-token".into();
    s.reference.journal_seq += 1;
    s.pools[0].token_behavior = "standard".into();
    replica.apply(record(&s, "BLOCK_COMMIT")).unwrap();
    let restored = replica.view("evm:31337").unwrap();
    assert!(!restored.graph.excluded_pools.contains_key(&id));
    assert!(restored
        .graph
        .adjacency
        .values()
        .flat_map(|edges| edges.iter())
        .any(|e| e.pool_id == id));
}

#[test]
fn incremental_graph_shares_untouched_buckets_and_keeps_published_view_immutable() {
    use std::sync::Arc;
    let mut s = snapshot();
    let replica = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    replica.install_baseline(s.clone()).unwrap();
    let first = replica.view("evm:31337").unwrap();
    s.reference.block.number_or_slot += 1;
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = "reserve-only".into();
    s.reference.journal_seq += 1;
    s.pools[0].reserve0 = "2000000".parse().unwrap();
    replica.apply(record(&s, "BLOCK_COMMIT")).unwrap();
    let reserve = replica.view("evm:31337").unwrap();
    assert!(Arc::ptr_eq(&first.graph, &reserve.graph));
    assert_eq!(first.graph_version, reserve.graph_version);
    assert_ne!(first.pools[0].reserve0, reserve.pools[0].reserve0);
    let untouched: Vec<_> = reserve
        .graph
        .adjacency
        .iter()
        .filter(|(_, edges)| edges.iter().all(|e| e.pool_id != s.pools[0].pool_id))
        .map(|(token, edges)| (token.clone(), edges.clone()))
        .collect();
    assert!(!untouched.is_empty());
    let old_token = s.pools[0].token1.clone();
    s.pools[0].token1 = "NEW_TOKEN".into();
    s.reference.block.number_or_slot += 1;
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = "topology-change".into();
    s.reference.journal_seq += 1;
    replica.apply(record(&s, "BLOCK_COMMIT")).unwrap();
    let changed = replica.view("evm:31337").unwrap();
    assert!(!Arc::ptr_eq(&reserve.graph, &changed.graph));
    assert_eq!(changed.graph_version, reserve.graph_version + 1);
    for (token, edges) in untouched {
        assert!(Arc::ptr_eq(&edges, &changed.graph.adjacency[&token]));
    }
    assert!(reserve.graph.adjacency[&old_token]
        .iter()
        .any(|e| e.pool_id == s.pools[0].pool_id));
    assert!(!reserve.graph.adjacency.contains_key("NEW_TOKEN"));
    assert!(changed.graph.adjacency.contains_key("NEW_TOKEN"));
    assert_eq!(
        changed.graph.adjacency,
        xone_routing::Graph::build(&s.pools).unwrap().adjacency
    );
    assert!(reserve.is_valid());
}

#[test]
fn baseline_scope_expansion_reuses_untouched_edges_and_revokes_old_generation() {
    use std::sync::Arc;
    let mut s = snapshot();
    let replica = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    replica.install_baseline(s.clone()).unwrap();
    let old = replica.view("evm:31337").unwrap();
    let mut extra = s.pools[0].clone();
    extra.pool_id = "expanded-pool".into();
    extra.token0 = "NEW_A".into();
    extra.token1 = "NEW_B".into();
    s.pools.push(extra);
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    s.reference.online_generation += 1;
    s.reference.journal_seq += 1;
    replica.install_baseline(s.clone()).unwrap();
    let expanded = replica.view("evm:31337").unwrap();
    assert!(!old.is_valid());
    assert!(expanded.is_valid());
    assert_ne!(expanded.reference.scope_hash, old.reference.scope_hash);
    assert_eq!(expanded.graph_version, old.graph_version + 1);
    for (token, edges) in &old.graph.adjacency {
        assert!(Arc::ptr_eq(edges, &expanded.graph.adjacency[token]));
    }
    assert!(!old.graph.adjacency.contains_key("NEW_A"));
    assert!(expanded.graph.adjacency.contains_key("NEW_A"));
    assert_eq!(
        expanded.graph.adjacency,
        xone_routing::Graph::build(&s.pools).unwrap().adjacency
    );
    replica.suspend("evm:31337");
    replica.restore_owner_checkpoint(s).unwrap();
    let restored = replica.view("evm:31337").unwrap();
    assert!(!expanded.is_valid());
    assert!(restored.is_valid());
    assert!(Arc::ptr_eq(&expanded.graph, &restored.graph));
    assert_eq!(restored.graph_version, expanded.graph_version + 1);
}

#[test]
fn numeric_rebaseline_reuses_graph_without_reusing_previous_validity_flag() {
    use std::sync::Arc;
    let mut s = snapshot();
    let replica = Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)]);
    replica.install_baseline(s.clone()).unwrap();
    let old = replica.view("evm:31337").unwrap();
    s.pools[0].reserve0 = "2100000".parse().unwrap();
    s.reference.online_generation += 1;
    s.reference.journal_seq += 1;
    replica.install_baseline(s).unwrap();
    let next = replica.view("evm:31337").unwrap();
    assert!(!old.is_valid());
    assert!(next.is_valid());
    assert!(Arc::ptr_eq(&old.graph, &next.graph));
    assert_ne!(old.pools[0].reserve0, next.pools[0].reserve0);
    assert_eq!(next.graph_version, old.graph_version + 1);
}
