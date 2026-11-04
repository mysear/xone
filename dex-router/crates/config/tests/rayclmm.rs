use xone_config::*;
#[test]
fn rayclmm_requires_finalized_explicit_version_and_bitmap_window() {
    let mut cfg = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    cfg.chains.retain(|ch| ch.key.starts_with("solana:"));
    cfg.chains[0].confirmation_tag = "finalized".into();
    let p: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    let s = &p["raydium_clmm"];
    let v = serde_json::json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"fee_numerator":p["fee_numerator"],"fee_denominator":1000000,"program_id":s["program_id"],"accounts":[p["pool_id"],s["vault0"],s["vault1"],s["amm_config"],p["token0"],p["token1"],s["observation"]],"raydium_clmm":{"expected_programdata":s["programdata"],"expected_program_code_sha256":s["program_code_sha256"],"expected_deployed_slot":s["deployed_slot"],"expected_token_program_code_sha256":s["token_program_code_sha256"],"tick_array_starts":[-3840,0,3840],"max_steps":512}});
    cfg.chains[0].pools = vec![serde_json::from_value(v).unwrap()];
    cfg.validate("dex-router").unwrap();
    cfg.chains[0].pools[0].protocol = "raydium-clmm-token-fee".into();
    cfg.validate("dex-router").unwrap();
    let original = cfg.chains[0].pools[0]
        .raydium_clmm
        .as_ref()
        .unwrap()
        .expected_programdata
        .clone();
    cfg.chains[0].pools[0]
        .raydium_clmm
        .as_mut()
        .unwrap()
        .expected_programdata = xone_domain::orca::PROGRAM_DATA.into();
    assert!(cfg.validate("dex-router").is_err());
    cfg.chains[0].pools[0]
        .raydium_clmm
        .as_mut()
        .unwrap()
        .expected_programdata = original;
    cfg.chains[0].confirmation_tag = "confirmed".into();
    assert!(cfg.validate("dex-router").is_err());
    cfg.chains.retain(|ch| ch.key.starts_with("solana:"));
    cfg.chains[0].confirmation_tag = "finalized".into();
    cfg.chains[0].pools[0].protocol = "raydium-clmm-custom-seed".into();
    cfg.chains[0].pools[0]
        .raydium_clmm
        .as_mut()
        .unwrap()
        .tick_array_starts = vec![30660, 30720, 30780];
    cfg.validate("dex-router").unwrap();
    cfg.chains[0].pools[0].protocol = "raydium-clmm-dynamic".into();
    cfg.validate("dex-router").unwrap();
    cfg.chains[0].pools[0].protocol = "raydium-clmm-bitmap".into();
    cfg.validate("dex-router").unwrap();
    cfg.chains[0].pools[0].protocol = "raydium-clmm-static".into();
    assert!(cfg.validate("dex-router").is_err());
    cfg.chains[0].pools[0].protocol = "raydium-clmm-custom-seed".into();
    cfg.chains[0].pools[0]
        .raydium_clmm
        .as_mut()
        .unwrap()
        .tick_array_starts = vec![-61500, -61440, -61380];
    cfg.validate("dex-router").unwrap();
    cfg.chains[0].pools[0]
        .raydium_clmm
        .as_mut()
        .unwrap()
        .tick_array_starts = vec![0, 1, 3840];
    assert!(cfg.validate("dex-router").is_err());
}
