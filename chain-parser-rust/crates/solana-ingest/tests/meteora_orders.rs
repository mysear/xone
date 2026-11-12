use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{meteora::*, Pool};
use xone_solana_ingest::{meteora, raydium::RpcAccount};
fn check(fixture: &str) {
    let f: Value = serde_json::from_slice(&std::fs::read(fixture).unwrap()).unwrap();
    let p: PoolConfig = serde_json::from_value(f["decoder"]["config"].clone()).unwrap();
    let ch:Chain=serde_json::from_value(json!({"key":"solana:mainnet-beta","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d","rpc_urls":[],"pools":[p],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
    for row in f["decoder"]["cases"].as_array().unwrap() {
        let mut accounts = BTreeMap::new();
        for a in [&f["decoder"]["shared_accounts"], &row["accounts"]] {
            for (id, raw) in a.as_object().unwrap() {
                accounts.insert(id.clone(), RpcAccount::decode(raw).unwrap());
            }
        }
        let expected: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
        assert_eq!(
            meteora::decode(&ch, &p, &accounts, 300000000).unwrap(),
            expected
        );
        let mut old = expected.clone();
        old.protocol = PROFILE.into();
        old.adapter_version = ADAPTER.into();
        assert!(old.validate().is_err());
        let mut bad = accounts.clone();
        bad.get_mut(&p.id).unwrap().data[35] = 255;
        assert!(meteora::decode(&ch, &p, &bad, 300000000).is_err());
    }
}

#[test]
fn orders_profile_decodes_complete_raw_layers_and_rejects_legacy_relabeling() {
    check(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/contracts/meteora-orders.json"
    ));
}
#[test]
fn orders_exact_out_banks_decode_all_raw_layers() {
    check(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/contracts/meteora-orders-exact-out.json"
    ));
}
