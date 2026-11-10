use serde_json::json;
#[test]
fn lfj_config_is_versioned_complete_and_bounded() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;

    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/lfj-v21-state.json")).unwrap();
    for case in data["cases"].as_array().unwrap() {
        cfg.chains[0].pools = vec![serde_json::from_value(case["config"].clone()).unwrap()];
        cfg.validate("dex-router").unwrap();
    }
    cfg.chains[0].pools = vec![serde_json::from_value(data["cases"][0]["config"].clone()).unwrap()];
    for bad in 0..15 {
        let mut changed = cfg.clone();
        let p = &mut changed.chains[0].pools[0];
        match bad{
   0=>p.lfj=None,1=>p.lfj.as_mut().unwrap().token_policy="unknown-token-extensions".into(),2=>p.lfj.as_mut().unwrap().bin_step=0,
   3=>p.lfj.as_mut().unwrap().max_bins=0,4=>p.lfj.as_mut().unwrap().max_steps=0,
   5=>p.lfj.as_mut().unwrap().expected_implementation=p.id.clone(),6=>p.lfj.as_mut().unwrap().expected_token0_code_sha256.clear(),
   7=>p.protocol="uniswap-v2".into(),8=>p.v2=Some(serde_json::from_value(json!({"expected_factory":format!("0x{}","1".repeat(40)),"expected_code_sha256":"a".repeat(64)})).unwrap()),
   9=>p.lfj.as_mut().unwrap().max_bins=4097,10=>p.lfj.as_mut().unwrap().max_steps=8193,
   11=>p.fee_numerator=1,12=>p.fee_denominator=1_000_000,
   13=>p.lfj.as_mut().unwrap().expected_factory=p.token0.clone(),14=>p.accounts.push(p.token0.clone()),_=>unreachable!()
  }
        assert!(changed.validate("dex-router").is_err(), "bad {bad}");
    }
}
