use super::*;

// Provision these two single-partition topics in the disposable authority-test
// broker. The runner tears down that broker and its topics after the test.
const BAD_TOPIC: &str = "test.rust-startup-post-open-a";
const GOOD_TOPIC: &str = "test.rust-startup-post-open-b";

fn configure_authority(
    cfg: &mut ConfigSnapshot,
    instance: String,
    a: &Chain,
    topic_a: &str,
    b: &Chain,
    topic_b: &str,
) {
    let dsn_path =
        std::env::var("XONE_AUTHORITY_TEST_DSN_FILE").expect("isolated PG DSN file required");
    let broker = std::env::var("XONE_AUTHORITY_TEST_BROKER").expect("isolated Kafka required");
    cfg.control_dsn = std::fs::read_to_string(&dsn_path)
        .expect("test DSN unavailable")
        .trim()
        .into();
    cfg.config.control.dsn_file = dsn_path;
    cfg.config.control.backend = "postgres".into();
    cfg.config.control.instance_id = instance;
    cfg.config.control.lease_ttl_ms = 120_000;
    cfg.config.control.renewal_ms = 2_000;
    cfg.config.control.min_commit_remaining_ms = 7_000;
    cfg.config.journal.transport = "kafka".into();
    cfg.config.journal.kafka.brokers = vec![broker];
    cfg.config.journal.kafka.topics = BTreeMap::from([
        (a.key.clone(), topic_a.into()),
        (b.key.clone(), topic_b.into()),
    ]);
    cfg.config.journal.kafka.transaction_timeout_ms = 4_000;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated PG/Kafka via XONE_AUTHORITY_TEST_DSN_FILE and XONE_AUTHORITY_TEST_BROKER"]
async fn rust_post_open_store_failure_releases_lease_and_keeps_peer_ready() {
    let unique = format!(
        "post-open-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let (bad, _, bad_task) = provider(&format!("{unique}-a"), 0, 70).await;
    let (good, _, good_task) = provider(&format!("{unique}-b"), 0, 72).await;
    let (mut cfg, path) = configuration(vec![bad.clone(), good.clone()]);
    configure_authority(&mut cfg, unique, &bad, BAD_TOPIC, &good, GOOD_TOPIC);
    let parser = Parser::new(cfg).unwrap();
    let corrupt = b"invalid-json";
    parser.stores[&bad.key]
        .lock()
        .unwrap()
        .db
        .put(b"scope/ledger-v1", corrupt)
        .unwrap();

    // Keep cleanup outside the fallible body so a regression still closes the
    // real writers, joins the local RPC providers and removes the state tree.
    let started = std::time::Instant::now();
    let outcome = async {
        parser.start().await.map_err(|e| e.0)?;
        wait_until(|| parser.pending_start(&bad.key)).await;
        if parser.current(&bad.key).is_ok() {
            return Err("corrupt chain unexpectedly became ready".to_string());
        }
        if parser.writers[&bad.key].lock().await.is_some() {
            return Err("failed post-open writer was installed".to_string());
        }
        if parser
            .pending_starts
            .lock()
            .map_err(|_| "pending lock poisoned".to_string())?
            .get(&bad.key)
            .map(String::as_str)
            != Some("SCOPE_STORE_CORRUPT")
        {
            return Err("test did not reach the post-open ledger reconciliation gate".to_string());
        }
        if chain_status(&parser, &bad.key).await != StatusCode::SERVICE_UNAVAILABLE {
            return Err("corrupt chain HTTP did not reject the baseline".to_string());
        }
        let healthy = parser.current(&good.key).map_err(|e| e.0)?;
        if healthy.reference.block.chain_key != good.key
            || chain_status(&parser, &good.key).await != StatusCode::OK
        {
            return Err("post-open failure prevented the healthy peer baseline".to_string());
        }
        {
            let store = parser.stores[&bad.key]
                .lock()
                .map_err(|_| "state lock poisoned".to_string())?;
            if store.prepared().map_err(|e| e.0)?.is_some() {
                return Err("test did not isolate the scope-ledger recovery gate".to_string());
            }
            if store
                .db
                .get(b"scope/ledger-v1")
                .map_err(|e| e.to_string())?
                .as_deref()
                != Some(corrupt.as_slice())
            {
                return Err("failed recovery changed the corrupt evidence".to_string());
            }
        }
        let pg = parser
            .control
            .get()
            .ok_or_else(|| "missing authority connection".to_string())?;
        if started.elapsed() >= Duration::from_secs(30) {
            return Err("startup exceeded the unexpired-lease test window".to_string());
        }
        // This acquire happens long before the 120-second lease could expire.
        // The pre-fix implementation returns LEASE_NOT_OWNED here, even though
        // startup has already isolated the failed chain and reports B ready.
        let peer = tokio::time::timeout(
            Duration::from_secs(2),
            pg.acquire(
                &bad.key,
                &bad.owner_service,
                bad.assignment_version,
                "post-open-release-peer",
                120_000,
            ),
        )
        .await
        .map_err(|_| "peer acquire deadline".to_string())?
        .map_err(|e| format!("uninstalled writer leaked its acquired lease: {}", e.0))?;
        pg.release(&peer).await.map_err(|e| e.0)?;
        if parser.current(&good.key).map_err(|e| e.0)? != healthy {
            return Err("peer lease recovery changed the healthy generation".to_string());
        }
        Ok::<(), String>(())
    }
    .await;
    parser.close().await;
    bad_task.abort();
    good_task.abort();
    let _ = bad_task.await;
    let _ = good_task.await;
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
    outcome.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated PG/Kafka via XONE_AUTHORITY_TEST_DSN_FILE and XONE_AUTHORITY_TEST_BROKER"]
async fn slow_peer_and_cancelled_tick_preserve_authority_and_kafka_commit() {
    let unique = format!("chain-worker-{}", std::process::id());
    let (a, slow, a_task) = provider(&format!("{unique}-a"), 3, 70).await;
    let (b, fast, b_task) = provider(&format!("{unique}-b"), 0, 72).await;
    let (mut cfg, path) = configuration(vec![a.clone(), b.clone()]);
    let topic = "test.rust-chain-actor-b";
    configure_authority(
        &mut cfg,
        unique.clone(),
        &a,
        "test.rust-chain-actor-a",
        &b,
        topic,
    );
    cfg.config.journal.request_timeout_ms = 30_000;
    let parser = Parser::new(cfg).unwrap();
    parser.start().await.unwrap();
    wait_until(|| slow.genesis_calls.load(Ordering::Acquire) == 1).await;
    let first = parser.current(&b.key).unwrap().reference;
    let deadline = || async {
        parser.writers[&b.key]
            .lock()
            .await
            .as_ref()
            .unwrap()
            .lease
            .expires_ms
    };
    let initial_expiry = deadline().await;
    tokio::time::timeout(Duration::from_secs(8), async {
        while deadline().await <= initial_expiry {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("idle peer lease did not renew");

    fast.slot.store(101, Ordering::Release);
    fast.fault.store(3, Ordering::Release);
    let caller = parser.clone();
    let waiter = tokio::spawn(async move { caller.tick().await });
    wait_until(|| fast.genesis_calls.load(Ordering::Acquire) == 2).await;
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    fast.resume.add_permits(1);
    wait_until(|| {
        parser
            .current(&b.key)
            .is_ok_and(|s| s.reference.block.number_or_slot == 101)
    })
    .await;
    let second = parser.current(&b.key).unwrap().reference;
    assert_eq!(second.block.parent_hash, first.block.hash);
    assert_eq!(second.writer_epoch, first.writer_epoch);
    assert_eq!(second.journal_seq, first.journal_seq + 1);
    assert!(second.online_generation > first.online_generation);

    tokio::time::timeout(Duration::from_secs(8), parser.close())
        .await
        .unwrap();
    slow.resume.add_permits(1);
    assert!(parser.stores[&a.key].lock().unwrap().previous().is_none());
    assert!(parser.writers[&b.key].lock().await.is_none());
    let pg = parser.control.get().unwrap();
    let peer = pg
        .acquire(
            &b.key,
            &b.owner_service,
            b.assignment_version,
            "released-peer",
            120_000,
        )
        .await
        .unwrap();
    assert!(peer.writer_epoch > first.writer_epoch);
    pg.release(&peer).await.unwrap();

    let brokers = parser.config.config.journal.kafka.brokers.clone();
    let journal_limit = parser.config.config.journal.max_payload_bytes;
    let records = tokio::task::spawn_blocking(move || {
        use xone_kafka_transport::{Cursor, Reader};
        let mut reader = Reader::open(
            &brokers,
            &unique,
            &Cursor {
                topic: topic.into(),
                partition: 0,
                offset: 0,
            },
            journal_limit,
            128,
            Duration::from_secs(5),
        )
        .unwrap();
        let mut records = Vec::new();
        let limit = std::time::Instant::now() + Duration::from_secs(5);
        while records.len() < 3 && std::time::Instant::now() < limit {
            if let Some(unit) = reader.poll(Duration::from_millis(50)).unwrap() {
                records.push((unit.kind, unit.snapshot.reference));
            }
        }
        records
    })
    .await
    .unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0], ("ONLINE_BASELINE_COMMIT".into(), first));
    assert_eq!(
        records[1],
        ("ONLINE_BASELINE_COMMIT".into(), second.clone())
    );
    assert_eq!(records[2].0, "ONLINE_SCOPE_REVOKED");
    assert_eq!(records[2].1.block, second.block);
    assert_eq!(records[2].1.journal_seq, second.journal_seq + 1);
    a_task.abort();
    b_task.abort();
    let _ = a_task.await;
    let _ = b_task.await;
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
}
