use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::{Pool, Result};
use xone_solana_ingest::raydium::{decode_raydium_cpmm, required_accounts, RpcAccount};
use xone_solana_ingest::rpc::{CPMM_PROGRAM, TOKEN_PROGRAM};

const AUTH: &str = "GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL";
const PROGRAMDATA: &str = "DMawCQzbgNTmbzaESc7o6pvL1KAeetY8zA7jNpzntHhU";
const CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
const SYSVAR: &str = "Sysvar1111111111111111111111111111111111111";
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const LOADER2: &str = "BPFLoader2111111111111111111111111111111111";

fn key(byte: u8) -> String {
    bs58::encode([byte; 32]).into_string()
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn put_key(data: &mut [u8], offset: usize, key: &str) {
    data[offset..offset + 32].copy_from_slice(&bs58::decode(key).into_vec().unwrap());
}
fn put_u64(data: &mut [u8], offset: usize, n: u64) {
    data[offset..offset + 8].copy_from_slice(&n.to_le_bytes());
}
fn account(owner: &str, data: Vec<u8>, executable: bool) -> RpcAccount {
    RpcAccount {
        owner: owner.into(),
        data,
        executable,
        lamports: 10_000_000,
    }
}

struct Fixture {
    chain: Chain,
    pool: PoolConfig,
    accounts: BTreeMap<String, RpcAccount>,
}
impl Fixture {
    fn new() -> Self {
        let ids = vec![
            key(1),
            "HM5LJoRU1xb4K3op9TGxz6udWZgMgVkPsQhXgMjTkxbL".into(),
            "52dRiRbWkzpdLThFBcdffZM2tcutQd5vP2BSxqeZrMgR".into(),
            "9mppGwQVvK1wnpUPoEyAmiXdJVrvnUYhJMrebeAxWMHu".into(),
            key(5),
            key(6),
        ];
        let code = b"pinned cp-swap fixture code";
        let token_code = b"pinned immutable spl-token fixture code";
        let pool: PoolConfig = serde_json::from_value(json!({
            "id": ids[0], "protocol": "raydium-cpmm-creator", "token0": ids[4], "token1": ids[5],
            "program_id": CPMM_PROGRAM, "accounts": ids,
            "fee_numerator": 2500, "fee_denominator": 1000000,
            "raydium_cpmm": {"expected_programdata": PROGRAMDATA,
                "expected_program_code_sha256": hash(code), "expected_deployed_slot": 50,
                "expected_token_program_code_sha256": hash(token_code)}
        }))
        .unwrap();
        let chain: Chain = serde_json::from_value(json!({
            "key": "solana:mainnet", "owner_service": "test-parser", "assignment_version": 1,
            "genesis_hash": key(20), "rpc_urls": [], "confirmation_tag": "finalized",
            "auto_reorg_window": 32, "pools": [pool]
        }))
        .unwrap();
        let mut state = vec![0; 637];
        state[..8].copy_from_slice(&[247, 237, 227, 245, 215, 195, 222, 70]);
        for (off, address) in [
            (8, ids[3].as_str()),
            (40, &key(9)),
            (72, &ids[1]),
            (104, &ids[2]),
            (136, "8ZomKXgUfMSiNqdBZdADNrPLXMd1gv44ydEFicEFdx8M"),
            (168, &ids[4]),
            (200, &ids[5]),
            (232, TOKEN_PROGRAM),
            (264, TOKEN_PROGRAM),
            (296, "4RMj4iVF4TkHLBTKaJ1Ws8AePaLqraWzmXwdYf2AH4yy"),
        ] {
            put_key(&mut state, off, address);
        }
        state[328] = 253;
        state[330..333].copy_from_slice(&[9, 6, 9]);
        for (off, n) in [
            (333, 1000),
            (341, 17),
            (349, 29),
            (357, 40),
            (365, 42),
            (373, 1000),
            (381, 7),
            (397, 13),
            (405, 19),
        ] {
            put_u64(&mut state, off, n);
        }
        state[389] = 2;
        state[390] = 1;
        let mut cfg = vec![0; 236];
        cfg[..8].copy_from_slice(&[218, 244, 33, 104, 203, 203, 43, 111]);
        cfg[8] = 255;
        cfg[10..12].copy_from_slice(&0x1234u16.to_le_bytes());
        for (off, n) in [
            (12, 2500),
            (20, 120000),
            (28, 40000),
            (108, 500),
            (116, 100000),
        ] {
            put_u64(&mut cfg, off, n);
        }
        put_key(&mut cfg, 44, &key(9));
        put_key(&mut cfg, 76, &key(10));
        let mut accounts = BTreeMap::new();
        accounts.insert(ids[0].clone(), account(CPMM_PROGRAM, state, false));
        accounts.insert(ids[3].clone(), account(CPMM_PROGRAM, cfg, false));
        for (i, amount, decimals) in [(0, 10000, 6), (1, 20000, 9)] {
            let mut vault = vec![0; 165];
            put_key(&mut vault, 0, &ids[4 + i]);
            put_key(&mut vault, 32, AUTH);
            put_u64(&mut vault, 64, amount);
            vault[108] = 1;
            accounts.insert(ids[1 + i].clone(), account(TOKEN_PROGRAM, vault, false));
            let mut mint = vec![0; 82];
            mint[44] = decimals;
            mint[45] = 1;
            accounts.insert(ids[4 + i].clone(), account(TOKEN_PROGRAM, mint, false));
        }
        let mut program = vec![0; 36];
        program[..4].copy_from_slice(&2u32.to_le_bytes());
        put_key(&mut program, 4, PROGRAMDATA);
        accounts.insert(CPMM_PROGRAM.into(), account(LOADER, program, true));
        let mut programdata = vec![0; 45];
        programdata[..4].copy_from_slice(&3u32.to_le_bytes());
        put_u64(&mut programdata, 4, 50);
        programdata[12] = 1;
        put_key(&mut programdata, 13, &key(30));
        programdata.extend_from_slice(code);
        accounts.insert(PROGRAMDATA.into(), account(LOADER, programdata, false));
        accounts.insert(
            TOKEN_PROGRAM.into(),
            account(LOADER2, token_code.to_vec(), true),
        );
        let mut clock = vec![0; 40];
        for (off, n) in [(0, 100), (8, 900), (16, 7), (24, 8), (32, 1000)] {
            put_u64(&mut clock, off, n);
        }
        accounts.insert(CLOCK.into(), account(SYSVAR, clock, false));
        Self {
            chain,
            pool,
            accounts,
        }
    }
    fn decode(&self) -> Result<Pool> {
        decode_raydium_cpmm(&self.chain, &self.pool, &self.accounts, 100)
    }
    fn data(&mut self, index: usize) -> &mut Vec<u8> {
        &mut self
            .accounts
            .get_mut(&self.pool.accounts[index])
            .unwrap()
            .data
    }
}

#[test]
fn decodes_creator_layout_with_pinned_programs_and_bank_clock() {
    let f = Fixture::new();
    let pool = f.decode().unwrap();
    let s = pool.raydium_cpmm.as_ref().unwrap();
    pool.validate().unwrap();
    assert_eq!(pool.protocol, "raydium-cpmm-creator");
    assert_eq!(pool.adapter_version, "raydium-cpmm/f4b9b263-spl-1");
    assert_eq!(pool.reserve0.to_string(), "9930");
    assert_eq!(pool.reserve1.to_string(), "19910");
    assert_eq!((s.raw_vault0, s.raw_vault1), (10000, 20000));
    assert_eq!((s.authority.as_str(), s.authority_bump), (AUTH, 253));
    assert_eq!(
        (
            s.bank_slot,
            s.bank_unix_timestamp,
            s.bank_epoch,
            s.recent_epoch
        ),
        (100, 1000, 7, 7)
    );
    assert_eq!((s.mint0_decimals, s.mint1_decimals), (6, 9));
    assert_eq!(
        (
            s.creator_fee_on,
            s.creator_fee_enabled,
            s.creator_fee_rate,
            s.creator_fee_share_rate
        ),
        (2, true, 500, 100000)
    );
    assert_eq!(
        (s.protocol_fee_rate, s.fund_fee_rate, pool.fee_numerator),
        (120000, 40000, 2500)
    );
    assert_eq!(
        (s.protocol_fees0, s.fund_fees0, s.creator_fees0),
        (17, 40, 13)
    );
    assert_eq!(
        (s.protocol_fees1, s.fund_fees1, s.creator_fees1),
        (29, 42, 19)
    );
}

#[test]
fn required_accounts_are_ten_unique_canonical_keys() {
    let f = Fixture::new();
    let ids = required_accounts(&f.pool).unwrap();
    assert_eq!(&ids[..6], &f.pool.accounts);
    assert_eq!(ids.len(), 10);
    for address in [CPMM_PROGRAM, PROGRAMDATA, TOKEN_PROGRAM, CLOCK] {
        assert!(ids.contains(&address.into()));
    }
    assert_eq!(
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        10
    );
    let mut p = f.pool.clone();
    p.accounts[1] = p.accounts[0].clone();
    assert!(required_accounts(&p).is_err());
    let mut p = f.pool.clone();
    p.id = key(99);
    assert!(required_accounts(&p).is_err());
    let mut p = f.pool.clone();
    p.token0 = key(99);
    assert!(required_accounts(&p).is_err());
    let mut p = f.pool.clone();
    p.accounts[0] = "0OIl".into();
    p.id = p.accounts[0].clone();
    assert!(required_accounts(&p).is_err());
    let mut p = f.pool.clone();
    p.raydium_cpmm = None;
    assert!(required_accounts(&p).is_err());
}

#[test]
fn rpc_account_preserves_executable_and_lamports_and_rejects_malformed_rpc() {
    let good = json!({"owner": LOADER2, "data": [STANDARD.encode(b"code"), "base64"], "executable": true, "lamports": 99});
    let a = RpcAccount::decode(&good).unwrap();
    assert!(a.executable);
    assert_eq!(a.lamports, 99);
    assert_eq!(a.data, b"code");
    for field in ["owner", "data", "executable", "lamports"] {
        let mut v = good.clone();
        v.as_object_mut().unwrap().remove(field);
        assert!(RpcAccount::decode(&v).is_err(), "{field}");
    }
    for (field, value) in [
        ("owner", json!("bad")),
        ("data", json!(["!!!", "base64"])),
        ("data", json!(["YQ==", "base64+zstd"])),
        ("data", json!(["YQ==", "base64", "extra"])),
        ("executable", json!(0)),
        ("lamports", json!(-1)),
    ] {
        let mut v = good.clone();
        v[field] = value;
        assert!(RpcAccount::decode(&v).is_err(), "{field}: {v}");
    }
    assert!(RpcAccount::decode(&serde_json::Value::Null).is_err());
}

#[test]
fn creator_modes_disable_flag_and_creation_gate_preserve_existing_ledgers() {
    for mode in 0..=2 {
        for enabled in 0..=1 {
            let mut f = Fixture::new();
            f.data(0)[389] = mode;
            f.data(0)[390] = enabled;
            f.data(3)[9] = 1;
            let p = f.decode().unwrap();
            let s = p.raydium_cpmm.unwrap();
            assert_eq!(s.creator_fee_on, mode as u32);
            assert_eq!(s.creator_fee_enabled, enabled == 1);
            assert_eq!(s.creator_fees0, 13);
            assert_eq!(p.reserve0.to_string(), "9930");
        }
    }
}

#[test]
fn rejects_wrong_layout_padding_flags_pdas_and_account_relations() {
    for (account, offset, value) in [
        (0, 0, 0),
        (3, 0, 0),
        (0, 389, 3),
        (0, 390, 2),
        (0, 391, 1),
        (0, 413, 1),
        (3, 124, 1),
        (3, 9, 2),
        (0, 328, 252),
        (3, 8, 254),
        (0, 329, 4),
        (0, 329, 8),
        (0, 331, 7),
        (0, 332, 8),
        (1, 108, 0),
        (1, 108, 2),
        (4, 45, 0),
        (4, 45, 2),
        (4, 0, 2),
        (4, 46, 2),
        (1, 72, 1),
        (1, 121, 1),
        (1, 129, 1),
        (1, 109, 2),
        (1, 109, 1),
    ] {
        let mut f = Fixture::new();
        f.data(account)[offset] = value;
        assert!(
            f.decode().is_err(),
            "account {account} offset {offset} value {value}"
        );
    }
    for (account, offset) in [
        (0, 8),
        (0, 72),
        (0, 104),
        (0, 136),
        (0, 168),
        (0, 200),
        (0, 232),
        (0, 264),
        (0, 296),
        (1, 0),
        (1, 32),
        (2, 0),
        (2, 32),
    ] {
        let mut f = Fixture::new();
        put_key(f.data(account), offset, &key(99));
        assert!(f.decode().is_err(), "account {account} key offset {offset}");
    }
    for (account, len) in [
        (0, 0),
        (0, 636),
        (0, 638),
        (3, 235),
        (3, 237),
        (1, 164),
        (1, 166),
        (4, 81),
        (4, 83),
    ] {
        let mut f = Fixture::new();
        f.data(account).resize(len, 0);
        assert!(f.decode().is_err(), "account {account} len {len}");
    }
    for i in 0..6 {
        let mut f = Fixture::new();
        let id = f.pool.accounts[i].clone();
        f.accounts.remove(&id);
        assert!(f.decode().is_err());
        let mut f = Fixture::new();
        f.accounts.get_mut(&id).unwrap().executable = true;
        assert!(f.decode().is_err());
        let mut f = Fixture::new();
        f.accounts.get_mut(&id).unwrap().owner = key(99);
        assert!(f.decode().is_err());
    }
}

#[test]
fn rejects_program_upgrades_loader_changes_and_bank_clock_mismatch() {
    for id in [CPMM_PROGRAM, PROGRAMDATA, TOKEN_PROGRAM, CLOCK] {
        let mut f = Fixture::new();
        f.accounts.remove(id);
        assert!(f.decode().is_err(), "missing {id}");
        let mut f = Fixture::new();
        f.accounts.get_mut(id).unwrap().owner = key(99);
        assert!(f.decode().is_err(), "owner {id}");
        let mut f = Fixture::new();
        let a = f.accounts.get_mut(id).unwrap();
        a.executable = !a.executable;
        assert!(f.decode().is_err(), "executable {id}");
    }
    for (id, off) in [
        (CPMM_PROGRAM, 0),
        (CPMM_PROGRAM, 4),
        (PROGRAMDATA, 0),
        (PROGRAMDATA, 4),
        (PROGRAMDATA, 12),
        (PROGRAMDATA, 45),
        (TOKEN_PROGRAM, 0),
        (CLOCK, 0),
    ] {
        let mut f = Fixture::new();
        f.accounts.get_mut(id).unwrap().data[off] ^= 4;
        assert!(f.decode().is_err(), "{id} offset {off}");
    }
    for (id, len) in [
        (CPMM_PROGRAM, 35),
        (CPMM_PROGRAM, 37),
        (PROGRAMDATA, 44),
        (PROGRAMDATA, 45),
        (CLOCK, 39),
        (CLOCK, 41),
    ] {
        let mut f = Fixture::new();
        f.accounts.get_mut(id).unwrap().data.resize(len, 0);
        assert!(f.decode().is_err(), "{id} len {len}");
    }
    let mut f = Fixture::new();
    f.pool.raydium_cpmm.as_mut().unwrap().expected_deployed_slot = 101;
    put_u64(&mut f.accounts.get_mut(PROGRAMDATA).unwrap().data, 4, 101);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    f.accounts.get_mut(TOKEN_PROGRAM).unwrap().owner = LOADER.into();
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    put_u64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 32, u64::MAX);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    put_u64(f.data(0), 373, 1001);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    put_u64(f.data(0), 381, 8);
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    f.chain.confirmation_tag = "confirmed".into();
    assert!(f.decode().is_err());
    let mut f = Fixture::new();
    f.chain.key = "evm:1".into();
    assert!(f.decode().is_err());
}

