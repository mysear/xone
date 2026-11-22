use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::sync::atomic::{AtomicU64, AtomicU8};

#[path = "service_writer_recovery_tests.rs"]
mod writer_recovery;

#[path = "../tests/orca_support.rs"]
mod orca_support;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct Provider {
    fault: Arc<AtomicU8>, // 0: normal, 1: HTTP unavailable, 2: wrong genesis, 3: paused genesis
    slot: Arc<AtomicU64>,
    genesis_calls: Arc<AtomicU64>,
    resume: Arc<tokio::sync::Semaphore>,
    genesis: String,
    hash: String,
    timestamp: u64,
    accounts: Arc<BTreeMap<String, Value>>,
}

async fn rpc_reply(State(p): State<Provider>, Json(q): Json<Value>) -> axum::response::Response {
    if p.fault.load(Ordering::Acquire) == 1 {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"offline"})),
        )
            .into_response();
    }
    let method = q["method"].as_str().unwrap();
    if method == "getGenesisHash" {
        p.genesis_calls.fetch_add(1, Ordering::AcqRel);
        if p.fault.load(Ordering::Acquire) == 3 {
            p.resume.acquire().await.unwrap().forget();
        }
    }
    let slot = p.slot.load(Ordering::Acquire);
    let result = match method {
        "getGenesisHash" if p.fault.load(Ordering::Acquire) == 2 => json!("wrong-genesis"),
        "getGenesisHash" => json!(p.genesis),
        "getMultipleAccounts" => {
            let accounts = q["params"][0]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| {
                    let key = id.as_str().unwrap();
                    let mut value = p.accounts.get(key).cloned().unwrap_or(Value::Null);
                    if key == orca_support::CLOCK {
                        let mut clock =
                            STANDARD.decode(value["data"][0].as_str().unwrap()).unwrap();
                        orca_support::put64(&mut clock, 0, slot);
                        value["data"][0] = json!(STANDARD.encode(clock));
                    }
                    value
                })
                .collect::<Vec<_>>();
            json!({"context":{"slot":slot},"value":accounts})
        }
        "getBlock" => {
            let height = q["params"][0].as_u64().unwrap();
            let (hash, parent) = match height {
                100 => (p.hash.clone(), orca_support::key(71)),
                101 => (orca_support::key(78), p.hash.clone()),
                _ => panic!("unknown fixture block {height}"),
            };
            json!({"blockTime":p.timestamp,"blockhash":hash,"previousBlockhash":parent,"parentSlot":height-1})
        }
        _ => panic!("unexpected RPC method: {q}"),
    };
    Json(json!({"jsonrpc":"2.0","id":1,"result":result})).into_response()
}

