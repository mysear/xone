use curve25519_dalek::edwards::CompressedEdwardsY;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{Pool, Result};
use xone_solana_ingest::raydium::RpcAccount;
use xone_solana_ingest::raydium_clmm::{decode_raydium_clmm, required_accounts};
use xone_solana_ingest::rpc::TOKEN_PROGRAM;

const PROGRAM: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const LOADER2: &str = "BPFLoader2111111111111111111111111111111111";
const CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
const SYSVAR: &str = "Sysvar1111111111111111111111111111111111111";
fn bytes(k: &str) -> Vec<u8> {
    bs58::decode(k).into_vec().unwrap()
}
fn key(n: u8) -> String {
    bs58::encode([n; 32]).into_string()
}
fn digest(d: &[u8]) -> String {
    format!("{:x}", Sha256::digest(d))
}
fn pda(seeds: &[&[u8]], program: &str) -> (String, u8) {
    for b in (0..=255u8).rev() {
        let mut h = Sha256::new();
        for s in seeds {
            h.update(s);
        }
        h.update([b]);
        h.update(bytes(program));
        h.update(b"ProgramDerivedAddress");
        let v: [u8; 32] = h.finalize().into();
        if CompressedEdwardsY(v).decompress().is_none() {
            return (bs58::encode(v).into_string(), b);
        }
    }
    panic!("PDA");
}
fn put_key(d: &mut [u8], o: usize, k: &str) {
    d[o..o + 32].copy_from_slice(&bytes(k));
}
fn put64(d: &mut [u8], o: usize, n: u64) {
    d[o..o + 8].copy_from_slice(&n.to_le_bytes());
}
fn put128(d: &mut [u8], o: usize, n: u128) {
    d[o..o + 16].copy_from_slice(&n.to_le_bytes());
}
fn disc(name: &str) -> [u8; 8] {
    Sha256::digest(format!("account:{name}").as_bytes())[..8]
        .try_into()
        .unwrap()
}
fn account(owner: &str, data: Vec<u8>, executable: bool) -> RpcAccount {
    RpcAccount {
        owner: owner.into(),
        data,
        executable,
        lamports: 100_000_000,
    }
}
struct Fixture {
    chain: Chain,
    pool: PoolConfig,
    accounts: BTreeMap<String, RpcAccount>,
    arrays: Vec<String>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_mints(key(1), key(2))
    }
    fn with_mints(m0: String, m1: String) -> Self {
        let (cfg, cb) = pda(&[b"amm_config", &0x1234u16.to_be_bytes()], PROGRAM);
        let (pool, pb) = pda(&[b"pool", &bytes(&cfg), &bytes(&m0), &bytes(&m1)], PROGRAM);
        let vault0 = pda(&[b"pool_vault", &bytes(&pool), &bytes(&m0)], PROGRAM).0;
        let vault1 = pda(&[b"pool_vault", &bytes(&pool), &bytes(&m1)], PROGRAM).0;
        let obs = pda(&[b"observation", &bytes(&pool)], PROGRAM).0;
        let pd = pda(&[&bytes(PROGRAM)], LOADER).0;
        let code = b"synthetic-clmm-code-proof";
        let token_code = b"synthetic-classic-token-proof";
        let ids = vec![pool.clone(), vault0, vault1, cfg, m0, m1, obs];
        let starts: [i32; 3] = [-60, 0, 60];
        let arrays = starts
            .iter()
            .map(|s| pda(&[b"tick_array", &bytes(&pool), &s.to_be_bytes()], PROGRAM).0)
            .collect::<Vec<_>>();
        let pool: PoolConfig = serde_json::from_value(json!({
            "id": pool, "protocol":"raydium-clmm-static", "token0":ids[4], "token1":ids[5],
            "fee_numerator":9999, "fee_denominator":1000000, "program_id":PROGRAM, "accounts":ids,
            "raydium_clmm":{"expected_programdata":pd,"expected_program_code_sha256":digest(code),
                "expected_deployed_slot":50,"expected_token_program_code_sha256":digest(token_code),
                "tick_array_starts":starts,"max_steps":100}
        }))
        .unwrap();
        let chain: Chain = serde_json::from_value(json!({
            "key":"solana:mainnet","owner_service":"test-parser","assignment_version":1,
            "genesis_hash":key(20),"rpc_urls":[],"confirmation_tag":"finalized",
            "auto_reorg_window":32,"pools":[pool]
        }))
        .unwrap();
        let mut state = vec![0; 1544];
        state[..8].copy_from_slice(&disc("PoolState"));
        state[8] = pb;
        for (o, k) in [
            (9, &ids[3]),
            (41, &key(9)),
            (73, &ids[4]),
            (105, &ids[5]),
            (137, &ids[1]),
            (169, &ids[2]),
            (201, &ids[6]),
        ] {
            put_key(&mut state, o, k);
        }
        state[233] = 6;
        state[234] = 6;
        state[235..237].copy_from_slice(&1u16.to_le_bytes());
        put128(&mut state, 237, 1000);
        put128(&mut state, 253, 1u128 << 64);
        put128(&mut state, 277, 17);
        put128(&mut state, 293, 19);
        put64(&mut state, 309, 23);
        put64(&mut state, 317, 29);
        // RewardInfo::new(creator) preserves authority on otherwise uninitialized rewards.
        for i in 0..3 {
            put_key(&mut state, 397 + i * 169 + 121, &key(9));
        }
        put64(&mut state, 904 + 8 * 8, 1); // array zero: bitmap bit 512
        put64(&mut state, 1064, 31);
        put64(&mut state, 1072, 37);
        put64(&mut state, 1080, 999);
        put64(&mut state, 1088, 7);
        let mut cfgdata = vec![0; 117];
        cfgdata[..8].copy_from_slice(&disc("AmmConfig"));
        cfgdata[8] = cb;
        cfgdata[9..11].copy_from_slice(&0x1234u16.to_le_bytes());
        put_key(&mut cfgdata, 11, &key(9));
        cfgdata[43..47].copy_from_slice(&120000u32.to_le_bytes());
        cfgdata[47..51].copy_from_slice(&2500u32.to_le_bytes());
        cfgdata[51..53].copy_from_slice(&1u16.to_le_bytes());
        cfgdata[53..57].copy_from_slice(&40000u32.to_le_bytes());
        put_key(&mut cfgdata, 61, &key(10));
        let mut a = BTreeMap::new();
        a.insert(ids[0].clone(), account(PROGRAM, state, false));
        a.insert(ids[3].clone(), account(PROGRAM, cfgdata, false));
        for i in 0..2 {
            let mut v = vec![0; 165];
            put_key(&mut v, 0, &ids[4 + i]);
            put_key(&mut v, 32, &ids[0]);
            put64(&mut v, 64, 10000 + i as u64 * 10000);
            v[108] = 1;
            let mut m = vec![0; 82];
            m[44] = 6;
            m[45] = 1;
            a.insert(ids[1 + i].clone(), account(TOKEN_PROGRAM, v, false));
            a.insert(ids[4 + i].clone(), account(TOKEN_PROGRAM, m, false));
        }
        let mut observation = vec![0; 4483];
        observation[..8].copy_from_slice(&disc("ObservationState"));
        observation[8] = 1;
        put64(&mut observation, 9, 7);
        put_key(&mut observation, 19, &ids[0]);
        observation[51..55].copy_from_slice(&990u32.to_le_bytes());
        observation[55..63].copy_from_slice(&(-123i64).to_le_bytes());
        a.insert(ids[6].clone(), account(PROGRAM, observation, false));
        for (i, start) in starts.iter().enumerate() {
            let mut d = vec![0; 10240];
            d[..8].copy_from_slice(&disc("TickArrayState"));
            put_key(&mut d, 8, &ids[0]);
            d[40..44].copy_from_slice(&start.to_le_bytes());
            if *start == 0 {
                put128(&mut d, 44 + 20, 1000);
                put128(&mut d, 44 + 4, 1000);
                put128(&mut d, 44 + 36, 43);
                d[10124] = 1;
            }
            put64(&mut d, 10125, 7);
            a.insert(arrays[i].clone(), account(PROGRAM, d, false));
        }
        let mut program = vec![0; 36];
        program[..4].copy_from_slice(&2u32.to_le_bytes());
        put_key(&mut program, 4, &pd);
        a.insert(PROGRAM.into(), account(LOADER, program, true));
        let mut pdata = vec![0; 45];
        pdata[..4].copy_from_slice(&3u32.to_le_bytes());
        put64(&mut pdata, 4, 50);
        pdata[12] = 1;
        put_key(&mut pdata, 13, &key(30));
        pdata.extend_from_slice(code);
        a.insert(pd, account(LOADER, pdata, false));
        a.insert(
            TOKEN_PROGRAM.into(),
            account(LOADER2, token_code.to_vec(), true),
        );
        let mut clock = vec![0; 40];
        put64(&mut clock, 0, 100);
        put64(&mut clock, 16, 7);
        put64(&mut clock, 32, 1000);
        a.insert(CLOCK.into(), account(SYSVAR, clock, false));
        Self {
            chain,
            pool,
            accounts: a,
            arrays,
        }
    }
    fn decode(&self) -> Result<Pool> {
        decode_raydium_clmm(&self.chain, &self.pool, &self.accounts, 100)
    }
    fn state(&mut self) -> &mut Vec<u8> {
        &mut self.accounts.get_mut(&self.pool.id).unwrap().data
    }
    fn data(&mut self, i: usize) -> &mut Vec<u8> {
        &mut self.accounts.get_mut(&self.pool.accounts[i]).unwrap().data
    }
    fn array(&mut self, i: usize) -> &mut Vec<u8> {
        &mut self.accounts.get_mut(&self.arrays[i]).unwrap().data
    }
}

