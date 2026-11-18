//! Explicit OP packed-attributes arithmetic. The caller authenticates the
//! chain, active fee rules, deposit, signed wire, gas used and source age.
//! This module does not select a hardfork or confer source/execution trust.
use serde::Serialize;
use sha2::{Digest, Sha256};
use xone_domain::{err, Amount, Result};

pub const VERSION: &str = "xone-op-packed-rollup-cost/1";
pub const UPSTREAM: &str = super::op_l1_cost::UPSTREAM;
pub const MAX_ENCODED_BYTES: usize = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DataFeeRule {
    Ecotone,
    Fjord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum OperatorFeeRule {
    Isthmus,
    Jovian,
}

#[derive(Clone, Debug, Serialize)]
pub struct PackedParameters {
    format: &'static str,
    attributes_sha256: String,
    l1_timestamp: u64,
    l1_block_number: u64,
    l1_block_hash: String,
    sequence_number: u64,
    l1_base_fee: Amount,
    l1_blob_base_fee: Amount,
    base_fee_scalar: u32,
    blob_base_fee_scalar: u32,
    operator_fee_scalar: Option<u32>,
    operator_fee_constant: Option<u64>,
    da_footprint_scalar: Option<u16>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DataCost {
    pub model_version: &'static str,
    pub upstream_commit: &'static str,
    pub rule: DataFeeRule,
    pub parameters: PackedParameters,
    pub encoded_transaction_sha256: String,
    pub encoded_transaction_bytes: usize,
    pub zero_bytes: u64,
    pub nonzero_bytes: u64,
    pub calldata_gas: u64,
    pub fastlz_bytes: u64,
    pub estimated_da_bytes_scaled: Option<u64>,
    pub estimated_da_scale: u64,
    pub l1_gas_used: u64,
    pub l1_data_fee_native: Amount,
}

#[derive(Clone, Debug, Serialize)]
pub struct OperatorCost {
    pub model_version: &'static str,
    pub rule: OperatorFeeRule,
    pub attributes_sha256: String,
    pub gas_used: u64,
    pub scalar: u32,
    pub constant: u64,
    pub operator_fee_native: Amount,
}

/// Decode only the three exact, known packed encodings. Their selectors do
/// not determine the active fee formula (Ecotone/Fjord share an encoding).
/// A missing operator parameter stays absent, never implicitly zero.
pub fn decode_packed_attributes(data: &[u8]) -> Result<PackedParameters> {
    let format = match (data.get(..4), data.len()) {
        (Some([0x44, 0x0a, 0x5e, 0x20]), 164) => "ecotone",
        (Some([0x09, 0x89, 0x99, 0xbe]), 176) => "isthmus",
        (Some([0x3d, 0xb6, 0xbe, 0x2b]), 178) => "jovian",
        _ => return Err(err("OP_PACKED_ATTRIBUTES_ABI")),
    };
    let u32_at = |i| u32::from_be_bytes(data[i..i + 4].try_into().unwrap());
    let u64_at = |i| u64::from_be_bytes(data[i..i + 8].try_into().unwrap());
    Ok(PackedParameters {
        format,
        attributes_sha256: hex::encode(Sha256::digest(data)),
        sequence_number: u64_at(12),
        l1_timestamp: u64_at(20),
        l1_block_number: u64_at(28),
        l1_block_hash: format!("0x{}", hex::encode(&data[100..132])),
        l1_base_fee: Amount::from_be_bytes(&data[36..68])?,
        l1_blob_base_fee: Amount::from_be_bytes(&data[68..100])?,
        base_fee_scalar: u32_at(4),
        blob_base_fee_scalar: u32_at(8),
        operator_fee_scalar: (data.len() >= 176).then(|| u32_at(164)),
        operator_fee_constant: (data.len() >= 176).then(|| u64_at(168)),
        da_footprint_scalar: (data.len() == 178)
            .then(|| u16::from_be_bytes(data[176..178].try_into().unwrap())),
    })
}

/// The full encoded signed user transaction is required. This is a byte-cost
/// kernel, not an RLP/signature validator. Deposits and blob transactions have
/// different semantics and cannot be passed as fee-paying user transactions.
pub fn data_cost(
    p: &PackedParameters,
    rule: DataFeeRule,
    encoded: &[u8],
    max_bytes: usize,
) -> Result<DataCost> {
    if encoded.is_empty() || encoded.len() > max_bytes || encoded.len() > MAX_ENCODED_BYTES {
        return Err(err("OP_PACKED_TRANSACTION_BYTES"));
    }
    if !matches!(encoded[0], 1 | 2 | 4 | 0xc0..=0xff) {
        return Err(err("OP_PACKED_TRANSACTION_TYPE_UNSUPPORTED"));
    }
    if encoded[0] == 4 && p.operator_fee_scalar.is_none() {
        return Err(err("OP_PACKED_TRANSACTION_RULES_REQUIRED"));
    }
    // Ecotone arithmetic must not be selected for a later attributes format.
    if rule == DataFeeRule::Ecotone && p.format != "ecotone" {
        return Err(err("OP_PACKED_DATA_RULE_MISMATCH"));
    }
    let zero_bytes = encoded.iter().filter(|b| **b == 0).count() as u64;
    let nonzero_bytes = encoded.len() as u64 - zero_bytes;
    let calldata_gas = 4 * zero_bytes + 16 * nonzero_bytes;
    let fastlz_bytes = fastlz_len(encoded);
    let (estimated_da_bytes_scaled, l1_gas_used, numerator, denominator) = match rule {
        DataFeeRule::Ecotone => (None, calldata_gas, calldata_gas, 16_000_000u64),
        DataFeeRule::Fjord => {
            let size = (836_500i64 * fastlz_bytes as i64 - 42_585_600).max(100_000_000) as u64;
            (
                Some(size),
                size * 16 / 1_000_000,
                size,
                1_000_000_000_000u64,
            )
        }
    };
    let fee_scaled = &p.l1_base_fee.0 * p.base_fee_scalar * 16u32
        + &p.l1_blob_base_fee.0 * p.blob_base_fee_scalar;
    let fee = Amount::checked(fee_scaled * numerator / denominator)?;
    Ok(DataCost {
        model_version: VERSION,
        upstream_commit: UPSTREAM,
        rule,
        parameters: p.clone(),
        encoded_transaction_sha256: hex::encode(Sha256::digest(encoded)),
        encoded_transaction_bytes: encoded.len(),
        zero_bytes,
        nonzero_bytes,
        calldata_gas,
        fastlz_bytes,
        estimated_da_bytes_scaled,
        estimated_da_scale: 1_000_000,
        l1_gas_used,
        l1_data_fee_native: fee,
    })
}

/// Operator cost is independent of L1 DA cost and execution gas price. An
/// explicit rule is necessary: Jovian changes the scalar's unit by 1e8.
/// The caller must supply fee-charged gas, including applicable refund rules.
pub fn operator_cost(
    p: &PackedParameters,
    rule: OperatorFeeRule,
    gas_used: u64,
) -> Result<OperatorCost> {
    let scalar = p
        .operator_fee_scalar
        .ok_or_else(|| err("OP_OPERATOR_PARAMETERS_MISSING"))?;
    let constant = p
        .operator_fee_constant
        .ok_or_else(|| err("OP_OPERATOR_PARAMETERS_MISSING"))?;
    if rule == OperatorFeeRule::Isthmus && p.format == "jovian" {
        return Err(err("OP_OPERATOR_RULE_MISMATCH"));
    }
    let product = gas_used.to_string().parse::<Amount>()?.0 * scalar;
    let fee = match rule {
        OperatorFeeRule::Isthmus => product / 1_000_000u32,
        OperatorFeeRule::Jovian => product * 100u32,
    } + constant;
    Ok(OperatorCost {
        model_version: VERSION,
        rule,
        attributes_sha256: p.attributes_sha256.clone(),
        gas_used,
        scalar,
        constant,
        operator_fee_native: Amount::checked(fee)?,
    })
}

// FastLZ length algorithm adapted from op-geth core/types/rollup_cost.go,
// commit 79e9d244748a1201fe686ca8a882b5fe7e173348.
// Copyright 2022 The go-ethereum Authors. SPDX-License-Identifier: LGPL-3.0-or-later
// Original license: https://www.gnu.org/licenses/lgpl-3.0.html
fn fastlz_len(input: &[u8]) -> u64 {
    let mut table = vec![0usize; 8192];
    let u24 = |i| input[i] as u32 | (input[i + 1] as u32) << 8 | (input[i + 2] as u32) << 16;
    let hash = |v: u32| ((2_654_435_769u32.wrapping_mul(v) >> 19) & 0x1fff) as usize;
    let literals =
        |n: usize| (33 * (n / 32) + if !n.is_multiple_of(32) { n % 32 + 1 } else { 0 }) as u64;
    let limit = input.len().saturating_sub(13);
    let mut anchor = 0;
    let mut ip = 2;
    let mut size = 0;
    while ip < limit {
        let reference;
        loop {
            let value = u24(ip);
            let slot = hash(value);
            let previous = table[slot];
            table[slot] = ip;
            let distance = ip - previous;
            if ip >= limit {
                reference = previous;
                break;
            }
            ip += 1;
            if distance <= 8191 && value == u24(previous) {
                reference = previous;
                break;
            }
        }
        if ip >= limit {
            break;
        }
        ip -= 1;
        size += literals(ip - anchor);
        // Upstream cmp includes the mismatching byte in its return value.
        let mut length = 0;
        let mut end = limit + 9 - (ip + 3);
        while length < end {
            if input[reference + 3 + length] != input[ip + 3 + length] {
                end = 0;
            }
            length += 1;
        }
        let l = length - 1;
        size += (3 * (l / 262) + if l % 262 >= 6 { 3 } else { 2 }) as u64;
        ip += length;
        for _ in 0..2 {
            table[hash(u24(ip))] = ip;
            ip += 1;
        }
        anchor = ip;
    }
    size + literals(input.len() - anchor)
}
