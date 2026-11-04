//! T34.9: publish the complete canonical bank and separate execution exclusions.
use serde_json::{json, Value};
use std::collections::BTreeSet;
use xone_domain::{hash, scope_hash, Pool, Snapshot};
use xone_routing::{search, Budget};
use xone_state_replica::{QuoteView, Replica};

fn raw() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/managed-v2-shared-vault.json"
    ))
    .unwrap()
}
fn source() -> Snapshot {
    let bank = raw();
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.pools = bank["banks"]["H38"]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::from_value(r["pool"].clone()).unwrap())
        .collect();
    let h = &bank["banks"]["H38"]["header"];
    let hex =
        |v: &Value| u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
    s.reference.block.number_or_slot = hex(&h["number"]);
    s.reference.block.timestamp_ms = hex(&h["timestamp"]) * 1000;
    s.reference.block.hash = h["hash"].as_str().unwrap().into();
    s.reference.block.parent_hash = h["parentHash"].as_str().unwrap().into();
    seal(&mut s);
    s
}
fn seal(s: &mut Snapshot) {
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    let mut adapters: Vec<_> = s.pools.iter().map(|p| p.adapter_version.clone()).collect();
    adapters.sort();
    adapters.dedup();
    s.reference.adapter_set_hash = hash(&adapters).unwrap();
}
fn ordinary() -> Pool {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.pools[0].clone()
}
fn weighted(managed: &Pool, same_vault: bool) -> Pool {
    // A legal, old three-underlying static weighted profile, not a Managed
    // object with an incompatible test model attached.
    let d: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/multi-asset-state.json"
    ))
    .unwrap();
    let mut p: Pool = serde_json::from_value(d["pools"][0].clone()).unwrap();
    if same_vault {
        let observed = managed.multi_asset.as_ref().unwrap();
        let s = p.multi_asset.as_mut().unwrap();
        s.vault = observed.vault.clone();
        s.vault_code_sha256 = observed.vault_code_sha256.clone();
    }
    p.validate().unwrap();
    p
}
fn mixed() -> Snapshot {
    let mut s = source();
    s.pools.push(weighted(&s.pools[0], true));
    s.pools.push(ordinary());
    seal(&mut s);
    s
}

