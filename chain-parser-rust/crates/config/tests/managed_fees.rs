use xone_config::*;
#[test]
fn managed_bpt_fee_config_requires_full_identity_and_separate_native_fee_domain() {
    let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/managed-fees-wire.json"
    ))
    .unwrap();
    let p = &d["snapshot"]["pools"][0];
    let s = &p["multi_asset"];
    let tokens = s["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| serde_json::json!({"token":a["token"],"expected_code_sha256":a["code_sha256"]}))
        .collect::<Vec<_>>();
    let pool:PoolConfig=serde_json::from_value(serde_json::json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"program_id":"","accounts":[],"fee_numerator":800000000000000000u64,"fee_denominator":1000000000000000000u64,"multi_asset":{"expected_code_sha256":s["code_sha256"],"expected_vault":s["vault"],"expected_vault_code_sha256":s["vault_code_sha256"],"pool_id":s["pool_id"],"token_policy":"standard-no-bpt","tokens":tokens}})).unwrap();
    cfg.service.name = "chain-parser-go".into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].key = "evm:31337".into();
    cfg.chains[0].owner_service = "chain-parser-go".into();
    cfg.chains[0].pools = vec![pool];
    assert!(
        cfg.validate("chain-parser-go").is_err(),
        "reference-only Managed runtime must stay unadmitted"
    );
    for i in 0..7 {
        let mut c = cfg.clone();
        let p = &mut c.chains[0].pools[0];
        match i {
            0 => p.fee_numerator += 1,
            1 => p.protocol = "balancer-v2-managed-snapshot".into(),
            2 => p.multi_asset.as_mut().unwrap().token_policy = "standard-with-own-bpt".into(),
            3 => {
                let v = p.multi_asset.as_mut().unwrap();
                v.expected_implementation = p.id.clone();
                v.expected_implementation_code_sha256 = v.expected_code_sha256.clone();
            }
            4 => p.multi_asset.as_mut().unwrap().tokens.truncate(1),
            5 => p.multi_asset.as_mut().unwrap().tokens[2]
                .expected_code_sha256
                .clear(),
            _ => {
                p.multi_asset.as_mut().unwrap().pool_id = format!("{}0000{}1", p.id, "0".repeat(19))
            }
        };
        assert!(c.validate("chain-parser-go").is_err(), "case{i}");
    }
}
