use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{meteora::*, Pool};
use xone_solana_ingest::{meteora, raydium::RpcAccount};
#[test]
fn reward_clock_identity_and_original_bin_aliases_decode_without_enabling_orders() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-rewards.json"
        ))
        .unwrap(),
    )
    .unwrap();
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
        bad.get_mut(&p.id).unwrap().data[264 + 32] ^= 1;
        assert!(meteora::decode(&ch, &p, &bad, 300000000).is_err());
    }
}
