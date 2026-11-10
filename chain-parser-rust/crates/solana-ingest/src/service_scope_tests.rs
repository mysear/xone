use super::*;
fn fixture() -> Parser {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    cfg.config.state.path = std::env::temp_dir()
        .join(format!(
            "xone-scope-http-{}-{}",
            std::process::id(),
            now_ms()
        ))
        .to_string_lossy()
        .into();
    cfg.token = "unit-test-explicit-api-token-12345".into();
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    snapshot.reference.block.timestamp_ms = now_ms();
    let source = Arc::new(move |_: &Chain| Ok(snapshot.clone()));
    Parser::with_source(cfg, Some(source)).unwrap()
}
#[tokio::test]
async fn authenticated_scope_control_uses_current_complete_generation() {
    let mut parser = fixture();
    parser.start().await.unwrap();
    let chain = parser.config.config.chains[0].key.clone();
    let snapshot = parser.current(&chain).unwrap();
    let id = snapshot.pools[0].pool_id.clone();
    let mut c = parser.config.as_ref().clone();
    let mut p: xone_config::PoolConfig=serde_json::from_value(json!({"id":id,"protocol":"raydium-cpmm","token0":"A","token1":"B","fee_numerator":25,"fee_denominator":10000,"program_id":"program","accounts":[]})).unwrap();
    p.id = id.clone();
    c.config.chains[0].pools = vec![p];
    parser.config = Arc::new(c);
    let request = |body: String| {
        axum::http::Request::builder()
            .method("POST")
            .uri("/")
            .body(axum::body::Body::from(body))
            .unwrap()
    };
    let good =
        json!({"pool_ids":[id],"expected_online_generation":snapshot.reference.online_generation})
            .to_string();
    let response = ensure_pool_scope(
        State(parser.clone()),
        Path(chain.clone()),
        HeaderMap::new(),
        request(good.clone()),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", parser.config.token).parse().unwrap(),
    );
    for _ in 0..2 {
        let response = ensure_pool_scope(
            State(parser.clone()),
            Path(chain.clone()),
            headers.clone(),
            request(good.clone()),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
    for (body,status)in [(json!({"pool_ids":["unknown"],"expected_online_generation":snapshot.reference.online_generation}).to_string(),StatusCode::NOT_FOUND),(json!({"pool_ids":[id,id],"expected_online_generation":snapshot.reference.online_generation}).to_string(),StatusCode::BAD_REQUEST),(json!({"pool_ids":[id],"expected_online_generation":snapshot.reference.online_generation+1}).to_string(),StatusCode::CONFLICT),("{\"pool_ids\":[],\"unknown\":true}".into(),StatusCode::BAD_REQUEST)]{let response=ensure_pool_scope(State(parser.clone()),Path(chain.clone()),headers.clone(),request(body)).await.into_response();assert_eq!(response.status(),status);}
    let directory = std::path::PathBuf::from(&parser.config.config.state.path);
    drop(parser);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_http_waiter_cannot_release_actor_before_commit_result_settles() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let actor = Arc::new(tokio::sync::Mutex::new(()));
    let guard = actor.clone().try_lock_owned().unwrap();
    let entered = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicBool::new(false));
    let inside = entered.clone();
    let done = completed.clone();
    let work = spawn_owned_scope_recovery(guard, async move {
        tokio::task::spawn_blocking(move || {
            inside.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(120));
            done.store(true, Ordering::SeqCst);
        })
        .await
        .unwrap();
        Ok(())
    });
    let waiter = tokio::spawn(async move {
        let _ = work.await;
    });
    while !entered.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    waiter.abort();
    let _ = waiter.await;
    assert!(
        actor.try_lock().is_err(),
        "actor released while commit continues"
    );
    while !completed.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    drop(
        tokio::time::timeout(Duration::from_secs(1), actor.lock())
            .await
            .unwrap(),
    );
}

#[tokio::test]
async fn normal_yellowstone_start_is_cancellable_without_rpc_or_supplier_token() {
    let mut cfg = xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    let path = std::env::temp_dir().join(format!(
        "xone-normal-red-{}-{}",
        std::process::id(),
        now_ms()
    ));
    cfg.config.state.path = path.to_string_lossy().into();
    cfg.config.chains[0].confirmation_tag = "finalized".into();
    let stream = serde_json::from_value(json!({"endpoint":"http://127.0.0.1:1","source_id":"self-hosted",
        "proto_commit":"10f0af69d150637d8e3eaaf83b179d640988dd3c","allow_loopback_plaintext":true,
        "connect_timeout_ms":50,"idle_timeout_ms":100,"wal_ack_timeout_ms":1000,"reconnect_delay_ms":10,
        "max_frame_bytes":1048576,"max_wal_bytes":8388608,"max_wal_records":1000,"max_replay_records":100,
        "max_replay_slots":32,"max_stream_bytes":8388608,"max_stream_slots":32,"max_stream_updates":1024,
        "max_manifest_bytes":1048576,"max_catalog_pools":16,"manifest_directory":"missing-public-manifests","catalog_file":"missing-catalog.json"})).unwrap();
    cfg.config
        .yellowstone
        .insert(cfg.config.chains[0].key.clone(), stream);
    let parser = Parser::new(cfg).unwrap();
    let result = parser.start().await;
    assert!(
        result.is_ok(),
        "normal stream service starts without falling back to RPC: {result:?}"
    );
    assert!(parser.current("solana:localnet").is_err());
    parser.close().await;
    drop(parser);
    std::fs::remove_dir_all(path).unwrap();
}
