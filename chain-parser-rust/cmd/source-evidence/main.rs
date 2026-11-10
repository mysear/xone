//! Finite read-only real Yellowstone capture, with raw WAL before projection.
//! Never changes an online store or grants readiness from a captured stream.
use serde::Deserialize;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use xone_solana_ingest::{
    async_frame_wal::StreamWalWriter,
    stream::{
        project_yellowstone, FrameWal, FrameWalLimits, StreamLimits, PROJECTION_SCHEMA_COMMIT,
    },
    yellowstone::{Subscription, TransportConfig},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    chain_key: String,
    endpoint: String,
    source_id: String,
    token_file: Option<PathBuf>,
    wal_path: PathBuf,
    frame_limit: usize,
    max_frame_bytes: usize,
    max_wal_bytes: usize,
    max_wal_records: usize,
    connect_timeout_ms: u64,
    idle_timeout_ms: u64,
    allow_loopback_plaintext: bool,
    proto_commit: String,
}
fn load(path: &Path) -> Result<Config, String> {
    let raw = std::fs::read(path).map_err(|_| "SOURCE_CAPTURE_CONFIG_READ")?;
    if raw.len() > 65536 {
        return Err("SOURCE_CAPTURE_CONFIG_LIMIT".into());
    }
    let mut c: Config =
        serde_json::from_slice(&raw).map_err(|_| "SOURCE_CAPTURE_CONFIG_INVALID")?;
    if !c.chain_key.starts_with("solana:")
        || c.chain_key.len() <= 7
        || c.proto_commit != PROJECTION_SCHEMA_COMMIT
        || c.frame_limit == 0
        || c.frame_limit > c.max_wal_records
        || c.max_wal_records > 100000
        || c.max_wal_bytes > 1024 * 1024 * 1024
        || c.max_wal_bytes < c.max_frame_bytes
    {
        return Err("SOURCE_CAPTURE_CONFIG_CONSTRAINT".into());
    }
    let root = path.parent().ok_or("SOURCE_CAPTURE_CONFIG_PARENT")?;
    if c.wal_path.is_relative() {
        c.wal_path = root.join(c.wal_path)
    };
    if let Some(p) = c.token_file.as_mut() {
        if p.is_relative() {
            *p = root.join(&p)
        }
    }
    transport(&c).validate().map_err(|e| e.0)?;
    Ok(c)
}
fn transport(c: &Config) -> TransportConfig {
    TransportConfig {
        endpoint: c.endpoint.clone(),
        source_id: c.source_id.clone(),
        connect_timeout_ms: c.connect_timeout_ms,
        idle_timeout_ms: c.idle_timeout_ms,
        max_frame_bytes: c.max_frame_bytes,
        allow_loopback_plaintext: c.allow_loopback_plaintext,
    }
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 && args.len() != 4
        || args.get(1).map(String::as_str) != Some("--config")
        || args.len() == 4 && args[3] != "--check-config"
    {
        return Err("usage: source-evidence-rust --config FILE [--check-config]".into());
    }
    let c = load(Path::new(&args[2]))?;
    if args.len() == 4 {
        println!(
            "{}",
            json!({"valid":true,"scope":"finite-read-only-raw-stream-capture","ready":false})
        );
        return Ok(());
    }
    let token = if let Some(path) = &c.token_file {
        let meta = std::fs::symlink_metadata(path).map_err(|_| "SOURCE_CAPTURE_TOKEN_READ")?;
        if !meta.is_file() || meta.len() > 4096 {
            return Err("SOURCE_CAPTURE_TOKEN_PERMISSIONS".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err("SOURCE_CAPTURE_TOKEN_PERMISSIONS".into());
            }
        }
        Some(
            std::fs::read_to_string(path)
                .map_err(|_| "SOURCE_CAPTURE_TOKEN_READ")?
                .trim()
                .to_owned(),
        )
    } else {
        None
    };
    let wal = FrameWal::open(
        &c.wal_path,
        &c.chain_key,
        FrameWalLimits {
            max_frame_bytes: c.max_frame_bytes,
            max_total_bytes: c.max_wal_bytes,
            max_records: c.max_wal_records,
        },
    )
    .map_err(|e| e.0)?;
    let session = format!(
        "capture-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "SOURCE_CAPTURE_CLOCK")?
            .as_nanos()
    );
    let generation = wal.generation();
    let writer = StreamWalWriter::open(wal, c.idle_timeout_ms).map_err(|e| e.0)?;
    let mut subscription = Subscription::connect(transport(&c), session, token.as_deref(), None)
        .await
        .map_err(|e| e.0)?;
    let mut captured = 0;
    let mut last_cursor = None;
    while captured < c.frame_limit {
        let record = subscription.capture_next(&writer).await.map_err(|e| e.0)?;
        // Parser errors leave the exact source frame and cursor in durable WAL.
        project_yellowstone(
            &record,
            StreamLimits {
                max_bytes: c.max_frame_bytes,
                max_slots: 32,
                max_updates: 100000,
            },
        )
        .map_err(|e| e.0)?;
        last_cursor = Some(record.cursor);
        captured += 1;
    }
    writer.close().await.map_err(|e| e.0)?;
    println!(
        "{}",
        json!({"passed":true,"scope":"finite-captured-protobuf-frames-only","chain_key":c.chain_key,"frames":captured,"wal_generation":generation,"last_sequence":last_cursor.map(|c|c.sequence),"chain_identity_verified":false,"account_bank_completeness_verified":false,"ready":false,"r03_complete":false})
    );
    Ok(())
}
