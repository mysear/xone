use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{meteora::MeteoraState, Pool};
use xone_solana_ingest::{meteora, raydium::RpcAccount};
#[test]
fn token2022_owner_tlv_epoch_program_identity_and_withheld_wire_decode() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-token2022.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let p: PoolConfig = serde_json::from_value(f["decoder"]["config"].clone()).unwrap();
    let ch:Chain=serde_json::from_value(json!({"key":"solana:mainnet-beta","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d","rpc_urls":[],"pools":[p],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
    for row in f["decoder"]["cases"].as_array().unwrap() {
        let expected: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
        let m = expected.meteora.as_ref().unwrap();
        let mut accounts = BTreeMap::new();
        for object in [&f["decoder"]["shared_accounts"], &row["accounts"]] {
            for (id, raw) in object.as_object().unwrap() {
                accounts.insert(id.clone(), RpcAccount::decode(raw).unwrap());
            }
        }
        let actual = meteora::decode(&ch, &p, &accounts, m.slot).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            meteora::required_accounts(&p).unwrap().len(),
            accounts.len()
        );
        assert_eq!(MeteoraState::from_proto(m.to_proto().unwrap()).unwrap(), *m);
        let mut bad = accounts.clone();
        bad.get_mut(&p.token0).unwrap().owner = xone_solana_ingest::rpc::TOKEN_PROGRAM.into();
        assert!(meteora::decode(&ch, &p, &bad, m.slot).is_err());
        let mut bad = accounts.clone();
        bad.get_mut(&p.token0).unwrap().data[166] = 14;
        assert!(meteora::decode(&ch, &p, &bad, m.slot).is_err());
        let mut bad = accounts.clone();
        bad.get_mut(&m.token2022.as_ref().unwrap().programdata)
            .unwrap()
            .data[45] ^= 1;
        assert!(meteora::decode(&ch, &p, &bad, m.slot).is_err());
        let mut bad = accounts.clone();
        bad.get_mut("SysvarC1ock11111111111111111111111111111111")
            .unwrap()
            .data[16] ^= 1;
        let changed = meteora::decode(&ch, &p, &bad, m.slot).unwrap();
        assert_ne!(changed.meteora.as_ref().unwrap().token2022, m.token2022);
    }
}
