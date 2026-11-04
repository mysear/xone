use xone_config::*;
fn pool() -> PoolConfig {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/orca-adaptive-wire.json"
    ))
    .unwrap();
    let p = &d["snapshot"]["pools"][0];
    let s = &p["orca_whirlpool"];
    serde_json::from_value(serde_json::json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"fee_numerator":p["fee_numerator"],"fee_denominator":1000000,"program_id":s["program_id"],"accounts":[p["pool_id"],s["vault0"],s["vault1"],s["whirlpools_config"],p["token0"],p["token1"]],"orca_whirlpool":{"expected_programdata":s["programdata"],"expected_program_code_sha256":s["program_code_sha256"],"expected_deployed_slot":s["deployed_slot"],"expected_token_program_code_sha256":s["token_program_code_sha256"],"tick_array_starts":s["tick_arrays"].as_array().unwrap().iter().map(|a|a["start_tick_index"].clone()).collect::<Vec<_>>(),"max_steps":8192}})).unwrap()
}
#[test]
fn adaptive_profile_is_classic_spl_finalized_and_three_arrays_only() {
    let mut c = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    c.chains.retain(|ch| ch.key.starts_with("solana:"));
    c.chains[0].confirmation_tag = "finalized".into();
    c.chains[0].pools = vec![pool()];
    c.validate("dex-router").unwrap();
    for bad in 0..7 {
        let mut q = c.clone();
        let ch = &mut q.chains[0];
        let p = &mut ch.pools[0];
        match bad {
            0 => ch.confirmation_tag = "confirmed".into(),
            1 => p
                .orca_whirlpool
                .as_mut()
                .unwrap()
                .tick_array_starts
                .push(11264),
            2 => p
                .orca_whirlpool
                .as_mut()
                .unwrap()
                .expected_program_code_sha256
                .clear(),
            3 => p.orca_whirlpool.as_mut().unwrap().max_steps = 0,
            4 => p.program_id = p.id.clone(),
            5 => p.fee_numerator = 60001,
            _ => p.accounts.pop().map(|_| ()).unwrap(),
        };
        assert!(q.validate("dex-router").is_err(), "bad {bad}")
    }
}
