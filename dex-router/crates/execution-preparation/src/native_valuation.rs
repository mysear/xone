//! Authenticated atomic price ratios for already measured transaction costs.
//! An authority signature identifies an approved source; it does not attest
//! bank consensus, source independence, or completeness of the whole quote.
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use xone_domain::{
    err, hash,
    native_valuation::{block_hash, canonical_hex, PricePolicy},
    Amount, Result,
};

pub const VERSION: &str = "xone-authenticated-native-valuation/1";
const DOMAIN: &str = "xone-native-token-price/1:";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PriceAnchor {
    pub chain_key: String,
    pub genesis_hash: String,
    pub block_number: u64,
    pub block_hash: String,
    pub parent_hash: String,
    pub timestamp_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceObservation {
    pub schema_version: u32,
    pub policy_hash: String,
    pub anchor: PriceAnchor,
    pub token: String,
    /// Token atoms per native atom; neither decimal scale is implicit.
    pub token_atoms: Amount,
    pub native_atoms: Amount,
    pub observed_at_ms: u64,
    pub valid_until_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_feed: Option<crate::market_feed::MarketRead>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceSignature {
    pub key_id: String,
    pub signature: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPrice {
    pub observation: PriceObservation,
    pub signatures: Vec<PriceSignature>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceBook {
    pub schema_version: u32,
    pub prices: Vec<SignedPrice>,
}
pub fn signing_message(observation: &PriceObservation) -> Result<Vec<u8>> {
    Ok(format!("{DOMAIN}{}", hash(observation)?).into_bytes())
}

// Constructed only after authorization. Request data cannot construct a rate
// that conversion functions would accidentally treat as authenticated.
#[derive(Clone, Debug)]
pub struct AuthorizedPrice {
    observation: PriceObservation,
    policy: PricePolicy,
    signers: Vec<String>,
    observation_hash: String,
    market_evidence: Option<Value>,
    market_valid_until_ms: Option<u64>,
}
impl SignedPrice {
    pub fn authorize(&self, policy: &PricePolicy) -> Result<AuthorizedPrice> {
        let bad = || err("NATIVE_VALUATION_AUTHORIZATION");
        let o = &self.observation;
        if o.schema_version != 1
            || o.policy_hash != policy.identity()?
            || o.anchor.chain_key != policy.chain_key
            || o.anchor.genesis_hash != policy.genesis_hash
            || o.token != policy.token
            || !block_hash(&o.anchor.block_hash)
            || !block_hash(&o.anchor.parent_hash)
            || o.anchor.timestamp_ms == 0
            || o.token_atoms == Amount::zero()
            || o.native_atoms == Amount::zero()
            || o.token_atoms.to_be_bytes().is_err()
            || o.native_atoms.to_be_bytes().is_err()
            || o.valid_until_ms <= o.observed_at_ms
            || o.valid_until_ms - o.observed_at_ms > policy.max_validity_ms
            || self.signatures.len() < policy.threshold
            || self.signatures.len() > policy.signers.len()
        {
            return Err(bad());
        }
        let market = match (&policy.market_feed, &o.market_feed) {
            (None, None) => None,
            (Some(_), Some(read)) => {
                let d = crate::market_feed::derive(policy, &o.anchor, read)?;
                if d.token_atoms != o.token_atoms
                    || d.native_atoms != o.native_atoms
                    || o.observed_at_ms != o.anchor.timestamp_ms
                    || o.valid_until_ms > d.valid_until_ms
                    || o.valid_until_ms <= read.source_read_at_ms
                {
                    return Err(bad());
                }
                Some(d)
            }
            _ => return Err(bad()),
        };
        let message = signing_message(o)?;
        let mut seen = BTreeSet::new();
        for signature in &self.signatures {
            if !seen.insert(signature.key_id.clone()) || !canonical_hex(&signature.signature, 64) {
                return Err(bad());
            }
            let key = policy.signers.get(&signature.key_id).ok_or_else(bad)?;
            let key = hex::decode(key).map_err(|_| bad())?;
            let signature = hex::decode(&signature.signature).map_err(|_| bad())?;
            UnparsedPublicKey::new(&ED25519, key)
                .verify(&message, &signature)
                .map_err(|_| bad())?;
        }
        Ok(AuthorizedPrice {
            observation: o.clone(),
            policy: policy.clone(),
            signers: seen.into_iter().collect(),
            observation_hash: hash(o)?,
            market_valid_until_ms: market.as_ref().map(|m| m.valid_until_ms),
            market_evidence: market.map(|m| m.evidence),
        })
    }
}
impl AuthorizedPrice {
    pub fn validate_at(&self, anchor: &PriceAnchor, execution_timestamp_ms: u64) -> Result<()> {
        let o = &self.observation;
        if &o.anchor != anchor || execution_timestamp_ms < anchor.timestamp_ms {
            return Err(err("NATIVE_VALUATION_ANCHOR"));
        }
        if o.observed_at_ms > anchor.timestamp_ms
            || anchor.timestamp_ms - o.observed_at_ms > self.policy.max_age_ms
            || execution_timestamp_ms < o.observed_at_ms
            || execution_timestamp_ms - o.observed_at_ms > self.policy.max_age_ms
            || execution_timestamp_ms >= o.valid_until_ms
            || self
                .market_valid_until_ms
                .is_some_and(|until| execution_timestamp_ms >= until)
        {
            return Err(err("NATIVE_VALUATION_TIME"));
        }
        Ok(())
    }
    /// Keep the 512-bit product until division; round cost up once, including
    /// dust. Only the final representable token amount is limited to uint256.
    pub fn convert_cost(&self, cost: &Amount, anchor: &PriceAnchor, at_ms: u64) -> Result<Amount> {
        self.validate_at(anchor, at_ms)?;
        cost.to_be_bytes()?;
        let o = &self.observation;
        let product = &cost.0 * &o.token_atoms.0;
        let remainder = &product % &o.native_atoms.0;
        let mut result = product / &o.native_atoms.0;
        if remainder != Amount::zero().0 {
            result += 1u8;
        }
        Amount::checked(result)
    }
    pub fn evidence(&self) -> Value {
        let mut evidence = json!({"modelVersion":VERSION,"authority":"CONFIGURED_ORACLE_SIGNER_QUORUM",
            "sourceId":self.policy.source_id,"policyHash":self.observation.policy_hash,
            "observationHash":self.observation_hash,"anchor":self.observation.anchor,
            "token":self.observation.token,"tokenAtoms":self.observation.token_atoms,"nativeAtoms":self.observation.native_atoms,
            "observedAtMs":self.observation.observed_at_ms,"validUntilMs":self.observation.valid_until_ms,
            "signers":self.signers,"threshold":self.policy.threshold,"costRounding":"CEIL_ONCE_AFTER_FULL_WIDTH_PRODUCT",
            "priceTimePolicy":"EXECUTION_BLOCK_TIME_NOT_CURRENT_WALL_CLOCK",
            "bankConsensusAttested":false,"sourceIndependenceAttested":false});
        if let Some(market) = &self.market_evidence {
            evidence["marketFeed"] = market.clone();
        }
        evidence
    }
    pub fn token(&self) -> &str {
        &self.observation.token
    }
}
impl PriceBook {
    pub fn authorize(&self, policies: &[PricePolicy]) -> Result<Vec<AuthorizedPrice>> {
        if self.schema_version != 1 || self.prices.is_empty() || self.prices.len() > 128 {
            return Err(err("NATIVE_VALUATION_BOOK"));
        }
        let mut seen = BTreeSet::new();
        self.prices
            .iter()
            .map(|price| {
                let o = &price.observation;
                // A pinned local bank gets one observation per authorized pair.
                if !seen.insert((&o.anchor.chain_key, &o.anchor.genesis_hash, &o.token)) {
                    return Err(err("NATIVE_VALUATION_BOOK"));
                }
                let mut matching = policies.iter().filter(|p| {
                    p.chain_key == o.anchor.chain_key
                        && p.genesis_hash == o.anchor.genesis_hash
                        && p.token == o.token
                });
                let policy = matching
                    .next()
                    .ok_or_else(|| err("NATIVE_VALUATION_AUTHORIZATION"))?;
                if matching.next().is_some() {
                    return Err(err("NATIVE_VALUATION_POLICY"));
                }
                price.authorize(policy)
            })
            .collect()
    }
}

/// This result covers one executed transaction and its audited payer debit.
/// Additional approvals/funding and candidate ranking remain separate scopes.
pub struct Settlement<'a> {
    pub input: &'a Amount,
    pub output: &'a Amount,
    pub sell_token: &'a str,
    pub buy_token: &'a str,
    pub debit: &'a Amount,
    pub anchor: &'a PriceAnchor,
    pub at_ms: u64,
}
pub fn evaluated_amounts(
    settlement: Settlement<'_>,
    sell: Option<&AuthorizedPrice>,
    buy: Option<&AuthorizedPrice>,
) -> Result<Value> {
    let Settlement {
        input,
        output,
        sell_token,
        buy_token,
        debit,
        anchor,
        at_ms,
    } = settlement;
    if sell.is_some_and(|p| p.token() != sell_token) || buy.is_some_and(|p| p.token() != buy_token)
    {
        return Err(err("NATIVE_VALUATION_TOKEN"));
    }
    input.to_be_bytes()?;
    output.to_be_bytes()?;
    debit.to_be_bytes()?;
    let mut result = json!({"modelVersion":VERSION,"scope":"EVALUATED_TRANSACTION_NETWORK_FEES_ONLY",
        "costBasis":"SOURCE_AUDITED_PAYER_NET_NATIVE_DEBIT","payerNetNativeDebit":debit,
        "actualInput":input,"actualOutput":output,"valuationTimestampMs":at_ms,
        "feeAdjustedBuyAmount":Value::Null,"feeAdjustedSellAmount":Value::Null,
        "buyUnavailableReason":"AUTHORIZED_TOKEN_PRICE_UNAVAILABLE","sellUnavailableReason":"AUTHORIZED_TOKEN_PRICE_UNAVAILABLE",
        "quoteRankingApplied":false,"additionalOperationsIncluded":false,"deductedFromQuoteOutput":false,"productionVerified":false});
    for (price, output_side) in [(buy, true), (sell, false)] {
        let Some(price) = price else {
            continue;
        };
        let side = if output_side { "buy" } else { "sell" };
        match price.convert_cost(debit, anchor, at_ms) {
            Ok(cost) => {
                result[format!("{side}Price")] = price.evidence();
                result[format!("{side}TokenFee")] =
                    serde_json::to_value(&cost).map_err(|_| err("NATIVE_VALUATION_RESULT"))?;
                let net = if output_side {
                    if output < &cost {
                        Err(err("NETWORK_FEE_EXCEEDS_GROSS_OUTPUT"))
                    } else {
                        Amount::checked(&output.0 - &cost.0)
                    }
                } else {
                    Amount::checked(&input.0 + &cost.0)
                };
                match net {
                    Ok(net) => {
                        result[if output_side {
                            "feeAdjustedBuyAmount"
                        } else {
                            "feeAdjustedSellAmount"
                        }] = serde_json::to_value(net)
                            .map_err(|_| err("NATIVE_VALUATION_RESULT"))?;
                        result[format!("{side}UnavailableReason")] = Value::Null;
                    }
                    Err(error) => result[format!("{side}UnavailableReason")] = error.0.into(),
                }
            }
            Err(error) => result[format!("{side}UnavailableReason")] = error.0.into(),
        }
    }
    Ok(result)
}
