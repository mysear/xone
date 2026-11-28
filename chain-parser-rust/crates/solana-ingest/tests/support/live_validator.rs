use super::{api, baseline, normal_bank, write_json};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command},
    time::Duration,
};
use tokio::{net::TcpStream, sync::mpsc, task::JoinHandle};
use xone_solana_ingest::{
    bank_manifest::{AdmittedCatalog, BankManifest},
    rpc::Rpc,
    service::{Parser, StreamEvidence},
    stream::PROJECTION_SCHEMA_COMMIT,
};

struct Validator(Child);

// Hold the existing socket open while the validator finalizes blocks, then
// close it so the Parser reconnects to the same Yellowstone retention window.
struct GrpcRelay {
    address: SocketAddr,
    cut: mpsc::Sender<Option<tokio::sync::oneshot::Sender<()>>>,
    task: JoinHandle<()>,
}

impl GrpcRelay {
    async fn start(upstream: SocketAddr) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (cut, mut commands) = mpsc::channel::<Option<tokio::sync::oneshot::Sender<()>>>(1);
        let task = tokio::spawn(async move {
            loop {
                let (mut client, _) = listener.accept().await.unwrap();
                let Ok(mut server) = TcpStream::connect(upstream).await else {
                    continue;
                };
                tokio::select! {
                    result = commands.recv() => {
                        let Some(Some(paused)) = result else { return; };
                        let _ = paused.send(());
                        if commands.recv().await.is_none() { return; }
                    }
                    _ = tokio::io::copy_bidirectional(&mut client, &mut server) => {}
                }
            }
        });
        Self { address, cut, task }
    }

    async fn pause(&self) {
        let (ack, paused) = tokio::sync::oneshot::channel();
        self.cut.send(Some(ack)).await.unwrap();
        paused.await.unwrap();
    }

    async fn disconnect(&self) {
        self.cut.send(None).await.unwrap();
    }
}

impl Drop for GrpcRelay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Drop for Validator {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_validator(
    binary: &Path,
    root: &Path,
    rpc_port: u16,
    faucet_port: u16,
    source: Option<&Path>,
) -> Validator {
    let name = if source.is_some() {
        "geyser"
    } else {
        "genesis"
    };
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(format!("{name}.log")))
        .unwrap();
    let mut command = Command::new(binary);
    command
        .arg("--ledger")
        .arg(root.join("ledger"))
        .arg("--account-dir")
        .arg(root.join("accounts"))
        .args(["--bind-address", "127.0.0.1", "--rpc-port"])
        .arg(rpc_port.to_string())
        .arg("--faucet-port")
        .arg(faucet_port.to_string())
        .args(["--ticks-per-slot", "64"])
        .env_remove("XONE_BANK_SOURCE_CONFIG")
        .stdout(log.try_clone().unwrap())
        .stderr(log);
    if let Some(source) = source {
        command
            .arg("--geyser-plugin-config")
            .arg(root.join("geyser.json"))
            .env("XONE_BANK_SOURCE_CONFIG", source);
    }
    Validator(command.spawn().unwrap())
}

