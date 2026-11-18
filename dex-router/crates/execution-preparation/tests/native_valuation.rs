use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use xone_domain::{native_valuation::PricePolicy, Amount};
use xone_execution_preparation::native_valuation::*;

fn vectors() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/native-valuation-vectors.json"
    ))
    .unwrap()
}
fn source() -> (PricePolicy, SignedPrice, PriceAnchor) {
    let v = vectors();
    (
        serde_json::from_value(v["policy"].clone()).unwrap(),
        serde_json::from_value(v["cases"][3]["signed"].clone()).unwrap(),
        serde_json::from_value(v["anchor"].clone()).unwrap(),
    )
}
fn resign(price: &mut SignedPrice) {
    let message = signing_message(&price.observation).unwrap();
    price.signatures = [1u8, 2]
        .into_iter()
        .map(|i| {
            let key = Ed25519KeyPair::from_seed_unchecked(&[i; 32]).unwrap();
            PriceSignature {
                key_id: i.to_string(),
                signature: hex::encode(key.sign(&message).as_ref()),
            }
        })
        .collect();
}

#[test]
fn independent_solders_signatures_and_42_python_integer_conversions_match() {
    let v = vectors();
    let policy: PricePolicy = serde_json::from_value(v["policy"].clone()).unwrap();
    let anchor: PriceAnchor = serde_json::from_value(v["anchor"].clone()).unwrap();
    for i in [1u8, 2, 3] {
        let key = Ed25519KeyPair::from_seed_unchecked(&[i; 32]).unwrap();
        assert_eq!(
            policy.signers[&i.to_string()],
            hex::encode(key.public_key().as_ref())
        );
    }
    let rows = v["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 42);
    for row in rows {
        let signed: SignedPrice = serde_json::from_value(row["signed"].clone()).unwrap();
        let price = signed.authorize(&policy).unwrap();
        let cost: Amount = row["cost"].as_str().unwrap().parse().unwrap();
        let actual = price.convert_cost(&cost, &anchor, anchor.timestamp_ms);
        if row["overflow"] == true {
            assert_eq!(actual.unwrap_err().0, "AMOUNT_OVERFLOW");
        } else {
            assert_eq!(
                actual.unwrap().to_string(),
                row["expected"].as_str().unwrap()
            );
        }
    }
}

#[test]
fn authority_quorum_policy_rotation_unknown_signers_and_payload_substitutions_fail() {
    let (policy, signed, _) = source();
    let value = serde_json::to_value(&signed).unwrap();
    for (field, new) in [
        ("token_atoms", json!("2")),
        ("native_atoms", json!("0")),
        ("token", json!("0x0000000000000000000000000000000000000020")),
        ("observed_at_ms", json!(1)),
        ("valid_until_ms", json!(u64::MAX)),
        ("policy_hash", json!("00".repeat(32))),
    ] {
        let mut v = value.clone();
        v["observation"][field] = new;
        let price: SignedPrice = serde_json::from_value(v).unwrap();
        assert!(price.authorize(&policy).is_err(), "{field}");
    }
    for mode in [
        "missing",
        "duplicate",
        "unknown",
        "bad",
        "uppercase",
        "extra",
    ] {
        let mut p = signed.clone();
        match mode {
            "missing" => {
                p.signatures.pop();
            }
            "duplicate" => p.signatures[1] = p.signatures[0].clone(),
            "unknown" => p.signatures[1].key_id = "unconfigured".into(),
            "bad" => p.signatures[1].signature = "00".repeat(64),
            "uppercase" => p.signatures[1].signature = p.signatures[1].signature.to_uppercase(),
            _ => p.signatures.push(PriceSignature {
                key_id: "3".into(),
                signature: "00".repeat(64),
            }),
        }
        assert!(p.authorize(&policy).is_err(), "{mode}");
    }
    let mut changed = policy.clone();
    changed.max_age_ms -= 1;
    assert!(signed.authorize(&changed).is_err());
    let mut changed = policy.clone();
    changed
        .signers
        .insert("2".into(), changed.signers["1"].clone());
    assert!(signed.authorize(&changed).is_err());
    let mut unknown = value;
    unknown["observation"]["acceptCallerPrice"] = true.into();
    assert!(serde_json::from_value::<SignedPrice>(unknown).is_err());
}

#[test]
fn signed_prices_cannot_cross_source_blocks_chains_time_or_expiry() {
    let (policy, mut signed, anchor) = source();
    let price = signed.authorize(&policy).unwrap();
    for field in [
        "chain_key",
        "genesis_hash",
        "block_number",
        "block_hash",
        "parent_hash",
        "timestamp_ms",
    ] {
        let mut changed = serde_json::to_value(&anchor).unwrap();
        changed[field] = match field {
            "block_number" => json!(11),
            "timestamp_ms" => json!(anchor.timestamp_ms + 1),
            "chain_key" => json!("evm:10"),
            _ => json!(format!("0x{}", "44".repeat(32))),
        };
        assert_eq!(
            price
                .validate_at(
                    &serde_json::from_value(changed).unwrap(),
                    anchor.timestamp_ms
                )
                .unwrap_err()
                .0,
            "NATIVE_VALUATION_ANCHOR"
        );
    }
    assert!(price
        .validate_at(&anchor, anchor.timestamp_ms + 59999)
        .is_ok());
    assert_eq!(
        price
            .validate_at(&anchor, anchor.timestamp_ms + 60000)
            .unwrap_err()
            .0,
        "NATIVE_VALUATION_TIME"
    );
    assert!(price.validate_at(&anchor, anchor.timestamp_ms - 1).is_err());
    signed.observation.observed_at_ms = anchor.timestamp_ms + 1;
    signed.observation.valid_until_ms = anchor.timestamp_ms + 60001;
    resign(&mut signed);
    assert_eq!(
        signed
            .authorize(&policy)
            .unwrap()
            .validate_at(&anchor, anchor.timestamp_ms + 2)
            .unwrap_err()
            .0,
        "NATIVE_VALUATION_TIME"
    );
    signed.observation.observed_at_ms = anchor.timestamp_ms - 60001;
    signed.observation.valid_until_ms = anchor.timestamp_ms - 1;
    resign(&mut signed);
    assert!(signed
        .authorize(&policy)
        .unwrap()
        .validate_at(&anchor, anchor.timestamp_ms)
        .is_err());
}

#[test]
fn transaction_net_amounts_bind_token_units_round_up_and_never_clamp_or_double_charge() {
    let (policy, mut signed, anchor) = source();
    signed.observation.token_atoms = "3".parse().unwrap();
    signed.observation.native_atoms = "2".parse().unwrap();
    resign(&mut signed);
    let buy = signed.authorize(&policy).unwrap();
    let mut sell_policy = policy.clone();
    sell_policy.token = "0x0000000000000000000000000000000000000020".into();
    signed.observation.token = sell_policy.token.clone();
    signed.observation.policy_hash = sell_policy.identity().unwrap();
    resign(&mut signed);
    let sell = signed.authorize(&sell_policy).unwrap();
    let input: Amount = "7".parse().unwrap();
    let output: Amount = "10".parse().unwrap();
    let debit: Amount = "3".parse().unwrap();
    let calculate = |out: &Amount, cost: &Amount| {
        evaluated_amounts(
            Settlement {
                input: &input,
                output: out,
                debit: cost,
                sell_token: &sell_policy.token,
                buy_token: &policy.token,
                anchor: &anchor,
                at_ms: anchor.timestamp_ms,
            },
            Some(&sell),
            Some(&buy),
        )
        .unwrap()
    };
    let value = calculate(&output, &debit);
    assert_eq!(value["feeAdjustedBuyAmount"], "5");
    assert_eq!(value["feeAdjustedSellAmount"], "12");
    assert_eq!(value["quoteRankingApplied"], false);
    assert_eq!(value["additionalOperationsIncluded"], false);
    assert_eq!(
        calculate(&"5".parse().unwrap(), &debit)["feeAdjustedBuyAmount"],
        "0"
    );
    let negative = calculate(&"4".parse().unwrap(), &debit);
    assert!(negative["feeAdjustedBuyAmount"].is_null());
    assert_eq!(
        negative["buyUnavailableReason"],
        "NETWORK_FEE_EXCEEDS_GROSS_OUTPUT"
    );
    assert_eq!(
        calculate(&output, &Amount::zero())["feeAdjustedBuyAmount"],
        "10"
    );
    let max: Amount = ((num_bigint::BigUint::from(1u8) << 256usize)
        - num_bigint::BigUint::from(1u8))
    .to_string()
    .parse()
    .unwrap();
    let overflow = evaluated_amounts(
        Settlement {
            input: &max,
            output: &output,
            debit: &debit,
            sell_token: &sell_policy.token,
            buy_token: &policy.token,
            anchor: &anchor,
            at_ms: anchor.timestamp_ms,
        },
        Some(&sell),
        Some(&buy),
    )
    .unwrap();
    assert!(overflow["feeAdjustedSellAmount"].is_null());
    assert_eq!(overflow["sellUnavailableReason"], "AMOUNT_OVERFLOW");
    assert_eq!(overflow["feeAdjustedBuyAmount"], "5");
    assert!(evaluated_amounts(
        Settlement {
            input: &input,
            output: &output,
            debit: &debit,
            sell_token: &policy.token,
            buy_token: &sell_policy.token,
            anchor: &anchor,
            at_ms: anchor.timestamp_ms
        },
        Some(&sell),
        Some(&buy)
    )
    .is_err());
    let unknown = evaluated_amounts(
        Settlement {
            input: &input,
            output: &output,
            debit: &debit,
            sell_token: &sell_policy.token,
            buy_token: &policy.token,
            anchor: &anchor,
            at_ms: anchor.timestamp_ms,
        },
        None,
        None,
    )
    .unwrap();
    assert!(unknown["feeAdjustedBuyAmount"].is_null());
    let book = PriceBook {
        schema_version: 1,
        prices: vec![signed.clone(), signed],
    };
    assert!(book.authorize(&[sell_policy]).is_err());
}
