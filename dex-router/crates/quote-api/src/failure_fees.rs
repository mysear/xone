//! Observed fees of failed recent-blockhash swaps, separate from settlement.
use crate::{execution_receipts::Receipt, transaction_lifecycle::receipt_transaction};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_domain::{err, hash, Result};

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| err("RECEIPT_META_MISSING"))
}
type TokenState = (String, String, u64, u8, Option<String>);
fn tokens(meta: &Value, phase: &str, accounts: usize) -> Result<BTreeMap<usize, TokenState>> {
    let values = required(meta, phase)?
        .as_array()
        .ok_or_else(|| err("RECEIPT_META_INVALID"))?;
    let mut result = BTreeMap::new();
    for item in values {
        let invalid = || err("RECEIPT_TOKEN_IDENTITY");
        let index = item["accountIndex"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n < accounts)
            .ok_or_else(invalid)?;
        let mint = item["mint"].as_str().ok_or_else(invalid)?;
        let owner = item["owner"].as_str().ok_or_else(invalid)?;
        xone_domain::raydium::validate_pubkey(mint).map_err(|_| invalid())?;
        xone_domain::raydium::validate_pubkey(owner).map_err(|_| invalid())?;
        let raw = item["uiTokenAmount"]["amount"]
            .as_str()
            .ok_or_else(invalid)?;
        let amount = raw.parse::<u64>().map_err(|_| invalid())?;
        if amount.to_string() != raw {
            return Err(invalid());
        }
        let decimals = item["uiTokenAmount"]["decimals"]
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .ok_or_else(invalid)?;
        let program = match item.get("programId") {
            None => None,
            Some(v) => {
                let key = v.as_str().ok_or_else(invalid)?;
                if ![
                    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
                    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
                ]
                .contains(&key)
                {
                    return Err(invalid());
                }
                Some(key.to_owned())
            }
        };
        if result
            .insert(
                index,
                (mint.into(), owner.into(), amount, decimals, program),
            )
            .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(result)
}
fn instruction_failure(value: &Value, instructions: usize) -> bool {
    let Some(object) = value.as_object().filter(|o| o.len() == 1) else {
        return false;
    };
    let Some(items) = object
        .get("InstructionError")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
    else {
        return false;
    };
    items[0]
        .as_u64()
        .is_some_and(|n| n <= u8::MAX.into() && (n as usize) < instructions)
        && (items[1].as_str().is_some_and(|s| !s.is_empty())
            || items[1].as_object().is_some_and(|o| {
                o.len() == 1
                    && o.get("Custom")
                        .and_then(Value::as_u64)
                        .is_some_and(|n| n <= u32::MAX.into())
            }))
}
pub(crate) fn observed(receipt: &Receipt, tx: &Value, status_error: &Value) -> Result<Value> {
    let info = receipt_transaction(receipt, tx)?;
    let meta = required(tx, "meta")?;
    let meta_error = meta.get("err").ok_or_else(|| err("RECEIPT_META_MISSING"))?;
    if meta_error != status_error {
        return Err(err("RECEIPT_FAILURE_STATUS_MISMATCH"));
    }
    // Account-loading, nonce and unknown error classes need their own fee rules.
    let instructions = required(&receipt.preparation, "instructions")?
        .as_array()
        .ok_or_else(|| err("RECEIPT_META_INVALID"))?
        .len();
    if !instruction_failure(meta_error, instructions) {
        return Err(err("RECEIPT_FAILURE_UNSUPPORTED"));
    }
    let fee = required(meta, "fee")?
        .as_u64()
        .ok_or_else(|| err("RECEIPT_FEE_INVALID"))?;
    let balances = |phase| -> Result<Vec<u64>> {
        let values = required(meta, phase)?
            .as_array()
            .filter(|v| v.len() == info.account_keys.len())
            .ok_or_else(|| err("RECEIPT_BALANCE_INVALID"))?;
        values
            .iter()
            .map(|v| v.as_u64().ok_or_else(|| err("RECEIPT_BALANCE_INVALID")))
            .collect()
    };
    let before = balances("preBalances")?;
    let after = balances("postBalances")?;
    if before[0].checked_sub(fee) != Some(after[0]) || before[1..] != after[1..] {
        return Err(err("RECEIPT_FAILURE_BALANCE_MISMATCH"));
    }
    let pre = tokens(meta, "preTokenBalances", before.len())?;
    let post = tokens(meta, "postTokenBalances", before.len())?;
    if pre != post {
        return Err(err("RECEIPT_FAILURE_TOKEN_MISMATCH"));
    }
    let p = &receipt.preparation;
    for (account, mint, owner, existing) in [
        (
            "sourceTokenAccount",
            &receipt.context.sell_token,
            &receipt.context.caller,
            p["sourceNativeSol"] != true && p["sourceAccountCreation"] != true,
        ),
        (
            "destinationTokenAccount",
            &receipt.context.buy_token,
            &receipt.context.recipient,
            p["destinationNativeSol"] != true
                && p["accountCreation"] != true
                && p["unwrapDestinationSol"] != true,
        ),
    ] {
        if existing {
            let index = info
                .account_keys
                .iter()
                .position(|key| p[account].as_str() == Some(key))
                .ok_or_else(|| err("RECEIPT_META_MISSING"))?;
            let token = pre.get(&index).ok_or_else(|| err("RECEIPT_META_MISSING"))?;
            if &token.0 != mint || &token.1 != owner {
                return Err(err("RECEIPT_TOKEN_IDENTITY"));
            }
        }
    }
    Ok(
        json!({"modelVersion":"xone-solana-failed-fee/1","sourceAuthority":"RPC_TRANSACTION_RECEIPT","scope":"FAILED_PREPARED_RECENT_BLOCKHASH_INSTRUCTION","chainKey":receipt.context.chain_key,"preparationHash":p["preparationHash"],"transactionSha256":info.transaction_sha256,"signature":info.signature,"slot":receipt.slot,"nativeUnit":"LAMPORTS","networkFeeNative":fee.to_string(),"payerNativeBefore":before[0].to_string(),"payerNativeAfter":after[0].to_string(),"payerLamportDelta":(-i128::from(fee)).to_string(),"nativeAccountCount":before.len(),"tokenAccountCount":pre.len(),"observedNativeRollbackMatched":true,"observedTokenRollbackMatched":true,"retainedRentDepositNative":"0","settlementApplied":false,"netBuyAmountEstimate":Value::Null,"netSellAmountEstimate":Value::Null,"quoteRankingApplied":false,"historicalBankVerified":false,"independentProductionVerification":false,"transactionResultHash":hash(tx)?,"metaProjectionHash":hash(meta)?,"onchainError":meta_error}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_fee_token_rollback_rejects_duplicates_identity_and_integer_tampering() {
        let mint = "So11111111111111111111111111111111111111112";
        let item = json!({"accountIndex":1,"mint":mint,"owner":mint,"uiTokenAmount":{"amount":"18446744073709551615","decimals":9}});
        let base = json!({"preTokenBalances":[item]});
        assert_eq!(
            tokens(&base, "preTokenBalances", 2).unwrap()[&1].2,
            u64::MAX
        );
        for (field, value) in [
            ("accountIndex", json!(2)),
            ("mint", json!("bad")),
            ("owner", Value::Null),
            ("programId", json!("bad")),
        ] {
            let mut v = base.clone();
            v["preTokenBalances"][0][field] = value;
            assert!(tokens(&v, "preTokenBalances", 2).is_err(), "{field}");
        }
        for value in [
            json!(1),
            json!("01"),
            json!("+1"),
            json!("18446744073709551616"),
        ] {
            let mut v = base.clone();
            v["preTokenBalances"][0]["uiTokenAmount"]["amount"] = value;
            assert!(tokens(&v, "preTokenBalances", 2).is_err());
        }
        let duplicate =
            json!({"preTokenBalances":[base["preTokenBalances"][0],base["preTokenBalances"][0]]});
        assert!(tokens(&duplicate, "preTokenBalances", 2).is_err());
        assert!(tokens(&json!({}), "preTokenBalances", 2)
            .unwrap_err()
            .0
            .ends_with("MISSING"));
    }
    #[test]
    fn failed_fee_only_accepts_explicit_instruction_error_metadata() {
        assert!(instruction_failure(
            &json!({"InstructionError":[0,"InvalidAccountData"]}),
            1
        ));
        assert!(instruction_failure(
            &json!({"InstructionError":[255,{"Custom":u32::MAX}]}),
            256
        ));
        assert!(!instruction_failure(
            &json!({"InstructionError":[1,"InvalidAccountData"]}),
            1
        ));
        for v in [
            Value::Null,
            json!(false),
            json!("AccountNotFound"),
            json!({"InstructionError":[256,"InvalidAccountData"]}),
            json!({"InstructionError":[0,{}]}),
            json!({"InstructionError":[0,{"Custom":-1}]}),
            json!({"InstructionError":[0,"x"],"extra":true}),
        ] {
            assert!(!instruction_failure(&v, 256), "{v}");
        }
    }
}
