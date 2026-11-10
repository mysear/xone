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
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_public_grpc_publishes_only_complete_bank_then_revokes_and_restarts_head_first() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let (url, http) = api(&parser).await;
    let c = h.connection().await;
    assert_eq!(c.request.from_slot, None);
    assert!(c.request.blocks["full-block"].account_include.is_empty());
    let m = h.manifest(&c, 100);
    h.save_manifest(&m);
    h.send_block(&c, 100, false).await;
    wait(&parser, |s| s.captured_cursor >= 1).await;
    assert!(
        !baseline(&url).await.status().is_success(),
        "block counts do not prove finalized bank"
    );
    c.send.send(Ok(finalized(100))).await.unwrap();
    wait(&parser, |s| s.phase == "LIVE").await;
    let first: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(first["reference"]["block"]["number_or_slot"], 100);
    assert_eq!(first["pools"].as_array().unwrap().len(), 1);
    assert!(
        first["history_gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["from"] == 1
                && g["to"] == 99
                && g["reason"] == "HEAD_FIRST_CURRENT_BANK_BASELINE"),
        "cold HEAD_FIRST must not imply complete pre-baseline history: {first}"
    );
    let evidence: xone_solana_ingest::service::StreamEvidence =
        serde_json::from_str(first["source"].as_str().unwrap()).unwrap();
    assert_eq!(evidence.wal_cursor, 2);
    assert_eq!(evidence.session_id, c.session);
    assert_eq!(evidence.catalog_identity, h.catalog.identity().unwrap());
    let unit = xone_kafka_transport::Unit {
        kind: "ONLINE_BASELINE_COMMIT".into(),
        snapshot: serde_json::from_value(first.clone()).unwrap(),
        normalized_facts: None,
    };
    let wire = xone_kafka_transport::encode_unit(&unit).unwrap();
    assert_eq!(
        xone_kafka_transport::decode_unit(&wire)
            .unwrap()
            .snapshot
            .source,
        unit.snapshot.source
    );
    assert_eq!(
        parser
            .stream_status("solana:localnet")
            .unwrap()
            .committed_cursor,
        2
    );
    let old_session = c.session.clone();
    drop(c);
    let c2 = h.connection().await;
    assert_ne!(c2.session, old_session);
    assert_eq!(c2.request.from_slot, Some(101));
    let journal: Value = reqwest::Client::new()
        .get(format!(
            "{url}/internal/chains/solana:localnet/journal?after=0"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        journal
            .as_array()
            .unwrap()
            .iter()
            .any(|record| record["kind"] == "ONLINE_SCOPE_REVOKED"),
        "EOF must revoke through the actual normal owner journal: {journal}"
    );
    assert!(
        !baseline(&url).await.status().is_success(),
        "EOF revokes normal owner state"
    );
    // The short replay request cannot cover the actual new head: reconnect must
    // explicitly discard it and acquire HEAD_FIRST rather than replay unbounded.
    c2.send.send(Ok(finalized(120))).await.unwrap();
    let c3 = h.connection().await;
    assert_eq!(c3.request.from_slot, None);
    let m = h.manifest(&c3, 120);
    h.save_manifest(&m);
    h.send_block(&c3, 120, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let second: Value = baseline(&url).await.json().await.unwrap();
    assert!(second["history_gaps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|g| g["reason"] == "HEAD_FIRST_SOURCE_SESSION_CONTINUITY_UNPROVEN"));
    let generation = second["reference"]["online_generation"].as_u64().unwrap();
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c3);
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c4 = h.connection().await;
    assert_eq!(c4.request.from_slot, None);
    assert_ne!(c4.session, old_session);
    let (url, http) = api(&parser).await;
    assert!(!baseline(&url).await.status().is_success());
    let m = h.manifest(&c4, 130);
    h.save_manifest(&m);
    h.send_block(&c4, 130, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let third: Value = baseline(&url).await.json().await.unwrap();
    assert!(third["reference"]["online_generation"].as_u64().unwrap() > generation);
    assert!(third["history_gaps"].as_array().unwrap().len() >= 2);
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c4);
    let wal = FrameWal::open(
        &h.directory.0.join("state/yellowstone/solana-localnet"),
        "solana:localnet",
        FrameWalLimits {
            max_frame_bytes: 1048576,
            max_total_bytes: 8388608,
            max_records: 1000,
        },
    )
    .unwrap();
    assert_eq!(wal.recovery_cursor().sequence, 7);
    assert_eq!(wal.read(1).unwrap().frame.source.session_id, old_session);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_raw_ack_survives_unsupported_frame_and_missing_or_cross_session_bank_proof() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let (url, http) = api(&parser).await;
    let c = h.connection().await;
    c.send.send(Ok(vec![255, 0, 128])).await.unwrap();
    let c2 = h.connection().await;
    assert_ne!(c.session, c2.session);
    assert!(!baseline(&url).await.status().is_success());
    h.send_block(&c2, 100, true).await;
    wait(&parser, |s| s.captured_cursor >= 3).await;
    let mut m = h.manifest(&c2, 101);
    m.session_id = c.session.clone();
    let path = h
        .directory
        .0
        .join("manifests")
        .join(format!("{}-101-201.json", c2.session));
    write_json(&path, &m);
    h.send_block(&c2, 101, true).await;
    let c3 = h.connection().await;
    assert_ne!(c2.session, c3.session);
    assert!(!baseline(&url).await.status().is_success());
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
    drop(c2);
    drop(c3);
    let wal = FrameWal::open(
        &h.directory.0.join("state/yellowstone/solana-localnet"),
        "solana:localnet",
        FrameWalLimits {
            max_frame_bytes: 1048576,
            max_total_bytes: 8388608,
            max_records: 1000,
        },
    )
    .unwrap();
    assert_eq!(wal.read(1).unwrap().frame.protobuf, vec![255, 0, 128]);
    assert_eq!(wal.recovery_cursor(), WalCursor::start());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_admitted_catalog_expands_normal_scope_without_startup_yaml_pool() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let (url, http) = api(&parser).await;
    let c = h.connection().await;
    let m = h.manifest(&c, 100);
    h.save_manifest(&m);
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let before: Value = baseline(&url).await.json().await.unwrap();
    let generation = before["reference"]["online_generation"].as_u64().unwrap();
    let mut pool = h.catalog.pools[0].clone();
    let id = bs58::encode([42u8; 32]).into_string();
    let old = pool.id.clone();
    pool.id = id.clone();
    pool.accounts[0] = id.clone();
    let account = &h.accounts[&old];
    h.accounts.insert(
        id.clone(),
        Account {
            owner: account.owner.clone(),
            data: account.data.clone(),
        },
    );
    h.catalog.revision = 2;
    h.catalog.pools.push(pool);
    write_json(&h.directory.0.join("catalog.json"), &h.catalog);
    assert!(
        !baseline(&url).await.status().is_success(),
        "catalog mutation immediately fences old scope"
    );
    let response = reqwest::Client::new()
        .post(format!(
            "{url}/internal/chains/solana:localnet/ensure-pool-scope"
        ))
        .json(&json!({"pool_ids":[id],"expected_online_generation":generation}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    let body: Value = response.json().await.unwrap();
    assert_eq!(
        body["error"], "POOL_SCOPE_STREAM_RECOVERY_PENDING",
        "new current admission must not be rejected against startup YAML: {body}"
    );
    c.send.send(Ok(bytes(6, &[]))).await.unwrap();
    let c2 = h.connection().await;
    let m = h.manifest(&c2, 101);
    h.save_manifest(&m);
    h.send_block(&c2, 101, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let after: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(after["pools"].as_array().unwrap().len(), 2);
    let response=reqwest::Client::new().post(format!("{url}/internal/chains/solana:localnet/ensure-pool-scope")).json(&json!({"pool_ids":[id],"expected_online_generation":after["reference"]["online_generation"]})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<Value>().await.unwrap()["strict_historical_bank_proof"],
        true
    );
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
    drop(c2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_manifest_version_dependency_digest_and_freshness_are_required() {
    for mutation in ["version", "dependency", "digest", "freshness"] {
        let mut h = Harness::new().await;
        let parser = Parser::new(h.cfg.clone()).unwrap();
        parser.start().await.unwrap();
        let (url, http) = api(&parser).await;
        let c = h.connection().await;
        let mut m = h.manifest(&c, 100);
        match mutation {
            "version" => m.version = 2,
            "dependency" => {
                m.final_versions.pop();
            }
            "digest" => m.final_versions[0].account_sha256 = "00".repeat(32),
            "freshness" => m.timestamp_ms = 1,
            _ => unreachable!(),
        }
        h.save_manifest(&m);
        h.send_block(&c, 100, true).await;
        let c2 = h.connection().await;
        assert!(!baseline(&url).await.status().is_success(), "{mutation}");
        parser.close().await;
        http.abort();
        drop(parser);
        drop(c);
        drop(c2);
    }
}

#[test]
fn concrete_manifest_source_recovers_through_durable_scope_controller_without_online_ready_claim() {
    use std::sync::Arc;
    use xone_solana_ingest::{manifest_recovery::ManifestRecoverySource, scope::*};
    let directory = Temp::new();
    let (chain, rows) = normal_bank::archive();
    let now = xone_observability::now_ms();
    let accounts = rows
        .iter()
        .map(|(id, a)| {
            let key = key(id).unwrap();
            (
                key,
                Arc::new(AccountUpdate {
                    bank: BankId {
                        slot: 100,
                        bank_id: 200,
                    },
                    pubkey: key,
                    owner: key_of(&a.owner),
                    lamports: 1,
                    executable: false,
                    rent_epoch: 0,
                    write_version: 2,
                    is_startup: false,
                    txn_signature: None,
                    data: a.data.clone(),
                }),
            )
        })
        .collect();
    let bank = CommittedBank {
        bank: BankId {
            slot: 100,
            bank_id: 200,
        },
        blockhash: blockhash(100),
        parent: BankAnchor {
            bank: BankId {
                slot: 99,
                bank_id: 199,
            },
            blockhash: blockhash(99),
        },
        accounts,
        account_versions: BTreeMap::new(),
        transactions: BTreeMap::new(),
    };
    let wal = FrameWal::open(
        &directory.0.join("wal"),
        &chain.key,
        FrameWalLimits {
            max_frame_bytes: 1024,
            max_total_bytes: 10000,
            max_records: 10,
        },
    )
    .unwrap();
    drop(wal);
    let mut wal = FrameWal::open(
        &directory.0.join("wal"),
        &chain.key,
        FrameWalLimits {
            max_frame_bytes: 1024,
            max_total_bytes: 10000,
            max_records: 10,
        },
    )
    .unwrap();
    let cursor = wal
        .capture(RawFrame {
            source: SourceSession {
                source_id: "public-geyser".into(),
                session_id: "test-source-session".into(),
            },
            sequence: 1,
            receive_timestamp_ms: now,
            protobuf: bytes(6, &[]),
        })
        .unwrap();
    let receipt = wal.read(cursor.sequence).unwrap();
    let mut source =
        ManifestRecoverySource::from_committed(&chain, &bank, now, &receipt, 100000, 2).unwrap();
    source.verify_complete(&chain).unwrap();
    let fence = Fence {
        chain_key: chain.key.clone(),
        chain_identity: chain.genesis_hash.clone(),
        owner: chain.owner_service.clone(),
        assignment: 1,
        generation: 1,
        canonical_epoch: 1,
        writer_epoch: 1,
        adapter_version: "fixture-pinned-adapter".into(),
        schema_version: "1".into(),
    };
    let budget = ScopeLimits {
        max_jobs: 2,
        max_account_bytes: 100000,
        max_tail_events: 16,
        max_tail_bytes: 100000,
        max_tail_age_ms: 5000,
        max_baseline_age_ms: 5000,
        shard_accounts: 2,
    };
    let dependencies = bank
        .accounts
        .keys()
        .map(|id| ScopeDependency {
            id: bs58::encode(id).into_string(),
            max_bytes: 10000,
        })
        .collect::<Vec<_>>();
    let store = Arc::new(
        FileScopeStore::open(
            &directory.0.join("scope"),
            fence.clone(),
            FileScopeLimits {
                max_jobs: 2,
                max_account_bytes: 100000,
                max_retained_jobs: 4,
                max_snapshot_bytes: 1_000_000,
                attempt_lease_ms: 1000,
                max_authority_lease_ms: 5000,
            },
            Arc::new(xone_observability::now_ms),
        )
        .unwrap(),
    );
    let cancel = Cancellation::new();
    store.authorize(&cancel, fence.clone(), now + 4000).unwrap();
    let controller = ScopeController::new(
        ScopeConfig {
            fence: fence.clone(),
            limits: budget.clone(),
            catalog: BTreeMap::from([(chain.pools[0].id.clone(), dependencies)]),
            trusted_sources: vec!["public-geyser".into()],
        },
        store.clone(),
        xone_observability::now_ms,
    )
    .unwrap();
    let job = controller
        .ensure_pool_scope(
            &cancel,
            EnsurePoolScopeRequest {
                fence,
                anchor: ScopeAnchor {
                    height: 100,
                    hash: blockhash(100),
                    timestamp_ms: now,
                    cursor: receipt.cursor.sequence,
                },
                pool_ids: vec![chain.pools[0].id.clone()],
                budget,
                history_after: 0,
            },
        )
        .unwrap();
    controller.recover(&cancel, &job.id, &mut source).unwrap();
    assert_eq!(store.job(&job.id).unwrap().phase, ScopePhase::ScopedReady);
    assert_eq!(store.recovery_data(&job.id).unwrap().accounts.len(), 6);
    assert_eq!(store.readiness_scope(), "RECOVERY_DATA_ONLY");
    store.close().unwrap();
    let mut invalid = receipt.clone();
    invalid.cursor = WalCursor::start();
    assert!(
        ManifestRecoverySource::from_committed(&chain, &bank, now, &invalid, 100000, 2).is_err(),
        "a zero cursor is not a durable recovery input"
    );
}
fn key_of(value: &str) -> Pubkey {
    key(value).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_geyser_ping_is_durable_before_keepalive_request_and_grants_no_readiness() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let mut c = h.connection().await;
    c.send.send(Ok(bytes(6, &[]))).await.unwrap();
    let response = tokio::time::timeout(Duration::from_millis(300), c.client_requests.recv()).await;
    assert!(
        response.is_ok(),
        "normal public Geyser subscription must answer server ping"
    );
    assert_eq!(response.unwrap().unwrap(), vec![0x4a, 0x02, 0x08, 0x01]);
    let status = parser.stream_status("solana:localnet").unwrap();
    assert!(status.captured_cursor >= 1);
    assert_ne!(status.phase, "LIVE");
    parser.close().await;
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_higher_finalized_incomplete_bank_revokes_previous_scope_until_completion() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let (url, http) = api(&parser).await;
    let c = h.connection().await;
    let m = h.manifest(&c, 100);
    h.save_manifest(&m);
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let m = h.manifest(&c, 101);
    h.save_manifest(&m);
    c.send.send(Ok(finalized(101))).await.unwrap();
    wait(&parser, |s| {
        s.captured_cursor >= 3 && s.phase == "ASSEMBLING_UNCERTAIN"
    })
    .await;
    assert!(
        !baseline(&url).await.status().is_success(),
        "higher finalized bank without dependencies must suppress previous ready scope"
    );
    h.send_block(&c, 101, false).await;
    wait(&parser, |s| s.phase == "LIVE" && s.committed_cursor == 4).await;
    let snapshot: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(snapshot["reference"]["block"]["number_or_slot"], 101);
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_finalized_owner_does_not_request_unfinalized_bank_manifests() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    assert_eq!(
        c.request.slots["scope"].filter_by_commitment,
        Some(true),
        "normal finalized owner must not demand a completed future-bank manifest on CreatedBank"
    );
    assert_eq!(c.request.slots["scope"].interslot_updates, Some(false));
    parser.close().await;
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn normal_owner_rotates_only_committed_segments_and_never_restores_archived_session_ready() {
    let mut h = Harness::new().await;
    let cfg = h.cfg.config.yellowstone.get_mut("solana:localnet").unwrap();
    cfg.max_wal_records = 6;
    cfg.max_replay_records = 4;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    for slot in 100..=102 {
        let m = h.manifest(&c, slot);
        h.save_manifest(&m);
        h.send_block(&c, slot, true).await;
        wait(&parser, |s| s.committed_cursor >= (slot - 99) * 2).await;
    }
    let next = h.connection().await;
    assert_ne!(next.session, c.session);
    let archive = h
        .directory
        .0
        .join("state/yellowstone/solana-localnet.archives");
    assert!(
        archive.is_dir(),
        "a committed full segment must be archived before ingestion can continue"
    );
    let (url, http) = api(&parser).await;
    assert!(
        !baseline(&url).await.status().is_success(),
        "archived receipt session is not live authority"
    );
    let m = h.manifest(&next, 103);
    h.save_manifest(&m);
    h.send_block(&next, 103, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
    drop(next);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_public_bank_observer_exports_completion_into_real_grpc_owner_scope() {
    use xone_bank_source::{FrozenAccount, NativeIdentity, Observer, SourceConfig};
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    let mut source = Observer::new(SourceConfig {
        chain_key: "solana:localnet".into(),
        genesis_hash: "localnet".into(),
        source_id: "public-geyser".into(),
        catalog_file: h.directory.0.join("catalog.json"),
        manifest_directory: h.directory.0.join("manifests"),
        max_bytes: 8 * 1024 * 1024,
        max_updates: 1024,
        max_banks: 16,
        max_sessions: 4,
        refs_endpoint: None,
        refs_token_file: None,
    })
    .unwrap();
    source.genesis("localnet").unwrap();
    // A real native notification path is required; account bytes come from the
    // separate immutable Bank read below, never from the received Geyser block.
    let rows = h.manifest(&c, 100).parent_accounts;
    for (index, row) in rows.iter().enumerate() {
        let mut r = row.clone();
        r.write_slot = 99;
        r.write_bank_id = 199;
        r.write_version = if index == 0 { 0 } else { 1 };
        source
            .observe(serde_json::from_value(serde_json::to_value(r).unwrap()).unwrap())
            .unwrap();
    }
    let native = |id: &str| {
        let a = &h.accounts[id];
        Ok(FrozenAccount {
            pubkey: id.into(),
            owner: a.owner.clone(),
            lamports: 1,
            executable: false,
            rent_epoch: 0,
            data_base64: STANDARD.encode(&a.data),
        })
    };
    let now = xone_observability::now_ms();
    source
        .frozen(
            NativeIdentity {
                slot: 99,
                bank_id: 199,
                blockhash: blockhash(99),
                state_hash: blockhash(90),
                parent_bank_id: None,
                timestamp_ms: now,
            },
            native,
        )
        .unwrap();
    for row in rows {
        let mut r = row;
        r.write_slot = 100;
        r.write_bank_id = 200;
        r.write_version = 2;
        source
            .observe(serde_json::from_value(serde_json::to_value(r).unwrap()).unwrap())
            .unwrap();
    }
    let mut repeated = h.manifest(&c, 100).parent_accounts.remove(0);
    repeated.write_slot = 100;
    repeated.write_bank_id = 200;
    repeated.write_version = 3;
    h.wire_versions.insert(repeated.pubkey.clone(), 3);
    source
        .observe(serde_json::from_value(serde_json::to_value(repeated).unwrap()).unwrap())
        .unwrap();
    let manifests = source
        .frozen(
            NativeIdentity {
                slot: 100,
                bank_id: 200,
                blockhash: blockhash(100),
                state_hash: blockhash(91),
                parent_bank_id: Some(199),
                timestamp_ms: now,
            },
            native,
        )
        .unwrap();
    assert_eq!(
        manifests.len(),
        1,
        "normal subscription must register a bounded public native-source session request"
    );
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    assert!(baseline(&url).await.status().is_success());
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subset_first_and_requested_scope_publish_through_same_owner_without_file_ready() {
    let mut h = Harness::new().await;
    let mut pool = h.catalog.pools[0].clone();
    let old = pool.id.clone();
    let id = bs58::encode([42u8; 32]).into_string();
    pool.id = id.clone();
    pool.accounts[0] = id.clone();
    let a = &h.accounts[&old];
    h.accounts.insert(
        id.clone(),
        Account {
            owner: a.owner.clone(),
            data: a.data.clone(),
        },
    );
    h.catalog.pools.push(pool);
    write_json(&h.directory.0.join("catalog.json"), &h.catalog);
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    let m = h.manifest(&c, 100);
    h.save_manifest(&m);
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    let before: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(
        before["pools"].as_array().unwrap().len(),
        1,
        "startup subset must not wait to decode/publish every catalog pool"
    );
    let reply=reqwest::Client::new().post(format!("{url}/internal/chains/solana:localnet/ensure-pool-scope")).json(&json!({"pool_ids":[id],"expected_online_generation":before["reference"]["online_generation"]})).send().await.unwrap();
    assert_eq!(reply.status(), 200);
    let after: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(after["pools"].as_array().unwrap().len(), 2);
    assert!(
        after["reference"]["online_generation"].as_u64().unwrap()
            > before["reference"]["online_generation"].as_u64().unwrap()
    );
    assert_eq!(after["reference"]["block"], before["reference"]["block"]);
    let bootstrap = reqwest::Client::new()
        .get(format!("{url}/internal/chains/solana:localnet/bootstrap"))
        .send()
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), 200);
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_waits_for_native_manifest_on_same_session_without_promoting_old_raw_frames() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| {
        s.captured_cursor >= 2 && s.session_id == c.session
    })
    .await;
    let m = h.manifest(&c, 101);
    h.save_manifest(&m);
    h.send_block(&c, 101, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    let snapshot: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(snapshot["reference"]["block"]["number_or_slot"], 101);
    assert_eq!(
        parser.stream_status("solana:localnet").unwrap().session_id,
        c.session
    );
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_cannot_publish_a_known_conflicting_finalized_parent() {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    let m = h.manifest(&c, 100);
    h.save_manifest(&m);
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    drop(c);
    let c2 = h.connection().await;
    let mut m = h.manifest(&c2, 101);
    m.parent.blockhash = blockhash(55);
    h.wire_parent_hash = Some(m.parent.blockhash.clone());
    h.save_manifest(&m);
    h.send_block(&c2, 101, true).await;
    wait(&parser, |s| {
        s.last_error == "STREAM_FINALIZED_PARENT_CONFLICT"
    })
    .await;
    let (url, http) = api(&parser).await;
    assert!(!baseline(&url).await.status().is_success());
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c2);
}

/// The isolated PG/Kafka runner enables this explicitly. Missing infrastructure
/// fails this test; ordinary unit runs do not claim external authority coverage.
#[cfg(feature = "external-authority-tests")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_postgres_kafka_scope_publish_revoke_restart_and_fence() {
    let mut h = Harness::new().await;
    let dsn_path = std::env::var("XONE_AUTHORITY_TEST_DSN_FILE").expect("isolated PG DSN required");
    let broker = std::env::var("XONE_AUTHORITY_TEST_BROKER").expect("isolated Kafka required");
    h.cfg.control_dsn = std::fs::read_to_string(&dsn_path).unwrap();
    h.cfg.config.control.dsn_file = dsn_path;
    h.cfg.config.journal.transport = "kafka".into();
    h.cfg.config.journal.kafka.brokers = vec![broker.clone()];
    h.cfg.config.journal.kafka.topics = BTreeMap::from([(
        "solana:localnet".into(),
        "canonical.solana-localnet.v1".into(),
    )]);
    h.cfg.config.journal.kafka.transaction_timeout_ms = 4000;
    h.cfg.config.control.backend = "postgres".into();
    h.cfg.config.control.instance_id = "normal-yellowstone-authority".into();
    h.cfg.config.control.lease_ttl_ms = 30000;
    h.cfg.config.control.renewal_ms = 2000;
    h.cfg.config.control.min_commit_remaining_ms = 7000;
    h.cfg.config.startup.max_baseline_age_ms = 30000;
    h.cfg
        .config
        .yellowstone
        .get_mut("solana:localnet")
        .unwrap()
        .idle_timeout_ms = 30000;

    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let (url, http) = api(&parser).await;
    let c = h.connection().await;
    h.save_manifest(&h.manifest(&c, 100));
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let first: xone_domain::Snapshot = baseline(&url).await.json().await.unwrap();
    let first_ref = first.reference.clone();
    assert!(first_ref.writer_epoch > 0);
    assert!(first_ref.online_generation > 0);
    let brokers = vec![broker.clone()];
    let (unit, cursor) = tokio::task::spawn_blocking(move || {
        xone_kafka_transport::tail(
            &brokers,
            "canonical.solana-localnet.v1",
            16 << 20,
            1024,
            Duration::from_secs(6),
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(unit.unwrap().snapshot.reference, first_ref);
    assert!(cursor.offset > 0);

    drop(c);
    let c2 = h.connection().await;
    assert!(!baseline(&url).await.status().is_success());
    let brokers = vec![broker.clone()];
    let (unit, _) = tokio::task::spawn_blocking(move || {
        xone_kafka_transport::tail(
            &brokers,
            "canonical.solana-localnet.v1",
            16 << 20,
            1024,
            Duration::from_secs(6),
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(unit.unwrap().kind, "ONLINE_SCOPE_REVOKED");
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c2);

    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c3 = h.connection().await;
    assert_eq!(c3.request.from_slot, None);
    let (url, http) = api(&parser).await;
    assert!(!baseline(&url).await.status().is_success());
    h.save_manifest(&h.manifest(&c3, 130));
    h.send_block(&c3, 130, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let second: xone_domain::Snapshot = baseline(&url).await.json().await.unwrap();
    assert!(second.reference.writer_epoch > first_ref.writer_epoch);
    assert!(second.reference.online_generation > first_ref.online_generation);
    assert_eq!(second.reference.block.number_or_slot, 130);

    let pg = xone_control::Postgres::open(&h.cfg.control_dsn)
        .await
        .unwrap();
    let lease = xone_control::Lease {
        chain: "solana:localnet".into(),
        holder: h.cfg.config.control.instance_id.clone(),
        writer_epoch: second.reference.writer_epoch,
        generation: second.reference.online_generation,
        canonical_epoch: second.reference.canonical_epoch,
        journal_seq: second.reference.journal_seq,
        expires_ms: 0,
    };
    pg.release(&lease).await.unwrap();
    parser.tick().await;
    assert!(
        !baseline(&url).await.status().is_success(),
        "expired PG authority must withdraw current scope"
    );
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c3);

    // Slot gaps model 0.4-second Solana slots. They do not claim that this
    // process actually waited a month; stale-time cases are separate tests.
    let mut previous = second.reference;
    let mut current_slot = 130;
    for (hours, empty_owner) in [(1u64, false), (24, false), (720, false), (720, true)] {
        if empty_owner {
            let owner = h.directory.0.join("state/solana-localnet");
            assert!(
                owner.is_dir(),
                "empty-owner experiment requires an existing owner DB"
            );
            std::fs::remove_dir_all(owner).unwrap();
        }
        current_slot += hours * 9000;
        let started = std::time::Instant::now();
        let parser = Parser::new(h.cfg.clone()).unwrap();
        parser.start().await.unwrap();
        let connection = h.connection().await;
        assert_eq!(
            connection.request.from_slot, None,
            "long-gap restart must use HEAD_FIRST"
        );
        let (url, http) = api(&parser).await;
        assert!(!baseline(&url).await.status().is_success());
        h.save_manifest(&h.manifest(&connection, current_slot));
        h.send_block(&connection, current_slot, true).await;
        wait(&parser, |status| status.phase == "LIVE").await;
        let restored: xone_domain::Snapshot = baseline(&url).await.json().await.unwrap();
        assert_eq!(restored.reference.block.number_or_slot, current_slot);
        assert!(restored.reference.writer_epoch > previous.writer_epoch);
        assert!(restored.reference.online_generation > previous.online_generation);
        assert_eq!(restored.pools.len(), 1);
        println!(
            "{}",
            json!({"recovery_case": {
                "equivalent_offline_hours": hours, "empty_owner_kv": empty_owner,
                "current_slot": current_slot, "first_qualified_scope_ms": started.elapsed().as_millis(),
                "online_generation": restored.reference.online_generation,
                "source_frames_sent": 2, "resume_from_slot": connection.request.from_slot,
                "wallclock_downtime_simulated": false
            }})
        );
        previous = restored.reference;
        parser.close().await;
        http.abort();
        drop(parser);
        drop(connection);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_audit_jobs_do_not_block_two_budgeted_wal_rotations() {
    let mut h = Harness::new().await;
    let cfg = h.cfg.config.yellowstone.get_mut("solana:localnet").unwrap();
    cfg.max_wal_records = 6;
    cfg.max_replay_records = 4;
    cfg.archive_retention =
        Some(serde_json::from_value(json!({"max_segments":1,"max_bytes":8388608})).unwrap());
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    for slot in 100..=102 {
        let m = h.manifest(&c, slot);
        h.save_manifest(&m);
        h.send_block(&c, slot, true).await;
        wait(&parser, |s| s.committed_cursor >= (slot - 99) * 2).await;
    }
    let next = h.connection().await;
    for slot in 103..=105 {
        let m = h.manifest(&next, slot);
        h.save_manifest(&m);
        h.send_block(&next, slot, true).await;
        wait(&parser, |s| s.committed_cursor >= (slot - 102) * 2).await;
    }
    let third = h.connection().await;
    assert_ne!(third.session, next.session);
    assert_ne!(
        parser.stream_status("solana:localnet").unwrap().last_error,
        "WAL_SEGMENT_REFERENCES_PROTECTED"
    );
    println!(
        "Budgeted second rotation status: {:?}",
        parser.stream_status("solana:localnet").unwrap()
    );
    assert_eq!(
        std::fs::read_dir(
            h.directory
                .0
                .join("state/yellowstone/solana-localnet.archives")
        )
        .unwrap()
        .count(),
        1
    );
    let (url, http) = api(&parser).await;
    let m = h.manifest(&third, 106);
    h.save_manifest(&m);
    h.send_block(&third, 106, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    assert!(baseline(&url).await.status().is_success());
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
    drop(next);
    drop(third);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unrequested_missing_dependency_does_not_block_boot_subset() {
    let mut h = Harness::new().await;
    let mut pool = h.catalog.pools[0].clone();
    let old = pool.id.clone();
    let id = bs58::encode([42u8; 32]).into_string();
    pool.id = id.clone();
    pool.accounts[0] = id.clone();
    let a = &h.accounts[&old];
    h.accounts.insert(
        id.clone(),
        Account {
            owner: a.owner.clone(),
            data: a.data.clone(),
        },
    );
    h.catalog.pools.push(pool);
    write_json(&h.directory.0.join("catalog.json"), &h.catalog);
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    let mut m = h.manifest(&c, 100);
    m.parent_accounts.retain(|a| a.pubkey != id);
    m.final_versions.retain(|a| a.pubkey != id);
    m.expected_account_updates -= 1;
    h.save_manifest(&m);
    h.accounts.remove(&id);
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| {
        s.phase == "LIVE" || s.last_error == "BANK_SCOPE_INCOMPLETE"
    })
    .await;
    assert_eq!(
        parser.stream_status("solana:localnet").unwrap().phase,
        "LIVE",
        "unrequested dependency must not gate target"
    );
    let (url, http) = api(&parser).await;
    let reply = baseline(&url).await;
    assert!(reply.status().is_success());
    let body: Value = reply.json().await.unwrap();
    assert_eq!(body["pools"].as_array().unwrap().len(), 1);
    http.abort();
    println!(
        "Requested subset status: {:?}",
        parser.stream_status("solana:localnet").unwrap()
    );
    parser.close().await;
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_target_proof_projects_native_count_and_reconnects_for_scope_expansion() {
    let mut h = Harness::new().await;
    let mut pool = h.catalog.pools[0].clone();
    let old = pool.id.clone();
    let id = bs58::encode([42u8; 32]).into_string();
    pool.id = id.clone();
    pool.accounts[0] = id.clone();
    let a = &h.accounts[&old];
    h.accounts.insert(
        id.clone(),
        Account {
            owner: a.owner.clone(),
            data: a.data.clone(),
        },
    );
    h.catalog.pools.push(pool);
    write_json(&h.directory.0.join("catalog.json"), &h.catalog);
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    let request: Value = serde_json::from_slice(
        &std::fs::read(
            h.directory
                .0
                .join("manifests")
                .join(format!("{}.request", c.session)),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        !request["scope_accounts"]
            .as_array()
            .unwrap()
            .contains(&json!(id)),
        "native request must use target scope"
    );
    let mut m = h.manifest(&c, 100);
    m.version = 2;
    m.native_writes = m
        .parent_accounts
        .iter()
        .map(|a| {
            let mut row = a.clone();
            row.write_slot = 100;
            row.write_bank_id = 200;
            row.write_version = 2;
            row
        })
        .collect();
    let mut repeated = m
        .native_writes
        .iter()
        .find(|a| a.pubkey == old)
        .unwrap()
        .clone();
    repeated.write_version = 3;
    let account = repeated.account().unwrap();
    let f = m
        .final_versions
        .iter_mut()
        .find(|f| f.pubkey == old)
        .unwrap();
    f.write_version = 3;
    f.account_sha256 = account_fingerprint(&account)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    m.native_writes.push(repeated);
    m.expected_account_updates = m.native_writes.len() as u64;
    // The optional pool lacks both its parent and final account proofs. Its
    // native write must not alter the exact count for the requested scope.
    m.parent_accounts.retain(|a| a.pubkey != id);
    m.final_versions.retain(|a| a.pubkey != id);
    h.wire_versions.insert(old.clone(), 3);
    h.save_manifest(&m);
    let extra = h.accounts.remove(&id).unwrap();
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let (url, http) = api(&parser).await;
    let before: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(before["pools"].as_array().unwrap().len(), 1);
    let reply=reqwest::Client::new().post(format!("{url}/internal/chains/solana:localnet/ensure-pool-scope")).json(&json!({"pool_ids":[id],"expected_online_generation":before["reference"]["online_generation"]})).send().await.unwrap();
    assert_eq!(
        reply.status(),
        503,
        "a smaller certified bank cannot invent the new pool"
    );
    c.send.send(Ok(bytes(6, &[]))).await.unwrap();
    let next = h.connection().await;
    assert_ne!(c.session, next.session);
    let request: Value = serde_json::from_slice(
        &std::fs::read(
            h.directory
                .0
                .join("manifests")
                .join(format!("{}.request", next.session)),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(request["scope_accounts"]
        .as_array()
        .unwrap()
        .contains(&json!(id)));
    h.accounts.insert(id, extra);
    h.wire_versions.clear();
    let m = h.manifest(&next, 102);
    h.save_manifest(&m);
    h.send_block(&next, 102, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let after: Value = baseline(&url).await.json().await.unwrap();
    assert_eq!(after["pools"].as_array().unwrap().len(), 2);
    assert_eq!(after["reference"]["block"]["number_or_slot"], 102);
    parser.close().await;
    http.abort();
    drop(parser);
    drop(c);
    drop(next);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zero_native_version_is_present_metadata_and_never_bypasses_stale_parent_fence() {
    let h = Harness::new().await;
    let (send, _) = tokio::sync::mpsc::channel(1);
    let (_, client_requests) = tokio::sync::mpsc::channel(1);
    let connection = Connection {
        session: "zero-native-version".into(),
        request: SubscribeRequest::default(),
        send,
        client_requests,
    };
    let mut manifest = h.manifest(&connection, 100);
    manifest.version = 2;
    manifest.native_writes = manifest
        .parent_accounts
        .iter()
        .map(|row| {
            let mut row = row.clone();
            row.write_slot = 100;
            row.write_bank_id = 200;
            row.write_version = 0;
            row
        })
        .collect();
    manifest.final_versions = manifest
        .native_writes
        .iter()
        .map(|row| FinalVersion {
            pubkey: row.pubkey.clone(),
            write_version: 0,
            account_sha256: account_fingerprint(&row.account().unwrap())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        })
        .collect();
    h.save_manifest(&manifest);
    let path = h
        .directory
        .0
        .join("manifests/zero-native-version-100-200.json");
    let source = SourceSession {
        source_id: "public-geyser".into(),
        session_id: connection.session,
    };
    let bank = BankId {
        slot: 100,
        bank_id: 200,
    };
    let config = &h.cfg.config.yellowstone["solana:localnet"];
    let chain = &h.cfg.config.chains[0];
    let load = || BankManifest::load(&path, config, chain, &h.catalog, &source, &bank);
    let verified = load().expect("explicit observed version zero is not missing metadata");
    assert!(verified
        .native_writes
        .iter()
        .all(|account| account.write_version == 0));
    let mut assembler = StreamAssembler::new(StreamLimits {
        max_bytes: 8388608,
        max_slots: 16,
        max_updates: 1024,
    })
    .unwrap();
    assembler
        .anchor_versioned(verified.parent, verified.parent_proof, 1, verified.scope, 1)
        .unwrap();
    let error = assembler
        .seal_native_versions(&source, verified.native_writes)
        .unwrap_err();
    assert_eq!(error.0, "STREAM_STALE_ACCOUNT_VERSION");
    assert!(assembler.needs_reanchor());
    assert!(!assembler.ready());
    // Missing scalar identity and duplicate zero tuples remain invalid proofs.
    let mut missing = serde_json::to_value(&manifest).unwrap();
    missing["native_writes"][0]
        .as_object_mut()
        .unwrap()
        .remove("write_version");
    write_json(&path, &missing);
    assert!(load().is_err());
    manifest
        .native_writes
        .push(manifest.native_writes[0].clone());
    manifest.expected_account_updates += 1;
    write_json(&path, &manifest);
    assert!(load().is_err());
}