#[test]
fn exact_u64_reserves_and_checked_fee_ledgers() {
    let mut f = Fixture::new();
    put_u64(f.data(1), 64, u64::MAX);
    assert_eq!(
        f.decode().unwrap().reserve0.to_string(),
        (u64::MAX - 70).to_string()
    );
    for (account, off, n) in [
        (0, 341, u64::MAX),
        (0, 397, u64::MAX),
        (0, 341, 9990),
        (3, 12, 1000000),
        (3, 108, 1000001),
        (3, 20, 1000001),
        (3, 28, 1000001),
        (3, 116, 1000001),
        (3, 20, 960001),
    ] {
        let mut f = Fixture::new();
        put_u64(f.data(account), off, n);
        assert!(f.decode().is_err(), "{account} {off} {n}");
    }
    let mut f = Fixture::new();
    put_u64(f.data(3), 20, 960000);
    f.decode().unwrap();
    let mut f = Fixture::new();
    put_u64(f.data(3), 116, 1000000);
    f.decode().unwrap();
}

#[derive(Clone)]
struct ServerState {
    accounts: BTreeMap<String, RpcAccount>,
    requests: Vec<serde_json::Value>,
    block: serde_json::Value,
    change_hash: bool,
    truncate_batch: bool,
}
async fn serve_rpc(
    axum::extract::State(state): axum::extract::State<
        std::sync::Arc<std::sync::Mutex<ServerState>>,
    >,
    axum::Json(request): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let mut s = state.lock().unwrap();
    s.requests.push(request.clone());
    let result =
        match request["method"].as_str().unwrap() {
            "getGenesisHash" => json!(key(20)),
            "getMultipleAccounts" => {
                let mut values =
                    request["params"][0]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|id| {
                            s.accounts.get(id.as_str().unwrap()).map(|a| json!({
                    "owner": a.owner, "data": [STANDARD.encode(&a.data), "base64"],
                    "executable": a.executable, "lamports": a.lamports
                })).unwrap_or(serde_json::Value::Null)
                        })
                        .collect::<Vec<_>>();
                if s.truncate_batch {
                    values.pop();
                }
                json!({"context":{"slot":100},"value":values})
            }
            "getBlock" => {
                let mut b = s.block.clone();
                if s.change_hash
                    && s.requests
                        .iter()
                        .filter(|r| r["method"] == "getBlock")
                        .count()
                        > 1
                {
                    b["blockhash"] = json!(key(23));
                }
                b
            }
            _ => panic!("unexpected request {request}"),
        };
    axum::Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
