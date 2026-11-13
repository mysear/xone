use xone_domain::{hash, scope_hash, Record, Snapshot};
use xone_state_replica::Replica;
fn snapshot() -> Snapshot {
    serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap()
}
fn replica() -> Replica {
    Replica::new(vec![("evm:31337".into(), "chain-parser-go".into(), 1)])
}
#[test]
fn protocol_partitions_are_deterministic_state_bound_and_reorg_revoked() {
    let mut s = snapshot();
    let r = replica();
    r.install_baseline(s.clone()).unwrap();
    let first = r.view("evm:31337").unwrap();
    let another = replica();
    another.install_baseline(s.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&first.protocol_views).unwrap(),
        serde_json::to_value(&another.view("evm:31337").unwrap().protocol_views).unwrap()
    );
    let old = first.protocol_views.values().next().unwrap().clone();
    assert_eq!(old.parent_view_id, first.view_id);
    assert!(old.is_valid());
    assert!(!old.capabilities.production_ready);
    s.reference.block.number_or_slot += 1;
    s.reference.journal_seq += 1;
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = "next".into();
    s.pools[0].reserve0 = "1000099".parse().unwrap();
    let unit = Record {
        reference: s.reference.clone(),
        kind: "BLOCK_COMMIT".into(),
        record_id: "next".into(),
        payload_hash: hash(&s.pools).unwrap(),
        pools: s.pools.clone(),
    };
    r.apply(unit).unwrap();
    let next = r.view("evm:31337").unwrap();
    assert_ne!(
        old.view_id,
        next.protocol_views.values().next().unwrap().view_id
    );
    assert_ne!(
        old.state_hash,
        next.protocol_views.values().next().unwrap().state_hash
    );
    assert!(old.is_valid());
    r.suspend("evm:31337");
    assert!(r.view("evm:31337").is_err());
    assert!(!r.diagnostic_view("evm:31337").unwrap().is_valid());
    assert!(!old.is_valid());
    assert!(next.protocol_views.values().all(|p| !p.is_valid()));
}
#[test]
fn unsupported_pool_is_visible_without_disabling_qualified_protocol_scope() {
    let mut s = snapshot();
    let mut unknown = s.pools[0].clone();
    unknown.pool_id = "0x9999999999999999999999999999999999999999".into();
    unknown.protocol = "unknown-v4-hook".into();
    unknown.adapter_version = "unknown/1".into();
    s.pools.push(unknown);
    s.reference.scope_hash = scope_hash(&s.pools).unwrap();
    let mut adapters = s
        .pools
        .iter()
        .map(|p| p.adapter_version.clone())
        .collect::<Vec<_>>();
    adapters.sort();
    adapters.dedup();
    s.reference.adapter_set_hash = hash(&adapters).unwrap();
    let r = replica();
    r.install_baseline(s).unwrap();
    let view = r.view("evm:31337").unwrap();
    let unknown = view
        .protocol_views
        .values()
        .find(|p| p.protocol == "unknown-v4-hook")
        .unwrap();
    assert!(unknown.available_pool_ids.is_empty());
    assert!(unknown.capabilities.quote_modes.is_empty());
    assert!(!unknown.capabilities.unsigned_transaction);
    assert_eq!(unknown.excluded_pools.len(), 1);
    assert!(view
        .protocol_views
        .values()
        .any(|p| !p.available_pool_ids.is_empty()));
}
