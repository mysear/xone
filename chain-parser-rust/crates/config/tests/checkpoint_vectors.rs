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
