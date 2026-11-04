use xone_config::*;
#[test]
fn orca_static_config_requires_finalized_identity_and_array_budget() {
    let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    cfg.chains[0].confirmation_tag = "finalized".into();
    let p: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap();
    let s = &p["orca_whirlpool"];
    let value = serde_json::json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"fee_numerator":p["fee_numerator"],"fee_denominator":1000000,"program_id":s["program_id"],"accounts":[p["pool_id"],s["vault0"],s["vault1"],s["whirlpools_config"],p["token0"],p["token1"]],"orca_whirlpool":{"expected_programdata":s["programdata"],"expected_program_code_sha256":s["program_code_sha256"],"expected_deployed_slot":s["deployed_slot"],"expected_token_program_code_sha256":s["token_program_code_sha256"],"tick_array_starts":[-5632,0,5632],"max_steps":512}});
    cfg.chains[0].pools = vec![serde_json::from_value(value).unwrap()];
    cfg.validate("chain-parser-rust").unwrap();
    cfg.chains[0].confirmation_tag = "confirmed".into();
    assert!(cfg.validate("chain-parser-rust").is_err());
    cfg.chains[0].confirmation_tag = "finalized".into();
    cfg.chains[0].pools[0]
        .orca_whirlpool
        .as_mut()
        .unwrap()
        .tick_array_starts = vec![0, 0, 5632];
    assert!(cfg.validate("chain-parser-rust").is_err());
}