#[test]
fn pinned_layout_decodes_independent_clmm_state() {
    let f = Fixture::new();
    let ids = required_accounts(&f.pool).unwrap();
    assert_eq!(ids.len(), 14);
    assert!(ids.iter().all(|k| f.accounts.contains_key(k)));
    let p = f.decode().unwrap();
    p.validate().unwrap();
    let j = serde_json::to_value(p).unwrap();
    assert_eq!(j["protocol"], "raydium-clmm-static");
    assert_eq!(j["adapter_version"], "raydium-clmm/6d7a0f82-spl-static-1");
    assert_eq!(j["reserve0"], "0");
    assert_eq!(j["fee_numerator"], 2500);
    let s = &j["raydium_clmm"];
    assert_eq!(s["sqrt_price_x64"], "18446744073709551616");
    assert_eq!(s["raw_vault1"], 20000);
    assert_eq!(s["bitmap"].as_array().unwrap().len(), 16);
    assert_eq!(s["bitmap"][8], 1);
    assert_eq!(s["tick_arrays"][1]["ticks"][0]["liquidity_gross"], "1000");
    assert_eq!(s["tick_arrays"][1]["ticks"][0]["fee_growth_outside0"], "43");
    assert_eq!(s["observations"].as_array().unwrap().len(), 100);
    assert_eq!(s["observations"][0]["tick_cumulative"], -123);
}
#[test]
fn rejects_every_missing_dependency_and_executable_data_account() {
    let f = Fixture::new();
    for id in required_accounts(&f.pool).unwrap() {
        let mut f = Fixture::new();
        f.accounts.remove(&id);
        assert!(f.decode().is_err(), "missing {id}");
    }
    for i in 0..7 {
        let mut f = Fixture::new();
        f.accounts.get_mut(&f.pool.accounts[i]).unwrap().executable = true;
        assert!(f.decode().is_err(), "executable {i}");
    }
}
#[test]
fn rejects_unsupported_pool_bytes() {
    for o in [
        0, 273, 275, 325, 389, 390, 391, 392, 393, 397, 398, 454, 486, 550, 1032, 1096, 1176, 1288,
        1543,
    ] {
        let mut f = Fixture::new();
        f.state()[o] = 255;
        assert!(f.decode().is_err(), "pool offset {o}");
    }
}
#[test]
fn rejects_tick_orders_rewards_padding_count_and_bitmap_mismatch() {
    for o in [
        44 + 68,
        44 + 116,
        44 + 124,
        44 + 132,
        44 + 140,
        44 + 156,
        10124,
        10133,
        10239,
    ] {
        let mut f = Fixture::new();
        f.array(1)[o] = 7;
        assert!(f.decode().is_err(), "array offset {o}");
    }
    let mut f = Fixture::new();
    put64(f.state(), 968, 0);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    put64(f.state(), 960, 1u64 << 63);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    put128(f.array(0), 44 + 4, 1);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    f.array(1)[44..48].copy_from_slice(&1i32.to_le_bytes());
    assert!(f.decode().is_err());
}
#[test]
fn observes_active_entrypoint_open_time_and_clock_bounds() {
    let mut f = Fixture::new();
    put64(f.state(), 1080, 1000);
    assert!(f.decode().is_err());
    put64(f.state(), 1080, 999);
    assert!(f.decode().is_ok());
    for time in [u64::MAX, u32::MAX as u64 + 1] {
        let mut f = Fixture::new();
        put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 32, time);
        assert!(f.decode().is_err());
    }
    let mut f = Fixture::new();
    put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 0, 99);
    assert!(f.decode().is_err());
}
#[test]
fn rejects_invalid_observation_but_allows_linked_uninitialized_state() {
    for o in [0, 8, 17, 19, 63, 4482] {
        let mut f = Fixture::new();
        f.data(6)[o] = 255;
        assert!(f.decode().is_err(), "observation offset {o}");
    }
    let mut f = Fixture::new();
    f.data(6)[8] = 0;
    f.data(6)[51..4451].fill(0);
    assert!(f.decode().is_ok());
}
#[test]
fn rejects_proof_identity_and_nonfinalized_bank() {
    let mut f = Fixture::new();
    f.chain.confirmation_tag = "confirmed".into();
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    f.chain.key = "eip155:1".into();
    assert!(f.decode().is_err());
    for id in [
        PROGRAM.to_string(),
        TOKEN_PROGRAM.to_string(),
        Fixture::new()
            .pool
            .raydium_clmm
            .as_ref()
            .unwrap()
            .expected_programdata
            .clone(),
    ] {
        let mut f = Fixture::new();
        f.accounts.get_mut(&id).unwrap().data.push(1);
        assert!(f.decode().is_err(), "code {id}");
    }
}
#[test]
fn rejects_token_extensions_delegate_and_wrong_authority() {
    for o in [0, 32, 72, 108, 109, 121, 129] {
        let mut f = Fixture::new();
        f.data(1)[o] = 255;
        assert!(f.decode().is_err(), "vault offset {o}");
    }
    for o in [0, 45, 46] {
        let mut f = Fixture::new();
        f.data(4)[o] = 255;
        assert!(f.decode().is_err(), "mint offset {o}");
    }
    let mut f = Fixture::new();
    f.accounts.get_mut(&f.pool.accounts[4]).unwrap().owner = key(50);
    assert!(f.decode().is_err());
}
#[test]
fn rejects_bad_config_padding_rates_spacing_and_pda() {
    for o in [0, 8, 9, 57, 93, 116] {
        let mut f = Fixture::new();
        f.data(3)[o] ^= 255;
        assert!(f.decode().is_err(), "config offset {o}");
    }
    let mut f = Fixture::new();
    f.data(3)[47..51].copy_from_slice(&1_000_000u32.to_le_bytes());
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    f.data(3)[51..53].fill(0);
    assert!(f.decode().is_err());
}
#[test]
fn rejects_noncanonical_programdata_and_array_window_before_fetch() {
    let mut f = Fixture::new();
    f.pool.raydium_clmm.as_mut().unwrap().expected_programdata = key(50);
    assert!(required_accounts(&f.pool).is_err());
    for starts in [vec![0], vec![-60, 0, 0], vec![-60, 0, 120], vec![-1, 0, 60]] {
        let mut f = Fixture::new();
        f.pool.raydium_clmm.as_mut().unwrap().tick_array_starts = starts;
        assert!(f.decode().is_err());
    }
}

