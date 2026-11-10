//! Actual normal Parser, RocksDB and tonic gRPC, driven by an independent local
//! bank fixture. No supplier, token, live validator or mainnet access is required.
#[path = "support/normal_bank.rs"]
mod normal_bank;
use base64::{engine::general_purpose::STANDARD, Engine};
use prost::Message;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    convert::Infallible,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
    time::Duration,
};
use tonic::codegen::{http, Service};
use xone_config::ConfigSnapshot;
use xone_solana_ingest::{
    bank_manifest::*,
    rpc::Account,
    service::Parser,
    stream::*,
    yellowstone::{RawCodec, SubscribeRequest},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "xone-normal-grpc-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Connection {
    session: String,
    request: SubscribeRequest,
    send: tokio::sync::mpsc::Sender<Result<Vec<u8>, tonic::Status>>,
    client_requests: tokio::sync::mpsc::Receiver<Vec<u8>>,
}
#[derive(Clone)]
struct Geyser(tokio::sync::mpsc::Sender<Connection>);
struct Subscribe(Geyser);
impl tonic::server::StreamingService<Vec<u8>> for Subscribe {
    type Response = Vec<u8>;
    type ResponseStream = tokio_stream::wrappers::ReceiverStream<Result<Vec<u8>, tonic::Status>>;
    type Future = Pin<
        Box<
            dyn Future<Output = Result<tonic::Response<Self::ResponseStream>, tonic::Status>>
                + Send,
        >,
    >;
    fn call(&mut self, mut req: tonic::Request<tonic::Streaming<Vec<u8>>>) -> Self::Future {
        let service = self.0.clone();
        Box::pin(async move {
            assert!(
                req.metadata().get("x-token").is_none(),
                "public source must work without token"
            );
            let session = req
                .metadata()
                .get("x-xone-session-id")
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            let request =
                SubscribeRequest::decode(req.get_mut().message().await?.unwrap().as_slice())
                    .unwrap();
            let (send, recv) = tokio::sync::mpsc::channel(8);
            let (requests, client_requests) = tokio::sync::mpsc::channel(8);
            let mut inbound = req.into_inner();
            tokio::spawn(async move {
                while let Ok(Some(message)) = inbound.message().await {
                    if requests.send(message).await.is_err() {
                        break;
                    }
                }
            });
            service
                .0
                .send(Connection {
                    session,
                    request,
                    send,
                    client_requests,
                })
                .await
                .map_err(|_| tonic::Status::unavailable("test closed"))?;
            Ok(tonic::Response::new(
                tokio_stream::wrappers::ReceiverStream::new(recv),
            ))
        })
    }
}
impl Service<http::Request<tonic::body::Body>> for Geyser {
    type Response = http::Response<tonic::body::Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, req: http::Request<tonic::body::Body>) -> Self::Future {
        let s = self.clone();
        Box::pin(async move {
            Ok(tonic::server::Grpc::new(RawCodec)
                .streaming(Subscribe(s), req)
                .await)
        })
    }
}
impl tonic::server::NamedService for Geyser {
    const NAME: &'static str = "geyser.Geyser";
}
struct Harness {
    directory: Temp,
    cfg: ConfigSnapshot,
    catalog: AdmittedCatalog,
    accounts: BTreeMap<String, Account>,
    wire_versions: BTreeMap<String, u64>,
    wire_parent_hash: Option<String>,
    connections: tokio::sync::mpsc::Receiver<Connection>,
    server: tokio::task::JoinHandle<()>,
}
impl Harness {
    async fn new() -> Self {
        let directory = Temp::new();
        let (chain, accounts) = normal_bank::archive();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (send, connections) = tokio::sync::mpsc::channel(8);
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(Geyser(send))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
        let mut cfg =
            xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
        cfg.config.state.path = directory.0.join("state").to_string_lossy().into();
        cfg.config.chains = vec![chain.clone()];
        cfg.config.startup.max_baseline_age_ms = 5000;
        let stream=serde_json::from_value(json!({"endpoint":format!("http://{address}"),"source_id":"public-geyser",
            "proto_commit":PROJECTION_SCHEMA_COMMIT,"allow_loopback_plaintext":true,"connect_timeout_ms":500,"idle_timeout_ms":1000,"wal_ack_timeout_ms":1000,"reconnect_delay_ms":20,
            "max_frame_bytes":1048576,"max_wal_bytes":8388608,"max_wal_records":1000,"max_replay_records":10,"max_replay_slots":4,"max_stream_bytes":8388608,"max_stream_slots":16,"max_stream_updates":1024,
            "max_manifest_bytes":1048576,"max_catalog_pools":16,"manifest_directory":directory.0.join("manifests"),"catalog_file":directory.0.join("catalog.json")})).unwrap();
        cfg.config.yellowstone.insert(chain.key.clone(), stream);
        let catalog = AdmittedCatalog {
            version: 1,
            revision: 1,
            chain_key: chain.key,
            genesis_hash: chain.genesis_hash,
            pools: chain.pools,
        };
        std::fs::create_dir_all(directory.0.join("manifests")).unwrap();
        write_json(&directory.0.join("catalog.json"), &catalog);
        Self {
            directory,
            cfg,
            catalog,
            accounts,
            wire_versions: BTreeMap::new(),
            wire_parent_hash: None,
            connections,
            server,
        }
    }
    async fn connection(&mut self) -> Connection {
        tokio::time::timeout(Duration::from_secs(3), self.connections.recv())
            .await
            .unwrap()
            .unwrap()
    }
    fn manifest(&self, connection: &Connection, slot: u64) -> BankManifest {
        let parent = BankId {
            slot: slot - 1,
            bank_id: slot + 99,
        };
        let parent_accounts = self
            .accounts
            .iter()
            .map(|(id, a)| ManifestAccount {
                pubkey: id.clone(),
                owner: a.owner.clone(),
                lamports: 1,
                executable: false,
                rent_epoch: 0,
                write_slot: parent.slot,
                write_bank_id: parent.bank_id,
                write_version: slot - 99,
                data_base64: STANDARD.encode(&a.data),
                txn_signature_base64: None,
            })
            .collect::<Vec<_>>();
        let final_versions = parent_accounts
            .iter()
            .map(|a| {
                let mut update = a.account().unwrap();
                update.bank = BankId {
                    slot,
                    bank_id: slot + 100,
                };
                update.write_version = slot - 98;
                FinalVersion {
                    pubkey: a.pubkey.clone(),
                    write_version: slot - 98,
                    account_sha256: account_fingerprint(&update)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect(),
                }
            })
            .collect();
        BankManifest {
            version: 1,
            chain_key: self.catalog.chain_key.clone(),
            genesis_hash: self.catalog.genesis_hash.clone(),
            source_id: "public-geyser".into(),
            session_id: connection.session.clone(),
            catalog_identity: self.catalog.identity().unwrap(),
            parent: ManifestAnchor {
                slot: slot - 1,
                bank_id: slot + 99,
                blockhash: blockhash(slot - 1),
            },
            bank: ManifestAnchor {
                slot,
                bank_id: slot + 100,
                blockhash: blockhash(slot),
            },
            timestamp_ms: xone_observability::now_ms(),
            parent_accounts,
            expected_account_updates: self.accounts.len() as u64,
            final_versions,
            skipped_slots: vec![],
            native_writes: vec![],
        }
    }
    fn save_manifest(&self, m: &BankManifest) {
        write_json(
            &self.directory.0.join("manifests").join(format!(
                "{}-{}-{}.json",
                m.session_id, m.bank.slot, m.bank.bank_id
            )),
            m,
        );
    }
    async fn send_block(&self, c: &Connection, slot: u64, finalize: bool) {
        let mut block = [
            scalar(1, slot),
            bytes(2, blockhash(slot).as_bytes()),
            scalar(7, slot - 1),
            bytes(
                8,
                self.wire_parent_hash
                    .clone()
                    .unwrap_or_else(|| blockhash(slot - 1))
                    .as_bytes(),
            ),
            scalar(9, 0),
            scalar(10, self.accounts.len() as u64),
            scalar(12, 0),
            scalar(14, slot + 100),
        ]
        .concat();
        for (id, a) in &self.accounts {
            block.extend(bytes(
                11,
                &[
                    bytes(1, &bs58::decode(id).into_vec().unwrap()),
                    scalar(2, 1),
                    bytes(3, &bs58::decode(&a.owner).into_vec().unwrap()),
                    scalar(4, 0),
                    scalar(5, 0),
                    bytes(6, &a.data),
                    scalar(7, self.wire_versions.get(id).copied().unwrap_or(slot - 98)),
                ]
                .concat(),
            ));
        }
        c.send.send(Ok(bytes(5, &block))).await.unwrap();
        if finalize {
            c.send.send(Ok(finalized(slot))).await.unwrap();
        }
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.server.abort();
    }
}
fn write_json(path: &Path, value: &impl serde::Serialize) {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, serde_json::to_vec(value).unwrap()).unwrap();
    std::fs::rename(temp, path).unwrap();
}
fn blockhash(slot: u64) -> String {
    bs58::encode([slot as u8; 32]).into_string()
}
fn varint(mut n: u64) -> Vec<u8> {
    let mut out = vec![];
    while n >= 128 {
        out.push(n as u8 | 128);
        n >>= 7;
    }
    out.push(n as u8);
    out
}
fn scalar(tag: u64, n: u64) -> Vec<u8> {
    [varint(tag << 3), varint(n)].concat()
}
fn bytes(tag: u64, value: &[u8]) -> Vec<u8> {
    [
        varint(tag << 3 | 2),
        varint(value.len() as u64),
        value.to_vec(),
    ]
    .concat()
}
fn finalized(slot: u64) -> Vec<u8> {
    bytes(
        3,
        &[
            scalar(1, slot),
            scalar(2, slot - 1),
            scalar(3, 2),
            scalar(5, slot + 100),
        ]
        .concat(),
    )
}
async fn wait(parser: &Parser, check: impl Fn(&xone_solana_ingest::service::StreamStatus) -> bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let s = parser.stream_status("solana:localnet").unwrap();
            if check(&s) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "last stream status: {:?}",
            parser.stream_status("solana:localnet")
        )
    });
}
async fn api(parser: &Parser) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handler = parser.handler();
    (
        format!("http://{address}"),
        tokio::spawn(async move {
            axum::serve(listener, handler).await.unwrap();
        }),
    )
}
async fn baseline(url: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("{url}/internal/chains/solana:localnet/baseline"))
        .send()
        .await
        .unwrap()
}

