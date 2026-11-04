use serde_json::{json, Value};
use xone_config::*;
fn configured() -> Value {
    let mut base = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    base.service.name = "dex-router".into();
    base.state.engine = "memory".into();
    base.state.path.clear();
    base.routing = Routing {
        max_hops: 3,
        max_candidates: 32,
        max_expansions: 1000,
        split_parts: 20,
        max_splits: 2,
        route_activation_cost: "0".parse().unwrap(),
        max_state_age_ms: 180000,
        quote_ttl_ms: 5000,
    };
    base.api.quote_timeout_ms = 1000;
    base.api.quote_concurrency = 8;
    let mut value = serde_json::to_value(base).unwrap();
    value["simulation"] = json!({"backend":"revm-fixture","rpc_url":"","max_gas":15_000_000,"max_state_reads":4096,"timeout_ms":1000,"allowed_targets":["0x0000000000000000000000000000000000000001"],"state_file":"fixtures/evm-state.json","max_state_bytes":8_388_608,"max_instructions":2_000_000,"max_memory_bytes":67_108_864,"concurrency":2});
    value
}
#[test]
fn local_revm_configuration_is_explicit_and_bounded() {
    let config: Config = serde_json::from_value(configured()).unwrap();
    config.validate("dex-router").unwrap();
    for (field, value) in [
        ("state_file", json!("")),
        ("rpc_url", json!("http://127.0.0.1:8545")),
        ("max_state_bytes", json!(0)),
        ("max_state_bytes", json!(16_777_217)),
        ("max_instructions", json!(0)),
        ("max_instructions", json!(100_000_001)),
        ("max_memory_bytes", json!(0)),
        ("max_memory_bytes", json!(67_108_865)),
        ("concurrency", json!(0)),
        ("concurrency", json!(9)),
        ("allowed_targets", json!([])),
        ("allowed_targets", json!(["not-an-address"])),
        (
            "allowed_targets",
            json!(["0x0000000000000000000000000000000000000000"]),
        ),
    ] {
        let mut v = configured();
        v["simulation"][field] = value;
        let c: Config = serde_json::from_value(v).unwrap();
        assert!(c.validate("dex-router").is_err(), "{field}");
    }
}
#[test]
fn local_revm_settings_cannot_hide_in_a_disabled_backend() {
    for mode in ["disabled", "evm-rpc"] {
        let mut v = configured();
        v["simulation"]["backend"] = json!(mode);
        let c: Config = serde_json::from_value(v).unwrap();
        assert!(c.validate("dex-router").is_err());
    }
}
