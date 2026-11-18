use ring::signature::Ed25519KeyPair;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use xone_domain::{hash, native_valuation::PricePolicy, Amount};
use xone_execution_preparation::{
    market_feed::{derive, unsigned_observation, MarketRead},
    native_valuation::{signing_message, PriceAnchor, SignedPrice},
};
fn vectors() -> Value {
    serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_MARKET_FEED_VECTORS").unwrap()).unwrap(),
    )
    .unwrap()
}
fn de<T: DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}
fn resign(v: &Value, policy: &PricePolicy) -> SignedPrice {
    let mut signed: SignedPrice = de(v);
    signed.observation.policy_hash = policy.identity().unwrap();
    let message = signing_message(&signed.observation).unwrap();
    for s in &mut signed.signatures {
        let seed = s.key_id.parse::<u8>().unwrap();
        s.signature = hex::encode(
            Ed25519KeyPair::from_seed_unchecked(&[seed; 32])
                .unwrap()
                .sign(&message)
                .as_ref(),
        );
    }
    signed
}
fn word(n: u64) -> String {
    format!("{n:064x}")
}

#[test]
#[ignore = "requires current independently generated synthetic and public vectors"]
fn approved_market_prices_match_public_raw_returns_and_independent_full_width_ratios() {
    let v = vectors();
    assert_eq!(v["synthetic"].as_array().unwrap().len(), 48);
    assert!(!v["public"].as_array().unwrap().is_empty());
    for row in v["synthetic"]
        .as_array()
        .unwrap()
        .iter()
        .chain(v["public"].as_array().unwrap())
    {
        let c = &row["candidate"];
        let policy: PricePolicy = de(&c["policy"]);
        let anchor: PriceAnchor = de(&c["anchor"]);
        let read: MarketRead = de(&c["market_feed"]);
        let result = derive(&policy, &anchor, &read);
        if row["expected"]["overflow"] == true {
            assert_eq!(result.err().unwrap().0, "AMOUNT_OVERFLOW");
            continue;
        }
        let derived = result.unwrap();
        assert_eq!(
            derived.token_atoms.to_string(),
            row["expected"]["token_atoms"]
        );
        assert_eq!(
            derived.native_atoms.to_string(),
            row["expected"]["native_atoms"]
        );
        let unsigned = unsigned_observation(
            &policy,
            anchor.clone(),
            read,
            c["valid_until_ms"].as_u64().unwrap(),
        )
        .unwrap();
        assert_eq!(
            hash(&unsigned).unwrap(),
            hash(&row["signed"]["observation"]).unwrap()
        );
        let signed: SignedPrice = de(&row["signed"]);
        let price = signed.authorize(&policy).unwrap();
        price
            .validate_at(&anchor, anchor.timestamp_ms + 1000)
            .unwrap();
        assert_eq!(price.evidence()["marketFeed"], derived.evidence);
        assert_eq!(
            price.evidence()["marketFeed"]["storageProofVerified"],
            false
        );
        assert_eq!(price.evidence()["sourceIndependenceAttested"], false);
        if let Some(conversions) = row["conversions"].as_array() {
            for case in conversions {
                let cost: Amount = case["cost"].as_str().unwrap().parse().unwrap();
                let value = price.convert_cost(&cost, &anchor, anchor.timestamp_ms + 1000);
                if case["expected"].is_null() {
                    assert_eq!(value.err().unwrap().0, "AMOUNT_OVERFLOW");
                } else {
                    assert_eq!(value.unwrap().to_string(), case["expected"]);
                }
            }
        }
    }
}

