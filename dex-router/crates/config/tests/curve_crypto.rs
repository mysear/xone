use xone_config::*;
#[test]
fn crypto_config_requires_original_lp_proof_and_versioned_shape() {
    let mut cfg = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-service.json"
    ))
    .unwrap();
    let pool: PoolConfig = serde_json::from_value(d["cases"][0]["config"].clone()).unwrap();
    cfg.chains[0].pools = vec![pool];
    cfg.validate("dex-router").unwrap();
    for bad in 0..8 {
        let mut q = cfg.clone();
        let p = &mut q.chains[0].pools[0];
        let v = p.multi_asset.as_mut().unwrap();
        match bad {
            0 => v.expected_lp_token.clear(),
            1 => v.expected_lp_token_code_sha256 = "b".repeat(64),
            2 => v.expected_lp_token = p.token0.clone(),
            3 => v.expected_implementation = p.id.clone(),
            4 => v.tokens.truncate(1),
            5 => v.token_policy.clear(),
            6 => p.protocol = "curve-plain3-basic".into(),
            _ => v.expected_vault = p.id.clone(),
        };
        assert!(q.validate("dex-router").is_err(), "bad{bad}");
    }
}

#[test]
fn crypto_ramp_config_accepts_distinct_adapter_with_original_lp_proof() {
    let mut cfg = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-service.json"
    ))
    .unwrap();
    let mut p: PoolConfig = serde_json::from_value(f["cases"][0]["config"].clone()).unwrap();
    p.protocol = "curve-crypto2-ramp".into();
    cfg.chains[0].pools = vec![p];
    cfg.validate("dex-router").unwrap();
    cfg.chains[0].pools[0]
        .multi_asset
        .as_mut()
        .unwrap()
        .expected_lp_token_code_sha256 = "b".repeat(64);
    assert!(cfg.validate("dex-router").is_err());
}