struct PublicHistoryRpc {
    genesis: String,
    mode: std::sync::atomic::AtomicUsize,
    head: std::sync::atomic::AtomicU64,
    calls: std::sync::Mutex<Vec<Value>>,
    pending: tokio::sync::watch::Sender<bool>,
}
async fn public_history_rpc(
    axum::extract::State(s): axum::extract::State<std::sync::Arc<PublicHistoryRpc>>,
    axum::Json(body): axum::Json<Value>,
) -> axum::Json<Value> {
    s.calls.lock().unwrap().push(body.clone());
    let mode = s.mode.load(Ordering::SeqCst);
    let method = body["method"].as_str().unwrap();
    if method == "getBlocks" && mode == 4 {
        s.pending.send_replace(true);
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    let result = match method {
        "getGenesisHash" => json!(if mode == 1 {
            blockhash(77)
        } else {
            s.genesis.clone()
        }),
        "getSlot" => json!(s.head.load(Ordering::SeqCst)),
        "getBlocks" => {
            let from = body["params"][0].as_u64().unwrap();
            let to = body["params"][1].as_u64().unwrap();
            if mode == 2 {
                json!([to + 1])
            } else if mode == 7 {
                json!([])
            } else if mode == 8 {
                json!([98, 99, 100])
            } else {
                json!((from.max(98)..=to).collect::<Vec<_>>())
            }
        }
        "getBlocksWithLimit" => {
            let slot = if mode == 8 {
                150
            } else {
                body["params"][0].as_u64().unwrap().max(98)
            };
            if slot <= s.head.load(Ordering::SeqCst) {
                json!([slot])
            } else {
                json!([])
            }
        }
        "getBlock" => {
            let slot = body["params"][0].as_u64().unwrap();
            if mode == 3 && slot == 98 {
                Value::Null
            } else {
                let parent = if slot == 98 {
                    0
                } else if mode == 8 && slot == 150 {
                    100
                } else {
                    slot - 1
                };
                json!({"blockhash":blockhash(slot),"previousBlockhash":blockhash(parent),"parentSlot":parent,"blockTime":xone_observability::now_ms()/1000+if mode==6{60}else{0},"transactions":if slot==98{json!([{"transaction":{"signatures":[bs58::encode([9u8;64]).into_string()],"message":{"accountKeys":[],"instructions":[],"recentBlockhash":blockhash(0)}},"meta":{"fee":5000,"err":null}}])}else{json!([])}})
            }
        }
        _ => panic!("history requested an account-state method: {method}"),
    };
    axum::Json(json!({"jsonrpc":"2.0","id":1,"result":result}))
}
async fn setup(
    mode: usize,
) -> (
    Harness,
    std::sync::Arc<PublicHistoryRpc>,
    tokio::task::JoinHandle<()>,
) {
    let mut h = Harness::new().await;
    let (pending, _) = tokio::sync::watch::channel(false);
    let state = std::sync::Arc::new(PublicHistoryRpc {
        genesis: h.cfg.config.chains[0].genesis_hash.clone(),
        mode: std::sync::atomic::AtomicUsize::new(mode),
        head: std::sync::atomic::AtomicU64::new(100),
        calls: std::sync::Mutex::new(Vec::new()),
        pending,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    h.cfg.config.chains[0].rpc_urls = vec![format!("http://{}", listener.local_addr().unwrap())];
    h.cfg.config.history.enabled = true;
    h.cfg.config.history.max_worker_share = 0.15;
    let app = axum::Router::new()
        .route("/", axum::routing::post(public_history_rpc))
        .with_state(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (h, state, server)
}
async fn history_status(url: &str) -> Value {
    reqwest::Client::new()
        .get(format!(
            "{url}/internal/chains/solana:localnet/history-status"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}
async fn wait_history(url: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = history_status(url).await;
            if predicate(&status) {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    match result {
        Ok(status) => status,
        Err(_) => panic!("last history status: {}", history_status(url).await),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_tonic_history_http_commits_facts_without_changing_online_reference() {
    let (mut h, source, server) = setup(0).await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    assert_eq!(
        c.request.from_slot, None,
        "history must not turn the live subscription into old-slot replay"
    );
    h.save_manifest(&h.manifest(&c, 100));
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    let before: Value = baseline(&url).await.json().await.unwrap();
    let state = wait_history(&url, |s| s["queue"]["records"] == 1).await;
    assert_eq!(state["queue"]["historical_accounts_complete"], false);
    assert_eq!(state["queue"]["jobs"][0]["phase"], "Complete");
    assert!(
        c.client_requests.is_empty(),
        "history must not modify the Yellowstone subscription"
    );
    let after: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(before["reference"], after["reference"]);
    {
        let calls = source.calls.lock().unwrap();
        assert!(calls.iter().any(|r| r["method"] == "getBlocks"));
        assert!(calls.iter().all(|r| [
            "getGenesisHash",
            "getSlot",
            "getBlocks",
            "getBlocksWithLimit",
            "getBlock"
        ]
        .contains(&r["method"].as_str().unwrap())));
    }
    parser.close().await;
    http.abort();
    let _ = http.await;
    drop(parser);
    drop(c);
    {
        use xone_solana_ingest::{history_queue::*, store::Store};
        let chain = &h.cfg.config.chains[0];
        let db = std::sync::Arc::new(std::sync::Mutex::new(
            Store::open(
                &h.directory.0.join("state/solana-localnet"),
                &chain.owner_service,
                chain.assignment_version,
            )
            .unwrap(),
        ));
        let q = HistoryQueue::open(
            db.clone(),
            HistoryRegistration::new(
                &chain.key,
                &chain.genesis_hash,
                &chain.rpc_urls[0],
                true,
                HistoryLimits::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let pages = q.pages(1).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(!pages[0].historical_accounts_complete);
        assert_eq!(pages[0].skipped[0].from, 1);
        assert_eq!(pages[0].skipped[0].to, 97);
        assert_eq!(pages[0].produced[0].transactions[0]["meta"]["fee"], 5000);
        assert_eq!(
            pages[0]
                .raw
                .iter()
                .filter(|r| r.method == "getBlocks")
                .count(),
            1
        );
    }
    source.head.store(101, Ordering::SeqCst);
    let second = Parser::new(h.cfg.clone()).unwrap();
    second.start().await.unwrap();
    let c = h.connection().await;
    h.save_manifest(&h.manifest(&c, 101));
    h.send_block(&c, 101, true).await;
    wait(&second, |s| s.phase == "LIVE").await;
    let (url, http) = api(&second).await;
    let status = wait_history(&url, |s| s["queue"]["records"] == 2).await;
    assert_eq!(status["queue"]["covered_ranges"][0]["from"], 1);
    assert_eq!(status["queue"]["covered_ranges"][0]["to"], 100);
    assert_eq!(
        source
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["method"] == "getBlocks" && r["params"][0] == 1)
            .count(),
        1,
        "restart must not replay already committed history"
    );
    second.close().await;
    http.abort();
    server.abort();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_identity_future_and_unknown_history_never_advance_or_block_online() {
    for mode in [1, 2, 3, 6, 7] {
        let (mut h, _, server) = setup(mode).await;
        let parser = Parser::new(h.cfg.clone()).unwrap();
        parser.start().await.unwrap();
        let c = h.connection().await;
        h.save_manifest(&h.manifest(&c, 100));
        h.send_block(&c, 100, true).await;
        wait(&parser, |s| s.phase == "LIVE").await;
        let (url, http) = api(&parser).await;
        let state = wait_history(&url, |s| {
            s["queue"]["source_status"] == "UNKNOWN_OR_UNAVAILABLE"
        })
        .await;
        assert_eq!(state["queue"]["records"], 0);
        assert_eq!(state["queue"]["jobs"][0]["next"], 1);
        assert!(baseline(&url).await.status().is_success());
        parser.close().await;
        http.abort();
        server.abort();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paused_disabled_and_slow_history_do_not_hold_online_actor_or_close() {
    let (mut h, source, server) = setup(4).await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    h.save_manifest(&h.manifest(&c, 100));
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    let mut pending = source.pending.subscribe();
    tokio::time::timeout(Duration::from_secs(3), pending.wait_for(|p| *p))
        .await
        .unwrap()
        .unwrap();
    assert!(baseline(&url).await.status().is_success());
    tokio::time::timeout(Duration::from_millis(500), parser.tick())
        .await
        .unwrap();
    let pause = reqwest::Client::new()
        .post(format!(
            "{url}/internal/chains/solana:localnet/history-status"
        ))
        .json(&json!({"paused":true}))
        .send()
        .await
        .unwrap();
    assert!(pause.status().is_success());
    assert!(baseline(&url).await.status().is_success());
    tokio::time::timeout(Duration::from_secs(2), parser.close())
        .await
        .unwrap();
    http.abort();
    server.abort();
    let (mut h, source, server) = setup(0).await;
    h.cfg.config.history.enabled = false;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    h.save_manifest(&h.manifest(&c, 100));
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(source.calls.lock().unwrap().is_empty());
    parser.close().await;
    server.abort();
}

#[test]
fn all_chains_share_wallclock_quota_and_zero_share_is_disabled() {
    use xone_solana_ingest::history_runtime::HistoryScheduler;
    let start = std::time::Instant::now();
    let finish = start + Duration::from_millis(100);
    let mut scheduler = HistoryScheduler::new(0.25).unwrap();
    scheduler.finish(start, finish);
    assert!(scheduler.delay(finish) >= Duration::from_millis(566));
    assert!(!scheduler
        .delay(finish + Duration::from_millis(500))
        .is_zero());
    assert!(scheduler
        .delay(finish + Duration::from_millis(567))
        .is_zero());
    assert!(!HistoryScheduler::new(0.0).unwrap().delay(start).is_zero());
    assert!(HistoryScheduler::new(f64::NAN).is_err());
    assert!(HistoryScheduler::new(f64::INFINITY).is_err());
    let mut tiny = HistoryScheduler::new(1e-300).unwrap();
    tiny.finish(start, finish);
    assert_eq!(tiny.delay(finish), Duration::MAX);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_parent_stop_sender_releases_the_history_worker() {
    use xone_solana_ingest::{history_runtime::HistoryRuntime, rpc::Rpc, store::Store};
    let (mut h, _, server) = setup(0).await;
    h.cfg.config.history.enabled = false;
    let chain = &h.cfg.config.chains[0];
    let db = std::sync::Arc::new(std::sync::Mutex::new(
        Store::open(
            &h.directory.0.join("history-drop"),
            &chain.owner_service,
            chain.assignment_version,
        )
        .unwrap(),
    ));
    let stores = BTreeMap::from([(chain.key.clone(), db)]);
    let runtime = HistoryRuntime::new(
        &h.cfg,
        &stores,
        Rpc {
            http: reqwest::Client::new(),
            max_bytes: 4 << 20,
        },
    )
    .unwrap();
    let weak = std::sync::Arc::downgrade(&runtime);
    let (stop, receiver) = tokio::sync::watch::channel(false);
    runtime.start(receiver).await;
    drop(stop);
    drop(runtime);
    let finished = tokio::time::timeout(Duration::from_millis(500), async {
        while weak.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .is_ok();
    if let Some(runtime) = weak.upgrade() {
        runtime.close().await;
    }
    server.abort();
    assert!(
        finished,
        "a closed parent stop channel must terminate, not spin and retain the owner DB"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn billion_slot_gap_uses_bounded_produced_slot_pages_after_online_is_live() {
    let (mut h, source, server) = setup(0).await;
    let head = 1_000_000_100u64;
    source.head.store(head, Ordering::SeqCst);
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    assert_eq!(c.request.from_slot, None);
    h.save_manifest(&h.manifest(&c, head));
    h.send_block(&c, head, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    let status = wait_history(&url, |s| {
        s["queue"]["records"].as_u64().is_some_and(|n| n > 0)
    })
    .await;
    assert_eq!(status["queue"]["jobs"].as_array().unwrap().len(), 1);
    assert!(status["queue"]["jobs"][0]["next"].as_u64().unwrap() >= 129);
    assert!(baseline(&url).await.status().is_success());
    {
        let calls = source.calls.lock().unwrap();
        assert!(calls
            .iter()
            .filter(|r| r["method"] == "getBlocks")
            .all(|r| r["params"][1].as_u64().unwrap() - r["params"][0].as_u64().unwrap() < 128));
        assert!(calls
            .iter()
            .filter(|r| r["method"] == "getBlock" && r["params"][1]["transactionDetails"] == "full")
            .all(|r| r["params"][0].as_u64().unwrap() >= 98));
    }
    parser.close().await;
    http.abort();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sparse_page_tail_requires_a_real_public_successor_header() {
    let (mut h, source, server) = setup(8).await;
    let head = 1_000_000_100u64;
    source.head.store(head, Ordering::SeqCst);
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    h.save_manifest(&h.manifest(&c, head));
    h.send_block(&c, head, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    let status = wait_history(&url, |s| s["queue"]["records"] == 1).await;
    assert_eq!(status["queue"]["covered_ranges"][0]["to"], 128);
    assert!(source
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|r| r["method"] == "getBlocksWithLimit"
            && r["params"][0] == 129
            && r["params"][1] == 1));
    assert!(source
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|r| r["method"] == "getBlock"
            && r["params"][0] == 150
            && r["params"][1]["transactionDetails"] == "none"));
    assert!(baseline(&url).await.status().is_success());
    parser.close().await;
    http.abort();
    server.abort();
}