async fn server(
    f: &Fixture,
) -> (
    String,
    std::sync::Arc<std::sync::Mutex<ServerState>>,
    tokio::task::JoinHandle<()>,
) {
    let state = std::sync::Arc::new(std::sync::Mutex::new(ServerState {
        accounts: f.accounts.clone(),
        requests: vec![],
        change_hash: false,
        truncate_batch: false,
        block: json!({"blockhash":key(21),"previousBlockhash":key(22),"parentSlot":99,"blockTime":999}),
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = axum::Router::new()
        .route("/", axum::routing::post(serve_rpc))
        .with_state(state.clone());
    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (url, state, handle)
}
fn rpc() -> xone_solana_ingest::rpc::Rpc {
    xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    }
}

#[tokio::test]
async fn current_rpc_batches_all_proofs_and_rechecks_same_finalized_block() {
    let mut f = Fixture::new();
    let (url, state, handle) = server(&f).await;
    f.chain.rpc_urls = vec![url];
    let snapshot = rpc().current_baseline(&f.chain).await.unwrap();
    handle.abort();
    snapshot.validate("test-parser", 1).unwrap();
    assert_eq!(snapshot.reference.block.number_or_slot, 100);
    assert_eq!(snapshot.reference.block.timestamp_ms, 999000);
    assert_eq!(
        snapshot.pools[0]
            .raydium_cpmm
            .as_ref()
            .unwrap()
            .bank_unix_timestamp,
        1000
    );
    assert_eq!(
        snapshot.reference.adapter_set_hash,
        xone_domain::hash(&vec!["raydium-cpmm/f4b9b263-spl-1"]).unwrap()
    );
    let s = state.lock().unwrap();
    let batches = s
        .requests
        .iter()
        .filter(|r| r["method"] == "getMultipleAccounts")
        .collect::<Vec<_>>();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["params"][0].as_array().unwrap().len(), 10);
    assert_eq!(batches[0]["params"][1]["commitment"], "finalized");
    let blocks = s
        .requests
        .iter()
        .filter(|r| r["method"] == "getBlock")
        .collect::<Vec<_>>();
    assert_eq!(blocks.len(), 2);
    for block in blocks {
        assert_eq!(block["params"][0], 100);
        assert_eq!(block["params"][1]["commitment"], "finalized");
    }
}

#[tokio::test]
async fn current_rpc_rejects_changed_anchor_and_invalid_parent_slot() {
    for change in 0..4 {
        let mut f = Fixture::new();
        let (url, state, handle) = server(&f).await;
        f.chain.rpc_urls = vec![url];
        {
            let mut s = state.lock().unwrap();
            match change {
                0 => s.change_hash = true,
                1 => s.block["parentSlot"] = json!(100),
                2 => {
                    s.block.as_object_mut().unwrap().remove("parentSlot");
                }
                _ => s.truncate_batch = true,
            }
        }
        assert!(
            rpc().current_baseline(&f.chain).await.is_err(),
            "change {change}"
        );
        handle.abort();
    }
}

#[tokio::test]
async fn mixed_archived_and_creator_pools_share_one_atomic_account_batch() {
    let mut f = Fixture::new();
    let ids = (11..=16).map(key).collect::<Vec<_>>();
    let mut archived = serde_json::to_value(&f.pool).unwrap();
    archived["id"] = json!(ids[0]);
    archived["protocol"] = json!("raydium-cpmm");
    archived["accounts"] = json!(ids);
    archived["token0"] = json!(ids[4]);
    archived["token1"] = json!(ids[5]);
    archived["raydium_cpmm"] = serde_json::Value::Null;
    let archived: PoolConfig = serde_json::from_value(archived).unwrap();
    for (i, id) in ids.iter().enumerate() {
        let mut a = f.accounts[&f.pool.accounts[i]].clone();
        if i == 0 {
            a.data[389..].fill(0);
            put_u64(&mut a.data, 373, 999);
            for (offset, address) in [
                (8, &ids[3]),
                (72, &ids[1]),
                (104, &ids[2]),
                (168, &ids[4]),
                (200, &ids[5]),
            ] {
                put_key(&mut a.data, offset, address);
            }
        } else if i == 3 {
            a.data[108..].fill(0);
        } else if i == 1 || i == 2 {
            put_key(&mut a.data, 0, &ids[3 + i]);
        }
        f.accounts.insert(id.clone(), a);
    }
    f.chain.pools.push(archived);
    let (url, state, handle) = server(&f).await;
    f.chain.rpc_urls = vec![url];
    let snapshot = rpc().current_baseline(&f.chain).await.unwrap();
    handle.abort();
    snapshot.validate("test-parser", 1).unwrap();
    assert_eq!(snapshot.pools.len(), 2);
    assert_eq!(snapshot.pools[1].adapter_version, "raydium-cpmm/archive-v1");
    assert!(snapshot.pools[1].raydium_cpmm.is_none());
    let s = state.lock().unwrap();
    let batches = s
        .requests
        .iter()
        .filter(|r| r["method"] == "getMultipleAccounts")
        .collect::<Vec<_>>();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["params"][0].as_array().unwrap().len(), 16);
    assert_eq!(
        snapshot.reference.adapter_set_hash,
        xone_domain::hash(&vec![
            "raydium-cpmm/archive-v1",
            "raydium-cpmm/f4b9b263-spl-1"
        ])
        .unwrap()
    );
}