fn replica(s: &Snapshot) -> Replica {
    Replica::new(vec![(
        s.reference.block.chain_key.clone(),
        s.reference.owner_service.clone(),
        s.reference.owner_assignment_version,
    )])
}
fn can_quote(v: &QuoteView, p: &Pool) {
    let found = search(
        &v.graph,
        &v.pools,
        &p.token0,
        &p.token1,
        &"1000000000000000000".parse().unwrap(),
        Budget {
            split_parts: 1,
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 100,
        },
    )
    .unwrap();
    assert!(found
        .candidates
        .iter()
        .any(|r| r.edges[0].pool_id == p.pool_id));
}
fn assert_complete(v: &QuoteView, s: &Snapshot) {
    assert!(v.is_valid());
    assert_eq!(v.pools, s.pools);
    assert_eq!(v.reference, s.reference);
    assert_eq!(scope_hash(&v.pools).unwrap(), s.reference.scope_hash);
}
#[test]
fn original_shared_managed_bank_publishes_all_records_and_both_directions() {
    let s = source();
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    let replica = replica(&s);
    replica.install_baseline(s.clone()).unwrap();
    let view = replica.view(&s.reference.block.chain_key).unwrap();
    assert_complete(&view, &s);
    assert!(view.graph.excluded_pools.is_empty());
    assert_eq!(
        view.graph.adjacency.values().map(Vec::len).sum::<usize>(),
        4
    );
    for p in &s.pools {
        can_quote(&view, p);
    }
}
#[test]
fn mixed_vault_publish_keeps_closed_bank_and_only_unrelated_edges_in_either_order() {
    for reverse in [false, true] {
        let mut s = mixed();
        if reverse {
            s.pools.reverse();
            seal(&mut s);
        }
        s.validate(
            &s.reference.owner_service,
            s.reference.owner_assignment_version,
        )
        .unwrap();
        let replica = replica(&s);
        replica.install_baseline(s.clone()).unwrap();
        let view = replica.view(&s.reference.block.chain_key).unwrap();
        assert_complete(&view, &s);
        let expected: BTreeSet<_> = s
            .pools
            .iter()
            .filter(|p| p.multi_asset.is_some())
            .map(|p| p.pool_id.clone())
            .collect();
        assert_eq!(
            view.graph
                .excluded_pools
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected
        );
        assert!(view
            .graph
            .excluded_pools
            .values()
            .all(|r| r == "SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED"));
        let ordinary = s.pools.iter().find(|p| p.multi_asset.is_none()).unwrap();
        let edges: Vec<_> = view.graph.adjacency.values().flatten().collect();
        assert_eq!(edges.len(), 2);
        assert!(edges.iter().all(|e| e.pool_id == ordinary.pool_id));
        assert_ne!(
            scope_hash(std::slice::from_ref(ordinary)).unwrap(),
            view.reference.scope_hash
        );
        can_quote(&view, ordinary);
        assert_eq!(
            replica
                .last_reference(&s.reference.block.chain_key)
                .unwrap(),
            Some(s.reference.clone())
        );
        assert!(!replica.reorg_pending(&s.reference.block.chain_key).unwrap());
    }
}
#[test]
fn contradictory_shared_facts_reject_the_entire_replacement_and_preserve_old_view() {
    for (field, value, reason) in [
        ("vault_paused", json!(true), "SHARED_VAULT_PAUSE"),
        (
            "block_hash",
            json!(format!("0x{}", "ab".repeat(32))),
            "SHARED_VAULT_SOURCE",
        ),
    ] {
        let good = mixed();
        let replica = replica(&good);
        replica.install_baseline(good.clone()).unwrap();
        let old = replica.view(&good.reference.block.chain_key).unwrap();
        let mut bad = serde_json::to_value(&good).unwrap();
        bad["pools"][1]["multi_asset"]["managed_v2"][field] = value;
        let mut bad: Snapshot = serde_json::from_value(bad).unwrap();
        for p in &bad.pools {
            p.validate().unwrap();
        }
        bad.reference.online_generation += 1;
        bad.reference.journal_seq += 1;
        bad.reference.canonical_epoch += 1;
        seal(&mut bad);
        let error = replica.install_baseline(bad).unwrap_err();
        assert!(error.to_string().contains(reason), "{error}");
        let current = replica.view(&good.reference.block.chain_key).unwrap();
        assert!(std::sync::Arc::ptr_eq(&old, &current));
        assert_complete(&old, &good);
        can_quote(&old, good.pools.last().unwrap());
    }
}
#[test]
fn snapshot_source_header_mismatch_is_not_a_capability_exclusion() {
    let good = mixed();
    let replica = replica(&good);
    replica.install_baseline(good.clone()).unwrap();
    let old = replica.view(&good.reference.block.chain_key).unwrap();
    let mut bad = good.clone();
    bad.reference.online_generation += 1;
    bad.reference.journal_seq += 1;
    bad.reference.block.hash = format!("0x{}", "cd".repeat(32));
    assert!(replica
        .install_baseline(bad)
        .unwrap_err()
        .to_string()
        .contains("MANAGED_V2_SNAPSHOT_BANK"));
    assert_complete(&old, &good);
    assert!(std::sync::Arc::ptr_eq(
        &old,
        &replica.view(&good.reference.block.chain_key).unwrap()
    ));
}
#[test]
fn different_vault_and_weighted_only_scopes_remain_quote_capable() {
    let mut together = source();
    let w = weighted(&together.pools[0], false);
    together.pools.push(w.clone());
    seal(&mut together);
    let mut weighted_only = together.clone();
    weighted_only.pools = vec![w.clone()];
    seal(&mut weighted_only);
    for s in [together, weighted_only] {
        let replica = replica(&s);
        replica.install_baseline(s.clone()).unwrap();
        let view = replica.view(&s.reference.block.chain_key).unwrap();
        assert_complete(&view, &s);
        assert!(view.graph.excluded_pools.is_empty());
        can_quote(&view, &w);
    }
}
#[test]
fn cross_pool_bpt_excludes_the_whole_vault_without_erasing_canonical_rows() {
    let mut s = source();
    let bpt = s.pools[1].pool_id.clone();
    // A synthetic second registration of this BPT must be backed by the
    // same increased global wallet in the issuer pool's own declaration.
    let funding = s.pools[0]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .token_info[1]
        .cash
        .clone();
    s.pools[1]
        .multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .vault_bpt_actual_balance
        .0 += &funding.0;
    let bpt_wallet = s.pools[1]
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .vault_bpt_actual_balance
        .clone();
    let bpt_code = s.pools[1].multi_asset.as_ref().unwrap().code_sha256.clone();
    let a = &mut s.pools[0];
    a.token1 = bpt.clone();
    let multi = a.multi_asset.as_mut().unwrap();
    multi.assets[1].token = bpt;
    multi.assets[1].code_sha256 = bpt_code;
    multi.managed_v2.as_mut().unwrap().token_info[1].vault_actual_balance = bpt_wallet;
    a.validate().unwrap();
    s.pools.push(ordinary());
    seal(&mut s);
    let replica = replica(&s);
    replica.install_baseline(s.clone()).unwrap();
    let view = replica.view(&s.reference.block.chain_key).unwrap();
    assert_complete(&view, &s);
    assert_eq!(view.graph.excluded_pools.len(), 2);
    for p in &s.pools[..2] {
        assert_eq!(
            view.graph.excluded_pools[&p.pool_id],
            "SHARED_VAULT_BPT_UNSUPPORTED"
        );
    }
    assert_eq!(
        view.graph.adjacency.values().map(Vec::len).sum::<usize>(),
        2
    );
    can_quote(&view, &s.pools[2]);
}
