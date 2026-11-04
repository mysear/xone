use xone_domain::*;
use xone_solana_ingest::{rpc::skipped_slots, store::Store};
#[test]
fn rocksdb_restart_is_not_online_readiness() {
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    let path = std::env::temp_dir().join(format!(
        "xone-rocksdb-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        store.publish_baseline(snapshot.clone()).unwrap();
        store.history(&snapshot).unwrap();
        assert!(store.current().is_some());
        assert_eq!(store.journal(0, 10).unwrap().len(), 1);
    }
    {
        let mut store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        assert!(store.current().is_none());
        assert!(store.publish_baseline(snapshot.clone()).is_err());
        snapshot.reference.online_generation += 1;
        snapshot.reference.journal_seq += 1;
        snapshot.reference.writer_epoch += 1;
        store.publish_baseline(snapshot).unwrap();
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn skipped_slots_require_proven_range() {
    assert_eq!(skipped_slots(10, 13, 1, &[10, 12, 13]).unwrap(), vec![11]);
    assert!(skipped_slots(10, 13, 11, &[12, 13]).is_err());
    assert!(skipped_slots(10, 13, 1, &[10, 10, 12]).is_err());
}
#[test]
fn expired_authority_is_not_ready_after_process_pause() {
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    let path = std::env::temp_dir().join(format!("xone-expiry-{}", std::process::id()));
    {
        let mut store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        store.set_authority_deadline(std::time::Instant::now() - std::time::Duration::from_secs(1));
        store.publish_baseline(snapshot).unwrap();
        assert!(store.current().is_none());
        assert!(store.bootstrap().is_err());
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn committed_scope_revocation_is_durable_and_cannot_restore_old_readiness() {
    use xone_kafka_transport::{Cursor, Unit};
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    let path = std::env::temp_dir().join(format!("xone-revocation-{}", std::process::id()));
    {
        let mut store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        store.publish_baseline(snapshot.clone()).unwrap();
        snapshot.reference.journal_seq += 1;
        let u = Unit {
            kind: "ONLINE_SCOPE_REVOKED".into(),
            snapshot: snapshot.clone(),
            normalized_facts: None,
        };
        store.prepare(&u).unwrap();
        assert!(store.current().is_none());
        store
            .apply_prepared(Cursor {
                topic: "canonical.solana-localnet.v1".into(),
                partition: 0,
                offset: 7,
            })
            .unwrap();
        assert!(store.current().is_none());
        assert!(store.bootstrap().is_err());
        assert_eq!(store.previous().unwrap(), snapshot.reference);
        assert_eq!(
            store.journal(0, 10).unwrap().last().unwrap().kind,
            "ONLINE_SCOPE_REVOKED"
        );
    }
    {
        let mut store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        assert!(store.current().is_none());
        assert!(store.publish_baseline(snapshot.clone()).is_err());
        snapshot.reference.online_generation += 1;
        snapshot.reference.writer_epoch += 1;
        snapshot.reference.journal_seq += 1;
        store.publish_baseline(snapshot).unwrap();
        assert!(store.current().is_some());
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn malformed_revocation_cannot_replace_the_published_scope() {
    use xone_kafka_transport::Unit;
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    let path = std::env::temp_dir().join(format!("xone-revocation-invalid-{}", std::process::id()));
    {
        let mut store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        store.publish_baseline(s.clone()).unwrap();
        let mut bad = s;
        bad.reference.journal_seq += 1;
        bad.pools[0].reserve0 = "17".parse().unwrap();
        assert!(store
            .prepare(&Unit {
                kind: "ONLINE_SCOPE_REVOKED".into(),
                snapshot: bad,
                normalized_facts: None
            })
            .is_err());
        assert!(store.current().is_some());
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn pending_revocation_hides_locally_and_survives_restart_without_a_valid_source() {
    use xone_kafka_transport::Unit;
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    let path = std::env::temp_dir().join(format!("xone-pending-revoke-{}", std::process::id()));
    {
        let mut store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        store.publish_baseline(s.clone()).unwrap();
        s.reference.journal_seq += 1;
        store
            .prepare(&Unit {
                kind: "ONLINE_SCOPE_REVOKED".into(),
                snapshot: s.clone(),
                normalized_facts: None,
            })
            .unwrap();
        assert!(
            store.current().is_none(),
            "known-invalid scope remains locally ready before publication"
        );
        assert!(store.bootstrap().is_err());
    }
    {
        let store = Store::open(&path, "chain-parser-rust", 1).unwrap();
        let pending = store.prepared().unwrap().unwrap();
        assert_eq!(pending.kind, "ONLINE_SCOPE_REVOKED");
        assert_eq!(pending.snapshot, s);
        assert!(store.current().is_none());
    }
    std::fs::remove_dir_all(path).unwrap();
}