fn native_fixture() -> Fixture {
    let mut f = Fixture::new();
    let native = "So11111111111111111111111111111111111111112";
    let vault_id = "CEruA9x8zkNyvrgQFBnfbqyFgTBbgZADaHzYwfDMMtA7";
    let mut vault = f.accounts.remove(&f.pool.accounts[2]).unwrap();
    let mint = f.accounts.remove(&f.pool.accounts[5]).unwrap();
    put_key(&mut vault.data, 0, native);
    vault.data[109..113].copy_from_slice(&1u32.to_le_bytes());
    put_u64(&mut vault.data, 113, 2039280);
    vault.lamports = 2039280 + 20000;
    put_key(f.data(0), 104, vault_id);
    put_key(f.data(0), 200, native);
    f.pool.accounts[2] = vault_id.into();
    f.pool.accounts[5] = native.into();
    f.pool.token1 = native.into();
    f.accounts.insert(vault_id.into(), vault);
    f.accounts.insert(native.into(), mint);
    f.chain.pools[0] = f.pool.clone();
    f
}

#[test]
fn native_wsol_requires_rent_and_amount_backing_but_allows_unsynced_donations() {
    let f = native_fixture();
    assert_eq!(f.decode().unwrap().reserve1.to_string(), "19910");
    let mut f = native_fixture();
    f.accounts.get_mut(&f.pool.accounts[2]).unwrap().lamports += 500;
    f.decode().unwrap();
    for lamports in [0, 2039279, 2039280, 2039280 + 19999] {
        let mut f = native_fixture();
        f.accounts.get_mut(&f.pool.accounts[2]).unwrap().lamports = lamports;
        assert!(f.decode().is_err(), "lamports {lamports}");
    }
    let mut f = native_fixture();
    f.data(2)[109] = 0;
    assert!(f.decode().is_err());
    let mut f = native_fixture();
    put_u64(f.data(2), 113, u64::MAX);
    assert!(f.decode().is_err());
}

