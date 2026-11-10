//! Actual normal Parser, RocksDB and tonic gRPC, driven by an independent local
//! bank fixture. No supplier, token, live validator or mainnet access is required.
#![allow(dead_code)]
#[path = "../tests/support/normal_bank.rs"]
mod normal_bank;
use crate::{
    bank_manifest::*,
    rpc::Account,
    service::Parser,
    stream::*,
    yellowstone::{RawCodec, SubscribeRequest},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use prost::Message;
use serde_json::json;
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

async fn read_race(change_phase: bool) {
    let mut h = Harness::new().await;
    let parser = Parser::new(h.cfg.clone()).unwrap();
    parser.start().await.unwrap();
    let c = h.connection().await;
    let m = h.manifest(&c, 100);
    h.save_manifest(&m);
    h.send_block(&c, 100, true).await;
    wait(&parser, |s| s.phase == "LIVE").await;
    let count = std::sync::Arc::new(AtomicU64::new(0));
    let calls = count.clone();
    let statuses = parser.stream_statuses.clone();
    // Instrument only the test clock at the last authority read, representing a
    // concurrent finalized-incomplete ingress transition while authority I/O settles.
    parser.stores["solana:localnet"].lock().unwrap().scope_now = std::sync::Arc::new(move || {
        if calls.fetch_add(1, Ordering::SeqCst) + 1 == 7 {
            let mut statuses = statuses.lock().unwrap();
            let status = statuses.get_mut("solana:localnet").unwrap();
            if change_phase {
                status.phase = "ASSEMBLING_UNCERTAIN".into();
            } else {
                status.session_id = "replacement-session".into();
            }
        }
        xone_observability::now_ms()
    });
    let result = parser.checked_current("solana:localnet").await;
    println!(
        "REPRO checked_current success={} phase={} authority-clock-calls={}",
        result.is_ok(),
        parser.stream_status("solana:localnet").unwrap().phase,
        count.load(Ordering::SeqCst)
    );
    if change_phase {
        assert_eq!(
            parser.stream_status("solana:localnet").unwrap().phase,
            "ASSEMBLING_UNCERTAIN"
        );
        assert!(parser.current("solana:localnet").is_err());
    } else {
        assert_eq!(
            parser.stream_status("solana:localnet").unwrap().session_id,
            "replacement-session"
        );
    }
    assert!(
        result.is_err(),
        "final authority I/O must not return an invalidated scope"
    );
    parser.close().await;
    drop(parser);
    drop(c);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn last_authority_io_rechecks_ingress_visibility() {
    read_race(true).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn last_authority_io_rechecks_receipt_session() {
    read_race(false).await;
}
