use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{pump_amm, Pool};
use xone_solana_ingest::{pump, raydium::RpcAccount};
#[test]
fn original_pump_amm_accounts_program_versions_native_vaults_and_fee_config_decode() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/pump-amm-decoder.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let p: PoolConfig = serde_json::from_value(f["config"].clone()).unwrap();
    let ch:Chain=serde_json::from_value(json!({"key":"solana:mainnet-beta","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d","rpc_urls":[],"pools":[p],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(f["boost_cases"].as_array().unwrap())
    {
        let p: PoolConfig = if c.get("config").is_some() {
            serde_json::from_value(c["config"].clone()).unwrap()
        } else {
            p.clone()
        };

        let expected: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let mut accounts = BTreeMap::new();
        for a in [&f["shared_accounts"], &c["accounts"]] {
            for (id, raw) in a.as_object().unwrap() {
                accounts.insert(id.clone(), RpcAccount::decode(raw).unwrap());
            }
        }
        let actual =
            pump::decode(&ch, &p, &accounts, expected.pump.as_ref().unwrap().slot).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(pump::required_accounts(&p).unwrap().len(), accounts.len());
        assert_eq!(actual.protocol, pump_amm::PROFILE);
        for id in [
            &p.id,
            &p.pump.as_ref().unwrap().expected_programdata,
            &p.pump.as_ref().unwrap().expected_fee_programdata,
        ] {
            let mut bad = accounts.clone();
            let a = bad.get_mut(id).unwrap();
            let o = if a.data.len() > 45 { 45 } else { 0 };
            a.data[o] ^= 1;
            assert!(pump::decode(&ch, &p, &bad, expected.pump.as_ref().unwrap().slot).is_err());
        }
        let m = actual.pump.as_ref().unwrap();
        let mut bad = accounts.clone();
        bad.get_mut(&m.creator_vault).unwrap().lamports += 1;
        assert!(pump::decode(&ch, &p, &bad, m.slot).is_err());
    }
}
