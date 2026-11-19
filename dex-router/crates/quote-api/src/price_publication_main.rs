//! Offline journal bootstrap/advance/check; never signs or submits a transaction.
use serde::Deserialize;
use std::{io::Read, path::PathBuf};
use xone_domain::{err, native_valuation::NativeValuationConfig, Result};
use xone_execution_preparation::native_valuation::{PriceAnchor, PriceBook};
use xone_quote_api::price_publication::{AuthorizedPublication, Journal};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    policy: NativeValuationConfig,
    book: PriceBook,
    anchor: PriceAnchor,
    execution_timestamp_ms: u64,
}
fn run() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(err("PRICE_PUBLICATION_CLI_ARGUMENTS"));
    }
    let mode = args[0]
        .to_str()
        .ok_or_else(|| err("PRICE_PUBLICATION_CLI_ARGUMENTS"))?;
    if !["initialize", "advance", "check", "preview"].contains(&mode) {
        return Err(err("PRICE_PUBLICATION_CLI_ARGUMENTS"));
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("PRICE_PUBLICATION_CLI_INPUT"))?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(err("PRICE_PUBLICATION_CLI_INPUT_LIMIT"));
    }
    let input: Input =
        serde_json::from_slice(&bytes).map_err(|_| err("PRICE_PUBLICATION_CLI_INPUT"))?;
    let publication = AuthorizedPublication::authorize(
        &input.policy,
        &input.book,
        &input.anchor,
        input.execution_timestamp_ms,
    )?;
    let path = PathBuf::from(&args[1]);
    let mut journal = if mode == "initialize" {
        Journal::initialize(&path, &input.policy, &publication)?
    } else {
        Journal::open(&path, &input.policy)?
    };
    if mode == "preview" {
        if !journal.would_advance(&publication)? {
            return Err(err("PRICE_PUBLICATION_ALREADY_CURRENT"));
        }
        println!(
            "{}",
            serde_json::json!({"wouldAdvance":true,"published":false,
            "bookHash":publication.high_water().book_hash(),"revision":publication.high_water().revision()})
        );
        return Ok(());
    }
    let receipt = if mode == "advance" {
        journal.advance(&publication)?
    } else {
        let mut confirmed = journal.check(&publication)?;
        confirmed.changed = mode == "initialize";
        confirmed
    };
    println!(
        "{}",
        serde_json::json!({"receipt":receipt,"scope":"AUTHORIZED_LOCAL_PRICE_PUBLICATION_JOURNAL_ONLY",
        "normalRouterConnected":false,"productionVerified":false,"publicTransactionsSubmitted":false})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{}", serde_json::json!({"error":error.0}));
        std::process::exit(1);
    }
}
