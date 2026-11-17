#[test]
fn continuous_analytics_is_explicit_go_only_read_only_journal_role() {
    for mode in ["journal", "journal-continuous"] {
        let mut snapshot = xone_config::load(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
            if env!("CARGO_MANIFEST_DIR").contains("chain-parser-rust") {
                "chain-parser-rust"
            } else {
                "dex-router"
            },
        )
        .unwrap();
        let config = &mut snapshot.config;
        config.service.name = "chain-parser-go".into();
        config.state.engine = "pebble".into();
        config.chains = vec![xone_config::Chain {
            key: "evm:31337".into(),
            owner_service: "chain-parser-go".into(),
            assignment_version: 1,
            genesis_hash: format!("0x{}", "ab".repeat(32)),
            rpc_urls: vec![],
            pools: vec![],
            confirmation_tag: "latest".into(),
            auto_reorg_window: 32,
            analysis_interval_ms: 60000,
        }];
        config.service.mode = mode.into();
        assert!(config.validate("chain-parser-go").is_err());
        config.service.role = "analytics".into();
        config.journal.transport = "kafka".into();
        config.journal.kafka.brokers = vec!["127.0.0.1:9092".into()];
        config.journal.kafka.topics.clear();
        config
            .journal
            .kafka
            .topics
            .insert("evm:31337".into(), "canonical.evm".into());
        config.validate("chain-parser-go").unwrap();
        let roundtrip: xone_config::Config =
            serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
        roundtrip.validate("chain-parser-go").unwrap();
        config.chains[0].analysis_interval_ms = 0;
        assert!(config.validate("chain-parser-go").is_err());
        config.chains[0].analysis_interval_ms = 60000;
        config.control.backend = "postgres".into();
        config.control.dsn_file = "secrets/dsn".into();
        assert!(config.validate("chain-parser-go").is_err());
    }
}
