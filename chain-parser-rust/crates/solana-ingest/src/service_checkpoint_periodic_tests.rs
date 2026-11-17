use super::remote_tests::Fixture;
use super::*;
use xone_kafka_transport::{Bootstrap, Cursor, Unit};
fn committed(f: &Fixture, key: &str, offset: i64) -> Bootstrap {
    let mut store = f.parser.stores[key].lock().unwrap();
    let mut snapshot = store.current().unwrap();
    snapshot.reference.online_generation += 1;
    snapshot.reference.journal_seq += 1;
    let boot = Bootstrap {
        snapshot,
        cursor: Cursor {
            topic: format!("test.periodic.{}", key.replace(':', "-")),
            partition: 0,
            offset,
        },
    };
    store
        .prepare(&Unit {
            normalized_facts: None,
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: boot.snapshot.clone(),
        })
        .unwrap();
    store.apply_prepared(boot.cursor.clone()).unwrap();
    boot
}
async fn wait(predicate: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    loop {
        if predicate() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "periodic checkpoint condition timed out"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn current_catalog(f: &Fixture, key: &str, old_pin: Option<&str>) -> RemotePublication {
    let deadline = std::time::Instant::now() + Duration::from_secs(4);
    loop {
        let response = f
            .client
            .get(format!(
                "{}/internal/chains/{key}/checkpoint-catalog",
                f.owner
            ))
            .bearer_auth(&f.parser.config.token)
            .send()
            .await
            .unwrap();
        if response.status() == reqwest::StatusCode::OK {
            let p: RemotePublication =
                serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
            if old_pin.is_none_or(|old| old != p.manifest_sha256) {
                return p;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "chain {key} never acquired current periodic catalogue"
        );
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}
#[tokio::test]
async fn rust_periodic_checkpoint_is_explicit_and_manual_mode_stays_idle() {
    let f = Fixture::new_configured(0, false, "").await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(f.objects.lock().unwrap().requests.is_empty());
    assert!(!f.parser.checkpoint_metrics.active.load(Ordering::Acquire));
    assert!(f.parser.checkpoint_task.lock().await.is_none());
    assert_eq!(
        f.client
            .get(format!("{}/internal/checkpoints/status", f.owner))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.publish().await.0, reqwest::StatusCode::OK);
    f.close().await;
}
#[tokio::test]
async fn rust_periodic_checkpoint_rotates_past_failed_chain_and_publishes_new_generations() {
    let f = Fixture::new_configured(100, true, "fail-all").await;
    let first = f.key.clone();
    let second = f.parser.config.config.chains[1].key.clone();
    let bad = committed(&f, &first, 9007199254741041);
    let healthy = committed(&f, &second, 9007199254741051);
    let (manifest, _, _) =
        local::encode(&bad, f.parser.config.config.journal.max_payload_bytes).unwrap();
    {
        let mut state = f.objects.lock().unwrap();
        state.failed_id = Some(manifest.sha256);
        state.mode.clear();
    }
    let published = current_catalog(&f, &second, None).await;
    assert_eq!(published.manifest.reference, healthy.snapshot.reference);
    assert_eq!(
        published.manifest.cursor.next_offset,
        healthy.cursor.offset.to_string()
    );
    wait(|| f.parser.checkpoint_metrics.errors.load(Ordering::Relaxed) > 0).await;
    assert_eq!(
        f.catalog().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    let updated = committed(&f, &second, 9007199254741057);
    let replacement = current_catalog(&f, &second, Some(&published.manifest_sha256)).await;
    assert_eq!(replacement.manifest.reference, updated.snapshot.reference);
    assert_eq!(
        f.publish().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "manual batch must preserve explicit first failure"
    );
    f.objects.lock().unwrap().failed_id = None;
    let recovered = current_catalog(&f, &first, None).await;
    assert_eq!(recovered.manifest.reference, bad.snapshot.reference);
    for (key, expected) in [(&first, &bad), (&second, &updated)] {
        let actual = f.parser.stores[key].lock().unwrap().bootstrap().unwrap();
        assert_eq!(actual.cursor, expected.cursor);
        assert_eq!(actual.snapshot.reference, expected.snapshot.reference);
    }
    let status = f
        .client
        .get(format!("{}/internal/checkpoints/status", f.owner))
        .bearer_auth(&f.parser.config.token)
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), reqwest::StatusCode::OK);
    let status: Value = serde_json::from_slice(&status.bytes().await.unwrap()).unwrap();
    assert_eq!(status["enabled"], serde_json::json!(true));
    assert_eq!(status["active"], serde_json::json!(true));
    assert_eq!(f.objects.lock().unwrap().max_active, 1);
    if let Ok(directory) = std::env::var("XONE_RUST_PERIODIC_CHECKPOINT_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("rust-periodic-two-chain.json"),serde_json::to_vec_pretty(&serde_json::json!({"first":recovered,"second":replacement,"status":status,"max_active_object_requests":1,"checks":["no_manual_post_for_initial_publication","failed_chain_no_stale_catalog","healthy_generation_keeps_updating","manual_failure_keeps_rotation","failed_chain_recovers","owner_state_and_cursor_unchanged"],"verification_scope":"NORMAL_RUST_OWNER_TWO_CONTROLLED_CHAINS_AND_HTTP_OBJECT_API"})).unwrap()).unwrap();
    }
    f.parser.close().await;
    assert!(!f.parser.checkpoint_metrics.active.load(Ordering::Acquire));
    assert!(f.parser.checkpoint_task.lock().await.is_none());
    let count = f.objects.lock().unwrap().requests.len();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(f.objects.lock().unwrap().requests.len(), count);
    f.close().await;
}
#[tokio::test]
async fn rust_periodic_checkpoint_normal_shutdown_signal_cancels_remote_work_and_joins_task() {
    let f = Fixture::new_configured(100, false, "hold").await;
    tokio::time::timeout(Duration::from_secs(2), f.entered.notified())
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (send, receive) = tokio::sync::oneshot::channel::<()>();
    let parser = f.parser.clone();
    let server = tokio::spawn(super::super::serve_started_until(
        parser,
        listener,
        async move {
            let _ = receive.await;
        },
    ));
    send.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .expect("normal shutdown must cancel periodic HTTP before draining")
        .unwrap()
        .unwrap();
    assert!(!f.parser.started.load(Ordering::Acquire));
    assert!(!f.parser.checkpoint_metrics.active.load(Ordering::Acquire));
    assert!(f.parser.checkpoint_task.lock().await.is_none());
    assert!(f.objects.lock().unwrap().data.is_empty());
    let count = f.objects.lock().unwrap().requests.len();
    f.release.notify_waiters();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(f.objects.lock().unwrap().requests.len(), count);
    assert_eq!(
        f.publish().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.close().await;
}

#[tokio::test]
async fn rust_periodic_checkpoint_local_budget_rejects_before_file_and_remote_io() {
    let f = Fixture::new_budget(
        100,
        xone_config::CheckpointBudget {
            max_bytes: 1,
            max_files: 4,
            max_entries: 8,
        },
    )
    .await;
    wait(|| f.parser.checkpoint_metrics.errors.load(Ordering::Relaxed) > 0).await;
    assert!(f.objects.lock().unwrap().requests.is_empty());
    let root = f.parser.config.resolve(&f.parser.config.config.state.path);
    assert!(!root.join("checkpoints").exists());
    let (status, value) = f.publish().await;
    assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        value["error"],
        serde_json::json!("CHECKPOINT_LOCAL_BUDGET_EXCEEDED")
    );
    assert!(f.objects.lock().unwrap().requests.is_empty());
    assert!(!root.join("checkpoints").exists());
    f.close().await;
}
