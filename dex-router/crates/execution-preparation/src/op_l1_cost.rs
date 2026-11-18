//! Historical OP Regolith data-fee arithmetic. This is not a source verifier,
//! a transaction decoder, an execution adapter, or a token-price oracle.
use serde::Serialize;
use sha2::{Digest, Sha256};
use xone_domain::{err, Amount, Result};

pub const VERSION: &str = "xone-op-regolith-l1-data-cost/1";
pub const UPSTREAM: &str = "79e9d244748a1201fe686ca8a882b5fe7e173348";
const ECOTONE: u64 = 1_710_374_401;

/// Parameters decoded from a Bedrock-format L1 attributes calldata. Fields are
/// private so a parameter set cannot silently acquire a different chain/time.
#[derive(Clone, Debug, Serialize)]
pub struct Parameters {
    chain_key: String,
    execution_timestamp: u64,
    l1_timestamp: u64,
    l1_block_number: u64,
    l1_block_hash: String,
    l1_base_fee: Amount,
    overhead: Amount,
    scalar: Amount,
    attributes_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DataCost {
    pub model_version: &'static str,
    pub upstream_commit: &'static str,
    pub chain_key: String,
    pub execution_timestamp: u64,
    pub l1_block_number: u64,
    pub l1_timestamp: u64,
    pub l1_block_hash: String,
    pub attributes_sha256: String,
    pub encoded_transaction_sha256: String,
    pub encoded_transaction_bytes: usize,
    pub zero_bytes: u64,
    pub nonzero_bytes: u64,
    pub calldata_gas: u64,
    pub overhead_gas: Amount,
    pub l1_gas_used: String,
    pub l1_base_fee: Amount,
    pub scalar: Amount,
    pub scalar_denominator: u64,
    pub l1_data_fee_native: Amount,
}

fn u64_word(word: &[u8]) -> Result<u64> {
    if word.len() != 32 || word[..24].iter().any(|b| *b != 0) {
        return Err(err("OP_L1_ATTRIBUTES_ABI"));
    }
    Ok(u64::from_be_bytes(word[24..].try_into().unwrap()))
}

/// Only the explicitly supported OP/Base Regolith interval is admitted. The
/// first Ecotone block's special case and all later fee models stay closed.
/// The caller must authenticate the deposit identity, block and source; a hash
/// here identifies bytes and never confers canonicality or freshness.
pub fn decode_parameters(chain_key: &str, timestamp: u64, data: &[u8]) -> Result<Parameters> {
    let activation = match chain_key {
        "evm:10" => 1_686_068_903,
        "evm:8453" => 1_686_789_347,
        _ => return Err(err("OP_L1_FEE_CHAIN_UNSUPPORTED")),
    };
    if !(activation..ECOTONE).contains(&timestamp) {
        return Err(err("OP_L1_FEE_ACTIVATION_UNSUPPORTED"));
    }
    if data.len() != 4 + 8 * 32 || data[..4] != [0x01, 0x5d, 0x8e, 0xb9] {
        return Err(err("OP_L1_ATTRIBUTES_ABI"));
    }
    let words = &data[4..];
    let word = |i: usize| &words[i * 32..(i + 1) * 32];
    let l1_block_number = u64_word(word(0))?;
    let l1_timestamp = u64_word(word(1))?;
    u64_word(word(4))?; // sequence number
    if l1_timestamp > timestamp {
        return Err(err("OP_L1_ATTRIBUTES_ABI"));
    }
    let amount = |i| Amount::from_be_bytes(word(i));
    Ok(Parameters {
        chain_key: chain_key.into(),
        execution_timestamp: timestamp,
        l1_timestamp,
        l1_block_number,
        l1_block_hash: format!("0x{}", hex::encode(word(3))),
        l1_base_fee: amount(2)?,
        overhead: amount(6)?,
        scalar: amount(7)?,
        attributes_sha256: hex::encode(Sha256::digest(data)),
    })
}

/// Compute on the complete encoded user transaction, including its signature
/// and type prefix. A calldata-only slice or unsigned preparation is not the
/// input this API promises. The caller separately verifies the wire/hash and
/// source identity; no fee result proves that verification happened.
pub fn data_cost(parameters: &Parameters, encoded: &[u8], max_bytes: usize) -> Result<DataCost> {
    if encoded.is_empty() || max_bytes == 0 || encoded.len() > max_bytes {
        return Err(err("OP_L1_TRANSACTION_BYTES"));
    }
    // This historical scope admits legacy, access-list and EIP-1559 user wires.
    // Deposits have different fee semantics and must not be counted as users.
    if !matches!(encoded[0], 1 | 2 | 0xc0..=0xff) {
        return Err(err("OP_L1_TRANSACTION_TYPE_UNSUPPORTED"));
    }
    let zero_bytes = u64::try_from(encoded.iter().filter(|b| **b == 0).count())
        .map_err(|_| err("OP_L1_TRANSACTION_BYTES"))?;
    let nonzero_bytes =
        u64::try_from(encoded.len()).map_err(|_| err("OP_L1_TRANSACTION_BYTES"))? - zero_bytes;
    let calldata_gas = zero_bytes
        .checked_mul(4)
        .and_then(|n| nonzero_bytes.checked_mul(16).and_then(|m| n.checked_add(m)))
        .ok_or_else(|| err("OP_L1_TRANSACTION_BYTES"))?;
    // The upstream uses arbitrary precision, then converts the resulting fee
    // to uint256. Do not truncate or prematurely reject a wide intermediate.
    let gas = &parameters.overhead.0 + calldata_gas;
    let fee =
        Amount::checked((&gas * &parameters.l1_base_fee.0 * &parameters.scalar.0) / 1_000_000u64)?;
    Ok(DataCost {
        model_version: VERSION,
        upstream_commit: UPSTREAM,
        chain_key: parameters.chain_key.clone(),
        execution_timestamp: parameters.execution_timestamp,
        l1_block_number: parameters.l1_block_number,
        l1_timestamp: parameters.l1_timestamp,
        l1_block_hash: parameters.l1_block_hash.clone(),
        attributes_sha256: parameters.attributes_sha256.clone(),
        encoded_transaction_sha256: hex::encode(Sha256::digest(encoded)),
        encoded_transaction_bytes: encoded.len(),
        zero_bytes,
        nonzero_bytes,
        calldata_gas,
        overhead_gas: parameters.overhead.clone(),
        l1_gas_used: gas.to_string(),
        l1_base_fee: parameters.l1_base_fee.clone(),
        scalar: parameters.scalar.clone(),
        scalar_denominator: 1_000_000,
        l1_data_fee_native: fee,
    })
}
