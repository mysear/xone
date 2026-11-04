use xone_config::*;
#[test]
fn single_file_is_explicit_and_head_first() {
    let cfg = load("../../config/config-test.yaml", "dex-router").unwrap();
    assert_eq!(cfg.config.startup.mode, "HEAD_FIRST");
    assert!(!cfg.public_hash.is_empty());
}
#[test]
fn duplicate_alias_and_env_rejected() {
    for raw in ["a: 1\na: 2\n", "a: &x [1]\nb: *x\n", "a: ${HOME}\n"] {
        assert!(decode_document(raw.as_bytes()).is_err(), "{raw}");
    }
}

#[test]
fn explicit_auth_and_removed_debug_config() {
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("../../config/config-test.yaml").unwrap())
            .unwrap();
    let service = value["service"]["name"].as_str().unwrap().to_owned();
    let dir = std::env::temp_dir().join(format!("xone-auth-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config-test.yaml");
    let write =
        |doc: &serde_json::Value| std::fs::write(&path, serde_json::to_vec(doc).unwrap()).unwrap();
    write(&value);
    let without = load(&path, &service).unwrap();
    value["api"]["auth_token"] = "explicit-business-api-token-123456".into();
    value["journal"]["source_auth_tokens"] =
        serde_json::json!({"http://127.0.0.1:8091":"upstream-only-token-1234567890"});
    write(&value);
    let with = load(&path, &service).unwrap();
    assert_eq!(without.public_hash, with.public_hash);
    assert_eq!(without.economic_hash, with.economic_hash);
    assert_eq!(with.token, "explicit-business-api-token-123456");
    value["service"]["mode"] = "fixture".into();
    write(&value);
    assert!(load(&path, &service).is_err());
    value["service"]["mode"] = "rpc".into();
    value["chains"][0]["fixture_path"] = "sample.json".into();
    write(&value);
    assert!(load(&path, &service).is_err());
    write(&serde_json::json!({"schema_version":1,"release_id":"old","files":[]}));
    assert!(load(&path, &service).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
