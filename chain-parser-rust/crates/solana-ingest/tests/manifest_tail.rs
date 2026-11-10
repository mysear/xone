use std::{collections::BTreeMap, sync::Arc};
use xone_config::Chain;
use xone_solana_ingest::{
    manifest_recovery::{ManifestRecoverySource, TailBaseline},
    scope::*,
    stream::*,
};
fn bank(slot: u64) -> CommittedBank {
    let bank = BankId {
        slot,
        bank_id: slot,
    };
    CommittedBank {
        bank: bank.clone(),
        blockhash: format!("bank{slot}"),
        parent: BankAnchor {
            bank: BankId {
                slot: slot - 1,
                bank_id: slot - 1,
            },
            blockhash: format!("bank{}", slot - 1),
        },
        accounts: BTreeMap::from([(
            [1; 32],
            Arc::new(AccountUpdate {
                bank,
                pubkey: [1; 32],
                owner: [2; 32],
                lamports: 1,
                executable: false,
                rent_epoch: 0,
                write_version: slot,
                is_startup: false,
                txn_signature: None,
                data: vec![slot as u8],
            }),
        )]),
        account_versions: BTreeMap::new(),
        transactions: BTreeMap::new(),
    }
}
fn receipt(cursor: u64, session: &str) -> CapturedFrame {
    CapturedFrame {
        cursor: WalCursor {
            sequence: cursor,
            record_sha256: "ab".repeat(32),
        },
        wal_generation: 1,
        frame: RawFrame {
            source: SourceSession {
                source_id: "public".into(),
                session_id: session.into(),
            },
            sequence: cursor,
            receive_timestamp_ms: 1000,
            protobuf: vec![],
        },
    }
}
#[test]
fn nonzero_native_bank_tail_requires_same_session_and_real_ancestry() {
    let chain:Chain=serde_json::from_value(serde_json::json!({"key":"solana:localnet","genesis_hash":"localnet","owner_service":"chain-parser-rust","assignment_version":1,"rpc_urls":[],"pools":[],"confirmation_tag":"finalized","auto_reorg_window":1})).unwrap();
    let a = bank(10);
    let b = bank(11);
    let ra = receipt(1, "session");
    let rb = receipt(3, "session");
    let mut source =
        ManifestRecoverySource::from_committed(&chain, &b, 1000, &rb, 100000, 1).unwrap();
    source
        .set_tail(
            &chain,
            TailBaseline {
                bank: &a,
                receipt: &ra,
                timestamp_ms: 999,
            },
            1024,
        )
        .expect("a certified nonzero bank tail must restore from its actual predecessor");
    let key = bs58::encode([1; 32]).into_string();
    let budget = ScopeLimits {
        max_jobs: 1,
        max_account_bytes: 100000,
        max_tail_events: 4,
        max_tail_bytes: 100000,
        max_tail_age_ms: 100,
        max_baseline_age_ms: 5000,
        shard_accounts: 1,
    };
    let job = ScopeJob {
        id: "job".into(),
        fence: Fence {
            chain_key: chain.key.clone(),
            chain_identity: chain.genesis_hash.clone(),
            owner: chain.owner_service.clone(),
            assignment: 1,
            generation: 1,
            canonical_epoch: 1,
            writer_epoch: 1,
            adapter_version: "adapter".into(),
            schema_version: "1".into(),
        },
        anchor: source.anchor(),
        pool_ids: vec!["pool".into()],
        budget,
        history_after: 0,
        dependencies: source.dependencies().into_values().collect(),
        reserved_bytes: 10000,
        phase: ScopePhase::Admitted,
        attempt: 1,
        buffer_id: String::new(),
        baseline_hash: String::new(),
        cursor: 0,
        restored_accounts: 0,
        failure: String::new(),
        history_gap: None,
    };
    let cancel = Cancellation::new();
    let buffer = source.start_buffer(&cancel, &job).unwrap();
    let base = source
        .candidates(&cancel, &job, &job.anchor)
        .unwrap()
        .remove(0);
    assert_eq!(base.anchor.cursor, 1);
    let rows = source.restore_shard(&cancel, &job, &base, &[key]).unwrap();
    let tail = source
        .align_tail(&cancel, &job, &buffer, &base, &rows)
        .unwrap();
    assert_eq!(tail.events, 2);
    assert_eq!(tail.to.cursor, 3);
    assert_ne!(tail.accounts[0].data, rows[0].data);
    let wrong = receipt(1, "other-session");
    assert!(source
        .set_tail(
            &chain,
            TailBaseline {
                bank: &a,
                receipt: &wrong,
                timestamp_ms: 999
            },
            1024
        )
        .is_err());
}
