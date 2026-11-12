use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{meteora::*, Pool};
use xone_solana_ingest::{meteora, raydium::RpcAccount};
#[test]
fn bitmap_extension_raw_account_pda_owner_coverage_and_wire_are_bound() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/meteora-bitmap.json")).unwrap();
    for row in f["decoder"]["cases"].as_array().unwrap() {
        let p: PoolConfig = serde_json::from_value(row["config"].clone()).unwrap();
        let ch:Chain=serde_json::from_value(json!({"key":"solana:mainnet-beta","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d","rpc_urls":[],"pools":[p],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
        let mut accounts = BTreeMap::new();
        for object in [&f["decoder"]["shared_accounts"], &row["accounts"]] {
            for (id, a) in object.as_object().unwrap() {
                accounts.insert(id.clone(), RpcAccount::decode(a).unwrap());
            }
        }
        let expected: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
        let actual = meteora::decode(&ch, &p, &accounts, 300000000).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            meteora::required_accounts(&p).unwrap().len(),
            accounts.len()
        );
        let ext = p
            .meteora
            .as_ref()
            .unwrap()
            .bitmap_extension
            .as_ref()
            .unwrap();
        let mut bad = accounts.clone();
        bad.remove(ext);
        assert!(meteora::decode(&ch, &p, &bad, 300000000).is_err());
        let mut bad = accounts.clone();
        bad.get_mut(ext).unwrap().owner = "11111111111111111111111111111111".into();
        assert!(meteora::decode(&ch, &p, &bad, 300000000).is_err());
        let mut bad = accounts.clone();
        bad.get_mut(ext).unwrap().data[8] ^= 1;
        assert!(meteora::decode(&ch, &p, &bad, 300000000).is_err());
        let m = actual.meteora.as_ref().unwrap();
        assert_eq!(MeteoraState::from_proto(m.to_proto().unwrap()).unwrap(), *m);
        let mut old = actual.clone();
        old.protocol = PROFILE.into();
        old.adapter_version = ADAPTER.into();
        assert!(old.validate().is_err());
        let mut bad = actual.clone();
        bad.meteora
            .as_mut()
            .unwrap()
            .bitmap_extension
            .as_mut()
            .unwrap()
            .data = "00".repeat(BITMAP_SIZE);
        assert!(bad.validate().is_err());
    }
}