#[derive(Clone)]
struct MockRpc {
    accounts: std::sync::Arc<BTreeMap<String, serde_json::Value>>,
    calls: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    change_anchor: bool,
}
async fn serve_rpc(
    axum::extract::State(s): axum::extract::State<MockRpc>,
    axum::Json(q): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let mut calls = s.calls.lock().unwrap();
    calls.push(q.clone());
    let result = match q["method"].as_str().unwrap() {
        "getGenesisHash" => json!(key(20)),
        "getMultipleAccounts" => {
            json!({"context":{"slot":100},"value":q["params"][0].as_array().unwrap().iter().map(|id|s.accounts.get(id.as_str().unwrap()).cloned().unwrap_or(serde_json::Value::Null)).collect::<Vec<_>>()})
        }
        "getBlock" => {
            json!({"blockTime":1000,"blockhash":key(if s.change_anchor&&calls.iter().filter(|c|c["method"]=="getBlock").count()>1 {72}else{70}),"previousBlockhash":key(71),"parentSlot":99})
        }
        _ => panic!("unexpected method"),
    };
    axum::Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
async fn run_rpc(
    mut f: Fixture,
    changed: bool,
) -> (Result<xone_domain::Snapshot>, Vec<serde_json::Value>) {
    use base64::Engine;
    let accounts=f.accounts.iter().map(|(id,a)|(id.clone(),json!({"owner":a.owner,"data":[base64::engine::general_purpose::STANDARD.encode(&a.data),"base64"],"executable":a.executable,"lamports":a.lamports}))).collect();
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let state = MockRpc {
        accounts: std::sync::Arc::new(accounts),
        calls: calls.clone(),
        change_anchor: changed,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new()
                .route("/", axum::routing::post(serve_rpc))
                .with_state(state),
        )
        .await
        .unwrap()
    });
    f.chain.rpc_urls = vec![format!("http://{addr}/")];
    f.chain.pools = vec![f.pool];
    let result = xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    }
    .current_baseline(&f.chain)
    .await;
    task.abort();
    let requests = calls.lock().unwrap().clone();
    (result, requests)
}
#[tokio::test]
async fn rpc_fetches_all_dependencies_in_one_finalized_batch_and_rechecks_anchor() {
    let (result, calls) = run_rpc(Fixture::new(), false).await;
    let snapshot = result.unwrap();
    assert_eq!(
        snapshot.pools[0].raydium_clmm.as_ref().unwrap().bank_slot,
        100
    );
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[1]["method"], "getMultipleAccounts");
    assert_eq!(calls[1]["params"][0].as_array().unwrap().len(), 14);
    assert_eq!(calls[1]["params"][1]["commitment"], "finalized");
    let (result, _) = run_rpc(Fixture::new(), true).await;
    assert_eq!(result.unwrap_err().0, "ACCOUNT_BANK_ANCHOR_CHANGED");
}
#[tokio::test]
async fn rpc_rejects_nonfinalized_before_provider_selection() {
    let mut f = Fixture::new();
    f.chain.confirmation_tag = "confirmed".into();
    f.chain.pools = vec![f.pool];
    let rpc = xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    assert_eq!(
        rpc.current_baseline(&f.chain).await.unwrap_err().0,
        "RAYCLMM_REQUIRES_FINALIZED"
    );
}
#[test]
fn malformed_extreme_tick_and_truncated_layouts_fail_without_panicking() {
    for tick in [i32::MIN, i32::MAX, -443637, 443636] {
        let mut f = Fixture::new();
        f.state()[269..273].copy_from_slice(&tick.to_le_bytes());
        assert!(f.decode().is_err());
    }
    for i in 0..7 {
        let mut f = Fixture::new();
        f.data(i).truncate(1);
        assert!(f.decode().is_err());
    }
    for i in 0..3 {
        let mut f = Fixture::new();
        f.array(i).truncate(1);
        assert!(f.decode().is_err());
    }
}
#[test]
fn base_bitmap_bits_outside_proof_window_are_preserved() {
    let mut f = Fixture::new();
    put64(f.state(), 904, 1);
    let p = f.decode().unwrap();
    assert_eq!(p.raydium_clmm.unwrap().bitmap[0], 1);
}
#[test]
fn wrapped_sol_requires_native_flag_nine_decimals_and_lamport_backing() {
    let mut f = Fixture::with_mints(key(1), "So11111111111111111111111111111111111111112".into());
    f.state()[234] = 9;
    f.data(5)[44] = 9;
    f.data(2)[109..113].copy_from_slice(&1u32.to_le_bytes());
    put64(f.data(2), 113, 2039280);
    assert!(f.decode().is_ok());
    let id = f.pool.accounts[2].clone();
    f.accounts.get_mut(&id).unwrap().lamports = 2039280 + 19999;
    assert!(f.decode().is_err());
    f.accounts.get_mut(&id).unwrap().lamports = 2039280 + 20000;
    assert!(f.decode().is_ok());
    f.data(2)[109..113].fill(0);
    assert!(f.decode().is_err());
}
#[test]
fn rejects_all_account_owner_changes_and_future_epochs() {
    let f = Fixture::new();
    for id in required_accounts(&f.pool).unwrap() {
        let mut f = Fixture::new();
        f.accounts.get_mut(&id).unwrap().owner = key(50);
        assert!(f.decode().is_err(), "owner {id}");
    }
    let mut f = Fixture::new();
    put64(f.state(), 1088, 8);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    put64(f.data(6), 9, 8);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    put64(f.array(1), 10125, 8);
    assert!(f.decode().is_err());
}
#[test]
fn classic_none_option_payloads_can_be_stale_and_cleared_ticks_keep_index() {
    let mut f = Fixture::new();
    f.data(1)[76..108].fill(7);
    f.data(1)[133..165].fill(8);
    f.data(4)[4..36].fill(9);
    f.data(4)[50..82].fill(10);
    f.array(0)[44..48].copy_from_slice(&(-60i32).to_le_bytes());
    assert!(f.decode().is_ok());
}
#[test]
fn original_program_account_fixture_matches_full_independent_state() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/solana/rayclmm-base.json"
    ))
    .unwrap();
    let p: PoolConfig = serde_json::from_value(fixture["pool_config"].clone()).unwrap();
    let expected: Pool = serde_json::from_value(fixture["pool"].clone()).unwrap();
    let slot = fixture["metadata"]["bank_slot"].as_u64().unwrap();
    let chain:Chain=serde_json::from_value(json!({"key":expected.chain_key,"owner_service":"test-parser","assignment_version":1,"genesis_hash":key(20),"rpc_urls":[],"confirmation_tag":"finalized","auto_reorg_window":32,"pools":[p]})).unwrap();
    let accounts = fixture["rpc_accounts"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, a)| (id.clone(), RpcAccount::decode(a).unwrap()))
        .collect();
    let actual = decode_raydium_clmm(&chain, &p, &accounts, slot).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}