async fn provider(
    label: &str,
    fault: u8,
    hash: u8,
) -> (Chain, Provider, tokio::task::JoinHandle<()>) {
    // Exact synthetic account bytes pass the normal Orca/RPC decoders. This is
    // an isolation regression, not a claim about an independent mainnet bank.
    let mut fixture = orca_support::Fixture::new(false);
    let timestamp = now_ms() / 1000;
    orca_support::put64(
        &mut fixture.accounts.get_mut(orca_support::CLOCK).unwrap().data,
        32,
        timestamp,
    );
    let accounts = fixture
        .accounts
        .iter()
        .map(|(id, a)| {
            (
                id.clone(),
                json!({"owner":a.owner,"data":[STANDARD.encode(&a.data),"base64"],
            "executable":a.executable,"lamports":a.lamports}),
            )
        })
        .collect();
    let state = Provider {
        fault: Arc::new(AtomicU8::new(fault)),
        slot: Arc::new(AtomicU64::new(100)),
        genesis_calls: Arc::new(AtomicU64::new(0)),
        resume: Arc::new(tokio::sync::Semaphore::new(0)),
        genesis: format!("genesis-{label}"),
        hash: orca_support::key(hash),
        timestamp,
        accounts: Arc::new(accounts),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/", post(rpc_reply))
        .with_state(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut chain = fixture.chain;
    chain.key = format!("solana:startup-{label}");
    chain.owner_service = "chain-parser-rust".into();
    chain.genesis_hash = state.genesis.clone();
    chain.rpc_urls = vec![format!("http://{address}/")];
    chain.pools = vec![fixture.pool];
    (chain, state, task)
}

fn configuration(chains: Vec<Chain>) -> (ConfigSnapshot, std::path::PathBuf) {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    let path = std::env::temp_dir().join(format!(
        "xone-round243-state-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    cfg.config.state.path = path.to_string_lossy().into();
    cfg.config.history.enabled = false;
    cfg.config.chains = chains;
    (cfg, path)
}

async fn chain_status(parser: &Parser, key: &str) -> StatusCode {
    status(State(parser.clone()), HeaderMap::new(), Path(key.into()))
        .await
        .into_response()
        .status()
}

async fn wait_until(check: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("chain condition timed out");
}

#[tokio::test]
async fn rust_slow_rpc_allows_peer_start_and_commit_and_bounds_pending_work() {
    for slow_first in [true, false] {
        let (a, slow, a_task) = provider("paused", 3, 70).await;
        let (b, fast, b_task) = provider("fast", 0, 72).await;
        let chains = if slow_first {
            vec![a.clone(), b.clone()]
        } else {
            vec![b.clone(), a.clone()]
        };
        let (mut cfg, path) = configuration(chains);
        cfg.config.journal.request_timeout_ms = 30_000;
        let parser = Parser::new(cfg).unwrap();
        tokio::time::timeout(Duration::from_secs(5), parser.start())
            .await
            .unwrap()
            .unwrap();
        wait_until(|| slow.genesis_calls.load(Ordering::Acquire) == 1).await;
        assert_eq!(
            chain_status(&parser, &a.key).await,
            StatusCode::SERVICE_UNAVAILABLE
        );
        let busy_writer = parser.writers[&a.key].lock().await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), ready(State(parser.clone())))
                .await
                .unwrap()
                .into_response()
                .status(),
            StatusCode::OK
        );
        drop(busy_writer);
        let before = parser.current(&b.key).unwrap();
        fast.slot.store(101, Ordering::Release);
        for _ in 0..128 {
            parser.schedule_tick();
        }
        wait_until(|| {
            parser
                .current(&b.key)
                .is_ok_and(|s| s.reference.block.number_or_slot == 101)
        })
        .await;
        let after = parser.current(&b.key).unwrap();
        assert_eq!(
            after.reference.block.parent_hash,
            before.reference.block.hash
        );
        assert_eq!(
            after.reference.journal_seq,
            before.reference.journal_seq + 1
        );
        assert_eq!(chain_status(&parser, &b.key).await, StatusCode::OK);
        if slow_first {
            // Let startup and the single queued tick finish after the peer
            // advances. Both use the same bank, so only one baseline exists.
            slow.resume.add_permits(2);
            wait_until(|| slow.genesis_calls.load(Ordering::Acquire) == 2).await;
            let _settled = parser.chain_actors[&a.key].lock().await;
            assert_eq!(parser.current(&a.key).unwrap().reference.journal_seq, 1);
            assert_eq!(slow.genesis_calls.load(Ordering::Acquire), 2);
        }
        tokio::time::timeout(Duration::from_secs(2), parser.close())
            .await
            .unwrap();
        slow.resume.add_permits(1);
        assert!(parser.workers.lock().unwrap().is_empty());
        assert!(parser.stream_tasks.lock().await.is_empty());
        assert!(parser.current(&b.key).is_err());
        if !slow_first {
            assert!(parser.stores[&a.key].lock().unwrap().previous().is_none());
        }
        a_task.abort();
        b_task.abort();
        let _ = a_task.await;
        let _ = b_task.await;
        drop(parser);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[tokio::test]
async fn rust_partial_startup_isolates_rpc_failure_in_both_orders_and_tick_recovers() {
    for fault in [1, 2] {
        for failed_first in [true, false] {
            let (a, failed, a_task) = provider("a", fault, 70).await;
            let (b, healthy, b_task) = provider("b", 0, 72).await;
            let chains = if failed_first {
                vec![a.clone(), b.clone()]
            } else {
                vec![b.clone(), a.clone()]
            };
            let (cfg, path) = configuration(chains);
            let parser = Parser::new(cfg).unwrap();
            parser.start().await.unwrap();
            assert_eq!(
                chain_status(&parser, &a.key).await,
                StatusCode::SERVICE_UNAVAILABLE
            );
            assert_eq!(chain_status(&parser, &b.key).await, StatusCode::OK);
            assert_eq!(
                startup(State(parser.clone()))
                    .await
                    .into_response()
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                ready(State(parser.clone())).await.into_response().status(),
                StatusCode::OK
            );
            assert!(parser.stores[&a.key]
                .lock()
                .unwrap()
                .journal(0, 10)
                .unwrap()
                .is_empty());
            let before = parser.current(&b.key).unwrap();
            assert_eq!(before.reference.block.hash, healthy.hash);
            parser.tick().await;
            assert_eq!(
                chain_status(&parser, &a.key).await,
                StatusCode::SERVICE_UNAVAILABLE
            );
            assert_eq!(parser.current(&b.key).unwrap(), before);
            failed.fault.store(0, Ordering::Release);
            parser.tick().await;
            let recovered = parser.current(&a.key).unwrap();
            assert_eq!(recovered.reference.block.hash, failed.hash);
            assert_eq!(recovered.reference.block.chain_key, a.key);
            assert_eq!(recovered.reference.block.number_or_slot, 100);
            assert_eq!(recovered.pools.len(), 1);
            assert_eq!(recovered.reference.online_generation, 1);
            assert!(!parser.pending_start(&a.key));
            assert_eq!(chain_status(&parser, &a.key).await, StatusCode::OK);
            assert_eq!(
                parser.current(&b.key).unwrap(),
                before,
                "peer recovery changed healthy chain versions/state"
            );
            parser.close().await;
            a_task.abort();
            b_task.abort();
            let _ = a_task.await;
            let _ = b_task.await;
            drop(parser);
            std::fs::remove_dir_all(path).unwrap();
        }
    }
}

#[tokio::test]
async fn rust_all_failed_startup_returns_first_source_error_and_never_becomes_ready() {
    let (a, _, a_task) = provider("a", 1, 70).await;
    let (b, _, b_task) = provider("b", 2, 72).await;
    let (cfg, path) = configuration(vec![a.clone(), b.clone()]);
    let parser = Parser::new(cfg).unwrap();
    assert_eq!(parser.start().await.unwrap_err().0, "RPC_HTTP_STATUS");
    assert!(
        parser.pending_start(&a.key) && parser.pending_start(&b.key),
        "did not attempt both failed chains"
    );
    assert_eq!(
        startup(State(parser.clone()))
            .await
            .into_response()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        ready(State(parser.clone())).await.into_response().status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(parser.current(&a.key).is_err() && parser.current(&b.key).is_err());
    parser.close().await;
    a_task.abort();
    b_task.abort();
    let _ = a_task.await;
    let _ = b_task.await;
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn rust_partial_restart_retry_advances_only_failed_chain_writer_once() {
    let (a, failed, a_task) = provider("restart-a", 0, 70).await;
    let (b, _, b_task) = provider("restart-b", 0, 72).await;
    let (cfg, path) = configuration(vec![a.clone(), b.clone()]);
    let first = Parser::new(cfg.clone()).unwrap();
    first.start().await.unwrap();
    wait_until(|| first.current(&a.key).is_ok() && first.current(&b.key).is_ok()).await;
    let old = first.current(&a.key).unwrap().reference;
    first.close().await;
    drop(first);

    failed.fault.store(1, Ordering::Release);
    let restarted = Parser::new(cfg).unwrap();
    restarted.start().await.unwrap();
    wait_until(|| restarted.pending_start(&a.key)).await;
    assert!(restarted.current(&a.key).is_err());
    let healthy = restarted.current(&b.key).unwrap();
    failed.fault.store(0, Ordering::Release);
    restarted.tick().await;
    let recovered = restarted.current(&a.key).unwrap();
    assert_eq!(recovered.reference.writer_epoch, old.writer_epoch + 1);
    assert_eq!(
        recovered.reference.online_generation,
        old.online_generation + 1
    );
    assert_eq!(recovered.reference.canonical_epoch, old.canonical_epoch);
    assert_eq!(restarted.current(&b.key).unwrap(), healthy);
    restarted.tick().await;
    assert_eq!(
        restarted.current(&a.key).unwrap(),
        recovered,
        "retry kept startup allocation enabled"
    );
    assert_eq!(restarted.current(&b.key).unwrap(), healthy);
    restarted.close().await;
    a_task.abort();
    b_task.abort();
    let _ = a_task.await;
    let _ = b_task.await;
    drop(restarted);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn rust_legitimate_standby_starts_but_closed_start_does_not_succeed() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    let (test_cfg, path) = configuration(std::mem::take(&mut cfg.config.chains));
    let parser =
        Parser::with_source(test_cfg, Some(Arc::new(|_| Err(err("LEASE_NOT_OWNED"))))).unwrap();
    parser.start().await.unwrap();
    assert_eq!(
        ready(State(parser.clone())).await.into_response().status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    parser.close().await;
    assert_eq!(
        parser.start().await.unwrap_err().0,
        "PARSER_ALREADY_STARTED_OR_CLOSED"
    );
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn rust_close_during_source_acquisition_cannot_return_partial_startup_success() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    let (test_cfg, path) = configuration(std::mem::take(&mut cfg.config.chains));
    let mut parser = Parser::with_source(test_cfg, Some(Arc::new(|_| Err(err("unused"))))).unwrap();
    let stop = parser.stream_stop.clone();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    snapshot.reference.block.timestamp_ms = now_ms();
    parser.source = Some(Arc::new(move |_| {
        stop.send_replace(true);
        Ok(snapshot.clone())
    }));
    assert_eq!(
        parser.start().await.unwrap_err().0,
        "PARSER_ALREADY_STARTED_OR_CLOSED"
    );
    assert!(!parser.started.load(Ordering::Acquire));
    assert!(parser.current("solana:localnet").is_err());
    assert!(
        parser.stores["solana:localnet"]
            .lock()
            .unwrap()
            .previous()
            .is_none(),
        "source acquisition completed after close but still published a baseline"
    );
    parser.close().await;
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn rust_close_releases_task_registry_before_waiting_for_stream_exit() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    let (test_cfg, path) = configuration(std::mem::take(&mut cfg.config.chains));
    let parser = Parser::with_source(test_cfg, Some(Arc::new(|_| Err(err("unused"))))).unwrap();
    let exit = Arc::new(tokio::sync::Notify::new());
    let release = exit.clone();
    parser
        .stream_tasks
        .lock()
        .await
        .push(tokio::spawn(async move {
            release.notified().await;
        }));
    let closing = parser.clone();
    let close = tokio::spawn(async move { closing.close().await });
    // Keep the stream alive until close has drained its handle. This proves
    // that the registry remains available while close waits for task exit.
    let drained = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let tasks = parser.stream_tasks.lock().await;
            if *parser.stream_stop.borrow() && tasks.is_empty() {
                return;
            }
            drop(tasks);
            tokio::task::yield_now().await;
        }
    })
    .await;
    exit.notify_one();
    tokio::time::timeout(Duration::from_secs(2), close)
        .await
        .unwrap()
        .unwrap();
    assert!(
        drained.is_ok(),
        "close held the task registry while joining"
    );
    assert!(parser.stream_tasks.lock().await.is_empty());
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn rust_failed_initial_stream_revocation_isolated_then_retried_by_tick() {
    let (a, _, a_task) = provider("stream", 0, 70).await;
    let (b, _, b_task) = provider("rpc", 0, 72).await;
    let (mut cfg, path) = configuration(vec![a.clone(), b.clone()]);
    cfg.config.yellowstone.insert(a.key.clone(), serde_json::from_value(json!({
        "endpoint":"http://127.0.0.1:1","source_id":"startup-test",
        "proto_commit":"10f0af69d150637d8e3eaaf83b179d640988dd3c","allow_loopback_plaintext":true,
        "connect_timeout_ms":50,"idle_timeout_ms":100,"wal_ack_timeout_ms":1000,"reconnect_delay_ms":10,
        "max_frame_bytes":1048576,"max_wal_bytes":8388608,"max_wal_records":1000,"max_replay_records":100,
        "max_replay_slots":32,"max_stream_bytes":8388608,"max_stream_slots":32,"max_stream_updates":1024,
        "max_manifest_bytes":1048576,"max_catalog_pools":16,"manifest_directory":"missing-manifests",
        "catalog_file":"missing-catalog.json"})).unwrap());
    let parser = Parser::new(cfg).unwrap();
    parser.stores[&a.key]
        .lock()
        .unwrap()
        .db
        .put(b"online/prepared", b"invalid-json")
        .unwrap();
    parser.start().await.unwrap();
    wait_until(|| parser.pending_start(&a.key)).await;
    assert_eq!(
        parser.pending_starts.lock().unwrap()[&a.key],
        "PREPARED_CORRUPT"
    );
    assert!(parser.stream_tasks.lock().await.is_empty());
    assert!(
        !parser.source_wals.contains_key(&a.key),
        "stream chain must not fall back to RPC"
    );
    assert_eq!(
        chain_status(&parser, &a.key).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let healthy = parser.current(&b.key).unwrap();
    parser.stores[&a.key]
        .lock()
        .unwrap()
        .db
        .delete(b"online/prepared")
        .unwrap();
    parser.tick().await;
    assert!(!parser.pending_start(&a.key));
    assert_eq!(parser.stream_tasks.lock().await.len(), 1);
    assert_eq!(
        chain_status(&parser, &a.key).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "starting a stream task must not invent a qualified bank"
    );
    assert_eq!(parser.current(&b.key).unwrap(), healthy);
    parser.close().await;
    a_task.abort();
    b_task.abort();
    let _ = a_task.await;
    let _ = b_task.await;
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
}
