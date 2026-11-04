#[test]
fn adaptive_rewards_config_requires_finalized_identity_and_three_contiguous_arrays() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-rewards-pools.json"
    ))
    .unwrap();
    let mut c = xone_config::load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    c.chains.retain(|ch| ch.key.starts_with("solana:"));
    c.chains[0].confirmation_tag = "finalized".into();
    c.chains[0].pools = vec![serde_json::from_value(f["cases"][0]["config"].clone()).unwrap()];
    c.validate("dex-router").unwrap();
    for bad in 0..8 {
        let mut q = c.clone();
        let ch = &mut q.chains[0];
        let p = &mut ch.pools[0];
        let v = p.orca_whirlpool.as_mut().unwrap();
        match bad {
            0 => ch.confirmation_tag = "confirmed".into(),
            1 => v.tick_array_starts.push(11264),
            2 => v.tick_array_starts[2] += 88,
            3 => v.tick_array_starts[0] += 1,
            4 => v.max_steps = 0,
            5 => p.program_id = p.id.clone(),
            6 => v.expected_token_program_code_sha256.clear(),
            _ => v.tick_array_starts = vec![-5632, 0],
        };
        assert!(q.validate("dex-router").is_err(), "bad {bad}");
    }
}
