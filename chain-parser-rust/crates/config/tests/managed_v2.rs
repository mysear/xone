use xone_config::*;
// Synthetic expected identities only; chain provenance is a parser obligation.
const SYNTHETIC: &str = r#"{"protocol":"balancer-v2-managed-aum","token0":"0x0000000000000000000000000000000000000001","token1":"0x0000000000000000000000000000000000000002","fee_numerator":1000000000000,"fee_denominator":1000000000000000000,"id":"0x0000000000000000000000000000000000000046","program_id":"","accounts":[],"multi_asset":{"expected_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_vault":"0x0000000000000000000000000000000000000047","expected_vault_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","pool_id":"0x0000000000000000000000000000000000000046000100000000000000000000","tokens":[{"token":"0x0000000000000000000000000000000000000001","expected_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"token":"0x0000000000000000000000000000000000000002","expected_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"token_policy":"standard-no-bpt","managed_v2":{"expected_factory":"0x0000000000000000000000000000000000000050","expected_factory_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_pool_runtime_template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","expected_fee_provider":"0x0000000000000000000000000000000000000051","expected_fee_provider_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_protocol_collector":"0x0000000000000000000000000000000000000052","expected_protocol_collector_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expected_manager":"0x0000000000000000000000000000000000000053","expected_provider_fee_id":"3","expected_management_aum_fee":"10000000000000000","dependencies":[{"role":"circuit-breaker-lib","address":"0x000000000000000000000000000000000000005a","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-add-remove-token-lib","address":"0x000000000000000000000000000000000000005b","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-amm-lib","address":"0x000000000000000000000000000000000000005c","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"external-weighted-math","address":"0x000000000000000000000000000000000000005d","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"recovery-mode-helper","address":"0x000000000000000000000000000000000000005e","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}}}"#;
#[test]
fn config_independent_profile() {
    let mut c = shared_evm_config();
    let p: PoolConfig = serde_json::from_str(SYNTHETIC).unwrap();
    c.chains[0].pools = vec![p];
    c.validate("dex-router").unwrap();
}

use serde_json::{json, Value};
fn value() -> Value {
    serde_json::from_str(SYNTHETIC).unwrap()
}
#[test]
fn provider_fee_id_uint8_bounds() {
    let mut c = shared_evm_config();
    let mut mismatches = Vec::new();
    for (fee_id, valid) in [
        ("0", true),
        ("3", true),
        ("255", true),
        ("256", false),
        (
            "1606938044258990275541962092341162602522202993782792835301376",
            false,
        ),
        (
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            false,
        ),
    ] {
        let mut p = value();
        p["multi_asset"]["managed_v2"]["expected_provider_fee_id"] = json!(fee_id);
        c.chains[0].pools = vec![serde_json::from_value(p).unwrap()];
        if c.validate("dex-router").is_ok() != valid {
            mismatches.push(fee_id);
        }
    }
    assert!(
        mismatches.is_empty(),
        "unexpected validity for fee IDs {mismatches:?}"
    );
}

#[test]
fn capacities_boundaries_and_legacy_rejection() {
    let mut c = shared_evm_config();
    for n in [2, 39, 50, 51] {
        let mut p = value();
        let mut tokens = vec![];
        for i in 0..n {
            let mut a = p["multi_asset"]["tokens"][0].clone();
            a["token"] = json!(format!("0x{:040x}", i + 1));
            tokens.push(a)
        }
        p["multi_asset"]["tokens"] = json!(tokens);
        p["fee_numerator"] = json!(950_000_000_000_000_000u64);
        c.chains[0].pools = vec![serde_json::from_value(p).unwrap()];
        assert_eq!(c.validate("dex-router").is_ok(), n <= 50, "{n}");
    }
    for (path, v) in [
        ("/protocol", json!("balancer-v2-managed-fees")),
        ("/protocol", json!("curve-stableswap-ng")),
        ("/fee_numerator", json!(950_000_000_000_000_001u64)),
        (
            "/multi_asset/managed_v2/expected_management_aum_fee",
            json!("950000000000000001"),
        ),
        (
            "/multi_asset/managed_v2/expected_provider_fee_id",
            json!("9".repeat(79)),
        ),
        (
            "/multi_asset/managed_v2/expected_factory_code_sha256",
            json!(""),
        ),
        ("/multi_asset/token_policy", json!("standard-with-own-bpt")),
        (
            "/multi_asset/managed_v2/dependencies/0/role",
            json!("CircuitBreakerLib"),
        ),
    ] {
        let mut p = value();
        *p.pointer_mut(path).unwrap() = v;
        let p = serde_json::from_value::<PoolConfig>(p);
        if let Ok(p) = p {
            c.chains[0].pools = vec![p];
            assert!(c.validate("dex-router").is_err(), "{path}");
        }
    }
}
#[test]
fn strict_required_configuration() {
    let v = value();
    for key in v["multi_asset"]["managed_v2"].as_object().unwrap().keys() {
        for null in [false, true] {
            let mut p = v.clone();
            let m = p["multi_asset"]["managed_v2"].as_object_mut().unwrap();
            if null {
                m.insert(key.clone(), Value::Null);
            } else {
                m.remove(key);
            };
            assert!(serde_json::from_value::<PoolConfig>(p).is_err(), "{key}");
        }
    }
    for bad in [
        SYNTHETIC.replacen("\"managed_v2\":", "\"Managed_V2\":", 1),
        SYNTHETIC.replacen("\"expected_manager\":", "\"Expected_Manager\":", 1),
        SYNTHETIC.replacen(
            "\"expected_management_aum_fee\":\"10000000000000000\"",
            "\"expected_management_aum_fee\":\"0\",\"expected_management_aum_fee\":\"0\"",
            1,
        ),
    ] {
        assert!(serde_json::from_str::<PoolConfig>(&bad).is_err())
    }
    let mut p = v.clone();
    p["multi_asset"]["managed_v2"] = Value::Null;
    assert!(serde_json::from_value::<PoolConfig>(p).is_err());
}

#[test]
fn known_zero_expected_parameters() {
    let mut c = shared_evm_config();
    let mut p: PoolConfig = serde_json::from_str(SYNTHETIC).unwrap();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.expected_provider_fee_id = "0".parse().unwrap();
    m.expected_management_aum_fee = "0".parse().unwrap();
    c.chains[0].pools = vec![p];
    c.validate("dex-router").unwrap();
}

// The parser-rust runtime remains Solana-only. Exercise the shared EVM config
// schema under router policy, without admitting an EVM owner into parser-rust.
fn shared_evm_config() -> Config {
    let mut c = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    c.service.name = "dex-router".into();
    c.state.engine = "memory".into();
    c.chains[0].key = "evm:1".into();
    c.chains[0].owner_service = "chain-parser-go".into();
    c.routing.max_hops = 1;
    c.routing.max_candidates = 1;
    c.routing.max_expansions = 1;
    c.routing.split_parts = 1;
    c.routing.max_splits = 1;
    c.routing.max_state_age_ms = 1;
    c.routing.quote_ttl_ms = 1;
    c.simulation.backend = "disabled".into();
    c.simulation.max_gas = 1;
    c.simulation.max_state_reads = 1;
    c.simulation.timeout_ms = 1;
    c.api.quote_timeout_ms = 1;
    c.api.quote_concurrency = 1;
    c.validate("dex-router")
        .expect("synthetic shared EVM schema test policy");
    c
}
