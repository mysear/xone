#[test]
fn lm_config_requires_complete_pinned_identity_and_finite_source_limits() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    cfg.service.name = "chain-parser-go".into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].key = "evm:31337".into();
    cfg.chains[0].owner_service = "chain-parser-go".into();
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-lm-state.json"
    ))
    .unwrap();
    for c in f["cases"].as_array().unwrap() {
        cfg.chains[0].pools = vec![serde_json::from_value(c["config"].clone()).unwrap()];
        cfg.validate("chain-parser-go").unwrap();
    }
    cfg.chains[0].pools = vec![serde_json::from_value(f["cases"][2]["config"].clone()).unwrap()];
    for mode in 0..13 {
        let mut bad = cfg.clone();
        let p = &mut bad.chains[0].pools[0];
        let s = p.pancake_lm.as_mut().unwrap();
        match mode {
            0 => p.pancake_lm = None,
            1 => s.expected_master_chef_code_sha256.clear(),
            2 => s.expected_lm_pool_code_sha256.clear(),
            3 => s.expected_factory_code_sha256.clear(),
            4 => s.token_policy = "taxed".into(),
            5 => s.expected_lm_pool = p.id.clone(),
            6 => s.max_observations = 0,
            7 => s.max_observations = 4097,
            8 => s.max_steps = 0,
            9 => s.expected_cake = xone_domain::singleton::ZERO_ADDRESS.into(),
            10 => p.protocol = "pancake-v3".into(),
            11 => p.accounts.push(p.token0.clone()),
            _ => s.expected_token0_code_sha256.clear(),
        };
        assert!(bad.validate("chain-parser-go").is_err(), "{}", mode);
    }
}
