use xone_config::load;
#[test]
fn wallet_submission_is_opt_in_chain_scoped_and_requires_durable_receipts_and_sbf() {
    let base = load(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/config-test.yaml"),
        "dex-router",
    )
    .unwrap();
    let api = serde_json::to_value(&base.config.api).unwrap();
    for key in [
        "transaction_submission_enabled",
        "transaction_receipt_journal",
        "transaction_submission_chains",
        "transaction_max_pending",
    ] {
        assert!(api.get(key).is_none());
    }
    let mut good = base.config.clone();
    good.api.solana_runtime_manifest = "runtime.json".into();
    good.api.transaction_submission_enabled = true;
    good.api.transaction_receipt_journal = "receipts.ndjson".into();
    good.api.transaction_submission_chains = vec![good
        .chains
        .iter()
        .find(|c| c.key.starts_with("solana:"))
        .unwrap()
        .key
        .clone()];
    good.api.transaction_max_pending = 8;
    good.validate("dex-router").unwrap();
    for index in 0..8 {
        let mut c = good.clone();
        match index {
            0 => c.api.solana_runtime_manifest.clear(),
            1 => c.api.transaction_receipt_journal.clear(),
            2 => c.api.transaction_submission_chains.clear(),
            3 => c.api.transaction_submission_chains = vec!["evm:31337".into()],
            4 => c
                .api
                .transaction_submission_chains
                .push(c.api.transaction_submission_chains[0].clone()),
            5 => c.api.transaction_max_pending = 0,
            6 => c.api.transaction_max_pending = 257,
            7 => c.api.transaction_receipt_journal = "x\0bad".into(),
            _ => unreachable!(),
        }
        assert!(c.validate("dex-router").is_err(), "{index}");
    }
}
