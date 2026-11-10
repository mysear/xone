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

fn advance_scope_bank(s: &mut Snapshot, marker: u8) -> xone_domain::Record {
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = format!("0x{}", format!("{marker:02x}").repeat(32));
    s.reference.block.number_or_slot += 1;
    s.reference.journal_seq += 1;
    for pool in &mut s.pools {
        if let Some(state) = pool
            .multi_asset
            .as_mut()
            .and_then(|multi| multi.managed_v2.as_mut())
        {
            state.block_number = s.reference.block.number_or_slot;
            state.block_hash = s.reference.block.hash.clone();
        }
    }
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    xone_domain::Record {
        reference: s.reference.clone(),
        kind: "BLOCK_COMMIT".into(),
        record_id: format!("admission-{marker}"),
        payload_hash: hash(&s.pools).unwrap(),
        pools: s.pools.clone(),
    }
}

#[test]
fn block_commit_refreshes_admission_even_when_tokens_and_protocols_are_unchanged() {
    let mut initial = source();
    initial.pools.push(weighted(&initial.pools[0], false));
    initial.pools.push(ordinary());
    seal(&mut initial);
    let replica = replica(&initial);
    replica.install_baseline(initial.clone()).unwrap();
    let before = replica.view(&initial.reference.block.chain_key).unwrap();
    assert!(before.graph.excluded_pools.is_empty());
    let original_vault = initial.pools[2].multi_asset.as_ref().unwrap().vault.clone();
    let original_code = initial.pools[2]
        .multi_asset
        .as_ref()
        .unwrap()
        .vault_code_sha256
        .clone();
    let managed = initial.pools[0].multi_asset.as_ref().unwrap();
    let shared_vault = managed.vault.clone();
    let shared_code = managed.vault_code_sha256.clone();
    let mut current = initial.clone();
    current.pools[2].multi_asset.as_mut().unwrap().vault = shared_vault;
    current.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = shared_code;
    let record = advance_scope_bank(&mut current, 0xf1);
    replica.apply(record).unwrap();
    let excluded = replica.view(&current.reference.block.chain_key).unwrap();
    assert_eq!(
        excluded.graph.excluded_pools.len(),
        3,
        "current full-bank admission must replace the old graph"
    );
    assert_eq!(excluded.graph_version, before.graph_version + 1);
    assert!(excluded
        .graph
        .adjacency
        .values()
        .flatten()
        .all(|edge| edge.pool_id == current.pools[3].pool_id));
    assert!(
        before.graph.excluded_pools.is_empty(),
        "immutable older graph was changed"
    );
    can_quote(&excluded, &current.pools[3]);
    current.pools[2].multi_asset.as_mut().unwrap().vault = original_vault;
    current.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = original_code;
    replica
        .apply(advance_scope_bank(&mut current, 0xf2))
        .unwrap();
    let restored = replica.view(&current.reference.block.chain_key).unwrap();
    assert!(
        restored.graph.excluded_pools.is_empty(),
        "restored pools stay excluded"
    );
    assert_eq!(restored.graph_version, excluded.graph_version + 1);
    assert!(!std::sync::Arc::ptr_eq(&before.graph, &restored.graph));
    assert_eq!(excluded.graph.excluded_pools.len(), 3);
    can_quote(&restored, &current.pools[2]);
}

#[test]
fn admitted_bank_without_quote_edges_advances_watermark_revokes_then_recovers() {
    let mut bank = source();
    bank.pools.push(weighted(&bank.pools[0], false));
    seal(&mut bank);
    let replica = replica(&bank);
    replica.install_baseline(bank.clone()).unwrap();
    let previous = replica.view(&bank.reference.block.chain_key).unwrap();
    let ordinary_vault = bank.pools[2].multi_asset.as_ref().unwrap().vault.clone();
    let ordinary_code = bank.pools[2]
        .multi_asset
        .as_ref()
        .unwrap()
        .vault_code_sha256
        .clone();
    let managed = bank.pools[0].multi_asset.as_ref().unwrap();
    let vault = managed.vault.clone();
    let code = managed.vault_code_sha256.clone();
    bank.pools[2].multi_asset.as_mut().unwrap().vault = vault;
    bank.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = code;
    replica.apply(advance_scope_bank(&mut bank, 0xf3)).unwrap();
    assert!(replica.view(&bank.reference.block.chain_key).is_err());
    assert!(
        !previous.is_valid(),
        "old quote handle survived loss of all admissible routes"
    );
    assert_eq!(
        replica
            .last_reference(&bank.reference.block.chain_key)
            .unwrap(),
        Some(bank.reference.clone())
    );
    bank.pools[2].multi_asset.as_mut().unwrap().vault = ordinary_vault;
    bank.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = ordinary_code;
    replica.apply(advance_scope_bank(&mut bank, 0xf4)).unwrap();
    let recovered = replica.view(&bank.reference.block.chain_key).unwrap();
    assert!(recovered.graph.excluded_pools.is_empty());
    assert!(!previous.is_valid());
    can_quote(&recovered, &bank.pools[2]);
}

#[test]
fn changing_admission_cannot_bypass_an_explicit_scope_suspension() {
    let mut bank = source();
    bank.pools.push(weighted(&bank.pools[0], false));
    bank.pools.push(ordinary());
    seal(&mut bank);
    let replica = replica(&bank);
    replica.install_baseline(bank.clone()).unwrap();
    let saved = replica.view(&bank.reference.block.chain_key).unwrap();
    replica.suspend(&bank.reference.block.chain_key);
    let shared = bank.pools[0].multi_asset.as_ref().unwrap();
    let vault = shared.vault.clone();
    let code = shared.vault_code_sha256.clone();
    bank.pools[2].multi_asset.as_mut().unwrap().vault = vault;
    bank.pools[2]
        .multi_asset
        .as_mut()
        .unwrap()
        .vault_code_sha256 = code;
    let record = advance_scope_bank(&mut bank, 0xf5);
    assert_eq!(replica.apply(record).unwrap_err().0, "REANCHOR_REQUIRED");
    assert!(!saved.is_valid());
    assert!(replica.view(&bank.reference.block.chain_key).is_err());
    assert_eq!(
        replica
            .last_reference(&bank.reference.block.chain_key)
            .unwrap(),
        Some(saved.reference.clone())
    );
}
