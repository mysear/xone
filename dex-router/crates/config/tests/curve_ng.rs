use xone_config::*;
#[test]
fn ng_config_rejects_nonstandard_token_policy_proxy_and_unsupported_shape() {
    let mut cfg = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-state.json"
    ))
    .unwrap();
    let p: PoolConfig = serde_json::from_value(data["cases"][0]["config"].clone()).unwrap();
    cfg.chains[0].pools = vec![p];
    cfg.validate("dex-router").unwrap();
    for i in 0..6 {
        let mut bad = cfg.clone();
        let p = &mut bad.chains[0].pools[0];
        match i {
            0 => p.multi_asset.as_mut().unwrap().token_policy.clear(),
            1 => p
                .multi_asset
                .as_mut()
                .unwrap()
                .tokens
                .pop()
                .map(|_| ())
                .unwrap(),
            2 => {
                let v = p.multi_asset.as_mut().unwrap();
                v.expected_implementation = p.id.clone();
                v.expected_implementation_code_sha256 = v.expected_code_sha256.clone();
            }
            3 => {
                let v = p.multi_asset.as_mut().unwrap();
                v.expected_vault = p.id.clone();
                v.expected_vault_code_sha256 = v.expected_code_sha256.clone();
            }
            4 => p.fee_numerator = 5000000001,
            5 => p.protocol = "curve-plain3-basic".into(),
            _ => unreachable!(),
        };
        assert!(bad.validate("dex-router").is_err(), "mutation{i}");
    }
}