async fn wait_genesis(rpc: &Rpc, url: &str, validator: &mut Validator) -> String {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            assert!(
                validator.0.try_wait().unwrap().is_none(),
                "validator exited before RPC startup"
            );
            if let Ok(value) = rpc.call(url, "getGenesisHash", json!([])).await {
                return value.as_str().unwrap().to_owned();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("validator RPC startup timed out")
}

fn snapshot_proof(snapshot: &Value, directory: &Path) -> (StreamEvidence, BankManifest) {
    let evidence: StreamEvidence =
        serde_json::from_str(snapshot["source"].as_str().unwrap()).unwrap();
    let slot = snapshot["reference"]["block"]["number_or_slot"]
        .as_u64()
        .unwrap();
    let path = directory.join(format!(
        "{}-{slot}-{}.json",
        evidence.session_id, evidence.bank_id
    ));
    let manifest: BankManifest = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        xone_domain::hash(&manifest).unwrap(),
        evidence.manifest_sha256
    );
    assert_eq!(
        manifest.bank.blockhash,
        snapshot["reference"]["block"]["hash"]
    );
    (evidence, manifest)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the pinned patched Agave validator and matching Yellowstone plugin"]
async fn native_validator_stream_publishes_consecutive_frozen_banks() {
    let output = PathBuf::from(std::env::var_os("XONE_LIVE_YELLOWSTONE_OUTPUT").unwrap());
    let binary = PathBuf::from(std::env::var_os("XONE_LIVE_YELLOWSTONE_VALIDATOR").unwrap());
    let plugin = PathBuf::from(std::env::var_os("XONE_LIVE_YELLOWSTONE_PLUGIN").unwrap());
    assert!(output.is_absolute() && binary.is_absolute() && plugin.is_absolute());
    fs::create_dir(&output).unwrap();
    let accounts_dir = output.join("accounts");
    let manifests = output.join("manifests");
    fs::create_dir(&accounts_dir).unwrap();
    fs::create_dir(&manifests).unwrap();
    let (mut chain, accounts) = normal_bank::archive();
    for (key, account) in &accounts {
        write_json(
            &accounts_dir.join(format!("{key}.json")),
            &json!({
                "pubkey": key,
                "account": {
                    "owner": account.owner, "lamports": 10_000_000, "executable": false,
                    "rentEpoch": 0, "data": [STANDARD.encode(&account.data), "base64"],
                },
            }),
        );
    }
    let rpc_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let grpc_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let faucet_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let rpc_port = rpc_listener.local_addr().unwrap().port();
    let grpc_address = grpc_listener.local_addr().unwrap();
    let faucet_port = faucet_listener.local_addr().unwrap().port();
    drop(rpc_listener);
    drop(grpc_listener);
    let rpc_url = format!("http://127.0.0.1:{rpc_port}");
    let rpc = Rpc {
        http: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap(),
        max_bytes: 4 * 1024 * 1024,
    };
    let mut boot = start_validator(&binary, &output, rpc_port, 0, None);
    chain.genesis_hash = wait_genesis(&rpc, &rpc_url, &mut boot).await;
    drop(boot);
    chain.rpc_urls = vec![rpc_url.clone()];

    let catalog_file = output.join("catalog.json");
    let catalog = AdmittedCatalog {
        version: 1,
        revision: 1,
        chain_key: chain.key.clone(),
        genesis_hash: chain.genesis_hash.clone(),
        pools: chain.pools.clone(),
    };
    write_json(&catalog_file, &catalog);
    let source_file = output.join("native-source.json");
    write_json(
        &source_file,
        &json!({
            "chain_key":chain.key, "genesis_hash":chain.genesis_hash, "source_id":"local-agave-geyser",
            "catalog_file":catalog_file, "manifest_directory":manifests, "max_bytes":33_554_432,
            "max_updates":65_536, "max_banks":256, "max_sessions":4,
        }),
    );
    write_json(
        &output.join("geyser.json"),
        &json!({
            "libpath":plugin, "tokio":{"worker_threads":2},
            "grpc":{"listen":[{"address":grpc_address.to_string()}], "replay_stored_slots":64},
        }),
    );
    drop(faucet_listener);
    let mut validator =
        start_validator(&binary, &output, rpc_port, faucet_port, Some(&source_file));
    assert_eq!(
        wait_genesis(&rpc, &rpc_url, &mut validator).await,
        chain.genesis_hash
    );

    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    cfg.config.state.path = output.join("state").to_string_lossy().into_owned();
    cfg.config.startup.max_baseline_age_ms = 60_000;
    cfg.config.control.lease_ttl_ms = 20_000;
    cfg.config.chains = vec![chain.clone()];
    let relay = GrpcRelay::start(grpc_address).await;
    cfg.config.yellowstone.insert(
        chain.key.clone(),
        serde_json::from_value(json!({
            "endpoint":format!("http://{}", relay.address), "source_id":"local-agave-geyser",
            "proto_commit":PROJECTION_SCHEMA_COMMIT, "allow_loopback_plaintext":true,
            "connect_timeout_ms":3000, "idle_timeout_ms":45000, "wal_ack_timeout_ms":3000,
            "reconnect_delay_ms":100, "max_frame_bytes":4194304, "max_wal_bytes":67108864,
            "max_wal_records":10000, "max_replay_records":30, "max_replay_slots":16,
            "max_stream_bytes":33554432, "max_stream_slots":128, "max_stream_updates":65536,
            "max_manifest_bytes":4194304, "max_catalog_pools":16,
            "manifest_directory":manifests, "catalog_file":catalog_file,
        }))
        .unwrap(),
    );
    cfg.config.logging.level = "info".into();
    let mut parser = Parser::new(cfg.clone()).unwrap();
    parser.logger = xone_observability::Logger::with_writer(
        &cfg,
        Box::new(File::create(output.join("parser.log")).unwrap()),
    );
    parser.start().await.unwrap();
    let (url, http) = api(&parser).await;

    let mut snapshots: Vec<Value> = Vec::new();
    let slots = tokio::time::timeout(Duration::from_secs(90), async {
        let mut slots: Vec<u64> = Vec::new();
        loop {
            let response = baseline(&url).await;
            if response.status().is_success() {
                let snapshot: Value = response.json().await.unwrap();
                let slot = snapshot["reference"]["block"]["number_or_slot"]
                    .as_u64()
                    .unwrap();
                if slots.last().is_none_or(|previous| slot > *previous) {
                    if slots.last().is_some_and(|previous| slot != previous + 1) {
                        slots.clear();
                        snapshots.clear();
                    }
                    slots.push(slot);
                    snapshots.push(snapshot);
                    if slots.len() == 3 {
                        return slots;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "live parser stream status: {:?}",
            parser.stream_status(&chain.key)
        )
    });

    let mut previous_cursor = 0;
    let mut session = None;
    let mut blocks = Vec::with_capacity(slots.len());
    for (slot, snapshot) in slots.iter().zip(&snapshots) {
        assert_eq!(snapshot["pools"].as_array().unwrap().len(), 1);
        let (evidence, manifest) = snapshot_proof(snapshot, &manifests);
        assert_eq!(evidence.source_id, "local-agave-geyser");
        assert_eq!(evidence.proto_commit, PROJECTION_SCHEMA_COMMIT);
        assert_eq!(
            session.get_or_insert_with(|| evidence.session_id.clone()),
            &evidence.session_id,
        );
        assert!(evidence.wal_cursor > previous_cursor);
        previous_cursor = evidence.wal_cursor;
        assert_eq!(manifest.genesis_hash, chain.genesis_hash);
        assert_eq!(manifest.parent_accounts.len(), accounts.len());
        assert_eq!(manifest.final_versions.len(), accounts.len());
        for account in &manifest.parent_accounts {
            let seeded = &accounts[&account.pubkey];
            assert_eq!(account.owner, seeded.owner);
            assert_eq!(STANDARD.decode(&account.data_base64).unwrap(), seeded.data);
        }
        let block = rpc
            .call(
                &rpc_url,
                "getBlock",
                json!([slot, {
                    "commitment":"finalized", "transactionDetails":"none", "rewards":false,
                }]),
            )
            .await
            .unwrap();
        assert_eq!(block["blockhash"], snapshot["reference"]["block"]["hash"]);
        assert_eq!(block["blockhash"], manifest.bank.blockhash);
        assert_eq!(block["previousBlockhash"], manifest.parent.blockhash);
        blocks.push(block);
    }
    let keys = accounts.keys().collect::<Vec<_>>();
    let rpc_accounts = rpc.call(&rpc_url, "getMultipleAccounts", json!([keys, {
        "commitment":"finalized", "encoding":"base64", "minContextSlot":slots.last().unwrap(),
    }])).await.unwrap();
    let returned = rpc_accounts["value"].as_array().unwrap();
    assert_eq!(returned.len(), accounts.len());
    for (value, expected) in returned.iter().zip(accounts.values()) {
        assert_eq!(value["owner"], expected.owner);
        let data = value["data"][0].as_str().unwrap();
        assert_eq!(STANDARD.decode(data).unwrap(), expected.data);
    }
    write_json(&output.join("accepted-snapshots.json"), &snapshots);
    write_json(&output.join("rpc-blocks.json"), &blocks);
    write_json(&output.join("rpc-accounts.json"), &rpc_accounts);

    let changed_key = keys[0];
    let original_lamports = returned[0]["lamports"].as_u64().unwrap();
    let increment = 1_234_567;
    let signature = rpc
        .call(&rpc_url, "requestAirdrop", json!([changed_key, increment]))
        .await
        .unwrap();
    let signature = signature.as_str().unwrap();
    let write_slot = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let statuses = rpc
                .call(&rpc_url, "getSignatureStatuses", json!([[signature]]))
                .await
                .unwrap();
            let status = &statuses["value"][0];
            if let Some(slot) = status["slot"].as_u64() {
                assert!(status["err"].is_null(), "transaction failed: {status}");
                break slot;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let response = baseline(&url).await;
            if response.status().is_success() {
                let snapshot: Value = response.json().await.unwrap();
                if let Some(slot) = snapshot["reference"]["block"]["number_or_slot"].as_u64() {
                    assert!(
                        slot <= write_slot,
                        "parser skipped changed bank {write_slot}"
                    );
                    if slot == write_slot {
                        break snapshot;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "changed bank {write_slot}: {:?}",
            parser.stream_status(&chain.key)
        )
    });
    let (evidence, manifest) = snapshot_proof(&snapshot, &manifests);
    assert_eq!(
        session.as_ref(),
        Some(&evidence.session_id),
        "parser changed session before publishing the write"
    );
    assert!(evidence.wal_cursor > previous_cursor);
    let writes: Vec<_> = manifest
        .native_writes
        .iter()
        .filter(|a| &a.pubkey == changed_key)
        .collect();
    assert_eq!(writes.len(), 1);
    let write = writes[0];
    assert_eq!(write.lamports, original_lamports + increment);
    assert_eq!(write.write_slot, write_slot);
    assert_eq!(write.write_bank_id, evidence.bank_id);
    assert_eq!(
        bs58::encode(
            STANDARD
                .decode(write.txn_signature_base64.as_ref().unwrap())
                .unwrap()
        )
        .into_string(),
        signature
    );
    let block = rpc
        .call(
            &rpc_url,
            "getBlock",
            json!([write_slot, {
                "commitment":"finalized", "transactionDetails":"signatures", "rewards":false,
            }]),
        )
        .await
        .unwrap();
    assert_eq!(block["blockhash"], manifest.bank.blockhash);
    assert!(block["signatures"]
        .as_array()
        .unwrap()
        .contains(&json!(signature)));
    let updated = rpc
        .call(
            &rpc_url,
            "getAccountInfo",
            json!([changed_key, {
                "commitment":"finalized", "encoding":"base64", "minContextSlot":write_slot,
            }]),
        )
        .await
        .unwrap();
    assert_eq!(updated["value"]["lamports"], write.lamports);
    assert_eq!(updated["value"]["data"][0], write.data_base64);
    write_json(&output.join("changed-snapshot.json"), &snapshot);
    write_json(&output.join("changed-rpc-block.json"), &block);
    write_json(&output.join("changed-rpc-account.json"), &updated);

    tokio::time::timeout(Duration::from_secs(5), relay.pause())
        .await
        .unwrap();
    let finalized_slot = || async {
        rpc.call(&rpc_url, "getSlot", json!([{"commitment":"finalized"}]))
            .await
            .unwrap()
            .as_u64()
            .unwrap()
    };
    let before = finalized_slot().await;
    let after = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let after = finalized_slot().await;
            if after >= before + 3 {
                break after;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    relay.disconnect().await;
    let replay_status = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let status = parser.stream_status(&chain.key).unwrap();
            if status.session_id != evidence.session_id && status.session_committed {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "retained replay status: {:?}",
            parser.stream_status(&chain.key)
        )
    });
    assert!(
        replay_status.replay_sufficient,
        "retained replay lost its parent: {replay_status:?}"
    );
    let from = replay_status.requested_from_slot.unwrap();
    assert!(from <= before + 1);
    assert!(replay_status
        .replay_first_available_slot
        .is_none_or(|floor| floor <= from));
    let journal = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let journal: Value = reqwest::get(format!(
                "{url}/internal/chains/{}/journal?after={}",
                chain.key,
                snapshot["reference"]["journal_seq"]
                    .as_u64()
                    .unwrap_or_default()
            ))
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
            let rows = journal.as_array().unwrap();
            if (from..=after).all(|slot| {
                rows.iter().any(|row| {
                    row["kind"] == "ONLINE_BASELINE_COMMIT"
                        && row["reference"]["block"]["number_or_slot"] == slot
                })
            }) {
                break journal;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing retained slots {from}–{after}"));
    write_json(
        &output.join("replay-status.json"),
        &serde_json::to_value(&replay_status).unwrap(),
    );
    write_json(&output.join("replay-journal.json"), &journal);

    drop(validator);
    tokio::time::timeout(Duration::from_secs(55), async {
        loop {
            let status = parser.stream_status(&chain.key).unwrap();
            if status.session_id != replay_status.session_id && !status.session_committed {
                assert!(!baseline(&url).await.status().is_success());
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "source disconnect status: {:?}",
            parser.stream_status(&chain.key)
        )
    });
    let mut validator =
        start_validator(&binary, &output, rpc_port, faucet_port, Some(&source_file));
    assert_eq!(
        wait_genesis(&rpc, &rpc_url, &mut validator).await,
        chain.genesis_hash
    );
    let recovered = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let response = baseline(&url).await;
            if response.status().is_success() {
                let snapshot: Value = response.json().await.unwrap();
                let recovered: StreamEvidence =
                    serde_json::from_str(snapshot["source"].as_str().unwrap()).unwrap();
                if recovered.session_id != replay_status.session_id
                    && snapshot["reference"]["block"]["number_or_slot"]
                        .as_u64()
                        .unwrap()
                        > write_slot
                {
                    break snapshot;
                }
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "restarted source status: {:?}",
            parser.stream_status(&chain.key)
        )
    });
    let (replay_evidence, replay_manifest) = snapshot_proof(&recovered, &manifests);
    assert_ne!(replay_evidence.session_id, evidence.session_id);
    assert!(replay_evidence.wal_generation > evidence.wal_generation);
    assert!(
        recovered["reference"]["online_generation"]
            .as_u64()
            .unwrap()
            > snapshot["reference"]["online_generation"].as_u64().unwrap()
    );
    assert_eq!(replay_manifest.genesis_hash, chain.genesis_hash);
    let account = replay_manifest
        .parent_accounts
        .iter()
        .find(|a| &a.pubkey == changed_key)
        .unwrap();
    assert_eq!(account.lamports, write.lamports);
    let recovered_block = rpc
        .call(
            &rpc_url,
            "getBlock",
            json!([replay_manifest.bank.slot, {
                "commitment":"finalized", "transactionDetails":"none", "rewards":false,
            }]),
        )
        .await
        .unwrap();
    assert_eq!(recovered_block["blockhash"], replay_manifest.bank.blockhash);
    assert_eq!(
        recovered_block["previousBlockhash"],
        replay_manifest.parent.blockhash
    );
    write_json(&output.join("restarted-snapshot.json"), &recovered);
    write_json(&output.join("restarted-rpc-block.json"), &recovered_block);
    parser.close().await;
    http.close().await;
}
