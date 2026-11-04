use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use xone_solana_ingest::source_wal::{SourceWal, WalLimits};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn directory() -> PathBuf {
    std::env::temp_dir().join(format!(
        "xone-source-wal-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn limits() -> WalLimits {
    WalLimits {
        max_response_bytes: 4096,
        max_records: 8,
    }
}
#[test]
fn raw_response_roundtrips_exact_bytes_identity_and_durable_session() {
    let path = directory();
    let raw=b"{ \"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"context\":{\"slot\":123},\"value\":[null]} }\n";
    {
        let mut wal = SourceWal::open(&path, "solana:localnet", limits()).unwrap();
        let first = wal
            .append(
                "https://rpc.invalid/?token=secret",
                "getMultipleAccounts",
                &json!([["key"],{"commitment":"finalized"}]),
                raw,
                1000,
            )
            .unwrap();
        assert_eq!(first, 1);
        let record = wal.read(1).unwrap();
        assert_eq!(record.response, raw);
        assert_eq!(record.chain_key, "solana:localnet");
        assert_eq!(record.session_id, 1);
        assert_eq!(record.receive_timestamp_ms, 1000);
        assert_eq!(record.method, "getMultipleAccounts");
        assert_eq!(record.params[1]["commitment"], "finalized");
        assert!(!format!("{:?}", record).contains("secret"));
        assert_eq!(
            wal.append(
                "https://rpc.invalid/?token=secret",
                "getMultipleAccounts",
                &record.params,
                raw,
                1001
            )
            .unwrap(),
            1
        );
        assert_eq!(wal.len(), 1);
    }
    {
        let mut wal = SourceWal::open(&path, "solana:localnet", limits()).unwrap();
        assert_eq!(wal.read(1).unwrap().response, raw);
        assert_eq!(
            wal.append(
                "https://rpc.invalid/?token=secret",
                "getMultipleAccounts",
                &json!([["key"],{"commitment":"finalized"}]),
                raw,
                1002
            )
            .unwrap(),
            2
        );
        assert_eq!(wal.read(2).unwrap().session_id, 2);
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn wal_limits_are_fail_closed_and_do_not_advance_sequence() {
    let path = directory();
    {
        let mut wal = SourceWal::open(
            &path,
            "solana:localnet",
            WalLimits {
                max_response_bytes: 64,
                max_records: 2,
            },
        )
        .unwrap();
        assert!(wal
            .append("rpc", "getBlock", &json!([]), &[0; 65], 1)
            .is_err());
        assert_eq!(wal.len(), 0);
        assert_eq!(
            wal.append("rpc", "getBlock", &json!([1]), b"one", 1)
                .unwrap(),
            1
        );
        assert_eq!(
            wal.append("rpc", "getBlock", &json!([2]), b"two", 2)
                .unwrap(),
            2
        );
        assert_eq!(
            wal.append("rpc", "getBlock", &json!([3]), b"three", 3)
                .unwrap_err()
                .0,
            "SOURCE_WAL_CAPACITY"
        );
        assert_eq!(wal.len(), 2);
        assert!(wal.read(3).is_err());
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn wal_rejects_wrong_chain_concurrent_writer_and_payload_corruption() {
    let path = directory();
    {
        let mut wal = SourceWal::open(&path, "solana:localnet", limits()).unwrap();
        wal.append("rpc", "getBlock", &json!([1]), b"original", 1)
            .unwrap();
        assert!(SourceWal::open(&path, "solana:localnet", limits()).is_err());
    }
    assert_eq!(
        SourceWal::open(&path, "solana:other", limits())
            .err()
            .unwrap()
            .0,
        "SOURCE_WAL_IDENTITY"
    );
    {
        let db = rocksdb::DB::open_default(&path).unwrap();
        let mut bytes = db.get(b"record/00000000000000000001").unwrap().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        db.put(b"record/00000000000000000001", bytes).unwrap();
    }
    assert_eq!(
        SourceWal::open(&path, "solana:localnet", limits())
            .err()
            .unwrap()
            .0,
        "SOURCE_WAL_CORRUPT"
    );
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn wal_rejects_missing_record_or_uncommitted_extra_record() {
    for extra in [false, true] {
        let path = directory();
        {
            let mut wal = SourceWal::open(&path, "solana:localnet", limits()).unwrap();
            wal.append("rpc", "getBlock", &json!([1]), b"one", 1)
                .unwrap();
            wal.append("rpc", "getBlock", &json!([2]), b"two", 2)
                .unwrap();
        }
        {
            let db = rocksdb::DB::open_default(&path).unwrap();
            if extra {
                let b = db.get(b"record/00000000000000000002").unwrap().unwrap();
                db.put(b"record/00000000000000000003", b).unwrap();
            } else {
                db.delete(b"record/00000000000000000001").unwrap();
            }
        }
        assert_eq!(
            SourceWal::open(&path, "solana:localnet", limits())
                .err()
                .unwrap()
                .0,
            "SOURCE_WAL_CORRUPT"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn failed_sync_commit_poisons_dedup_and_new_writes_until_verified_reopen() {
    let path = directory();
    let limits = limits();
    let saved;
    {
        let mut wal = SourceWal::open(&path, "solana:localnet", limits).unwrap();
        wal.append("rpc", "getBlock", &json!([1]), b"A", 1).unwrap();
        let marker = path.join("SOURCE_HEAD");
        saved = std::fs::read(&marker).unwrap();
        std::fs::remove_file(&marker).unwrap();
        std::fs::create_dir(&marker).unwrap();
        assert_eq!(
            wal.append("rpc", "getBlock", &json!([2]), b"B", 2)
                .unwrap_err()
                .0,
            "SOURCE_WAL_WRITE"
        );
        assert_eq!(
            wal.append("rpc", "getBlock", &json!([1]), b"A", 3)
                .unwrap_err()
                .0,
            "SOURCE_WAL_REOPEN_REQUIRED"
        );
        assert_eq!(
            wal.append("rpc", "getBlock", &json!([3]), b"C", 4)
                .unwrap_err()
                .0,
            "SOURCE_WAL_REOPEN_REQUIRED"
        );
        std::fs::remove_dir(&marker).unwrap();
        std::fs::write(&marker, &saved).unwrap();
    }
    let wal = SourceWal::open(&path, "solana:localnet", limits).unwrap();
    assert_eq!(wal.len(), 2);
    assert_eq!(wal.read(2).unwrap().response, b"B");
    drop(wal);
    std::fs::remove_dir_all(path).unwrap();
}

// Run this helper as a child so a sync-acknowledged DB WAL is not flushed to SST
// during a normal DB destructor before the parent corrupts the underlying file.
#[test]
fn abrupt_writer_child() {
    let Ok(path) = std::env::var("XONE_SOURCE_WAL_CRASH_TEST_PATH") else {
        return;
    };
    let mut wal =
        SourceWal::open(std::path::Path::new(&path), "solana:localnet", limits()).unwrap();
    wal.append("rpc", "getBlock", &json!([1]), b"first-reception", 1)
        .unwrap();
    wal.append("rpc", "getBlock", &json!([2]), b"second-reception", 2)
        .unwrap();
    std::process::exit(0);
}
#[test]
fn underlying_rocksdb_log_corruption_truncation_and_loss_never_accept_an_old_prefix() {
    for damage in ["flip", "truncate", "delete"] {
        let path = directory();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "abrupt_writer_child", "--nocapture"])
            .env("XONE_SOURCE_WAL_CRASH_TEST_PATH", &path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        let logs = std::fs::read_dir(&path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "log"))
            .collect::<Vec<_>>();
        assert!(!logs.is_empty());
        for log in logs {
            let mut raw = std::fs::read(&log).unwrap();
            assert!(raw.len() > 64);
            match damage {
                "flip" => {
                    let at = raw.len() - 16;
                    raw[at] ^= 1;
                    std::fs::write(log, raw).unwrap();
                }
                "truncate" => {
                    raw.truncate(0);
                    std::fs::write(log, raw).unwrap();
                }
                _ => std::fs::remove_file(log).unwrap(),
            }
        }
        assert!(
            SourceWal::open(&path, "solana:localnet", limits()).is_err(),
            "silently accepted {damage}"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn durable_head_session_only_corruption_is_rejected_without_overwriting_evidence() {
    let path = directory();
    {
        let mut wal = SourceWal::open(&path, "solana:localnet", limits()).unwrap();
        wal.append("rpc", "getBlock", &json!([1]), b"A", 1).unwrap();
    }
    let marker = path.join("SOURCE_HEAD");
    let raw = std::fs::read(&marker).unwrap();
    let mut head: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    head["session"] = json!(0);
    let damaged = serde_json::to_vec(&head).unwrap();
    std::fs::write(&marker, &damaged).unwrap();
    assert!(SourceWal::open(&path, "solana:localnet", limits()).is_err());
    assert_eq!(std::fs::read(&marker).unwrap(), damaged);
    std::fs::remove_dir_all(path).unwrap();
}
