use xone_config::*;
#[test]
fn weighted_config_requires_explicit_versioned_identity_and_token_policy() {
    let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/weighted-state.json")).unwrap();
    cfg.service.name = "chain-parser-go".into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].key = "evm:31337".into();
    cfg.chains[0].owner_service = "chain-parser-go".into();
    cfg.chains[0].pools = serde_json::from_value(data["configs"].clone()).unwrap();
    cfg.validate("chain-parser-go").unwrap();
    for i in 0..7 {
        let mut c = cfg.clone();
        let p = &mut c.chains[0].pools[0];
        match i {
            0 => p.weighted = None,
            1 => p.weighted.as_mut().unwrap().token_policy.clear(),
            2 => p
                .weighted
                .as_mut()
                .unwrap()
                .expected_token0_code_sha256
                .clear(),
            3 => p.protocol = "uniswap-v2".into(),
            4 => p.protocol = "raydium-cpmm-creator".into(),
            5 => p.weighted.as_mut().unwrap().pool_id = format!("{}{}", p.id, "0".repeat(24)),
            6 => p.fee_numerator = 0,
            _ => unreachable!(),
        };
        assert!(c.validate("chain-parser-go").is_err(), "mutation {i}");
    }
}