#[tokio::test]
async fn rpc_rejects_null_proof_or_array_and_mismatched_clock_bank() {
    let mut f = Fixture::new();
    f.accounts.remove(CLOCK);
    let (r, _) = run_rpc(f, false).await;
    assert!(r.is_err());
    let mut f = Fixture::new();
    f.accounts.remove(&f.arrays[2]);
    let (r, _) = run_rpc(f, false).await;
    assert!(r.is_err());
    let mut f = Fixture::new();
    put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 0, 101);
    let (r, _) = run_rpc(f, false).await;
    assert_eq!(r.unwrap_err().0, "CLOCK_CONTEXT_MISMATCH");
}
#[tokio::test]
async fn rpc_clmm_shards_without_clock_reject_before_anchor() {
    let mut f = Fixture::new();
    f.chain.pools = (30..40)
        .map(|n| {
            let mut p = f.pool.clone();
            p.id = key(n);
            p.accounts[0] = p.id.clone();
            p
        })
        .collect();
    // run_rpc normally installs one pool; create an intentionally oversized profile
    // using distinct business keys and array PDAs to exercise guarded multi-fetch.
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let state = MockRpc {
        accounts: std::sync::Arc::new(BTreeMap::new()),
        calls: calls.clone(),
        change_anchor: false,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new()
                .route("/", axum::routing::post(serve_rpc))
                .with_state(state),
        )
        .await
        .unwrap()
    });
    // Distinct arrays and business accounts exceed 100 after shared proofs are deduplicated.
    for (i, p) in f.chain.pools.iter_mut().enumerate() {
        p.raydium_clmm.as_mut().unwrap().tick_array_starts = vec![-180, -120, -60, 0, 60, 120, 180];
        p.accounts[1] = key(60 + i as u8);
        p.accounts[2] = key(80 + i as u8);
        p.accounts[3] = key(100 + i as u8);
        p.accounts[6] = key(120 + i as u8);
    }
    f.chain.rpc_urls = vec![format!("http://{addr}/")];
    let result = xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    }
    .current_baseline(&f.chain)
    .await;
    task.abort();
    assert_eq!(result.unwrap_err().0, "ACCOUNT_SHARD_CLOCK_INVALID");
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1]["method"], "getMultipleAccounts");
    assert!(calls.iter().all(|q| q["method"] != "getBlock"));
}

