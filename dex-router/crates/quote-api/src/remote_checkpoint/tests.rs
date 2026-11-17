use super::*;
use crate::{HeaderMap, QuoteRequest, Router, StatusCode};
use axum::{body::Body, extract::State, http::Uri, response::IntoResponse};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use xone_domain::{Amount, Snapshot};
use xone_kafka_transport::{Cursor, Unit};
fn sha(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
pub(super) fn boot() -> Bootstrap {
    let mut snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../../test/fixtures/evm/baseline.json")).unwrap();
    snapshot.reference.block.timestamp_ms = xone_observability::now_ms();
    Bootstrap {
        snapshot,
        cursor: Cursor {
            topic: "test.remote-checkpoint".into(),
            partition: 0,
            offset: 9_007_199_254_741_009,
        },
    }
}
pub(super) struct Capture {
    pub(super) recovery: Option<Vec<u8>>,
    pub(super) recovery_reads: usize,
    pub(super) catalog: Vec<u8>,
    pub(super) manifest: Vec<u8>,
    pub(super) payload: Vec<u8>,
    pub(super) mode: String,
    pub(super) requests: Vec<String>,
    pub(super) catalog_reads: usize,
    pub(super) fallback: Option<Vec<u8>>,
}
pub(super) struct Fixture {
    pub(super) api: Api,
    pub(super) owner: String,
    pub(super) state: Arc<Mutex<Capture>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub(super) async fn fixture(boot: Bootstrap) -> Fixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let owner = format!("{base}/owner");
    let store = format!("{base}/objects");
    let payload = xone_kafka_transport::encode_bootstrap(&boot).unwrap();
    let manifest = checkpoint::Manifest {
        schema_version: 2,
        codec: checkpoint::CODEC.into(),
        reference: boot.snapshot.reference.clone(),
        cursor: checkpoint::ManifestCursor {
            topic: boot.cursor.topic.clone(),
            partition: 0,
            next_offset: boot.cursor.offset.to_string(),
        },
        payload: "bootstrap.pb".into(),
        sha256: sha(&payload),
        size: payload.len() as u64,
    };
    let raw = serde_json::to_vec(&manifest).unwrap();
    let catalog = Catalog {
        manifest_url: format!("{store}/public-v2/{}/manifest.json", manifest.sha256),
        manifest_sha256: sha(&raw),
        manifest,
    };
    let state = Arc::new(Mutex::new(Capture {
        recovery: None,
        recovery_reads: 0,
        catalog: serde_json::to_vec(&catalog).unwrap(),
        manifest: raw,
        payload,
        mode: String::new(),
        requests: vec![],
        catalog_reads: 0,
        fallback: None,
    }));
    let routes = Router::new()
        .fallback(axum::routing::get(
            |State(state): State<Arc<Mutex<Capture>>>, uri: Uri, headers: HeaderMap| async move {
                let (status, body, mode) = {
                    let mut s = state.lock().unwrap();
                    let path = uri.path();
                    s.requests.push(path.into());
                    let expected = if path.starts_with("/owner/") {
                        "Bearer owner-only-token-20261117"
                    } else {
                        "Bearer object-only-token-20261117"
                    };
                    assert_eq!(
                        headers.get("authorization").and_then(|h| h.to_str().ok()),
                        Some(expected)
                    );
                    let mode = s.mode.clone();
                    if path.ends_with("checkpoint-recovery-catalog") {
                        s.recovery_reads += 1;
                        if mode == "replay-revoked" && s.recovery_reads > 1 {
                            (StatusCode::SERVICE_UNAVAILABLE, vec![], mode)
                        } else if let Some(raw) = &s.recovery {
                            (StatusCode::OK, raw.clone(), mode)
                        } else {
                            (StatusCode::NOT_FOUND, vec![], mode)
                        }
                    } else if path.ends_with("checkpoint-catalog") {
                        s.catalog_reads += 1;
                        if mode == "revoked" && s.catalog_reads > 1 {
                            (StatusCode::SERVICE_UNAVAILABLE, vec![], mode)
                        } else {
                            (StatusCode::OK, s.catalog.clone(), mode)
                        }
                    } else if path.ends_with("/bootstrap") {
                        match &s.fallback {
                            Some(raw) => (StatusCode::OK, raw.clone(), mode),
                            None => (StatusCode::SERVICE_UNAVAILABLE, vec![], mode),
                        }
                    } else if path.ends_with("/manifest.json") {
                        (StatusCode::OK, s.manifest.clone(), mode)
                    } else if path.ends_with("/bootstrap.pb") {
                        let mut data = s.payload.clone();
                        match mode.as_str() {
                            "corrupt" => data[0] ^= 1,
                            "oversized" => data.push(1),
                            "short" => {
                                data.pop();
                            }
                            _ => {}
                        }
                        if mode == "missing" {
                            (StatusCode::NOT_FOUND, vec![], mode)
                        } else {
                            (StatusCode::OK, data, mode)
                        }
                    } else {
                        (StatusCode::NOT_FOUND, vec![], mode)
                    }
                };
                if uri.path().ends_with("bootstrap.pb") {
                    if mode == "slow" {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    if mode == "redirect" {
                        return (
                            StatusCode::FOUND,
                            [("location", "http://127.0.0.1:1/other")],
                            Body::empty(),
                        )
                            .into_response();
                    }
                    if mode == "encoded" {
                        return (StatusCode::OK, [("content-encoding", "gzip")], body)
                            .into_response();
                    }
                }
                (status, body).into_response()
            },
        ))
        .with_state(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, routes).await.unwrap();
    });
    let mut config = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    config.config.journal.checkpoint_store = Some(xone_config::CheckpointStore {
        publish_interval_ms: 0,
        base_url: store.clone(),
        allow_plaintext_development: true,
    });
    config
        .config
        .journal
        .source_auth_tokens
        .insert(owner.clone(), "owner-only-token-20261117".into());
    config
        .config
        .journal
        .source_auth_tokens
        .insert(store, "object-only-token-20261117".into());
    config
        .config
        .journal
        .kafka
        .topics
        .insert("evm:31337".into(), boot.cursor.topic);
    config.config.journal.source_urls = vec![owner.clone()];
    let api = Api::new(config).unwrap();
    Fixture {
        api,
        owner,
        state,
        task,
    }
}
#[tokio::test]
async fn remote_checkpoint_download_uses_exact_configured_credentials_and_reattests() {
    let expected = boot();
    let f = fixture(expected.clone()).await;
    let downloaded = f.api.remote_bootstrap(&f.owner, "evm:31337").await.unwrap();
    assert_eq!(downloaded.snapshot.reference, expected.snapshot.reference);
    assert_eq!(downloaded.cursor, expected.cursor);
    let s = f.state.lock().unwrap();
    assert_eq!(s.catalog_reads, 2);
    assert_eq!(s.requests.len(), 4);
    assert!(s.requests[1].ends_with("manifest.json"));
    assert!(s.requests[2].ends_with("bootstrap.pb"));
    assert!(
        f.api.replica.view("evm:31337").is_err(),
        "downloading alone must not activate state"
    );
}
#[tokio::test]
async fn remote_checkpoint_rejects_payload_failures_and_download_revocation() {
    for mode in [
        "corrupt",
        "oversized",
        "short",
        "missing",
        "redirect",
        "encoded",
        "revoked",
        "slow",
    ] {
        let mut f = fixture(boot()).await;
        f.state.lock().unwrap().mode = mode.into();
        if mode == "slow" {
            Arc::get_mut(&mut f.api.config)
                .unwrap()
                .config
                .journal
                .request_timeout_ms = 80;
        }
        assert!(
            f.api.remote_bootstrap(&f.owner, "evm:31337").await.is_err(),
            "{mode}"
        );
        assert!(f.api.replica.view("evm:31337").is_err());
    }
}
#[tokio::test]
async fn remote_checkpoint_rejects_catalogue_authority_location_and_limits_before_object_io() {
    for mode in [
        "pin",
        "location",
        "owner",
        "chain",
        "assignment",
        "size",
        "cursor",
    ] {
        let f = fixture(boot()).await;
        {
            let mut s = f.state.lock().unwrap();
            let mut c: Value = serde_json::from_slice(&s.catalog).unwrap();
            match mode {
                "pin" => c["manifest_sha256"] = json!("a".repeat(64)),
                "location" => {
                    c["manifest_url"] = json!("https://unconfigured.example/manifest.json")
                }
                "owner" => c["manifest"]["reference"]["owner_service"] = json!("chain-parser-rust"),
                "chain" => c["manifest"]["reference"]["block"]["chain_key"] = json!("evm:1"),
                "assignment" => c["manifest"]["reference"]["owner_assignment_version"] = json!(2),
                "size" => c["manifest"]["size"] = json!(checkpoint::MAX_BYTES + 1),
                _ => c["manifest"]["cursor"]["topic"] = json!("wrong.topic"),
            }
            if !["pin", "location"].contains(&mode) {
                let changed: checkpoint::Manifest =
                    serde_json::from_value(c["manifest"].clone()).unwrap();
                c["manifest_sha256"] = json!(sha(&serde_json::to_vec(&changed).unwrap()));
            }
            s.catalog = serde_json::to_vec(&c).unwrap();
        }
        assert!(
            f.api.remote_bootstrap(&f.owner, "evm:31337").await.is_err(),
            "{mode}"
        );
        assert_eq!(f.state.lock().unwrap().requests.len(), 1, "{mode}");
    }
}
#[tokio::test]
async fn remote_checkpoint_failure_can_select_current_owner_bootstrap_without_activating_it() {
    let expected = boot();
    let f = fixture(expected.clone()).await;
    {
        let mut s = f.state.lock().unwrap();
        s.mode = "corrupt".into();
        s.fallback = Some(xone_kafka_transport::encode_bootstrap(&expected).unwrap());
    }
    let (got, source) = f
        .api
        .recovery_bootstrap(&f.owner, "evm:31337")
        .await
        .unwrap();
    assert_eq!(source, "OWNER_CHECKPOINT_AND_KAFKA");
    assert_eq!(got.cursor, expected.cursor);
    assert!(f.api.replica.view("evm:31337").is_err());
}
#[tokio::test]
#[ignore = "requires the current validation run's independently exported Go vectors"]
async fn remote_checkpoint_downloads_current_go_wire_without_reserializing_payload() {
    let path =
        std::env::var("XONE_CHECKPOINT_VECTORS_OUTPUT").expect("current Go exports required");
    let document: Value = serde_json::from_slice(
        &std::fs::read(std::path::Path::new(&path).join("vectors.json")).unwrap(),
    )
    .unwrap();
    for v in document["vectors"].as_array().unwrap() {
        let raw = v["manifest"].as_str().unwrap().as_bytes();
        let manifest: checkpoint::Manifest = serde_json::from_slice(raw).unwrap();
        let payload = hex::decode(v["payload_hex"].as_str().unwrap()).unwrap();
        let boot = checkpoint::decode(
            raw,
            &payload,
            v["manifest_sha256"].as_str().unwrap(),
            8 << 20,
        )
        .unwrap();
        let f = fixture(boot.clone()).await;
        {
            let mut s = f.state.lock().unwrap();
            let mut c: Catalog = serde_json::from_slice(&s.catalog).unwrap();
            c.manifest = manifest;
            c.manifest_sha256 = v["manifest_sha256"].as_str().unwrap().into();
            s.catalog = serde_json::to_vec(&c).unwrap();
            s.manifest = raw.to_vec();
            s.payload = payload;
        }
        let got = f.api.remote_bootstrap(&f.owner, "evm:31337").await.unwrap();
        assert_eq!(got.cursor, boot.cursor);
        assert_eq!(got.snapshot.reference, boot.snapshot.reference);
    }
}
#[tokio::test]
#[ignore = "requires explicitly owned Kafka; never counts as runtime proof without it"]
async fn remote_checkpoint_normal_router_cold_restore_and_quote_with_actual_kafka() {
    let broker = std::env::var("XONE_TEST_KAFKA_BROKER").expect("owned Kafka required");
    let mut boot = boot();
    let topic = "test.remote-checkpoint";
    let mut producer = xone_kafka_transport::KafkaProducer::open(
        std::slice::from_ref(&broker),
        topic,
        "remote-checkpoint-writer",
        1024,
        8 << 20,
        Duration::from_secs(5),
    )
    .unwrap();
    boot.cursor = producer
        .publish(
            &Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: boot.snapshot.clone(),
            },
            || Ok(()),
        )
        .unwrap();
    let mut f = fixture(boot.clone()).await;
    {
        let config = &mut Arc::get_mut(&mut f.api.config).unwrap().config;
        config.chains.retain(|c| c.key == "evm:31337");
        config.journal.transport = "kafka".into();
        config.journal.kafka.brokers = vec![broker];
    }
    f.api.sync_sources().await;
    assert_eq!(
        f.api.replica.view("evm:31337").unwrap().reference,
        boot.snapshot.reference
    );
    assert_eq!(
        f.api
            .applied_cursors
            .lock()
            .unwrap()
            .get("evm:31337")
            .unwrap()
            .1,
        boot.cursor
    );
    assert_eq!(f.state.lock().unwrap().catalog_reads, 2);
    use tower::ServiceExt;
    let request = QuoteRequest {
        chain_key: "evm:31337".into(),
        kind: "EXACT_IN".into(),
        sell_token: "A".into(),
        buy_token: "B".into(),
        sell_amount: Amount::checked(1000u32.into()).unwrap(),
        caller: "alice".into(),
        recipient: "alice".into(),
        slippage_bps: 100,
        verification: "FAST".into(),
        constraints: None,
    };
    let token = f.api.config.token.clone();
    let response = f
        .api
        .handler()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/quote")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let quote: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(status, StatusCode::OK, "{quote}");
    if let Ok(directory) = std::env::var("XONE_REMOTE_CHECKPOINT_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("kafka-cold-recovery.json"),serde_json::to_vec_pretty(&json!({"snapshot_ref":boot.snapshot.reference,"cursor":boot.cursor,"quote":quote,"requests":f.state.lock().unwrap().requests,"owner_bootstrap_used":false,"empty_router_memory_at_start":true,"verification_scope":"LOCAL_API_REAL_KAFKA_CONFIGURED_HTTP_OBJECTS"})).unwrap()).unwrap();
    }
    assert_eq!(
        quote["snapshotRef"]["journal_seq"],
        json!(boot.snapshot.reference.journal_seq)
    );
    f.state.lock().unwrap().mode = "corrupt".into();
    f.api.replica.suspend("evm:31337");
    f.api.readers.lock().unwrap().clear();
    f.api.applied_cursors.lock().unwrap().clear();
    f.api.sync_sources().await;
    assert!(f.api.replica.view("evm:31337").is_err());
}
