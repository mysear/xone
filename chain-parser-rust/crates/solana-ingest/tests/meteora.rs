use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{meteora::*, Pool};
use xone_solana_ingest::{meteora, raydium::RpcAccount};
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/meteora-mm.json")).unwrap()
}
fn setup(f: &Value, index: usize) -> (Chain, PoolConfig, BTreeMap<String, RpcAccount>, Pool) {
    let p: PoolConfig = serde_json::from_value(f["decoder"]["config"].clone()).unwrap();
    let ch:Chain=serde_json::from_value(json!({"key":"solana:mainnet-beta","owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":"5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d","rpc_urls":[],"pools":[p],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
    let mut accounts = BTreeMap::new();
    for object in [
        &f["decoder"]["shared_accounts"],
        &f["decoder"]["cases"][index]["accounts"],
    ] {
        for (id, value) in object.as_object().unwrap() {
            accounts.insert(id.clone(), RpcAccount::decode(value).unwrap());
        }
    }
    let expected: Pool =
        serde_json::from_value(f["decoder"]["cases"][index]["pool"].clone()).unwrap();
    (ch, p, accounts, expected)
}
#[test]
fn all_recorded_bank_layouts_decode_with_explicit_executable_and_array_closure() {
    let f = data();
    assert_eq!(f["decoder"]["cases"].as_array().unwrap().len(), 70);
    for index in 0..70 {
        let (ch, p, accounts, expected) = setup(&f, index);
        let actual = meteora::decode(&ch, &p, &accounts, 300000000)
            .unwrap_or_else(|e| panic!("case {index}: {e}"));
        assert_eq!(actual, expected);
        assert_eq!(
            meteora::required_accounts(&p).unwrap().len(),
            accounts.len()
        );
        assert_eq!(
            xone_solana_ingest::rpc::required_scope(&ch).unwrap().len(),
            accounts.len()
        );
    }
}
#[test]
fn altered_program_clock_arrays_orders_rewards_tokens_and_backing_fail_closed() {
    let f = data();
    let (ch, p, accounts, _) = setup(&f, 3);
    let expect =
        |a: &BTreeMap<String, RpcAccount>| assert!(meteora::decode(&ch, &p, a, 300000000).is_err());
    let mut a = accounts.clone();
    a.remove(&p.accounts[6]);
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.accounts[6]).unwrap().data[24] ^= 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.accounts[6]).unwrap().data[56 + 48] = 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.id).unwrap().data[264] = 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.id).unwrap().data[35] = 2;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.id).unwrap().data[36] = 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.id).unwrap().data[880] = 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.id).unwrap().data[82] = 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.accounts[1]).unwrap().data[32] ^= 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.accounts[2]).unwrap().lamports = 0;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.accounts[3]).unwrap().data.push(0);
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.accounts[5]).unwrap().data[8..16].copy_from_slice(&100u64.to_le_bytes());
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(xone_solana_ingest::raydium::CLOCK).unwrap().data[0] ^= 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(&p.meteora.as_ref().unwrap().expected_programdata)
        .unwrap()
        .data[45] ^= 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(
        &p.meteora
            .as_ref()
            .unwrap()
            .token_program_upgrade
            .as_ref()
            .unwrap()
            .programdata,
    )
    .unwrap()
    .data[45] ^= 1;
    expect(&a);
    let mut a = accounts.clone();
    a.get_mut(PROGRAM).unwrap().executable = false;
    expect(&a);
    let mut a = accounts.clone();
    let oracle = &mut a.get_mut(&p.accounts[5]).unwrap().data;
    oracle[8..16].copy_from_slice(&0u64.to_le_bytes());
    oracle[16..24].copy_from_slice(&1u64.to_le_bytes());
    expect(&a);
    let mut unknown = p.clone();
    unknown
        .meteora
        .as_mut()
        .unwrap()
        .expected_token_program_code_sha256 = "00".repeat(32);
    assert!(meteora::required_accounts(&unknown).is_err());
    let mut unqualified: Pool =
        serde_json::from_value(f["decoder"]["cases"][3]["pool"].clone()).unwrap();
    unqualified
        .meteora
        .as_mut()
        .unwrap()
        .token_program_code_sha256 = "00".repeat(32);
    assert!(unqualified.validate().is_err());
    let mut config = p.clone();
    config.meteora.as_mut().unwrap().bin_array_indices.reverse();
    assert!(meteora::required_accounts(&config).is_err());
    let mut chain = ch.clone();
    chain.confirmation_tag = "confirmed".into();
    assert!(meteora::decode(&chain, &p, &accounts, 300000000).is_err());
}
#[tokio::test]
async fn ordinary_rpc_preserves_complete_bank_anchor_and_refuses_changed_anchor() {
    use axum::{routing::post, Json, Router};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let f = data();
    let (mut ch, _, _, expected) = setup(&f, 3);
    let bank = Arc::new({
        let mut m = f["decoder"]["shared_accounts"].as_object().unwrap().clone();
        m.extend(
            f["decoder"]["cases"][3]["accounts"]
                .as_object()
                .unwrap()
                .clone(),
        );
        m
    });
    let changed = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let genesis = ch.genesis_hash.clone();
    let b = bank.clone();
    let switch = changed.clone();
    let count = calls.clone();
    let app=Router::new().route("/",post(move|Json(req):Json<Value>|{let b=b.clone();let genesis=genesis.clone();let switch=switch.clone();let count=count.clone();async move{
        let result=match req["method"].as_str().unwrap(){
            "getGenesisHash"=>json!(genesis),
            "getMultipleAccounts"=>json!({"context":{"slot":300000000},"value":req["params"][0].as_array().unwrap().iter().map(|k|b[k.as_str().unwrap()].clone()).collect::<Vec<_>>()}),
            "getBlock"=>{let i=count.fetch_add(1,Ordering::SeqCst);json!({"blockhash":bs58::encode([if switch.load(Ordering::SeqCst)&&i%2==1{9}else{7};32]).into_string(),"previousBlockhash":bs58::encode([6u8;32]).into_string(),"parentSlot":299999999,"blockTime":1734392448})},
            _=>panic!("unexpected RPC {req}")};Json(json!({"jsonrpc":"2.0","id":req["id"],"result":result}))
    }}));
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    ch.rpc_urls = vec![format!("http://{}", socket.local_addr().unwrap())];
    let server = tokio::spawn(async move { axum::serve(socket, app).await.unwrap() });
    let rpc = xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 8 << 20,
    };
    let snapshot = rpc.current_baseline(&ch).await.unwrap();
    snapshot
        .validate(&ch.owner_service, ch.assignment_version)
        .unwrap();
    assert_eq!(snapshot.pools, vec![expected]);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let wire = xone_domain::wire_conversion::snapshot_to_proto(&snapshot).unwrap();
    let again = xone_domain::wire_conversion::snapshot_from_proto(wire).unwrap();
    assert_eq!(again, snapshot);
    changed.store(true, Ordering::SeqCst);
    assert_eq!(
        rpc.current_baseline(&ch).await.unwrap_err().0,
        "ACCOUNT_BANK_ANCHOR_CHANGED"
    );
    server.abort();
}
