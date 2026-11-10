//! Wire fixtures follow the pinned upstream geyser.proto tag numbers, independently
//! of the parser implementation; no generated transaction re-encoding is involved.
use xone_solana_ingest::stream::*;
fn varint(mut n: u64) -> Vec<u8> {
    let mut out = vec![];
    while n >= 128 {
        out.push(n as u8 | 128);
        n >>= 7;
    }
    out.push(n as u8);
    out
}
fn scalar(tag: u64, n: u64) -> Vec<u8> {
    [varint(tag << 3), varint(n)].concat()
}
fn bytes(tag: u64, data: &[u8]) -> Vec<u8> {
    [
        varint(tag << 3 | 2),
        varint(data.len() as u64),
        data.to_vec(),
    ]
    .concat()
}
fn hash(slot: u8) -> String {
    bs58::encode([slot; 32]).into_string()
}
fn limits() -> StreamLimits {
    StreamLimits {
        max_bytes: 100_000,
        max_slots: 4,
        max_updates: 100,
    }
}
fn record(raw: Vec<u8>) -> CapturedFrame {
    CapturedFrame {
        cursor: WalCursor::start(),
        wal_generation: 1,
        frame: RawFrame {
            source: SourceSession {
                source_id: "p".into(),
                session_id: "s".into(),
            },
            sequence: 1,
            receive_timestamp_ms: 9,
            protobuf: raw,
        },
    }
}
fn info() -> Vec<u8> {
    [
        bytes(1, &[1; 32]),
        scalar(2, 100),
        bytes(3, &[2; 32]),
        scalar(4, 1),
        scalar(5, u64::MAX),
        bytes(6, b"data"),
        scalar(7, 99),
        bytes(8, &[3; 64]),
    ]
    .concat()
}
fn tx_info() -> Vec<u8> {
    [
        bytes(1, &[4; 64]),
        scalar(2, 1),
        bytes(3, b"transaction"),
        bytes(4, b"meta"),
        scalar(5, 0),
    ]
    .concat()
}
fn full_block() -> Vec<u8> {
    [
        scalar(1, 11),
        bytes(2, hash(11).as_bytes()),
        scalar(7, 10),
        bytes(8, hash(10).as_bytes()),
        scalar(9, 1),
        bytes(6, &tx_info()),
        scalar(10, 1),
        bytes(11, &info()),
        scalar(12, 0),
        scalar(14, 123),
    ]
    .concat()
}
#[test]
fn pinned_account_and_transaction_fields_preserve_native_identity_and_nested_bytes() {
    let raw = bytes(
        2,
        &[bytes(1, &info()), scalar(2, 11), scalar(4, 123)].concat(),
    );
    let YellowstoneEvent::Account(a) = project_yellowstone(&record(raw), limits()).unwrap() else {
        panic!()
    };
    assert_eq!(
        a.bank,
        BankId {
            slot: 11,
            bank_id: 123
        }
    );
    assert_eq!(a.pubkey, [1; 32]);
    assert_eq!(a.owner, [2; 32]);
    assert_eq!(a.write_version, 99);
    assert_eq!(a.rent_epoch, u64::MAX);
    assert_eq!(a.txn_signature, Some([3; 64]));
    assert!(a.executable);
    let raw = bytes(
        4,
        &[bytes(1, &tx_info()), scalar(2, 11), scalar(3, 123)].concat(),
    );
    let YellowstoneEvent::Transaction(tx) = project_yellowstone(&record(raw), limits()).unwrap()
    else {
        panic!()
    };
    assert!(tx.is_vote);
    assert_eq!(tx.transaction, b"transaction");
    assert_eq!(tx.meta, b"meta");
}
#[test]
fn native_full_block_and_metadata_counts_project_without_making_a_scope_seal() {
    let YellowstoneEvent::ProviderBlock(block) =
        project_yellowstone(&record(bytes(5, &full_block())), limits()).unwrap()
    else {
        panic!()
    };
    assert_eq!(block.metadata.bank.bank_id, 123);
    assert_eq!(block.updated_account_count, 1);
    assert_eq!(block.accounts.len(), 1);
    assert_eq!(block.transactions.len(), 1);
    assert_eq!(block.metadata.parent.blockhash, hash(10));
    let raw = bytes(
        7,
        &[
            scalar(1, 11),
            bytes(2, hash(11).as_bytes()),
            scalar(6, 10),
            bytes(7, hash(10).as_bytes()),
            scalar(8, 1),
            scalar(9, 0),
            scalar(10, 123),
        ]
        .concat(),
    );
    assert!(matches!(
        project_yellowstone(&record(raw), limits()).unwrap(),
        YellowstoneEvent::Metadata(_)
    ));
}
#[test]
fn missing_native_bank_identity_unknown_status_and_partial_tx_are_rejected() {
    for raw in [
        bytes(2, &[bytes(1, &info()), scalar(2, 11)].concat()),
        bytes(
            3,
            &[scalar(1, 11), scalar(2, 10), scalar(3, 99), scalar(5, 123)].concat(),
        ),
        bytes(
            4,
            &[
                bytes(1, &[bytes(1, &[4; 64]), bytes(3, b"tx")].concat()),
                scalar(2, 11),
                scalar(3, 123),
            ]
            .concat(),
        ),
        bytes(10, b"transaction-status-is-not-a-transaction"),
        vec![0xff],
    ] {
        assert!(project_yellowstone(&record(raw), limits()).is_err());
    }
}
#[test]
fn actual_bankless_slot_status_is_observation_and_never_an_account_barrier() {
    for status in [3, 4] {
        let event = project_yellowstone(
            &record(bytes(3, &[scalar(1, 11), scalar(3, status)].concat())),
            limits(),
        )
        .unwrap();
        assert!(matches!(
            event,
            YellowstoneEvent::BanklessSlot { slot: 11, .. }
        ));
    }
    let event = project_yellowstone(
        &record(bytes(
            3,
            &[scalar(1, 11), scalar(2, 10), scalar(3, 2), scalar(5, 123)].concat(),
        )),
        limits(),
    )
    .unwrap();
    assert!(matches!(
        event,
        YellowstoneEvent::Slot(SlotUpdate {
            status: SlotStatus::Finalized,
            ..
        })
    ));
}
#[test]
fn malformed_duplicate_oneof_and_oversized_projection_fail_before_admission() {
    let valid = bytes(5, &full_block());
    assert!(project_yellowstone(
        &record(valid.clone()),
        StreamLimits {
            max_updates: 1,
            ..limits()
        }
    )
    .is_err());
    assert!(project_yellowstone(
        &record(valid.clone()),
        StreamLimits {
            max_bytes: 16,
            ..limits()
        }
    )
    .is_err());
    assert!(
        project_yellowstone(&record([valid.clone(), bytes(6, b"")].concat()), limits()).is_err()
    );
    let truncated = valid[..valid.len() - 1].to_vec();
    assert!(project_yellowstone(&record(truncated), limits()).is_err());
}

