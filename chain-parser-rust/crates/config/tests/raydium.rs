use xone_config::*;
#[test]
fn raydium_requires_explicit_program_version_and_finalized_context() {
    let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    cfg.chains[0].confirmation_tag = "finalized".into();
    let p: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/raydium-state.json")).unwrap();
    let s = &p["raydium_cpmm"];
    let value = serde_json::json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"fee_numerator":p["fee_numerator"],"fee_denominator":1000000,"program_id":s["program_id"],"accounts":[p["pool_id"],s["vault0"],s["vault1"],s["amm_config"],p["token0"],p["token1"]],"raydium_cpmm":{"expected_programdata":s["programdata"],"expected_program_code_sha256":s["program_code_sha256"],"expected_deployed_slot":s["deployed_slot"],"expected_token_program_code_sha256":s["token_program_code_sha256"]}});
    cfg.chains[0].pools = vec![serde_json::from_value(value).unwrap()];
    cfg.validate("chain-parser-rust").unwrap();
    cfg.chains[0].confirmation_tag = "confirmed".into();
    assert!(cfg.validate("chain-parser-rust").is_err());
    cfg.chains[0].confirmation_tag = "finalized".into();
    cfg.chains[0].pools[0]
        .raydium_cpmm
        .as_mut()
        .unwrap()
        .expected_program_code_sha256 = "aa".into();
    assert!(cfg.validate("chain-parser-rust").is_err());
}