#[test]
fn on_chain_fee_updates_are_authoritative_over_static_config_values() {
    let mut f = Fixture::new();
    f.pool.fee_numerator = 1234;
    put_u64(f.data(3), 12, 3200);
    let pool = f.decode().unwrap();
    assert_eq!(pool.fee_numerator, 3200);
}

#[test]
fn rejects_empty_net_reserves() {
    for (vault, fees) in [(1, 70), (2, 90)] {
        let mut f = Fixture::new();
        put_u64(f.data(vault), 64, fees);
        assert!(f.decode().is_err());
    }
}

#[tokio::test]
async fn shard_plan_counts_program_proofs_and_rejects_missing_dependencies() {
    let mut f = Fixture::new();
    // Sixteen pools require 96 data accounts + four shared proofs = 100.
    // Seventeen require106 and now enter guarded shards; missing business bytes must still fail.
    let pool = f.pool.clone();
    f.chain.pools.clear();
    for i in 0..17u8 {
        let mut p = pool.clone();
        p.accounts = (0..6).map(|n| key(32 + i * 6 + n)).collect();
        p.id = p.accounts[0].clone();
        p.token0 = p.accounts[4].clone();
        p.token1 = p.accounts[5].clone();
        f.chain.pools.push(p);
    }
    let (url, state, handle) = server(&f).await;
    f.chain.rpc_urls = vec![url];
    let error = rpc().current_baseline(&f.chain).await.unwrap_err();
    handle.abort();
    assert_eq!(error.0, "ACCOUNT_MISSING");
    let state = state.lock().unwrap();
    let batches = state
        .requests
        .iter()
        .filter(|r| r["method"] == "getMultipleAccounts")
        .collect::<Vec<_>>();
    assert_eq!(batches.len(), 2);
    for q in batches {
        let ids = q["params"][0].as_array().unwrap();
        assert!(ids.len() <= 100);
        assert!(ids.contains(&json!(CLOCK)));
    }
}

