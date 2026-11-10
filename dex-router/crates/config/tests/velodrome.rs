use serde_json::json;

#[test]
fn velodrome_config_preserves_separate_curve_and_factory_identity() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    for name in ["velodrome-v2-stable", "velodrome-v2-volatile"] {
        cfg.chains[0].pools = serde_json::from_value(json!([{
            "id":format!("0x{}", "3".repeat(40)), "protocol":name,
            "token0":format!("0x{}", "1".repeat(40)), "token1":format!("0x{}", "2".repeat(40)),
            "fee_numerator":4, "fee_denominator":10000, "program_id":"", "accounts":[],
            "stable":{"expected_code_sha256":"a".repeat(64),"expected_factory":format!("0x{}","4".repeat(40)),"expected_factory_code_sha256":"b".repeat(64)}
        }])).unwrap();
        cfg.validate("dex-router").unwrap();
        let profile = xone_domain::protocols::protocol(name).unwrap();
        assert_eq!(profile.adapter, format!("{name}/b3065d8b-1"));
        cfg.chains[0].pools[0]
            .stable
            .as_mut()
            .unwrap()
            .expected_factory_code_sha256
            .clear();
        assert!(cfg.validate("dex-router").is_err());
    }
}
