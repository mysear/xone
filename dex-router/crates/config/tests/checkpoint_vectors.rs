use serde_json::Value;
#[test]
#[ignore = "requires current Go configuration exports, never a stale fixture"]
fn current_go_checkpoint_config_defaults_and_secret_free_hashes_match() {
    let directory = std::env::var("XONE_CHECKPOINT_CONFIG_VECTORS_OUTPUT")
        .expect("current Go configuration exports required");
    let directory = std::path::Path::new(&directory);
    let document: Value =
        serde_json::from_slice(&std::fs::read(directory.join("vectors.json")).unwrap()).unwrap();
    let cases = document["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let name = case["file"].as_str().unwrap();
        assert!(
            name.starts_with("config-")
                && name.ends_with(".json")
                && !name.contains('/')
                && !name.contains('\\')
        );
        let config =
            xone_config::load(directory.join(name), case["expected"].as_str().unwrap()).unwrap();
        assert_eq!(
            config.public_hash,
            case["public_hash"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            config.economic_hash,
            case["economic_hash"].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
#[ignore = "requires this run's Go checkpoint quota config exports"]
fn current_go_checkpoint_local_budget_configs_and_hashes_match() {
    let directory = std::path::PathBuf::from(
        std::env::var("XONE_CHECKPOINT_CONFIG_VECTORS_OUTPUT")
            .expect("current Go quota config exports required"),
    );
    let document: Value =
        serde_json::from_slice(&std::fs::read(directory.join("budget-vectors.json")).unwrap())
            .unwrap();
    let cases = document["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let name = case["file"].as_str().unwrap();
        assert!(
            name.starts_with("budget-config-")
                && name.ends_with(".json")
                && !name.contains('/')
                && !name.contains('\\')
        );
        let loaded = xone_config::load(directory.join(name), "chain-parser-go").unwrap();
        assert_eq!(loaded.public_hash, case["public_hash"]);
        assert_eq!(loaded.economic_hash, case["economic_hash"]);
    }
}

#[test]
#[ignore = "requires the current Go read-only retention policy exports"]
fn current_go_checkpoint_retention_policies_and_hashes_match() {
    let directory = std::path::PathBuf::from(
        std::env::var("XONE_CHECKPOINT_CONFIG_VECTORS_OUTPUT")
            .expect("current Go retention policy exports required"),
    );
    let document: Value =
        serde_json::from_slice(&std::fs::read(directory.join("retention-vectors.json")).unwrap())
            .unwrap();
    let cases = document["vectors"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let name = case["file"].as_str().unwrap();
        assert!(
            name.starts_with("retention-config-") && !name.contains('/') && !name.contains('\\')
        );
        let loaded = xone_config::load(directory.join(name), "chain-parser-go").unwrap();
        assert_eq!(loaded.public_hash, case["public_hash"]);
        assert_eq!(loaded.economic_hash, case["economic_hash"]);
        let mut disabled = loaded.config.clone();
        disabled.service.role = "analytics".into();
        assert_eq!(
            disabled.validate("chain-parser-go").unwrap_err().0,
            "CHECKPOINT_RETENTION_OWNER"
        );
    }
}
