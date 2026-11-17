use super::*;
use xone_kafka_transport::{Cursor, Unit};
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn fixture() -> Parser {
    let mut config =
        xone_config::load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    config.config.state.path = std::env::temp_dir()
        .join(format!(
            "xone-rust-checkpoint-service-{}-{}-{}",
            std::process::id(),
            xone_observability::now_ms(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .to_string_lossy()
        .into_owned();
    config.token = "rust-checkpoint-api-test-20261118".into();
    let mut snapshot: xone_domain::Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    snapshot.reference.block.timestamp_ms = xone_observability::now_ms();
    Parser::with_source(
        config,
        Some(Arc::new(move |_: &xone_config::Chain| Ok(snapshot.clone()))),
    )
    .unwrap()
}
#[tokio::test]
async fn normal_rust_checkpoint_endpoint_requires_auth_and_committed_cursor() {
    let parser = fixture();
    let root = parser.config.resolve(&parser.config.config.state.path);
    parser.start().await.unwrap();
    let key = parser.config.config.chains[0].key.clone();
    let headers = || {
        let mut h = HeaderMap::new();
        h.insert(
            "authorization",
            format!("Bearer {}", parser.config.token).parse().unwrap(),
        );
        h
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/internal/checkpoints",
        listener.local_addr().unwrap()
    );
    let routes = parser.handler();
    let server = tokio::spawn(async move {
        axum::serve(listener, routes).await.unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    assert_eq!(
        client.post(&url).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        publish(State(parser.clone()), HeaderMap::new())
            .await
            .unwrap_err()
            .0,
        StatusCode::UNAUTHORIZED
    );
    let Json(initial) = publish(State(parser.clone()), headers()).await.unwrap();
    assert_eq!(initial["public_checkpoints"], serde_json::json!([]));
    let mut current = parser.current(&key).unwrap();
    current.reference.online_generation += 1;
    current.reference.journal_seq += 1;
    let cursor = Cursor {
        topic: "canonical.solana-localnet.v1".into(),
        partition: 0,
        offset: 9_007_199_254_741_011,
    };
    {
        let mut store = parser.stores[&key].lock().unwrap();
        store
            .prepare(&Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: current.clone(),
            })
            .unwrap();
        store.apply_prepared(cursor.clone()).unwrap();
    }
    let response = client
        .post(&url)
        .bearer_auth(&parser.config.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let response: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    let publications: Vec<local::Publication> =
        serde_json::from_value(response["public_checkpoints"].clone()).unwrap();
    assert_eq!(publications.len(), 1);
    let p = &publications[0];
    let got = local::read(
        &p.manifest_path,
        &p.manifest_sha256,
        parser.config.config.journal.max_payload_bytes,
    )
    .unwrap();
    assert_eq!(got.cursor, cursor);
    assert_eq!(got.snapshot.reference, current.reference);
    if let Ok(directory) = std::env::var("XONE_RUST_OWNER_CHECKPOINT_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("normal-rust-checkpoint-http.json"),serde_json::to_vec_pretty(&serde_json::json!({"response":response,"manifest_raw":std::fs::read_to_string(&p.manifest_path).unwrap(),"payload_hex":hex::encode(std::fs::read(p.manifest_path.with_file_name("bootstrap.pb")).unwrap()),"reference":current.reference,"cursor":cursor,"http_status":200,"verification_scope":"NORMAL_RUST_PARSER_CONTROLLED_SOURCE_COMMITTED_CURSOR_HTTP"})).unwrap()).unwrap();
    }
    let Json(again) = publish(State(parser.clone()), headers()).await.unwrap();
    assert_eq!(response, again);
    parser.close().await;
    assert_eq!(
        publish(State(parser.clone()), headers())
            .await
            .unwrap_err()
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    server.abort();
    let _ = server.await;
    drop(parser);
    std::fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn rust_checkpoint_gate_wait_is_bounded_and_cancelled_before_file_io() {
    let mut parser = fixture();
    let root = parser.config.resolve(&parser.config.config.state.path);
    Arc::get_mut(&mut parser.config)
        .unwrap()
        .config
        .journal
        .request_timeout_ms = 30;
    parser.start().await.unwrap();
    let key = parser.config.config.chains[0].key.clone();
    let mut current = parser.current(&key).unwrap();
    current.reference.online_generation += 1;
    current.reference.journal_seq += 1;
    {
        let mut store = parser.stores[&key].lock().unwrap();
        store
            .prepare(&Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: current,
            })
            .unwrap();
        store
            .apply_prepared(Cursor {
                topic: "test.rust.checkpoint".into(),
                partition: 0,
                offset: 7,
            })
            .unwrap();
    }
    let held = parser.checkpoint_gate.lock().await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", parser.config.token).parse().unwrap(),
    );
    let result = publish(State(parser.clone()), headers).await.unwrap_err();
    assert_eq!(
        result.1 .0["error"],
        serde_json::json!("CHECKPOINT_DEADLINE")
    );
    assert!(!root.join("checkpoints").exists());
    drop(held);
    parser.close().await;
    drop(parser);
    std::fs::remove_dir_all(root).unwrap();
}
