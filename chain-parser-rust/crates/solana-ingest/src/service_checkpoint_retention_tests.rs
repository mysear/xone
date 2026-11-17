use super::remote_tests::Fixture;
use super::*;
use xone_kafka_transport::checkpoint::retention::Plan;
async fn preview(f: &Fixture) -> (reqwest::StatusCode, Value) {
    let response = f
        .client
        .get(format!(
            "{}/internal/chains/{}/checkpoint-retention-plan",
            f.owner, f.key
        ))
        .bearer_auth(&f.parser.config.token)
        .send()
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        serde_json::from_slice(&response.bytes().await.unwrap()).unwrap(),
    )
}
#[tokio::test]
async fn normal_rust_retention_preview_requires_auth_and_preserves_objects_without_network() {
    let f = Fixture::new_retention().await;
    let current = f.parser.stores[&f.key].lock().unwrap().bootstrap().unwrap();
    assert_eq!(
        f.client
            .get(format!(
                "{}/internal/chains/{}/checkpoint-retention-plan",
                f.owner, f.key
            ))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        preview(&f).await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(f.publish().await.0, reqwest::StatusCode::OK);
    let before = f.objects.lock().unwrap().requests.len();
    let (status, value) = preview(&f).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{value}");
    let plan: Plan = serde_json::from_value(value).unwrap();
    assert!(!plan.applied);
    assert_eq!(plan.selected_objects, 0);
    assert_eq!(plan.reference, current.snapshot.reference);
    assert_eq!(plan.cursor.next_offset, current.cursor.offset.to_string());
    assert_eq!(plan.objects.len(), 1);
    assert_eq!(plan.objects[0].reason, "CURRENT_STATE");
    assert_eq!(
        plan.objects[0].content_coverage,
        "FULL_DECODE_AND_STORE_CERTIFICATE"
    );
    let (status, value) = preview(&f).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let repeat: Plan = serde_json::from_value(value).unwrap();
    assert_eq!(plan.namespace_sha256, repeat.namespace_sha256);
    assert_eq!(
        f.objects.lock().unwrap().requests.len(),
        before,
        "read-only preview issued remote requests"
    );
    if let Ok(directory) = std::env::var("XONE_RUST_RETENTION_PLAN_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("normal-rust-retention-plan.json"),serde_json::to_vec_pretty(&serde_json::json!({"plan":plan,"repeat_namespace_sha256":repeat.namespace_sha256,"remote_requests_unchanged":true,"verification_scope":"NORMAL_RUST_OWNER_AUTHENTICATED_READ_ONLY_RETENTION_PREVIEW"})).unwrap()).unwrap();
    }
    f.parser.stores[&f.key]
        .lock()
        .unwrap()
        .set_authority_deadline(std::time::Instant::now() - Duration::from_secs(1));
    assert_eq!(
        preview(&f).await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.parser.close().await;
    assert_eq!(
        preview(&f).await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.close().await;
}
#[tokio::test]
async fn rust_retention_preview_waiter_cancels_on_close_and_preserves_publication_gate() {
    let f = Fixture::new_retention().await;
    assert_eq!(f.publish().await.0, reqwest::StatusCode::OK);
    let gate = f.parser.checkpoint_gate.clone().lock_owned().await;
    let parser = f.parser.clone();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", parser.config.token).parse().unwrap(),
    );
    let key = f.key.clone();
    let waiter = tokio::spawn(retention_plan(
        State(parser.clone()),
        headers,
        super::super::Path(key),
    ));
    tokio::task::yield_now().await;
    let closing = tokio::spawn(async move {
        parser.close().await;
    });
    let result = tokio::time::timeout(Duration::from_millis(500), waiter)
        .await
        .expect("stop must cancel queued preview without acquiring the gate")
        .unwrap();
    assert!(result.is_err());
    assert!(
        f.parser.checkpoint_gate.try_lock().is_err(),
        "cancelling another waiter released owned gate"
    );
    drop(gate);
    tokio::time::timeout(Duration::from_millis(500), closing)
        .await
        .unwrap()
        .unwrap();
    f.close().await;
}
