use serde_json::{json, Value};
use xone_config::*;

#[test]
fn approved_market_policy_is_optional_and_ethereum_specific() {
    let base = load("../../config/config-test.yaml", "dex-router").unwrap();
    assert!(base.config.simulation.native_valuation.is_none());
    let f = json!({"proxy":format!("0x{}", "01".repeat(20)),"proxy_code_sha256":"11".repeat(32),
        "aggregator":format!("0x{}", "02".repeat(20)),"aggregator_code_sha256":"22".repeat(32),
        "description":"ETH / USD","decimals":8,"min_answer":"1","max_answer":"100000000000000000","max_age_ms":3600000});
    let value = json!({"source_id":"approved-feed","chain_key":"evm:1",
        "genesis_hash":xone_domain::native_valuation::ETHEREUM_GENESIS,
        "token":format!("0x{}", "10".repeat(20)),"signers":{"a":"11".repeat(32),"b":"22".repeat(32)},
        "threshold":2,"max_age_ms":3600000,"max_validity_ms":3600000,
        "market_feed":{"schema_version":1,"native_decimals":18,"token_decimals":6,"native_usd":f,"token_usd":f}});
    let p: xone_domain::native_valuation::PricePolicy =
        serde_json::from_value(value.clone()).unwrap();
    p.validate().unwrap();
    let mut unsupported = value.clone();
    unsupported["chain_key"] = "evm:10".into();
    let p: xone_domain::native_valuation::PricePolicy =
        serde_json::from_value(unsupported).unwrap();
    assert!(p.validate().is_err());
    let mut unknown = value;
    unknown["market_feed"]["trust_rpc"] = true.into();
    assert!(serde_json::from_value::<xone_domain::native_valuation::PricePolicy>(unknown).is_err());
}

#[test]
#[ignore = "requires current Go market-feed config exports"]
fn approved_market_policy_hashes_and_rejections_match_current_go() {
    let directory =
        std::path::PathBuf::from(std::env::var("XONE_MARKET_FEED_CONFIG_VECTORS").unwrap());
    let vectors: Value =
        serde_json::from_slice(&std::fs::read(directory.join("market-vectors.json")).unwrap())
            .unwrap();
    assert_eq!(vectors["valid"].as_array().unwrap().len(), 8);
    assert_eq!(vectors["invalid"].as_array().unwrap().len(), 14);
    for (rows, valid) in [("valid", true), ("invalid", false)] {
        for row in vectors[rows].as_array().unwrap() {
            let name = row["file"].as_str().unwrap();
            assert!(!name.contains('/') && !name.contains('\\') && name.ends_with(".json"));
            let loaded = load(directory.join(name), "dex-router");
            if valid {
                let c = loaded.unwrap();
                assert_eq!(c.public_hash, row["public_hash"].as_str().unwrap());
                assert_eq!(c.economic_hash, row["economic_hash"].as_str().unwrap());
            } else {
                assert!(loaded.is_err(), "{}", row["mode"]);
            }
        }
    }
}
