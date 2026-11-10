use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use xone_solana_ingest::stream::*;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "xone-stream-wal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn limits() -> FrameWalLimits {
    FrameWalLimits {
        max_frame_bytes: 4096,
        max_total_bytes: 50_000,
        max_records: 20,
    }
}
fn source() -> SourceSession {
    SourceSession {
        source_id: "provider".into(),
        session_id: "session".into(),
    }
}
fn frame(seq: u64, raw: &[u8]) -> RawFrame {
    RawFrame {
        source: source(),
        sequence: seq,
        receive_timestamp_ms: 123,
        protobuf: raw.to_vec(),
    }
}
#[test]
fn capture_is_durable_before_decoder_and_decode_failure_is_replayable() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    let mut a = StreamAssembler::new(StreamLimits {
        max_bytes: 4096,
        max_slots: 2,
        max_updates: 8,
    })
    .unwrap();
    a.anchor(
        BankAnchor {
            bank: BankId {
                slot: 10,
                bank_id: 10,
            },
            blockhash: "bank".into(),
        },
        source(),
        1,
        BTreeSet::from([[1; 32]]),
        wal.generation(),
    )
    .unwrap();
    let failure = capture_decode_admit(&mut wal, &mut a, frame(1, b"\x00\xffraw"), |record| {
        assert_eq!(record.cursor.sequence, 1);
        assert_eq!(record.frame.protobuf, b"\x00\xffraw");
        Err(xone_domain::err("PROTO_DECODE_FAILURE"))
    });
    assert!(failure.is_err());
    assert_eq!(wal.len(), 1);
    assert!(a.needs_reanchor());
    let cursor = wal.read(1).unwrap().cursor;
    let old_generation = wal.generation();
    drop(wal);
    let mut recovered = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    assert!(recovered.generation() > old_generation);
    assert_eq!(recovered.read(1).unwrap().frame.protobuf, b"\x00\xffraw");
    assert!(!recovered.is_current(&recovered.read(1).unwrap()));
    let mut seen = 0;
    recovered
        .replay_from(&WalCursor::start(), |_| {
            seen += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(seen, 1);
    recovered.checkpoint(&cursor).unwrap();
    drop(recovered);
    let recovered = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    assert_eq!(recovered.recovery_cursor(), cursor);
    assert!(recovered
        .replay_from(
            &WalCursor {
                sequence: 1,
                record_sha256: "wrong".into()
            },
            |_| Ok(())
        )
        .is_err());
}
#[test]
fn wal_is_bounded_does_not_dedup_arrivals_and_captures_before_sequence_admission() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(
        &dir.0,
        "solana:test",
        FrameWalLimits {
            max_records: 2,
            ..limits()
        },
    )
    .unwrap();
    wal.capture(frame(1, b"same")).unwrap();
    wal.capture(frame(1, b"same")).unwrap();
    assert_eq!(wal.len(), 2);
    assert!(wal.capture(frame(2, b"third")).is_err());
    assert!(wal.read(0).is_err());
}
#[test]
fn durable_head_detects_removed_tail_and_chain_identity_mismatch() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    wal.capture(frame(1, b"raw")).unwrap();
    drop(wal);
    assert!(FrameWal::open(&dir.0, "solana:other", limits()).is_err());
    let db = rocksdb::DB::open_default(&dir.0).unwrap();
    db.delete("frame/00000000000000000001").unwrap();
    drop(db);
    assert!(FrameWal::open(&dir.0, "solana:test", limits()).is_err());
}
#[test]
fn checkpoint_is_hash_bound_monotonic_and_decode_budget_failure_never_admits() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    let one = wal.capture(frame(1, b"one")).unwrap();
    let two = wal.capture(frame(2, b"two")).unwrap();
    wal.checkpoint(&two).unwrap();
    assert!(wal.checkpoint(&one).is_err());
    assert!(wal
        .checkpoint(&WalCursor {
            sequence: 3,
            record_sha256: "invented".into()
        })
        .is_err());
    let huge = vec![0; 4097];
    assert!(wal.capture(frame(3, &huge)).is_err());
    assert_eq!(wal.len(), 2);
}

