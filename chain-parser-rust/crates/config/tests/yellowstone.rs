use serde_json::json;
use xone_config::Config;
fn configured() -> serde_json::Value {
    let value: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../config/config-test.yaml")).unwrap();
    {
        let mut v = serde_json::to_value(value).unwrap();
        v["chains"][0]["confirmation_tag"] = json!("finalized");
        v
    }
}
fn stream() -> serde_json::Value {
    json!({"endpoint":"http://127.0.0.1:10000","source_id":"self-hosted-geyser",
        "proto_commit":"10f0af69d150637d8e3eaaf83b179d640988dd3c",
        "allow_loopback_plaintext":true,"connect_timeout_ms":1000,"idle_timeout_ms":2000,
        "wal_ack_timeout_ms":1000,"reconnect_delay_ms":10,"max_frame_bytes":1048576,
        "max_wal_bytes":8388608,"max_wal_records":1000,"max_replay_records":100,
        "max_replay_slots":32,"max_stream_bytes":8388608,"max_stream_slots":32,
        "max_stream_updates":1024,"max_manifest_bytes":1048576,"max_catalog_pools":16,
        "manifest_directory":"bank-manifests","catalog_file":"admitted-catalog.json"})
}
#[test]
fn normal_config_consumes_public_yellowstone_without_token() {
    let mut v = configured();
    let chain = v["chains"][0]["key"].as_str().unwrap().to_owned();
    v["yellowstone"] = json!({chain:stream()});
    let config: Config = serde_json::from_value(v).expect("normal Config must consume Yellowstone");
    config.validate("chain-parser-rust").unwrap();
}
#[test]
fn normal_yellowstone_rejects_wrong_pin_missing_and_unbounded_budgets() {
    for (field, replacement) in [
        ("proto_commit", json!("main")),
        ("max_replay_records", json!(0)),
        ("max_wal_bytes", json!(0)),
        ("max_stream_slots", json!(0)),
    ] {
        let mut v = configured();
        let chain = v["chains"][0]["key"].as_str().unwrap().to_owned();
        let mut source = stream();
        source[field] = replacement;
        v["yellowstone"] = json!({chain:source});
        if let Ok(config) = serde_json::from_value::<Config>(v) {
            assert!(config.validate("chain-parser-rust").is_err(), "{field}");
        }
    }
    let mut source = stream();
    source.as_object_mut().unwrap().remove("max_frame_bytes");
    let mut v = configured();
    let chain = v["chains"][0]["key"].as_str().unwrap().to_owned();
    v["yellowstone"] = json!({chain:source});
    assert!(serde_json::from_value::<Config>(v).is_err());
}

#[test]
fn normal_yellowstone_rejects_malformed_endpoint_before_state_open() {
    for endpoint in [
        "https://",
        "https://example.org/path",
        "https://example.org:70000",
        "https://example.org/extra/",
    ] {
        let mut v = configured();
        let chain = v["chains"][0]["key"].as_str().unwrap().to_owned();
        let mut s = stream();
        s["endpoint"] = json!(endpoint);
        v["yellowstone"] = json!({chain:s});
        let config: Config = serde_json::from_value(v).unwrap();
        assert!(config.validate("chain-parser-rust").is_err(), "{endpoint}");
    }
}

#[test]
fn independent_archive_budget_is_optional_bounded_and_consumed() {
    let mut v = configured();
    let chain = v["chains"][0]["key"].as_str().unwrap().to_owned();
    let mut source = stream();
    source["archive_retention"] = json!({"max_segments":8,"max_bytes":67108864u64});
    v["yellowstone"] = json!({chain.clone():source});
    let config: Config = serde_json::from_value(v.clone()).unwrap();
    config.validate("chain-parser-rust").unwrap();
    assert_eq!(
        config
            .yellowstone
            .values()
            .next()
            .unwrap()
            .archive_retention
            .as_ref()
            .unwrap()
            .max_bytes,
        67108864
    );
    v["yellowstone"][&chain]["archive_retention"]["max_segments"] = json!(0);
    assert!(serde_json::from_value::<Config>(v)
        .unwrap()
        .validate("chain-parser-rust")
        .is_err());
}
