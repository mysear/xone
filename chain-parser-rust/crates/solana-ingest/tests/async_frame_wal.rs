use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use xone_solana_ingest::{
    async_frame_wal::StreamWalWriter,
    stream::{FrameWal, FrameWalLimits, RawFrame, SourceSession},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "xone-async-frame-wal-{}-{unique}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
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
        max_frame_bytes: 1024,
        max_total_bytes: 10000,
        max_records: 10,
    }
}
fn frame(sequence: u64) -> RawFrame {
    RawFrame {
        source: SourceSession {
            source_id: "test-provider".into(),
            session_id: "test-session".into(),
        },
        sequence,
        receive_timestamp_ms: 123,
        protobuf: vec![0, 255, sequence as u8],
    }
}

#[tokio::test(flavor = "current_thread")]
async fn acknowledged_receipt_is_exact_durable_and_close_releases_the_wal_lock() {
    let dir = Temp::new();
    let wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    let generation = wal.generation();
    let writer = StreamWalWriter::open(wal, 1000).unwrap();
    let captured = writer.capture(frame(1)).await.unwrap();
    assert_eq!(captured.frame, frame(1));
    assert_eq!(captured.wal_generation, generation);
    assert_eq!(captured.cursor.sequence, 1);
    writer.close().await.unwrap();
    assert!(writer.is_sealed());
    let wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    let restored = wal.read(1).unwrap();
    assert_eq!(restored.frame, captured.frame);
    assert_eq!(restored.cursor, captured.cursor);
    assert!(wal.generation() > generation);
}

#[tokio::test(flavor = "current_thread")]
async fn capacity_error_seals_and_rejects_every_following_frame() {
    let dir = Temp::new();
    let limits = FrameWalLimits {
        max_records: 1,
        ..limits()
    };
    let wal = FrameWal::open(&dir.0, "solana:test", limits).unwrap();
    let writer = StreamWalWriter::open(wal, 1000).unwrap();
    writer.capture(frame(1)).await.unwrap();
    assert_eq!(
        writer.capture(frame(2)).await.unwrap_err().0,
        "STREAM_WAL_CAPACITY"
    );
    assert!(writer.is_sealed());
    assert_eq!(
        writer.capture(frame(3)).await.unwrap_err().0,
        "STREAM_WAL_WRITER_SEALED"
    );
    writer.close().await.unwrap();
    let wal = FrameWal::open(&dir.0, "solana:test", limits).unwrap();
    assert_eq!(wal.len(), 1);
    assert_eq!(wal.read(1).unwrap().frame, frame(1));
}

#[tokio::test(flavor = "current_thread")]
async fn dropping_an_idle_writer_is_nonblocking_and_eventually_releases_the_lock() {
    let dir = Temp::new();
    let wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    let writer = StreamWalWriter::open(wal, 1000).unwrap();
    let start = Instant::now();
    drop(writer);
    assert!(start.elapsed() < Duration::from_millis(100));
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(wal) = FrameWal::open(&dir.0, "solana:test", limits()) {
                drop(wal);
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn committed_checkpoint_uses_the_capture_writer_and_survives_reopen() {
    let dir = Temp::new();
    let writer = StreamWalWriter::open(
        FrameWal::open(&dir.0, "solana:test", limits()).unwrap(),
        1000,
    )
    .unwrap();
    let first = writer.capture(frame(1)).await.unwrap();
    writer.capture(frame(2)).await.unwrap();
    writer.checkpoint(first.cursor.clone()).await.unwrap();
    writer.close().await.unwrap();
    let wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    assert_eq!(wal.recovery_cursor(), first.cursor);
    let mut pending = vec![];
    wal.replay_from(&first.cursor, |r| {
        pending.push(r.frame.sequence);
        Ok(())
    })
    .unwrap();
    assert_eq!(pending, vec![2]);
}
#[tokio::test(flavor = "current_thread")]
async fn invalid_checkpoint_seals_the_same_writer_without_advancing_recovery() {
    let dir = Temp::new();
    let writer = StreamWalWriter::open(
        FrameWal::open(&dir.0, "solana:test", limits()).unwrap(),
        1000,
    )
    .unwrap();
    let receipt = writer.capture(frame(1)).await.unwrap();
    let mut bad = receipt.cursor;
    bad.record_sha256 = "00".repeat(32);
    assert!(writer.checkpoint(bad).await.is_err());
    assert!(writer.is_sealed());
    writer.close().await.unwrap();
    let wal = FrameWal::open(&dir.0, "solana:test", limits()).unwrap();
    assert_eq!(wal.recovery_cursor().sequence, 0);
}
