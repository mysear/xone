use xone_config::*;
fn configured() -> Config {
    let mut c = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/infinity-bin-wire.json"
    ))
    .unwrap();
    let p = &f["snapshot"]["pools"][0];
    let s = &p["infinity_bin"];
    c.chains[0].pools=vec![serde_json::from_value(serde_json::json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"fee_numerator":p["fee_numerator"],"fee_denominator":1000000,"program_id":"","accounts":[],"infinity_bin":{"manager":s["manager"],"expected_manager_code_sha256":s["manager_code_sha256"],"vault":s["vault"],"expected_vault_code_sha256":s["vault_code_sha256"],"pool_id":s["pool_id"],"bin_step":s["bin_step"],"lp_fee":s["lp_fee"],"expected_token0_code_sha256":s["token0_code_sha256"],"expected_token1_code_sha256":s["token1_code_sha256"],"token_policy":"standard","max_bins":64,"max_tree_words":64,"max_steps":64}})).unwrap()];
    c
}
#[test]
fn static_bin_config_rejects_wrong_key_and_unbounded_readsets() {
    let c = configured();
    c.validate("dex-router").unwrap();
    for i in 0..9 {
        let mut q = c.clone();
        let p = &mut q.chains[0].pools[0];
        let s = p.infinity_bin.as_mut().unwrap();
        match i {
            0 => s.max_bins = 1025,
            1 => s.max_tree_words = 0,
            2 => s.max_steps = 0,
            3 => s.token_policy.clear(),
            4 => s.expected_manager_code_sha256.clear(),
            5 => s.bin_step = 101,
            6 => s.pool_id = format!("0x{}", "0".repeat(64)),
            7 => p.token0 = s.vault.clone(),
            _ => p.protocol = "pancake-infinity-cl".into(),
        };
        assert!(q.validate("dex-router").is_err(), "{i}")
    }
}
