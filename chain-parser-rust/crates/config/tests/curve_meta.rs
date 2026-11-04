use xone_config::*;
#[test]
fn meta_config_requires_pinned_clone_base_proof_and_dependency_isolation() {
    let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    let doc: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-meta-service.json"
    ))
    .unwrap();
    let pool: PoolConfig = serde_json::from_value(doc["cases"][0]["config"].clone()).unwrap();
    cfg.service.name = "chain-parser-go".into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].key = "evm:31337".into();
    cfg.chains[0].owner_service = "chain-parser-go".into();
    cfg.chains[0].pools = vec![pool];
    cfg.validate("chain-parser-go").unwrap();
    for i in 0..8 {
        let mut c = cfg.clone();
        let p = &mut c.chains[0].pools[0];
        let v = p.multi_asset.as_mut().unwrap();
        match i {
            0 => v.expected_base_pool_code_sha256.clear(),
            1 => v.expected_implementation.clear(),
            2 => v.expected_implementation_code_sha256 = v.expected_code_sha256.clone(),
            3 => v.token_policy = "standard-no-bpt".into(),
            4 => v.tokens.truncate(1),
            5 => v.tokens[1].token = p.token0.clone(),
            6 => p.protocol = "curve-plain3-basic".into(),
            _ => p.fee_numerator = 100000001,
        };
        assert!(c.validate("chain-parser-go").is_err(), "mutation{i}");
    }
    let mut c = cfg.clone();
    let mut base = c.chains[0].pools[0].clone();
    base.id = xone_domain::curve_meta_state::BASE_POOL.into();
    c.chains[0].pools.push(base);
    assert!(c.validate("chain-parser-go").is_err());
}
