#[test]
fn adaptive_dynamic_config_requires_finalized_identity_and_contiguous_array_window() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-dynamic-pools.json"
    ))
    .unwrap();
    let mut c = xone_config::load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    c.chains.retain(|ch| ch.key.starts_with("solana:"));
    c.chains[0].confirmation_tag = "finalized".into();
    c.chains[0].pools = vec![serde_json::from_value(f["cases"][0]["config"].clone()).unwrap()];
    c.chains[0].pools[0].protocol = "orca-whirlpools-adaptive-dynamic".into();
    c.validate("chain-parser-rust").unwrap();
    let v = c.chains[0].pools[0].orca_whirlpool.as_mut().unwrap();
    v.tick_array_starts.push(11264);
    c.validate("chain-parser-rust").unwrap();
    let v = c.chains[0].pools[0].orca_whirlpool.as_mut().unwrap();
    v.tick_array_starts.extend([16896, 22528, 28160]);
    c.validate("chain-parser-rust").unwrap();
    for bad in 0..9 {
        let mut q = c.clone();
        let ch = &mut q.chains[0];
        let p = &mut ch.pools[0];
        let v = p.orca_whirlpool.as_mut().unwrap();
        match bad {
            0 => ch.confirmation_tag = "confirmed".into(),
            1 => v.tick_array_starts.push(33792),
            2 => v.tick_array_starts[2] += 88,
            3 => v.tick_array_starts[0] += 1,
            4 => v.max_steps = 0,
            5 => p.program_id = p.id.clone(),
            6 => v.expected_token_program_code_sha256.clear(),
            7 => v.tick_array_starts[6] += 5632,
            _ => v.tick_array_starts = vec![-5632, 0],
        };
        assert!(q.validate("chain-parser-rust").is_err(), "bad {bad}");
    }
}
