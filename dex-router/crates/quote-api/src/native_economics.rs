//! Native-unit economics for an individually executed preparation. Deposits
//! remain capital movements and never become network execution expense.
use serde_json::{json, Value};
use xone_domain::{err, Result};
use xone_execution_plan::Context;
const NATIVE: &str = xone_domain::pump::NATIVE_SOL;
const WSOL: &str = "So11111111111111111111111111111111111111112";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const SYSTEM: &str = "11111111111111111111111111111111";
#[derive(Clone, Copy)]
pub(super) enum Source {
    LocalSbf,
    CapturedSbf,
    RpcReceipt,
}
fn number(value: &Value, key: &str) -> Result<u64> {
    let raw = value[key]
        .as_str()
        .ok_or_else(|| err("NATIVE_ECONOMICS_INVALID"))?;
    let n = raw
        .parse::<u64>()
        .map_err(|_| err("NATIVE_ECONOMICS_INVALID"))?;
    if raw != n.to_string() {
        return Err(err("NATIVE_ECONOMICS_INVALID"));
    }
    Ok(n)
}
pub(super) fn evaluated(c: &Context, p: &Value, observed: &Value, source: Source) -> Result<Value> {
    let invalid = || err("NATIVE_ECONOMICS_INVALID");
    let receipt = matches!(source, Source::RpcReceipt);
    if p["ready"] != true
        || if receipt {
            observed["verified"] != true
        } else {
            observed["success"] != true
        }
    {
        return Err(invalid());
    }
    let input = number(observed, "actualInput")?;
    let output = number(observed, "actualOutput")?;
    let fee = number(observed, "feeLamports")?;
    if !receipt && fee != number(p, "feeLamports")? {
        return Err(invalid());
    }
    let retained = if receipt {
        number(observed, "actualPersistentRentLamports")?
    } else {
        number(p, "rentLamports")?
            .checked_sub(number(p, "rentRefundLamports")?)
            .ok_or_else(invalid)?
    };
    let raw = observed["payerLamportDelta"].as_str().ok_or_else(invalid)?;
    let delta = raw.parse::<i128>().map_err(|_| invalid())?;
    if delta.unsigned_abs() > u128::from(u64::MAX) || delta.to_string() != raw {
        return Err(invalid());
    }
    let native_input = c.sell_token == NATIVE;
    let native_output = c.buy_token == NATIVE;
    if (p["sourceNativeSol"] == true) != native_input
        || (p["destinationNativeSol"] == true) != native_output
    {
        return Err(invalid());
    }
    let input_direct =
        native_input && p["sourceTokenAccount"] == c.caller && p["sourceTokenProgram"] == SYSTEM;
    let input_unwrapped = c.sell_token == WSOL
        && p["refundWrappedSource"] == true
        && p["sourceAccountCreation"] == true
        && p["sourceTokenProgram"] == TOKEN;
    let output_direct = native_output
        && p["destinationTokenAccount"] == c.caller
        && p["destinationTokenProgram"] == SYSTEM;
    let output_unwrapped = c.buy_token == WSOL
        && p["unwrapDestinationSol"] == true
        && p["destinationTokenProgram"] == TOKEN;
    if (native_input && !input_direct)
        || (native_output && (!output_direct || c.recipient != c.caller))
        || (p["refundWrappedSource"] == true && !input_unwrapped)
        || (p["unwrapDestinationSol"] == true && (!output_unwrapped || c.recipient != c.caller))
    {
        return Err(invalid());
    }
    let input_eligible = input_direct || input_unwrapped;
    let output_eligible = (output_direct || output_unwrapped) && c.recipient == c.caller;
    let mut result = json!({"modelVersion":"xone-solana-native-economics/1","sourceAuthority":match source {Source::LocalSbf=>"PINNED_LOCAL_SBF_EXECUTION",Source::CapturedSbf=>"RPC_CAPTURE_SBF_EXECUTION",Source::RpcReceipt=>"RPC_TRANSACTION_RECEIPT"},
        "scope":"EVALUATED_PREPARED_TRANSACTION_ONLY","amountBasis":"OBSERVED_ACTUAL_SETTLEMENT","chainKey":c.chain_key,"preparationHash":p["preparationHash"],"transactionSha256":observed["transactionSha256"],
        "nativeUnit":"LAMPORTS","networkFeeNative":fee.to_string(),"networkFeeIncludedInSwapAmounts":false,"retainedRentDepositNative":retained.to_string(),"depositTreatment":"CAPITAL_DEPOSIT_SEPARATE_FROM_NETWORK_FEE",
        "actualPayerNativeDelta":delta.to_string(),"historicalBankVerified":false,"independentProductionVerification":false,"quoteRankingApplied":false,"deductedFromQuoteOutput":false,
        "netBuyAmountEstimate":Value::Null,"netSellAmountEstimate":Value::Null,"netBuyUnavailableReason":"OUTPUT_NOT_NATIVE_SETTLEMENT_ASSET","netSellUnavailableReason":"INPUT_NOT_NATIVE_SETTLEMENT_ASSET","tokenValuationStatus":"UNAVAILABLE","unavailableReason":"NON_NATIVE_SETTLEMENT_ASSET"});
    if input_eligible && output_eligible {
        return Err(invalid());
    }
    if output_eligible {
        let expected = i128::from(output) - i128::from(fee) - i128::from(retained);
        if delta != expected {
            return Err(invalid());
        }
        result["valuationToken"] = c.buy_token.clone().into();
        result["tokenValuationStatus"] = "NATIVE_UNIT_IDENTITY".into();
        match output.checked_sub(fee) {
            Some(net) => {
                result["netBuyAmountEstimate"] = net.to_string().into();
                result["netBuyUnavailableReason"] = Value::Null;
                result["unavailableReason"] = Value::Null;
            }
            None => {
                result["netBuyUnavailableReason"] = "NETWORK_FEE_EXCEEDS_GROSS_OUTPUT".into();
                result["unavailableReason"] = "NETWORK_FEE_EXCEEDS_GROSS_OUTPUT".into();
            }
        }
    } else if input_eligible {
        let expected = -i128::from(input) - i128::from(fee) - i128::from(retained);
        if delta != expected {
            return Err(invalid());
        }
        result["valuationToken"] = c.sell_token.clone().into();
        result["tokenValuationStatus"] = "NATIVE_UNIT_IDENTITY".into();
        match input.checked_add(fee) {
            Some(net) => {
                result["netSellAmountEstimate"] = net.to_string().into();
                result["netSellUnavailableReason"] = Value::Null;
                result["unavailableReason"] = Value::Null;
            }
            None => {
                result["netSellUnavailableReason"] = "NATIVE_INPUT_COST_EXCEEDS_U64".into();
                result["unavailableReason"] = "NATIVE_INPUT_COST_EXCEEDS_U64".into();
            }
        }
    }
    if input_eligible || output_eligible {
        result["valuationRatio"] = json!({"numerator":"1","denominator":"1","method":"NATIVE_UNIT_IDENTITY","priceObservedAtMs":Value::Null,"priceTimePolicy":"IDENTICAL_NATIVE_ATOMS_NO_MARKET_PRICE"});
        result["nativeCashReconciliation"] = "MATCHED".into();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(output_native: bool) -> (Context, Value, Value) {
        let c = Context {
            chain_key: "solana:controlled-test".into(),
            caller: "payer".into(),
            recipient: "payer".into(),
            sell_token: if output_native { "token" } else { NATIVE }.into(),
            buy_token: if output_native { NATIVE } else { "token" }.into(),
            slippage_bps: 0,
            deadline_ms: u64::MAX,
        };
        let p = json!({"ready":true,"sourceNativeSol":!output_native,"destinationNativeSol":output_native,"sourceTokenAccount":"payer","destinationTokenAccount":"payer","sourceTokenProgram":if output_native {TOKEN}else{SYSTEM},"destinationTokenProgram":if output_native {SYSTEM}else{TOKEN},"rentLamports":"2039280","rentRefundLamports":"0","feeLamports":"5000","preparationHash":"prepared"});
        let observed = json!({"success":true,"verified":true,"actualInput":"1000000","actualOutput":"30000","feeLamports":"5000","actualPersistentRentLamports":"2039280","payerLamportDelta":if output_native {"-2014280"}else{"-3044280"},"transactionSha256":"executed"});
        (c, p, observed)
    }
    #[test]
    fn native_fee_adjusted_amounts_keep_capital_deposits_out_of_network_expense() {
        let (c, p, o) = sample(true);
        let value = evaluated(&c, &p, &o, Source::CapturedSbf).unwrap();
        assert_eq!(value["netBuyAmountEstimate"], "25000");
        assert_eq!(value["actualPayerNativeDelta"], "-2014280");
        assert_eq!(value["retainedRentDepositNative"], "2039280");
        assert_eq!(value["tokenValuationStatus"], "NATIVE_UNIT_IDENTITY");
        assert_eq!(value["quoteRankingApplied"], false);
        let (c, p, o) = sample(false);
        let value = evaluated(&c, &p, &o, Source::LocalSbf).unwrap();
        assert_eq!(value["netSellAmountEstimate"], "1005000");
        assert!(value["netBuyAmountEstimate"].is_null());
    }
    #[test]
    fn fee_larger_than_output_is_explicitly_unavailable_and_exact_zero_is_not_clamped() {
        let (c, mut p, mut o) = sample(true);
        p["rentLamports"] = "0".into();
        for (gross, delta, net) in [("1000", "-4000", None), ("5000", "0", Some("0"))] {
            o["actualOutput"] = gross.into();
            o["payerLamportDelta"] = delta.into();
            let value = evaluated(&c, &p, &o, Source::LocalSbf).unwrap();
            assert_eq!(
                value["netBuyAmountEstimate"],
                net.map_or(Value::Null, |n| json!(n))
            );
            if net.is_none() {
                assert_eq!(
                    value["unavailableReason"],
                    "NETWORK_FEE_EXCEEDS_GROSS_OUTPUT"
                );
            }
        }
    }
    #[test]
    fn native_economics_uses_observed_receipt_rent_and_rejects_noncanonical_or_inconsistent_amounts(
    ) {
        let (c, p, mut o) = sample(true);
        o["feeLamports"] = "6000".into();
        o["actualPersistentRentLamports"] = "1000000".into();
        o["payerLamportDelta"] = "-976000".into();
        let v = evaluated(&c, &p, &o, Source::RpcReceipt).unwrap();
        assert_eq!(v["netBuyAmountEstimate"], "24000");
        assert!(evaluated(&c, &p, &o, Source::LocalSbf).is_err());
        for (key, bad) in [
            ("feeLamports", "06000"),
            ("actualInput", "18446744073709551616"),
            ("payerLamportDelta", "-976001"),
            ("actualPersistentRentLamports", "-1"),
        ] {
            let mut bad_observation = o.clone();
            bad_observation[key] = bad.into();
            assert!(
                evaluated(&c, &p, &bad_observation, Source::RpcReceipt).is_err(),
                "{key}"
            );
        }
    }
    #[test]
    fn only_explicit_native_settlement_gets_identity_valuation() {
        let (mut c, mut p, o) = sample(true);
        c.buy_token = WSOL.into();
        p["destinationNativeSol"] = false.into();
        p["destinationTokenProgram"] = TOKEN.into();
        p["unwrapDestinationSol"] = true.into();
        assert_eq!(
            evaluated(&c, &p, &o, Source::LocalSbf).unwrap()["netBuyAmountEstimate"],
            "25000"
        );
        p["unwrapDestinationSol"] = false.into();
        assert!(evaluated(&c, &p, &o, Source::LocalSbf).unwrap()["netBuyAmountEstimate"].is_null());
        let (mut c, mut p, o) = sample(false);
        c.sell_token = WSOL.into();
        p["sourceNativeSol"] = false.into();
        p["refundWrappedSource"] = true.into();
        p["sourceAccountCreation"] = true.into();
        p["sourceTokenProgram"] = TOKEN.into();
        assert_eq!(
            evaluated(&c, &p, &o, Source::LocalSbf).unwrap()["netSellAmountEstimate"],
            "1005000"
        );
        for (field, value) in [
            ("sourceAccountCreation", json!(false)),
            ("sourceTokenProgram", json!(SYSTEM)),
            ("sourceNativeSol", json!(true)),
        ] {
            let mut changed = p.clone();
            changed[field] = value;
            assert!(evaluated(&c, &changed, &o, Source::LocalSbf).is_err());
        }
        let (mut c, p, o) = sample(true);
        c.recipient = "different-recipient".into();
        assert!(evaluated(&c, &p, &o, Source::LocalSbf).is_err());
        let (c, mut p, o) = sample(true);
        p["destinationTokenAccount"] = "different-account".into();
        assert!(evaluated(&c, &p, &o, Source::LocalSbf).is_err());
    }
}
