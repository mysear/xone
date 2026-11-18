//! Approved Ethereum AggregatorV3 USD pairs. RPC returns are attested by the
//! configured signers, not authenticated account/storage or consensus proofs.
use crate::native_valuation::{PriceAnchor, PriceObservation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use xone_domain::{
    err, hash,
    native_valuation::{block_hash, canonical_hex, ChainlinkFeedPolicy, PricePolicy},
    Amount, Result,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedRead {
    pub proxy_code: String,
    pub aggregator_code: String,
    pub aggregator_return: String,
    pub decimals_return: String,
    pub description_return: String,
    pub round_return: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketRead {
    pub schema_version: u32,
    pub provider_id: String,
    pub source_read_at_ms: u64,
    pub token_decimals_return: String,
    pub native_usd: FeedRead,
    pub token_usd: FeedRead,
}
fn bad() -> xone_domain::Error {
    err("NATIVE_VALUATION_MARKET_EVIDENCE")
}
fn bytes(s: &str, max: usize) -> Result<Vec<u8>> {
    let s = s.strip_prefix("0x").ok_or_else(bad)?;
    if s.len() > max * 2 || !canonical_hex(s, s.len() / 2) {
        return Err(bad());
    }
    hex::decode(s).map_err(|_| bad())
}
fn words(s: &str, count: usize) -> Result<Vec<Amount>> {
    let raw = bytes(s, count * 32)?;
    if raw.len() != count * 32 {
        return Err(bad());
    }
    raw.chunks_exact(32).map(Amount::from_be_bytes).collect()
}
fn scalar(s: &str, value: u64) -> Result<()> {
    if words(s, 1)?[0] != Amount(value.into()) {
        return Err(bad());
    }
    Ok(())
}
fn timestamp(value: &Amount) -> Result<u64> {
    value
        .to_string()
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(1000))
        .ok_or_else(bad)
}
struct Decoded {
    answer: Amount,
    expires_ms: u64,
    evidence: Value,
}
fn decode(f: &ChainlinkFeedPolicy, r: &FeedRead, at_ms: u64) -> Result<Decoded> {
    for (code, expected) in [
        (&r.proxy_code, &f.proxy_code_sha256),
        (&r.aggregator_code, &f.aggregator_code_sha256),
    ] {
        let raw = bytes(code, 24_576)?;
        if raw.is_empty() || hex::encode(Sha256::digest(raw)) != *expected {
            return Err(bad());
        }
    }
    let address = bytes(&r.aggregator_return, 32)?;
    if address.len() != 32
        || address[..12] != [0; 12]
        || format!("0x{}", hex::encode(&address[12..])) != f.aggregator
    {
        return Err(bad());
    }
    scalar(&r.decimals_return, u64::from(f.decimals))?;
    let description = bytes(&r.description_return, 192)?;
    let mut expected = vec![0u8; 64];
    expected[31] = 32;
    expected[63] = f.description.len() as u8;
    expected.extend(f.description.as_bytes());
    expected.resize(64 + f.description.len().div_ceil(32) * 32, 0);
    if description != expected {
        return Err(bad());
    }
    let w = words(&r.round_return, 5)?;
    let started = timestamp(&w[2])?;
    let updated = timestamp(&w[3])?;
    let expires_ms = updated.checked_add(f.max_age_ms).ok_or_else(bad)?;
    // Equality is our conservative admitted profile, not a universal rule for
    // every AggregatorV3 implementation (some have different round semantics).
    if w[0] == Amount::zero()
        || w[0].0.bits() > 80
        || w[4] != w[0]
        || w[1] < f.min_answer
        || w[1] > f.max_answer
        || w[1].0.bits() > 255
        || started == 0
        || started > updated
        || updated > at_ms
        || at_ms >= expires_ms
    {
        return Err(bad());
    }
    Ok(Decoded {
        answer: w[1].clone(),
        expires_ms,
        evidence: json!({"proxy":f.proxy,"proxyCodeSha256":f.proxy_code_sha256,
            "aggregator":f.aggregator,"aggregatorCodeSha256":f.aggregator_code_sha256,
            "description":f.description,"decimals":f.decimals,"roundId":w[0],"answer":w[1],
            "startedAtMs":started,"updatedAtMs":updated,"maxAgeMs":f.max_age_ms,"validUntilMs":expires_ms}),
    })
}

pub struct DerivedMarketPrice {
    pub token_atoms: Amount,
    pub native_atoms: Amount,
    pub valid_until_ms: u64,
    pub evidence: Value,
}
/// Recompute both unit scales with full-width intermediates. Reduce before
/// uint256 admission; never round an intermediate feed ratio.
pub fn derive(
    policy: &PricePolicy,
    anchor: &PriceAnchor,
    read: &MarketRead,
) -> Result<DerivedMarketPrice> {
    policy.validate()?;
    let p = policy.market_feed.as_ref().ok_or_else(bad)?;
    if anchor.chain_key != policy.chain_key
        || anchor.genesis_hash != policy.genesis_hash
        || anchor.timestamp_ms == 0
        || !block_hash(&anchor.block_hash)
        || !block_hash(&anchor.parent_hash)
        || read.schema_version != 1
        || read.provider_id.len() != 68
        || !read.provider_id.starts_with("rpc-")
        || !canonical_hex(&read.provider_id[4..], 32)
        || read.source_read_at_ms < anchor.timestamp_ms
        || read.source_read_at_ms - anchor.timestamp_ms > policy.max_age_ms
    {
        return Err(bad());
    }
    scalar(&read.token_decimals_return, u64::from(p.token_decimals))?;
    let native = decode(&p.native_usd, &read.native_usd, anchor.timestamp_ms)?;
    let token = decode(&p.token_usd, &read.token_usd, anchor.timestamp_ms)?;
    let ten = Amount(10u64.into()).0;
    let numerator =
        native.answer.0 * ten.pow(u32::from(p.token_usd.decimals) + u32::from(p.token_decimals));
    let denominator =
        token.answer.0 * ten.pow(u32::from(p.native_usd.decimals) + u32::from(p.native_decimals));
    let (mut a, mut b) = (numerator.clone(), denominator.clone());
    while b != Amount::zero().0 {
        let next = &a % &b;
        a = b;
        b = next;
    }
    let token_atoms = Amount::checked(numerator / &a)?;
    let native_atoms = Amount::checked(denominator / a)?;
    let valid_until_ms = native.expires_ms.min(token.expires_ms);
    if read.source_read_at_ms >= valid_until_ms {
        return Err(bad());
    }
    Ok(DerivedMarketPrice {
        token_atoms,
        native_atoms,
        valid_until_ms,
        evidence: json!({"modelVersion":"xone-approved-chainlink-usd-pair/1",
            "authority":"CONFIGURED_SIGNERS_ATTEST_FIXED_BLOCK_RPC_READS",
            "rpcBlockPolicy":"EIP1898_BLOCK_HASH_REQUIRE_CANONICAL",
            "marketEvidenceHash":hash(read)?,"providerId":read.provider_id,"sourceReadAtMs":read.source_read_at_ms,
            "nativeDecimals":p.native_decimals,"tokenDecimals":p.token_decimals,
            "nativeUsd":native.evidence,"tokenUsd":token.evidence,
            "storageProofVerified":false,"bankConsensusAttested":false,
            "sourceIndependenceAttested":false,"sequencerSafetySupported":false}),
    })
}
pub fn unsigned_observation(
    policy: &PricePolicy,
    anchor: PriceAnchor,
    read: MarketRead,
    valid_until_ms: u64,
) -> Result<PriceObservation> {
    let d = derive(policy, &anchor, &read)?;
    if valid_until_ms <= read.source_read_at_ms
        || valid_until_ms > d.valid_until_ms
        || valid_until_ms - anchor.timestamp_ms > policy.max_validity_ms
    {
        return Err(bad());
    }
    Ok(PriceObservation {
        schema_version: 1,
        publication_revision: 0,
        policy_hash: policy.identity()?,
        token: policy.token.clone(),
        token_atoms: d.token_atoms,
        native_atoms: d.native_atoms,
        observed_at_ms: anchor.timestamp_ms,
        valid_until_ms,
        anchor,
        market_feed: Some(read),
    })
}
