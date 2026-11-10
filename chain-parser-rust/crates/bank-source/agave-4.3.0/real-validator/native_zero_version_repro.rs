//! Diagnostic replay of one ACTUALLY CAPTURED Bank/Geyser callback.
//! This is not the real-Bank integration success criterion; bank_export.rs is.
use std::{fs, path::PathBuf};
use xone_bank_source::{ManifestAccount, Observer, SourceConfig};
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
            | c.get(2).copied().unwrap_or(0) as u32;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn main() {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("captured bank-evidence directory"),
    );
    let cfg: SourceConfig =
        serde_json::from_slice(&fs::read(root.join("source-config.json")).unwrap()).unwrap();
    let events: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(root.join("native-notifications.json")).unwrap()).unwrap();
    let e = events
        .iter()
        .find(|e| e["write_version"] == 0)
        .expect("real trace must contain native version zero");
    assert_eq!(e["signature_present"], false);
    let bytes = e["raw_data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
        .collect::<Vec<_>>();
    let row = ManifestAccount {
        pubkey: e["pubkey"].as_str().unwrap().into(),
        owner: e["owner"].as_str().unwrap().into(),
        lamports: e["lamports"].as_u64().unwrap(),
        executable: e["executable"].as_bool().unwrap(),
        rent_epoch: e["rent_epoch"].as_u64().unwrap(),
        write_slot: e["slot"].as_u64().unwrap(),
        write_bank_id: e["bank_id"].as_u64().unwrap(),
        write_version: e["write_version"].as_u64().unwrap(),
        data_base64: base64(&bytes),
        txn_signature_base64: None,
    };
    println!(
        "captured native slot={} bank_id={} write_version={} pubkey={} data={}",
        row.write_slot, row.write_bank_id, row.write_version, row.pubkey, row.data_base64
    );
    let mut observer = Observer::new(cfg.clone()).unwrap();
    observer.genesis(&cfg.genesis_hash).unwrap();
    assert!(observer.keys().contains(&row.pubkey));
    let result = observer.observe(row);
    println!("kernel result for unchanged actual native callback: {result:?}");
    assert!(
        result.is_ok(),
        "a legitimate captured native version zero must not poison the exporter"
    );
}
