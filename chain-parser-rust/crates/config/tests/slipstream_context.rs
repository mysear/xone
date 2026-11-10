#[test]
fn contextual_config_requires_known_module_versions_and_explicit_origin_scope() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    cfg.service.name = "chain-parser-go".into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].key = "evm:31337".into();
    cfg.chains[0].owner_service = "chain-parser-go".into();
    let file: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-context-state.json"
    ))
    .unwrap();
    for case in file["cases"].as_array().unwrap() {
        cfg.chains[0].pools = vec![serde_json::from_value(case["config"].clone()).unwrap()];
        cfg.validate("chain-parser-go").unwrap();
    }
    let case = file["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["mode"] == "dynamic")
        .unwrap();
    cfg.chains[0].pools = vec![serde_json::from_value(case["config"].clone()).unwrap()];
    for bad in 0..17 {
        let mut value = cfg.clone();
        let p = &mut value.chains[0].pools[0];
        let s = p.slipstream_context.as_mut().unwrap();
        match bad {
            0 => p.slipstream_context = None,
            1 => s.expected_voter_code_sha256.clear(),
            2 => s.token_policy = "taxed".into(),
            3 => s.origins.clear(),
            4 => s.origins.push(s.origins[0].clone()),
            5 => s.swap_fee_module.kind = "unknown".into(),
            6 => s.unstaked_fee_module.kind = "dynamic".into(),
            7 => s.max_observations = 0,
            8 => s.max_observations = 4097,
            9 => s.max_steps = 0,
            10 => s.expected_implementation = p.id.clone(),
            11 => p.protocol = "aerodrome-slipstream".into(),
            12 => p.fee_numerator = 1,
            13 => s.origins = vec![xone_domain::singleton::ZERO_ADDRESS.into()],
            14 => s.swap_fee_module.kind = "none".into(),
            15 => s.expected_token0_code_sha256.clear(),
            16 => p.accounts.push(p.token0.clone()),
            _ => unreachable!(),
        }
        assert!(
            value.validate("chain-parser-go").is_err(),
            "contextual cfg {bad}"
        );
    }
}
