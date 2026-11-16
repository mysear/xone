use serde_json::{json, Value};
use xone_config::{Config, EvmPreparationConfig};
fn config() -> Config {
    xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap()
    .config
}
fn declaration() -> EvmPreparationConfig {
    serde_json::from_value(json!({"router":"0x0000000000000000000000000000000000000100","router_code_sha256":"11".repeat(32),
        "factory":"0x0000000000000000000000000000000000000200","factory_code_sha256":xone_domain::v2_swap::FACTORY_CODE_SHA256,
        "token_code_sha256":{"0x0000000000000000000000000000000000000010":"22".repeat(32),"0x0000000000000000000000000000000000000020":"33".repeat(32)},
        "timeout_ms":5000,"max_calls":128,"max_response_bytes":2097152})).unwrap()
}
#[test]
fn explicit_operator_declaration_is_optional_and_requires_one_pinned_evm_chain() {
    let mut c = config();
    assert!(c.api.evm_preparation.is_empty());
    assert!(c.validate("dex-router").is_ok());
    c.chains[0].genesis_hash = format!("0x{}", "ab".repeat(32));
    c.chains[0].rpc_urls = vec!["http://127.0.0.1:19999".into()];
    c.api
        .evm_preparation
        .insert(c.chains[0].key.clone(), declaration());
    assert!(c.validate("dex-router").is_ok());
    let mut bad = c.clone();
    bad.chains[0].rpc_urls.push("http://127.0.0.1:20000".into());
    assert!(bad.validate("dex-router").is_err());
    let mut bad = c.clone();
    bad.chains[0].genesis_hash = "".into();
    assert!(bad.validate("dex-router").is_err());
    let mut bad = c;
    let declared = bad.api.evm_preparation.remove(&bad.chains[0].key).unwrap();
    bad.api.evm_preparation.insert("evm:999".into(), declared);
    assert!(bad.validate("dex-router").is_err());
}
#[test]
fn bad_identity_and_budget_configurations_are_rejected() {
    let source = serde_json::to_value(declaration()).unwrap();
    for (field, value) in [
        (
            "router",
            json!("0x0000000000000000000000000000000000000000"),
        ),
        ("router_code_sha256", json!("00".repeat(32))),
        ("factory_code_sha256", json!("aa".repeat(32))),
        ("token_code_sha256", json!({})),
        ("timeout_ms", json!(15001)),
        ("max_calls", json!(31)),
        ("max_response_bytes", json!(16777217)),
    ] {
        let mut changed = source.clone();
        changed[field] = value;
        let c: EvmPreparationConfig = serde_json::from_value(changed).unwrap();
        assert!(c.validate().is_err(), "{field}");
    }
    let mut unknown: Value = source;
    unknown["trust_me"] = true.into();
    assert!(serde_json::from_value::<EvmPreparationConfig>(unknown).is_err());
}