#[test]
#[ignore = "requires independently generated feed returns"]
fn market_authorization_rejects_resigned_source_abi_upgrades_time_and_ratio_substitutions() {
    let vectors = vectors();
    let row = &vectors["synthetic"][12];
    let p: PricePolicy = de(&row["candidate"]["policy"]);
    let signed: SignedPrice = de(&row["signed"]);
    signed.authorize(&p).unwrap();
    let mut mutations: Vec<(&str, Value)> = vec![
        (
            "/observation/market_feed/native_usd/proxy_code",
            json!("0x6009"),
        ),
        (
            "/observation/market_feed/token_usd/aggregator_code",
            json!("0x6009"),
        ),
        (
            "/observation/market_feed/native_usd/aggregator_return",
            json!(format!("0x{}", "ff".repeat(32))),
        ),
        (
            "/observation/market_feed/native_usd/decimals_return",
            json!(format!("0x{}", word(9))),
        ),
        (
            "/observation/market_feed/token_decimals_return",
            json!(format!("0x{}", word(7))),
        ),
        (
            "/observation/market_feed/token_usd/description_return",
            json!("0x"),
        ),
        ("/observation/market_feed/provider_id", json!("rpc-forged")),
        ("/observation/market_feed/source_read_at_ms", json!(1)),
        ("/observation/token_atoms", json!("2")),
        ("/observation/native_atoms", json!("2")),
        ("/observation/observed_at_ms", json!(1700000000001u64)),
        ("/observation/valid_until_ms", json!(1700003600000u64)),
    ];
    let original = signed
        .observation
        .market_feed
        .as_ref()
        .unwrap()
        .native_usd
        .round_return
        .clone();
    let words: Vec<String> = original.as_bytes()[2..]
        .chunks_exact(64)
        .map(|b| std::str::from_utf8(b).unwrap().to_owned())
        .collect();
    for (index, replacement) in [
        (0, word(0)),
        (0, format!("{:0>64}", "1".to_owned() + &"0".repeat(20))),
        (1, word(0)),
        (1, "f".repeat(64)),
        (2, word(0)),
        (2, word(1700000001)),
        (3, word(1700000001)),
        (3, word(1699990000)),
        (3, "f".repeat(64)),
        (4, word(6)),
    ] {
        let mut w = words.clone();
        w[index] = replacement;
        mutations.push((
            "/observation/market_feed/native_usd/round_return",
            json!(format!("0x{}", w.join(""))),
        ));
    }
    for raw in [
        format!("{original}00"),
        original[..original.len() - 2].to_owned(),
        original.to_uppercase(),
    ] {
        mutations.push((
            "/observation/market_feed/native_usd/round_return",
            json!(raw),
        ));
    }
    for (pointer, value) in mutations {
        let mut bad = row["signed"].clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(resign(&bad, &p).authorize(&p).is_err(), "{pointer}: {bad}");
    }
    let mut changed_policy: Value = serde_json::to_value(&p).unwrap();
    changed_policy["market_feed"]["native_usd"]["aggregator"] =
        json!(format!("0x{}", "66".repeat(20)));
    let changed_policy: PricePolicy = de(&changed_policy);
    assert!(resign(&row["signed"], &changed_policy)
        .authorize(&changed_policy)
        .is_err());
}

#[test]
#[ignore = "requires independently generated feed returns"]
fn market_feed_age_is_checked_at_execution_without_reinterpreting_capture_time() {
    let v = vectors();
    let row = &v["synthetic"][12];
    let p: PricePolicy = de(&row["candidate"]["policy"]);
    let anchor: PriceAnchor = de(&row["candidate"]["anchor"]);
    let mut signed = row["signed"].clone();
    let until = anchor.timestamp_ms + 3_599_000;
    signed["observation"]["valid_until_ms"] = json!(until);
    let price = resign(&signed, &p).authorize(&p).unwrap();
    price.validate_at(&anchor, until - 1).unwrap();
    assert!(price.validate_at(&anchor, until).is_err());
    let mut crossed = anchor.clone();
    crossed.block_hash = format!("0x{}", "88".repeat(32));
    assert!(price
        .validate_at(&crossed, anchor.timestamp_ms + 1000)
        .is_err());
    // Reading an old round again does not renew that round's update time.
    signed["observation"]["market_feed"]["source_read_at_ms"] = json!(until);
    assert!(resign(&signed, &p).authorize(&p).is_err());
}

#[test]
#[ignore = "requires independently generated feed policies"]
fn market_policy_is_ethereum_only_and_feed_evidence_cannot_downgrade_to_plain_rates() {
    let v = vectors();
    let row = &v["synthetic"][12];
    let p: PricePolicy = de(&row["candidate"]["policy"]);
    for pointer in [
        "/chain_key",
        "/genesis_hash",
        "/market_feed/native_decimals",
        "/market_feed/token_decimals",
        "/market_feed/native_usd/description",
        "/market_feed/token_usd/decimals",
        "/market_feed/native_usd/min_answer",
        "/market_feed/token_usd/max_age_ms",
        "/market_feed/native_usd/proxy_code_sha256",
    ] {
        let mut value = serde_json::to_value(&p).unwrap();
        let old = value.pointer_mut(pointer).unwrap();
        *old = if old.is_number() {
            json!(999999999u64)
        } else {
            json!("invalid")
        };
        let parsed = serde_json::from_value::<PricePolicy>(value);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "{pointer}"
        );
    }
    let mut no_evidence = row["signed"].clone();
    no_evidence["observation"]
        .as_object_mut()
        .unwrap()
        .remove("market_feed");
    assert!(resign(&no_evidence, &p).authorize(&p).is_err());
    let mut plain = p.clone();
    plain.market_feed = None;
    assert!(resign(&row["signed"], &plain).authorize(&plain).is_err());
    let mut unsigned = row["signed"].clone();
    unsigned["observation"]["market_feed"]["token_usd"]["unapproved_round"] = json!(true);
    assert!(serde_json::from_value::<SignedPrice>(unsigned).is_err());
}
