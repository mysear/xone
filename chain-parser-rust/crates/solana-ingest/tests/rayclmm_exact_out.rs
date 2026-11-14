use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::Pool;
use xone_solana_ingest::raydium::RpcAccount;
use xone_solana_ingest::raydium_clmm::{decode_raydium_clmm, required_accounts};
fn data() -> Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/rayclmm-exact-out-frames.json"
        ))
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn output_native_checkpoints_decode_all_six_profiles_before_and_after() {
    let f = data();
    assert_eq!(f["checkpoints"].as_array().unwrap().len(), 6);
    let mut count = 0;
    for c in f["checkpoints"].as_array().unwrap() {
        for phase in ["before", "after"] {
            let fixture = &c[phase];
            let p: PoolConfig = serde_json::from_value(fixture["pool_config"].clone()).unwrap();
            let expected: Pool = serde_json::from_value(fixture["pool"].clone()).unwrap();
            let slot = fixture["metadata"]["bank_slot"].as_u64().unwrap();
            let chain:Chain=serde_json::from_value(json!({"key":expected.chain_key,"owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d","rpc_urls":[],"confirmation_tag":"finalized","auto_reorg_window":32,"pools":[p]})).unwrap();
            let accounts = fixture["rpc_accounts"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, a)| (id.clone(), RpcAccount::decode(a).unwrap()))
                .collect::<BTreeMap<_, _>>();
            for id in required_accounts(&p).unwrap() {
                assert!(
                    accounts.contains_key(&id),
                    "{} {phase}: missing {id}",
                    c["profile"]
                );
            }
            let decoded = decode_raydium_clmm(&chain, &p, &accounts, slot)
                .unwrap_or_else(|e| panic!("{} {phase}: {}", c["profile"], e.0));
            assert_eq!(
                decoded, expected,
                "{} {phase}: complete original serialized state",
                c["profile"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 12);
}
