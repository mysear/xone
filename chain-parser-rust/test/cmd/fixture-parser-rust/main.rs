//! Explicit offline debug entry. The production binary never reads synthetic samples.
use std::{path::PathBuf, sync::Arc};
use xone_domain::{err, Result, Snapshot};
use xone_solana_ingest::service::{serve, Parser};
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{}", e.0);
        std::process::exit(1);
    }
}
async fn run() -> Result<()> {
    let mut config = None;
    let mut fixtures = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = args.next(),
            "--fixtures" => fixtures = args.next().map(PathBuf::from),
            _ => return Err(err("DEBUG_ARGUMENT_INVALID")),
        }
    }
    let cfg = xone_config::load(
        config.ok_or_else(|| err("CONFIG_ARGUMENT_REQUIRED"))?,
        "chain-parser-rust",
    )?;
    let directory = fixtures.ok_or_else(|| err("DEBUG_FIXTURES_REQUIRED"))?;
    let limit = cfg.config.journal.max_payload_bytes;
    let source = Arc::new(move |chain: &xone_config::Chain| -> Result<Snapshot> {
        let file = directory.join(format!("{}.json", chain.key.replace(':', "-")));
        let file = if file.exists() {
            file
        } else {
            directory
                .join(chain.key.split(':').next().unwrap_or(""))
                .join("baseline.json")
        };
        let raw = std::fs::read(file).map_err(|_| err("DEBUG_SAMPLE_UNREADABLE"))?;
        if raw.len() > limit {
            return Err(err("DEBUG_SAMPLE_SIZE"));
        }
        serde_json::from_slice(&raw).map_err(|_| err("DEBUG_SAMPLE_SCHEMA"))
    });
    serve(Parser::with_source(cfg, Some(source))?).await
}
