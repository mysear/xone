//! Osaka-sensitive precompile components used by scoped Karst user execution.
use revm::{
    precompile::{modexp, secp256r1},
    primitives::{Bytes, PrecompileError, PrecompileOutput, PrecompileResult, U256},
};

fn padded_word(input: &[u8], offset: usize) -> U256 {
    let mut word = [0; 32];
    if let Some(tail) = input.get(offset..) {
        let count = tail.len().min(32);
        word[..count].copy_from_slice(&tail[..count]);
    }
    U256::from_be_bytes(word)
}
fn osaka_gas(base: u64, exponent: u64, modulus: u64, high: &U256) -> u64 {
    // These operands have already passed EIP-7823's 1024-byte limit, so
    // multiplication and iteration costs are bounded far below u64::MAX.
    let size = base.max(modulus);
    let complexity = if size <= 32 {
        16
    } else {
        2 * size.div_ceil(8).pow(2)
    };
    let bits = high.bit_len() as u64;
    let iterations = if exponent <= 32 {
        bits.saturating_sub(1)
    } else {
        16 * (exponent - 32) + bits.saturating_sub(1)
    }
    .max(1);
    (complexity * iterations).max(500)
}
pub(super) fn modexp_osaka(input: &Bytes, gas: u64) -> PrecompileResult {
    // The original Osaka implementation checks the 500 minimum before its
    // size limit, and the size limit before any exponentiation or allocation.
    if gas < 500 {
        return Err(PrecompileError::OutOfGas.into());
    }
    let lengths = [
        padded_word(input, 0),
        padded_word(input, 32),
        padded_word(input, 64),
    ];
    if lengths.iter().any(|n| *n > U256::from(1024)) {
        return Err(PrecompileError::Other("EIP-7823 input size".into()).into());
    }
    let [base, exponent, modulus] = lengths.map(|n| n.to::<usize>());
    let count = exponent.min(32);
    let mut high = [0; 32];
    if let Some(tail) = input.get(96 + base..) {
        let copy = tail.len().min(count);
        high[32 - count..32 - count + copy].copy_from_slice(&tail[..copy]);
    }
    let price = osaka_gas(
        base as u64,
        exponent as u64,
        modulus as u64,
        &U256::from_be_bytes(high),
    );
    // Reuse the locked modular arithmetic engine, with the new bounded gas
    // cost as both minimum and callback. This also corrects the old early
    // base=modulus=0 shortcut when a nonzero exponent raises the Osaka price.
    modexp::run_inner(input, gas, price, |_, _, _, _| price)
}
pub(super) fn p256_osaka(input: &Bytes, gas: u64) -> PrecompileResult {
    if gas < 6900 {
        return Err(PrecompileError::OutOfGas.into());
    }
    let output = secp256r1::p256_verify(input, gas)?;
    Ok(PrecompileOutput::new(6900, output.bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::revm_backend::{op_precompiles, OpFork};
    use revm::{db::EmptyDB, primitives::*, InnerEvmContext};
    #[test]
    fn karst_schedule_excludes_upgrade_blocks_future_rules_and_unverified_chains() {
        assert_eq!(OpFork::at(10, 1_783_526_401), None);
        assert_eq!(OpFork::at(10, 1_783_526_402), Some(OpFork::Karst));
        assert_eq!(OpFork::at(10, 1_799_683_200), Some(OpFork::Karst));
        assert_eq!(OpFork::at(10, 1_799_683_201), None);
        assert_eq!(OpFork::at(8453, 1_783_526_403), None);
        // Nonzero exponent costs more than 500 even if output length is zero.
        let mut input = vec![0; 128];
        input[63] = 32;
        input[96] = 0x80;
        let result = modexp_osaka(&input.into(), 10000).unwrap();
        assert!(result.bytes.is_empty());
        assert_eq!(result.gas_used, 16 * 255);
    }
    #[test]
    #[ignore = "requires fixed original modern OP precompile components"]
    fn karst_precompiles_match_original_components_gas_limits_and_error_order() {
        let vectors: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("XONE_KARST_PRECOMPILE_VECTORS").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(vectors["passed"], true);
        let mut count = 0;
        for row in vectors["cases"].as_array().unwrap() {
            let mut table = op_precompiles::load::<EmptyDB, ()>(OpFork::Karst);
            let input = &row["input"];
            let address: Address = input["address"].as_str().unwrap().parse().unwrap();
            let raw = Bytes::from(hex::decode(input["data"].as_str().unwrap()).unwrap());
            let mut context = InnerEvmContext::new(EmptyDB::default());
            let result = table
                .call(&address, &raw, input["gas"].as_u64().unwrap(), &mut context)
                .unwrap();
            assert_eq!(
                result.is_ok(),
                row["success"].as_bool().unwrap(),
                "{} {result:?}",
                input["id"]
            );
            assert_eq!(
                matches!(
                    &result,
                    Err(PrecompileErrors::Error(PrecompileError::OutOfGas))
                ),
                row["out_of_gas"].as_bool().unwrap(),
                "{} {result:?}",
                input["id"]
            );
            if let Ok(out) = result {
                assert_eq!(
                    out.gas_used,
                    row["gas_used"].as_u64().unwrap(),
                    "{}",
                    input["id"]
                );
                assert_eq!(
                    hex::encode(out.bytes),
                    row["output"].as_str().unwrap(),
                    "{}",
                    input["id"]
                );
            }
            count += 1;
        }
        assert!(count > 1000);
        assert_eq!(count, vectors["case_count"].as_u64().unwrap() as usize);
    }
}
