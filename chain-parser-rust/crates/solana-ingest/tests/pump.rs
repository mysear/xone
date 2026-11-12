use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::Pool;
use xone_solana_ingest::{pump, raydium::RpcAccount};
fn data() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/pump-bonding-decoder.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
fn setup(f: &Value, index: usize) -> (Chain, PoolConfig, BTreeMap<String, RpcAccount>, Pool) {
    let p: PoolConfig = serde_json::from_value(f["config"].clone()).unwrap();
    let ch:Chain=serde_json::from_value(json!({"key":"solana:mainnet-beta","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d","rpc_urls":[],"pools":[p],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
    let mut a = BTreeMap::new();
    for obj in [&f["shared_accounts"], &f["cases"][index]["accounts"]] {
        for (key, v) in obj.as_object().unwrap() {
            a.insert(key.clone(), pump::decode_account(v).unwrap());
        }
    }
    let expected: Pool = serde_json::from_value(f["cases"][index]["pool"].clone()).unwrap();
    (ch, p, a, expected)
}
#[test]
fn pump_fixed_programdata_fee_configuration_and_curve_decode_without_native_wsol_alias() {
    let f = data();
    for i in 0..5 {
        let (ch, p, a, expected) = setup(&f, i);
        let actual = pump::decode(&ch, &p, &a, 467957307).unwrap();
        assert!(actual == expected);
        assert_eq!(actual.token1, "native:solana:sol");
        assert_eq!(
            xone_solana_ingest::rpc::required_scope(&ch).unwrap().len(),
            a.len()
        );
    }
}
#[test]
fn pump_program_fee_token_clock_vault_completion_and_unknown_extension_checks_fail_closed() {
    let f = data();
    let (ch, p, a, _) = setup(&f, 1);
    let check =
        |a: &BTreeMap<String, RpcAccount>| assert!(pump::decode(&ch, &p, a, 467957307).is_err());
    for key in [
        &p.pump.as_ref().unwrap().expected_programdata,
        &p.pump.as_ref().unwrap().expected_fee_programdata,
    ] {
        let mut a = a.clone();
        a.get_mut(key).unwrap().data[45] ^= 1;
        check(&a);
    }
    let mut b = a.clone();
    b.get_mut(&p.id).unwrap().data[81] = 1;
    check(&b);
    let mut b = a.clone();
    b.get_mut(&p.id).unwrap().data[82] = 1;
    check(&b);
    let mut b = a.clone();
    b.get_mut(&p.id).unwrap().data[83] = 1;
    check(&b);
    let mut b = a.clone();
    b.get_mut(&p.id).unwrap().lamports += 1;
    check(&b);
    let mut b = a.clone();
    b.remove(&p.token0);
    check(&b);
    let mut b = a.clone();
    b.get_mut(xone_solana_ingest::raydium::CLOCK).unwrap().data[0] ^= 1;
    check(&b);
    let mut cfg = p.clone();
    cfg.token1 = "So11111111111111111111111111111111111111112".into();
    assert!(pump::required_accounts(&cfg).is_err());
    let mut chain = ch.clone();
    chain.confirmation_tag = "confirmed".into();
    assert!(pump::decode(&chain, &p, &a, 467957307).is_err());
}

#[test]
fn system_owned_fee_vault_admission_is_local_to_pump_and_rejects_code_or_data() {
    let empty = json!({"owner":xone_domain::pump::SYSTEM,"executable":false,"lamports":10000000,"data":["","base64"]});
    assert!(pump::decode_account(&empty).is_ok());
    assert!(RpcAccount::decode(&empty).is_err());
    let mut bad = empty.clone();
    bad["executable"] = true.into();
    assert!(pump::decode_account(&bad).is_err());
    let mut bad = empty;
    bad["data"][0] = "AA==".into();
    assert!(pump::decode_account(&bad).is_err());
}