#[test]
fn captured_real_frames_replay_with_independent_manifest_but_never_restore_live_readiness() {
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "xone-real-stream-replay-{}-{unique}",
        std::process::id()
    ));
    let dir = Temp(path);
    let wal_limits = FrameWalLimits {
        max_frame_bytes: 10_000,
        max_total_bytes: 100_000,
        max_records: 8,
    };
    let mut wal = FrameWal::open(&dir.0, "solana:test", wal_limits).unwrap();
    let source = record(vec![]).frame.source;
    let anchor = BankAnchor {
        bank: BankId {
            slot: 10,
            bank_id: 10,
        },
        blockhash: hash(10),
    };
    let scope = std::collections::BTreeSet::from([[1; 32]]);
    let mut live = StreamAssembler::new(limits()).unwrap();
    live.anchor(
        anchor.clone(),
        source.clone(),
        1,
        scope.clone(),
        wal.generation(),
    )
    .unwrap();
    let mut first = record(bytes(5, &full_block())).frame;
    first.sequence = 1;
    capture_decode_admit(&mut wal, &mut live, first, |r| {
        project_yellowstone(r, limits())
    })
    .unwrap();
    let mut second = record(bytes(
        3,
        &[scalar(1, 11), scalar(2, 10), scalar(3, 2), scalar(5, 123)].concat(),
    ))
    .frame;
    second.sequence = 2;
    let cursor = capture_decode_admit(&mut wal, &mut live, second, |r| {
        project_yellowstone(r, limits())
    })
    .unwrap();
    let expected = AccountUpdate {
        bank: BankId {
            slot: 11,
            bank_id: 123,
        },
        pubkey: [1; 32],
        owner: [2; 32],
        lamports: 100,
        executable: true,
        rent_epoch: u64::MAX,
        write_version: 99,
        is_startup: false,
        txn_signature: Some([3; 64]),
        data: b"data".to_vec(),
    };
    let manifest = AccountCompletion::VersionedBankManifest {
        bank: expected.bank.clone(),
        parent: ParentBank {
            slot: 10,
            blockhash: hash(10),
        },
        blockhash: hash(11),
        expected_account_updates: 1,
        final_versions: std::collections::BTreeMap::from([(
            [1; 32],
            ExpectedAccountVersion::of(&expected),
        )]),
    };
    live.seal_accounts(&source, manifest.clone()).unwrap();
    let committed = live.commit_next().unwrap().unwrap();
    assert!(live.ready());
    wal.checkpoint(&cursor).unwrap();
    drop(wal);
    let wal = FrameWal::open(&dir.0, "solana:test", wal_limits).unwrap();
    assert_eq!(wal.recovery_cursor(), cursor);
    let mut replay = StreamAssembler::new(limits()).unwrap();
    replay
        .anchor(anchor, source.clone(), 1, scope, wal.generation())
        .unwrap();
    // The independent manifest is explicitly restored from its separate authority.
    // Raw Yellowstone protobuf cannot recreate it.
    replay.seal_accounts(&source, manifest).unwrap();
    replay_decode_admit(&wal, &mut replay, &WalCursor::start(), |r| {
        project_yellowstone(r, limits())
    })
    .unwrap();
    let restored = replay.commit_next().unwrap().unwrap();
    assert_eq!(restored.accounts, committed.accounts);
    assert!(!replay.ready());
    assert!(project_and_admit(&mut replay, &wal.read(1).unwrap(), limits()).is_err());
    assert!(replay.needs_reanchor());
}
