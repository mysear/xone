//! Offline approval boundary: emit a validated observation and signing message.
//! This program never signs, broadcasts, changes configuration or reads an RPC.
use serde::Deserialize;
use std::io::Read;
use xone_domain::{err, native_valuation::PricePolicy, Result};
use xone_execution_preparation::{
    market_feed::{unsigned_observation, MarketRead},
    native_valuation::{signing_message, PriceAnchor},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    policy: PricePolicy,
    anchor: PriceAnchor,
    market_feed: MarketRead,
    valid_until_ms: u64,
}
fn run() -> Result<()> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("NATIVE_VALUATION_MARKET_INPUT"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(err("NATIVE_VALUATION_MARKET_INPUT_LIMIT"));
    }
    let c: Candidate =
        serde_json::from_slice(&bytes).map_err(|_| err("NATIVE_VALUATION_MARKET_INPUT"))?;
    let o = unsigned_observation(&c.policy, c.anchor, c.market_feed, c.valid_until_ms)?;
    println!(
        "{}",
        serde_json::json!({"observation":o,"signing_message_hex":hex::encode(signing_message(&o)?),
        "authorized":false,"productionVerified":false,"publicTransactionsSubmitted":false})
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", serde_json::json!({"error":e.0}));
        std::process::exit(1);
    }
}
