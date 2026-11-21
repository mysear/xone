use super::*;

// Provision these two single-partition topics in the disposable authority-test
// broker. The runner tears down that broker and its topics after the test.
const BAD_TOPIC: &str = "test.rust-startup-post-open-a";
const GOOD_TOPIC: &str = "test.rust-startup-post-open-b";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated PG/Kafka via XONE_AUTHORITY_TEST_DSN_FILE and XONE_AUTHORITY_TEST_BROKER"]
async fn rust_post_open_store_failure_releases_lease_and_keeps_peer_ready() {
    let dsn_path =
        std::env::var("XONE_AUTHORITY_TEST_DSN_FILE").expect("isolated PG DSN file required");
    let broker = std::env::var("XONE_AUTHORITY_TEST_BROKER").expect("isolated Kafka required");
    let unique = format!(
        "post-open-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let (bad, _, bad_task) = provider(&format!("{unique}-a"), 0, 70).await;
    let (good, _, good_task) = provider(&format!("{unique}-b"), 0, 72).await;
    let (mut cfg, path) = configuration(vec![bad.clone(), good.clone()]);
    cfg.control_dsn = std::fs::read_to_string(&dsn_path)
        .expect("test DSN unavailable")
        .trim()
        .into();
    cfg.config.control.dsn_file = dsn_path;
    cfg.config.control.backend = "postgres".into();
    cfg.config.control.instance_id = unique;
    cfg.config.control.lease_ttl_ms = 120_000;
    cfg.config.control.renewal_ms = 2_000;
    cfg.config.control.min_commit_remaining_ms = 7_000;
    cfg.config.journal.transport = "kafka".into();
    cfg.config.journal.kafka.brokers = vec![broker];
    cfg.config.journal.kafka.topics = BTreeMap::from([
        (bad.key.clone(), BAD_TOPIC.into()),
        (good.key.clone(), GOOD_TOPIC.into()),
    ]);
    cfg.config.journal.kafka.transaction_timeout_ms = 4_000;
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
        if parser.current(&bad.key).is_ok() || !parser.pending_start(&bad.key) {
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