#[tokio::test]
async fn rejects_noncanonical_or_self_parent_block_hashes() {
    for (field, value) in [
        ("blockhash", json!("bad")),
        ("previousBlockhash", json!("bad")),
        ("previousBlockhash", json!(key(21))),
    ] {
        let mut f = Fixture::new();
        let (url, state, handle) = server(&f).await;
        f.chain.rpc_urls = vec![url];
        state.lock().unwrap().block[field] = value;
        assert!(rpc().current_baseline(&f.chain).await.is_err(), "{field}");
        handle.abort();
    }
}

#[tokio::test]
async fn one_hundred_account_boundary_is_read_in_one_batch() {
    let mut f = Fixture::new();
    let original = f.pool.clone();
    f.chain.pools.clear();
    for i in 0..16u8 {
        let mut p = original.clone();
        p.accounts = (0..6).map(|n| key(32 + i * 6 + n)).collect();
        p.id = p.accounts[0].clone();
        p.token0 = p.accounts[4].clone();
        p.token1 = p.accounts[5].clone();
        f.chain.pools.push(p);
    }
    let (url, state, handle) = server(&f).await;
    f.chain.rpc_urls = vec![url];
    let error = rpc().current_baseline(&f.chain).await.unwrap_err();
    handle.abort();
    // The synthetic dependencies are absent; importantly, admission reaches the atomic RPC batch.
    assert_eq!(error.0, "ACCOUNT_MISSING");
    let s = state.lock().unwrap();
    let batches = s
        .requests
        .iter()
        .filter(|r| r["method"] == "getMultipleAccounts")
        .collect::<Vec<_>>();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["params"][0].as_array().unwrap().len(), 100);
}

#[test]
fn decodes_individually_bounded_fees_without_combined_trade_creator_cap() {
    for (trade_fee, creator_fee) in [(900000, 200000), (999500, 500), (2500, 1000000)] {
        for mode in 0..=2 {
            for enabled in [false, true] {
                let mut f = Fixture::new();
                put_u64(f.data(3), 12, trade_fee);
                put_u64(f.data(3), 108, creator_fee);
                f.data(0)[389] = mode;
                f.data(0)[390] = u8::from(enabled);
                let pool = f.decode().unwrap_or_else(|e| {
                    panic!("trade={trade_fee}, creator={creator_fee}, mode={mode}, enabled={enabled}: {e:?}")
                });
                pool.validate().unwrap();
                let state = pool.raydium_cpmm.as_ref().unwrap();
                assert_eq!(pool.fee_numerator, trade_fee);
                assert_eq!(state.creator_fee_rate, creator_fee);
                assert_eq!(state.creator_fee_on, u32::from(mode));
                assert_eq!(state.creator_fee_enabled, enabled);
                assert_eq!(pool.reserve0.to_string(), "9930");
                assert_eq!(pool.reserve1.to_string(), "19910");
            }
        }
    }
}
