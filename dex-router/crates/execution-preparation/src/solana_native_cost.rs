//! Message-bound RPC fee estimate and native funding classification. Rent
//! deposits and swap/wrap principal are not network fees or token valuation.
use crate::{AccountSnapshot, SolanaQuote};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use xone_domain::{err, hash, Result};

pub(crate) struct FeeObservation {
    pub message: Vec<u8>,
    pub observed_slot: u64,
    pub minimum_slot: u64,
    pub fee: Option<u64>,
    pub response_hash: String,
}
impl FeeObservation {
    pub fn read(params: &Value, response: &Value) -> Result<Self> {
        let invalid = || err("PREPARATION_NATIVE_COST_SOURCE_INVALID");
        let encoded = params[0]
            .as_str()
            .filter(|v| v.len() <= 2048)
            .ok_or_else(invalid)?;
        let message = STANDARD.decode(encoded).map_err(|_| invalid())?;
        if message.len() < 3
            || message[0] != 1
            || message[1] != 0
            || message.len() > 1232
            || params[1]["commitment"] != "finalized"
        {
            return Err(invalid());
        }
        Ok(Self {
            message,
            observed_slot: response["context"]["slot"].as_u64().ok_or_else(invalid)?,
            minimum_slot: params[1]["minContextSlot"].as_u64().ok_or_else(invalid)?,
            fee: response["value"].as_u64(),
            response_hash: hash(response)?,
        })
    }
}
fn number(value: &Value, key: &str) -> Result<u64> {
    let raw = value[key]
        .as_str()
        .ok_or_else(|| err("PREPARATION_NATIVE_COST_MISMATCH"))?;
    let n = raw
        .parse::<u64>()
        .map_err(|_| err("PREPARATION_NATIVE_COST_MISMATCH"))?;
    if raw != n.to_string() {
        return Err(err("PREPARATION_NATIVE_COST_MISMATCH"));
    }
    Ok(n)
}
pub(crate) fn attach(
    result: &mut Value,
    s: &SolanaQuote,
    accounts: Option<&AccountSnapshot>,
    observation: Option<&FeeObservation>,
) -> Result<()> {
    if result["ready"] != true {
        result["costs"] = json!({"modelVersion":"xone-solana-native-cost/1","status":"UNAVAILABLE","reason":"PREPARATION_NOT_READY","networkFeeNative":Value::Null,"totalNativeEstimate":Value::Null,"netBuyAmountEstimate":Value::Null,"netSellAmountEstimate":Value::Null});
        return Ok(());
    }
    let invalid = || err("PREPARATION_NATIVE_COST_MISMATCH");
    let source = accounts.ok_or_else(invalid)?;
    let fee = observation.ok_or_else(invalid)?;
    let message = STANDARD
        .decode(result["messageBase64"].as_str().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let packet = STANDARD
        .decode(result["unsignedTransaction"].as_str().ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    if packet.len() > 1232
        || packet.len() < 66
        || packet[0] != 1
        || packet[1..65].iter().any(|v| *v != 0)
        || packet[65..] != message
        || message != fee.message
        || fee.minimum_slot != source.observed_slot
        || fee.observed_slot < source.observed_slot
        || source.chain_key != s.context.chain_key
        || source.observed_slot != result["observedSlot"].as_u64().ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    let network = number(result, "feeLamports")?;
    let rent = number(result, "rentLamports")?;
    let refundable = number(result, "rentRefundLamports")?;
    let wrap = number(result, "wrapLamports")?;
    if result
        .get("sourceNativeSol")
        .is_some_and(|v| !v.is_boolean())
        || (result["sourceNativeSol"] == true && wrap != 0)
    {
        return Err(invalid());
    }
    let principal = if result["sourceNativeSol"] == true {
        number(result, "requiredInput")?
    } else {
        wrap
    };
    let funding = network
        .checked_add(rent)
        .and_then(|v| v.checked_add(principal))
        .ok_or_else(invalid)?;
    let payer = number(result, "payerLamports")?;
    let retained = rent.checked_sub(refundable).ok_or_else(invalid)?;
    if fee.fee != Some(network)
        || number(result, "totalFundingLamports")? != funding
        || source
            .accounts
            .get(&s.context.caller)
            .and_then(|v| v["lamports"].as_u64())
            != Some(payer)
        || payer < funding
    {
        return Err(invalid());
    }
    result["costs"] = json!({
        "modelVersion":"xone-solana-native-cost/1","status":"MESSAGE_BOUND_RPC_ESTIMATE","chainFeeCompleteness":"PREPARED_MESSAGE_RPC_ESTIMATE",
        "scope":"DECLARED_SINGLE_SIGNER_PREPARED_SOLANA_TRANSACTION",
        "nativeUnit":"LAMPORTS","chainKey":source.chain_key,"genesisHash":source.genesis_hash,
        "planHash":s.plan_hash,"quoteViewId":s.quote_view_id,"messageSha256":format!("{:x}",Sha256::digest(&message)),
        "sourceAuthority":"PREPARATION_RPC_OBSERVATION","historicalBankVerified":false,
        "accountObservedSlot":source.observed_slot,"feeObservedSlot":fee.observed_slot,"feeMinimumContextSlot":fee.minimum_slot,
        "feeContextPolicy":"MIN_CONTEXT_SLOT_NOT_SAME_BANK",
        "validity":{"recentBlockhash":result["recentBlockhash"],"lastValidBlockHeight":result["lastValidBlockHeight"],"quoteExpiresAtMs":s.context.deadline_ms},"feeResponseHash":fee.response_hash,"accountSnapshotHash":hash(source)?,
        "networkFeeNative":network.to_string(),"nativeKnownSubtotal":network.to_string(),
        "nativeKnownSubtotalKind":"NETWORK_FEE_ONLY","nativeFundingRequired":funding.to_string(),
        "rentDepositNative":rent.to_string(),"expectedRefundableRentNative":refundable.to_string(),"expectedRetainedRentDepositNative":retained.to_string(),
        "swapOrWrapPrincipalNative":principal.to_string(),"wrapPrincipalNative":wrap.to_string(),
        "feeBreakdown":[{"kind":"RPC_MESSAGE_NETWORK_FEE","amountNative":network.to_string(),"includedInKnownSubtotal":true}],
        "deductedFromQuoteOutput":false,"tokenValuationStatus":"UNAVAILABLE",
        "totalNativeEstimate":Value::Null,"netBuyAmountEstimate":Value::Null,"netSellAmountEstimate":Value::Null
    });
    result
        .as_object_mut()
        .ok_or_else(invalid)?
        .remove("preparationHash");
    result["preparationHash"] = hash(result)?.into();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use xone_domain::{Pool, Snapshot};
    use xone_execution_plan::{
        solana_unsigned::{
            compile_message_many, unsigned_packet, SolanaAccountMeta, SolanaInstruction,
        },
        Context,
    };
    fn sample() -> (SolanaQuote, AccountSnapshot, FeeObservation, Value) {
        let b: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/meteora-execution-bank.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let pool: Pool = serde_json::from_value(b["pool"].clone()).unwrap();
        let mut snapshot: Snapshot =
            serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
        snapshot.reference.block.chain_key = pool.chain_key.clone();
        let s = SolanaQuote {
            context: Context {
                chain_key: pool.chain_key.clone(),
                caller: b["caller"].as_str().unwrap().into(),
                recipient: b["recipient"].as_str().unwrap().into(),
                sell_token: pool.token0.clone(),
                buy_token: pool.token1.clone(),
                slippage_bps: 50,
                deadline_ms: u64::MAX,
            },
            pool,
            plan_hash: "f".repeat(64),
            quote_view_id: "explicit-synthetic-fee-test".into(),
            snapshot: snapshot.reference,
            zero: true,
            exact: true,
            amount: 1000,
            limit: 2000,
            expected_input: 699,
            expected_output: 1000,
        };
        let message = compile_message_many(
            &s.context.caller,
            b["blockhash"].as_str().unwrap(),
            &[SolanaInstruction {
                program_id: crate::SYSTEM.into(),
                accounts: vec![SolanaAccountMeta {
                    pubkey: s.context.caller.clone(),
                    is_signer: true,
                    is_writable: true,
                }],
                data_base64: STANDARD.encode([0u8; 4]),
            }],
        )
        .unwrap();
        let account = AccountSnapshot {
            schema_version: 1,
            chain_key: s.context.chain_key.clone(),
            genesis_hash: "explicit-test-only-genesis".into(),
            observed_slot: 10,
            accounts: BTreeMap::from([(s.context.caller.clone(), json!({"lamports":10000000}))]),
        };
        let obs = FeeObservation::read(
            &json!([STANDARD.encode(&message),{"commitment":"finalized","minContextSlot":10}]),
            &json!({"context":{"slot":11},"value":5000}),
        )
        .unwrap();
        let result = json!({"ready":true,"messageBase64":STANDARD.encode(&message),"unsignedTransaction":STANDARD.encode(unsigned_packet(&message).unwrap()),"observedSlot":10,"feeLamports":"5000","rentLamports":"2039280","rentRefundLamports":"2039280","wrapLamports":"100000","payerLamports":"10000000","totalFundingLamports":"2144280","preparationHash":"old"});
        (s, account, obs, result)
    }
    #[test]
    fn solana_native_cost_binds_rpc_message_and_separates_fee_rent_and_principal() {
        let (s, a, o, mut r) = sample();
        attach(&mut r, &s, Some(&a), Some(&o)).unwrap();
        assert_eq!(r["costs"]["nativeKnownSubtotal"], "5000");
        assert_eq!(r["costs"]["nativeFundingRequired"], "2144280");
        assert_eq!(r["costs"]["expectedRetainedRentDepositNative"], "0");
        assert_eq!(r["costs"]["swapOrWrapPrincipalNative"], "100000");
        assert_eq!(r["costs"]["feeObservedSlot"], 11);
        assert_eq!(r["costs"]["accountObservedSlot"], 10);
        assert_eq!(r["costs"]["historicalBankVerified"], false);
        assert!(r["costs"]["netBuyAmountEstimate"].is_null());
        let h = r
            .as_object_mut()
            .unwrap()
            .remove("preparationHash")
            .unwrap();
        assert_eq!(h, hash(&r).unwrap());
    }
    #[test]
    fn solana_native_cost_rejects_changed_wire_amounts_source_balances_and_fee_slots() {
        for key in [
            "feeLamports",
            "rentLamports",
            "rentRefundLamports",
            "wrapLamports",
            "payerLamports",
            "totalFundingLamports",
        ] {
            let (s, a, o, mut r) = sample();
            r[key] = "9999999".into();
            assert!(attach(&mut r, &s, Some(&a), Some(&o)).is_err(), "{key}");
        }
        for mode in 0..6 {
            let (s, mut a, mut o, mut r) = sample();
            match mode {
                0 => o.message[2] ^= 1,
                1 => o.observed_slot = 9,
                2 => o.minimum_slot = 9,
                3 => a.accounts.get_mut(&s.context.caller).unwrap()["lamports"] = json!(9999999),
                4 => r["unsignedTransaction"] = STANDARD.encode([0u8; 70]).into(),
                _ => a.chain_key = "solana:wrong-source".into(),
            }
            assert!(attach(&mut r, &s, Some(&a), Some(&o)).is_err(), "{mode}");
        }
        let (s, a, o, mut r) = sample();
        r["feeLamports"] = u64::MAX.to_string().into();
        assert!(attach(&mut r, &s, Some(&a), Some(&o)).is_err());
        let (s, a, _, mut r) = sample();
        assert!(attach(&mut r, &s, Some(&a), None).is_err());
    }
    #[test]
    fn solana_native_cost_native_input_is_principal_and_blocked_preparation_is_unavailable() {
        let (s, a, o, mut r) = sample();
        r["sourceNativeSol"] = true.into();
        r["requiredInput"] = "100000".into();
        r["wrapLamports"] = "0".into();
        attach(&mut r, &s, Some(&a), Some(&o)).unwrap();
        assert_eq!(r["costs"]["swapOrWrapPrincipalNative"], "100000");
        assert_eq!(r["costs"]["nativeKnownSubtotal"], "5000");
        let mut r = json!({"ready":false,"blockers":["INSUFFICIENT_FEE_BALANCE"]});
        attach(&mut r, &s, None, None).unwrap();
        assert_eq!(r["costs"]["status"], "UNAVAILABLE");
        assert!(r["costs"]["networkFeeNative"].is_null());
    }
}