#[test]
fn token_fee_profile_reads_actual_fee_byte_and_keeps_old_admission_closed() {
    for mode in [1u8, 2] {
        let mut f = Fixture::new();
        f.accounts.get_mut(&f.pool.id).unwrap().data[390] = mode;
        assert!(decode_raydium_clmm(&f.chain, &f.pool, &f.accounts, 100).is_err());
        f.pool.protocol = "raydium-clmm-token-fee".into();
        let p = decode_raydium_clmm(&f.chain, &f.pool, &f.accounts, 100).unwrap();
        assert_eq!(p.raydium_clmm.unwrap().collect_fee_on, u32::from(mode));
        assert_eq!(p.adapter_version, "raydium-clmm/6d7a0f82-spl-token-fee-1");
    }
    for mode in [0u8, 3, 255] {
        let mut f = Fixture::new();
        f.pool.protocol = "raydium-clmm-token-fee".into();
        f.accounts.get_mut(&f.pool.id).unwrap().data[390] = mode;
        assert!(decode_raydium_clmm(&f.chain, &f.pool, &f.accounts, 100).is_err());
    }
}

#[tokio::test]
async fn token_fee_rpc_preserves_fee_mode_in_finalized_atomic_bank() {
    for mode in [1u8, 2] {
        let mut f = Fixture::new();
        f.pool.protocol = "raydium-clmm-token-fee".into();
        f.accounts.get_mut(&f.pool.id).unwrap().data[390] = mode;
        let (snapshot, calls) = run_rpc(f, false).await;
        let snapshot = snapshot.unwrap();
        assert_eq!(
            snapshot.pools[0]
                .raydium_clmm
                .as_ref()
                .unwrap()
                .collect_fee_on,
            u32::from(mode)
        );
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[1]["params"][1]["commitment"], "finalized");
    }
    let mut f = Fixture::new();
    f.pool.protocol = "raydium-clmm-token-fee".into();
    f.chain.confirmation_tag = "confirmed".into();
    let (result, _) = run_rpc(f, false).await;
    assert_eq!(result.unwrap_err().0, "RAYCLMM_REQUIRES_FINALIZED");
}