#[test]
fn independent_head_rejects_a_self_consistent_database_rollback() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    let first = wal.capture(frame(1, b"first")).unwrap();
    wal.capture(frame(2, b"second")).unwrap();
    drop(wal);
    let db = rocksdb::DB::open_default(&dir.0).unwrap();
    let raw = db.get("frame/00000000000000000001").unwrap().unwrap();
    let mut meta: serde_json::Value =
        serde_json::from_slice(&db.get("stream-meta").unwrap().unwrap()).unwrap();
    meta["sequence"] = serde_json::json!(1);
    meta["bytes"] = serde_json::json!(raw.len());
    meta["last_hash"] = serde_json::json!(first.record_sha256);
    db.put("stream-meta", serde_json::to_vec(&meta).unwrap())
        .unwrap();
    db.delete("frame/00000000000000000002").unwrap();
    drop(db);
    assert!(FrameWal::open(&dir.0, "solana:test", limits()).is_err());
}
#[test]
fn payload_damage_and_disk_budget_fail_closed() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    wal.capture(frame(1, b"payload")).unwrap();
    drop(wal);
    let db = rocksdb::DB::open_default(&dir.0).unwrap();
    let mut raw = db.get("frame/00000000000000000001").unwrap().unwrap();
    *raw.last_mut().unwrap() ^= 1;
    db.put("frame/00000000000000000001", raw).unwrap();
    drop(db);
    assert!(FrameWal::open(&dir.0, "solana:test", limits()).is_err());
    let dir = Temp::new();
    let mut wal = FrameWal::open(
        &dir.0,
        "solana:test",
        FrameWalLimits {
            max_total_bytes: 1,
            ..limits()
        },
    )
    .unwrap();
    assert!(wal.capture(frame(1, b"x")).is_err());
    assert!(wal.is_empty());
}
#[test]
fn replay_decoder_failures_invalidate_and_multi_session_receipts_are_preserved() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    wal.capture(frame(1, b"session-a")).unwrap();
    let mut next = frame(1, b"session-b");
    next.source.session_id = "next-session".into();
    wal.capture(next).unwrap();
    let mut receipts = vec![];
    wal.replay_from(&WalCursor::start(), |record| {
        receipts.push(record.frame.source.session_id);
        Ok(())
    })
    .unwrap();
    assert_eq!(receipts, vec!["session", "next-session"]);
    let mut a = StreamAssembler::new(StreamLimits {
        max_bytes: 4096,
        max_slots: 2,
        max_updates: 8,
    })
    .unwrap();
    a.anchor(
        BankAnchor {
            bank: BankId {
                slot: 10,
                bank_id: 10,
            },
            blockhash: "anchor".into(),
        },
        source(),
        1,
        BTreeSet::from([[1; 32]]),
        wal.generation(),
    )
    .unwrap();
    assert!(
        replay_decode_admit(&wal, &mut a, &WalCursor::start(), |_| Ok(
            YellowstoneEvent::Heartbeat
        ))
        .is_err()
    );
    assert!(a.needs_reanchor());
    assert!(!a.ready());
}

#[test]
fn a_sequence_gap_frame_is_durable_even_when_assembler_refuses_it() {
    let dir = Temp::new();
    let mut wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    let mut a = StreamAssembler::new(StreamLimits {
        max_bytes: 4096,
        max_slots: 2,
        max_updates: 8,
    })
    .unwrap();
    a.anchor(
        BankAnchor {
            bank: BankId {
                slot: 10,
                bank_id: 10,
            },
            blockhash: "anchor".into(),
        },
        source(),
        1,
        BTreeSet::from([[1; 32]]),
        wal.generation(),
    )
    .unwrap();
    assert!(
        capture_decode_admit(&mut wal, &mut a, frame(2, b"gap-frame"), |_| Ok(
            YellowstoneEvent::Heartbeat
        ))
        .is_err()
    );
    assert!(a.needs_reanchor());
    assert_eq!(wal.len(), 1);
    assert_eq!(wal.read(1).unwrap().frame.sequence, 2);
    drop(wal);
    let wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    assert_eq!(wal.read(1).unwrap().frame.protobuf, b"gap-frame");
}
