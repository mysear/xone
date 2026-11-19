use serde_json::Value;
use xone_config::load;

#[test]
#[ignore = "requires current Go price journal configurations"]
fn durable_journal_config_matches_go_hashes_and_rejects_invalid_paths() {
    let dir = std::path::PathBuf::from(std::env::var("XONE_PRICE_JOURNAL_CONFIGS").unwrap());
    let rows: Vec<Value> =
        serde_json::from_slice(&std::fs::read(dir.join("hashes.json")).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    for (name, row) in ["default", "durable"].into_iter().zip(rows) {
        assert_eq!(row["name"], name);
        let c = load(dir.join(format!("{name}.json")), "dex-router").unwrap();
        assert_eq!(row["public_hash"], c.public_hash);
        assert_eq!(row["economic_hash"], c.economic_hash);
        let mut policy = c.config.simulation.native_valuation.unwrap();
        assert_eq!(
            policy.publication_journal.as_deref(),
            (name == "durable").then_some("state/prices.journal")
        );
        for path in [
            String::new(),
            "p".repeat(4097),
            "bad\0path".into(),
            policy.observations_file.clone(),
        ] {
            policy.publication_journal = Some(path);
            assert!(policy.validate().is_err());
        }
    }
}
