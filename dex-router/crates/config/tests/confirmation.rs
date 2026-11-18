#[test]
fn evm_confirmation_policy_rejects_historical_pending_and_unknown_tags() {
    let mut snapshot = xone_config::load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        if env!("CARGO_MANIFEST_DIR").contains("chain-parser-rust") {
            "chain-parser-rust"
        } else {
            "dex-router"
        },
    )
    .unwrap();
    // Validate the public Go service configuration shape in either standalone compiler.
    snapshot.config.service.name = "chain-parser-go".into();
    snapshot.config.state.engine = "pebble".into();
    snapshot.config.chains = vec![xone_config::Chain {
        key: "evm:31337".into(),
        owner_service: "chain-parser-go".into(),
        assignment_version: 1,
        genesis_hash: format!("0x{}", "ab".repeat(32)),
        rpc_urls: vec!["http://127.0.0.1:8545".into()],
        pools: vec![],
        confirmation_tag: "latest".into(),
        auto_reorg_window: 32,
        analysis_interval_ms: 0,
    }];
    for tag in ["latest", "safe", "finalized"] {
        snapshot.config.chains[0].confirmation_tag = tag.into();
        snapshot.config.validate("chain-parser-go").unwrap();
    }
    for tag in [
        "",
        "pending",
        "earliest",
        "0x1",
        "0x0",
        "123",
        "LATEST",
        "safe ",
        " finalized",
        "finalized\n",
        "confirmed",
        "private-input-must-not-leak",
    ] {
        snapshot.config.chains[0].confirmation_tag = tag.into();
        assert_eq!(
            snapshot.config.validate("chain-parser-go").unwrap_err().0,
            "CONFIG_CHAIN_CONFIRMATION"
        );
    }
}

#[test]
#[ignore = "requires this run's independent Go confirmation policy exports"]
fn current_go_evm_confirmation_policy_and_hashes_match() {
    let directory = std::path::PathBuf::from(
        std::env::var("XONE_CONFIRMATION_CONFIG_OUTPUT")
            .expect("current Go confirmation exports required"),
    );
    let values: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.join("confirmation-vectors.json")).unwrap(),
    )
    .unwrap();
    let cases = values["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15);
    for row in cases {
        let name = row["file"].as_str().unwrap();
        assert!(
            name.starts_with("confirmation-")
                && name.ends_with(".json")
                && !name.contains('/')
                && !name.contains('\\')
        );
        let actual = xone_config::load(directory.join(name), "chain-parser-go");
        if row["valid"] == true {
            let config = actual.unwrap();
            assert_eq!(config.public_hash, row["public_hash"]);
            assert_eq!(config.economic_hash, row["economic_hash"]);
            assert_eq!(config.config.chains[0].confirmation_tag, row["tag"]);
        } else {
            assert_eq!(actual.err().unwrap().0, row["error"].as_str().unwrap());
        }
    }
}
