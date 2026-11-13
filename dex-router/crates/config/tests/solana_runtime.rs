use xone_config::load;
#[test]
fn optional_solana_runtime_manifest_is_explicit_bounded_and_omitted_by_default() {
    let c = load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    assert!(!serde_json::to_value(&c.config.api)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("solana_runtime_manifest"));
    for path in [
        "test/contracts/sbf-runtime/meteora.json",
        "/tmp/xone-local-runtime.json",
    ] {
        let mut v = c.config.clone();
        v.api.solana_runtime_manifest = path.into();
        v.validate("dex-router").unwrap();
    }
    for path in ["x\0bad".to_string(), "a".repeat(4097)] {
        let mut v = c.config.clone();
        v.api.solana_runtime_manifest = path;
        assert!(v.validate("dex-router").is_err());
    }
}
