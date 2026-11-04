use xone_domain::*;
use xone_state_replica::*;
#[test]
fn a_changed_third_asset_rebuilds_the_complete_graph_without_mutating_old_handle() {
    let doc: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/five-family-evm-wire.json"
    ))
    .unwrap();
    let mut s: Snapshot = serde_json::from_value(doc["snapshot"].clone()).unwrap();
    let replica = Replica::new(vec![(
        s.reference.block.chain_key.clone(),
        s.reference.owner_service.clone(),
        s.reference.owner_assignment_version,
    )]);
    replica.install_baseline(s.clone()).unwrap();
    let old = replica.view("evm:31337").unwrap();
    let old_token = s.pools[0].multi_asset.as_ref().unwrap().assets[2]
        .token
        .clone();
    let token = "0x000000000000000000000000000000000000ffff".to_string();
    s.pools[0].multi_asset.as_mut().unwrap().assets[2].token = token.clone();
    s.reference.block.number_or_slot += 1;
    s.reference.block.parent_hash = s.reference.block.hash.clone();
    s.reference.block.hash = format!("0x{}", "12".repeat(32));
    s.reference.journal_seq += 1;
    for p in &mut s.pools {
        if let Some(a) = &mut p.singleton_advanced {
            a.snapshot_block_hash = s.reference.block.hash.clone();
        }
    }
    let record = Record {
        reference: s.reference.clone(),
        kind: "BLOCK_COMMIT".into(),
        record_id: "third-token-change".into(),
        payload_hash: hash(&s.pools).unwrap(),
        pools: s.pools,
    };
    replica.apply(record).unwrap();
    let new = replica.view("evm:31337").unwrap();
    assert!(new.graph.adjacency.contains_key(&token));
    assert!(!new.graph.adjacency.contains_key(&old_token));
    assert!(old.graph.adjacency.contains_key(&old_token));
    assert!(!old.graph.adjacency.contains_key(&token));
    assert_eq!(new.graph_version, old.graph_version + 1);
}
