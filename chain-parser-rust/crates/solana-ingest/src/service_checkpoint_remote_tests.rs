use super::*;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, Uri},
    response::IntoResponse,
    Router,
};
use std::{collections::BTreeMap, sync::Mutex};
use xone_kafka_transport::{Cursor, Unit};
const OBJECT_TOKEN: &str = "rust-object-token-only-20261118";
#[derive(Default)]
pub(super) struct Objects {
    pub(super) failed_id: Option<String>,
    active: usize,
    pub(super) max_active: usize,
    pub(super) data: BTreeMap<String, Vec<u8>>,
    pub(super) requests: Vec<String>,
    pub(super) mode: String,
}
struct RequestActive(Arc<Mutex<Objects>>);
impl Drop for RequestActive {
    fn drop(&mut self) {
        self.0.lock().unwrap().active -= 1;
    }
}
type ObjectState = (
    Arc<Mutex<Objects>>,
    Arc<tokio::sync::Notify>,
    Arc<tokio::sync::Notify>,
);
struct Task(Option<tokio::task::JoinHandle<()>>);
impl Drop for Task {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort()
        }
    }
}
impl Task {
    async fn close(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}
pub(super) struct Fixture {
    pub(super) parser: Parser,
    pub(super) objects: Arc<Mutex<Objects>>,
    pub(super) entered: Arc<tokio::sync::Notify>,
    pub(super) release: Arc<tokio::sync::Notify>,
    object_task: Task,
    owner_task: Task,
    root: std::path::PathBuf,
    pub(super) client: reqwest::Client,
    pub(super) owner: String,
    pub(super) base: String,
    pub(super) key: String,
}
impl Fixture {
    async fn new() -> Self {
        Self::new_configured(0, false, "").await
    }
    pub(super) async fn new_configured(interval: u64, multi: bool, initial_mode: &str) -> Self {
        Self::new_options(interval, multi, initial_mode, None).await
    }
    pub(super) async fn new_budget(interval: u64, budget: xone_config::CheckpointBudget) -> Self {
        Self::new_options(interval, false, "", Some(budget)).await
    }
    async fn new_options(
        interval: u64,
        multi: bool,
        initial_mode: &str,
        budget: Option<xone_config::CheckpointBudget>,
    ) -> Self {
        let objects = Arc::new(Mutex::new(Objects {
            mode: initial_mode.into(),
            ..Objects::default()
        }));
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/bucket", listener.local_addr().unwrap());
        let state = (objects.clone(), entered.clone(), release.clone());
        let routes = Router::new()
            .fallback(axum::routing::any(
                |State((objects, entered, release)): State<ObjectState>,
                 uri: Uri,
                 method: axum::http::Method,
                 headers: HeaderMap,
                 body: Bytes| async move {
                    assert_eq!(
                        headers.get("authorization").and_then(|v| v.to_str().ok()),
                        Some(format!("Bearer {OBJECT_TOKEN}").as_str())
                    );
                    let _active = {
                        let mut state = objects.lock().unwrap();
                        state.active += 1;
                        state.max_active = state.max_active.max(state.active);
                        RequestActive(objects.clone())
                    };
                    let (status, mut data, mode) = {
                        let mut state = objects.lock().unwrap();
                        let path = uri.path();
                        state.requests.push(format!("{method} {path}"));
                        let mode = if state
                            .failed_id
                            .as_ref()
                            .is_some_and(|id| path.contains(&format!("/{id}/")))
                        {
                            "fail-all".to_owned()
                        } else {
                            state.mode.clone()
                        };
                        if mode == "fail-all" {
                            (StatusCode::SERVICE_UNAVAILABLE, vec![], mode)
                        } else if method == axum::http::Method::GET {
                            match state.data.get(path) {
                                Some(raw) => (StatusCode::OK, raw.clone(), mode),
                                None => (StatusCode::NOT_FOUND, vec![], mode),
                            }
                        } else if method == axum::http::Method::PUT {
                            assert_eq!(
                                headers.get("if-none-match").and_then(|v| v.to_str().ok()),
                                Some("*")
                            );
                            if mode == "fail-manifest" && path.ends_with("manifest.json") {
                                (StatusCode::SERVICE_UNAVAILABLE, vec![], mode)
                            } else if state.data.contains_key(path) {
                                (StatusCode::PRECONDITION_FAILED, vec![], mode)
                            } else {
                                if !(mode == "lost-payload" && path.ends_with("bootstrap.pb")) {
                                    state.data.insert(path.into(), body.to_vec());
                                }
                                (StatusCode::CREATED, vec![], mode)
                            }
                        } else {
                            (StatusCode::METHOD_NOT_ALLOWED, vec![], mode)
                        }
                    };
                    if mode == "hold"
                        || (mode == "pause-payload"
                            && method == axum::http::Method::GET
                            && status == StatusCode::OK
                            && uri.path().ends_with("bootstrap.pb"))
                    {
                        entered.notify_one();
                        release.notified().await;
                    }
                    if method == axum::http::Method::GET && uri.path().ends_with("bootstrap.pb") {
                        if mode == "redirect" {
                            return (
                                StatusCode::FOUND,
                                [("location", "/unconfigured-target")],
                                Vec::<u8>::new(),
                            )
                                .into_response();
                        }
                        if mode == "encoded" {
                            return (status, [("content-encoding", "gzip")], data).into_response();
                        }
                        if mode == "corrupt-read" && !data.is_empty() {
                            data[0] ^= 1;
                        }
                    }
                    (status, data).into_response()
                },
            ))
            .with_state(state);
        let object_task = Task(Some(tokio::spawn(async move {
            axum::serve(listener, routes).await.unwrap();
        })));
        let parser = super::tests::fixture_configured(|snapshot| {
            snapshot.config.journal.checkpoint_store = Some(xone_config::CheckpointStore {
                local_budget: budget.clone(),
                base_url: base.clone(),
                allow_plaintext_development: true,
                publish_interval_ms: interval,
            });
            snapshot
                .config
                .journal
                .source_auth_tokens
                .insert(base.clone(), OBJECT_TOKEN.into());
            if multi {
                let mut second = snapshot.config.chains[0].clone();
                second.key = "solana:other".into();
                snapshot.config.chains.push(second);
            }
        });
        let root = parser.config.resolve(&parser.config.config.state.path);
        parser.start().await.unwrap();
        let key = parser.config.config.chains[0].key.clone();
        let mut snapshot = parser.current(&key).unwrap();
        snapshot.reference.online_generation += 1;
        snapshot.reference.journal_seq += 1;
        {
            let mut store = parser.stores[&key].lock().unwrap();
            store
                .prepare(&Unit {
                    normalized_facts: None,
                    kind: "ONLINE_BASELINE_COMMIT".into(),
                    snapshot,
                })
                .unwrap();
            store
                .apply_prepared(Cursor {
                    topic: "canonical.solana-localnet.v1".into(),
                    partition: 0,
                    offset: 9_007_199_254_741_013,
                })
                .unwrap();
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let owner = format!("http://{}", listener.local_addr().unwrap());
        let routes = parser.handler();
        let owner_task = Task(Some(tokio::spawn(async move {
            axum::serve(listener, routes).await.unwrap();
        })));
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        Self {
            parser,
            objects,
            entered,
            release,
            object_task,
            owner_task,
            root,
            client,
            owner,
            base,
            key,
        }
    }
    pub(super) async fn publish(&self) -> (reqwest::StatusCode, Value) {
        let response = self
            .client
            .post(format!("{}/internal/checkpoints", self.owner))
            .bearer_auth(&self.parser.config.token)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let raw = response.bytes().await.unwrap();
        (status, serde_json::from_slice(&raw).unwrap())
    }
    pub(super) async fn catalog(&self) -> (reqwest::StatusCode, Value) {
        let response = self
            .client
            .get(format!(
                "{}/internal/chains/{}/checkpoint-catalog",
                self.owner, self.key
            ))
            .bearer_auth(&self.parser.config.token)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (
            status,
            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap(),
        )
    }
    pub(super) async fn close(self) {
        self.parser.close().await;
        self.owner_task.close().await;
        self.object_task.close().await;
        drop(self.parser);
        std::fs::remove_dir_all(self.root).unwrap();
    }
}
#[tokio::test]
async fn normal_rust_remote_checkpoint_half_upload_retry_and_current_catalog_are_bound() {
    let f = Fixture::new().await;
    let before = f.parser.stores[&f.key].lock().unwrap().bootstrap().unwrap();
    assert_eq!(
        f.client
            .get(format!(
                "{}/internal/chains/{}/checkpoint-catalog",
                f.owner, f.key
            ))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.catalog().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.objects.lock().unwrap().mode = "fail-manifest".into();
    let (status, _) = f.publish().await;
    assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(f.objects.lock().unwrap().data.len(), 1);
    assert_eq!(
        f.catalog().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.objects.lock().unwrap().mode.clear();
    let (status, response) = f.publish().await;
    assert_eq!(status, reqwest::StatusCode::OK, "{response}");
    let remote: Vec<RemotePublication> =
        serde_json::from_value(response["remote_checkpoints"].clone()).unwrap();
    assert_eq!(remote.len(), 1);
    assert!(remote[0]
        .manifest_url
        .starts_with(&format!("{}/public-v2/", f.base)));
    assert_eq!(remote[0].manifest.cursor.next_offset, "9007199254741013");
    let (status, catalog) = f.catalog().await;
    assert_eq!(status, reqwest::StatusCode::OK, "{catalog}");
    assert_eq!(catalog, response["remote_checkpoints"][0]);
    let count = f.objects.lock().unwrap().requests.len();
    let (status, again) = f.publish().await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(response, again);
    {
        let state = f.objects.lock().unwrap();
        assert_eq!(state.data.len(), 2);
        assert!(!state.requests[count..]
            .iter()
            .any(|r| r.starts_with("PUT ")));
    }
    let after = f.parser.stores[&f.key].lock().unwrap().bootstrap().unwrap();
    assert_eq!(after.cursor, before.cursor);
    assert_eq!(after.snapshot.reference, before.snapshot.reference);
    if let Ok(directory) = std::env::var("XONE_RUST_REMOTE_CHECKPOINT_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        let p: local::Publication =
            serde_json::from_value(response["public_checkpoints"][0].clone()).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("normal-rust-remote-checkpoint.json"),serde_json::to_vec_pretty(&serde_json::json!({"response":response,"catalog":catalog,"manifest_raw":std::fs::read_to_string(&p.manifest_path).unwrap(),"payload_hex":hex::encode(std::fs::read(p.manifest_path.with_file_name("bootstrap.pb")).unwrap()),"reference":after.snapshot.reference,"cursor":after.cursor,"checks":["half_upload_unqualified","conditional_retry_idempotent","current_catalog_bound","owner_cursor_unchanged"],"verification_scope":"NORMAL_RUST_OWNER_AND_SEPARATE_HTTP_OBJECT_API"})).unwrap()).unwrap();
    }
    {
        let mut state = f.parser.stores[&f.key].lock().unwrap();
        let mut changed = state.current().unwrap();
        changed.reference.online_generation += 1;
        changed.reference.journal_seq += 1;
        state
            .prepare(&Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: changed,
            })
            .unwrap();
        state
            .apply_prepared(Cursor {
                topic: before.cursor.topic,
                partition: 0,
                offset: before.cursor.offset + 4,
            })
            .unwrap();
    }
    assert_eq!(
        f.catalog().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.close().await;
}
#[tokio::test]
async fn rust_remote_checkpoint_false_ack_corruption_redirect_and_encoding_never_certify() {
    for mode in ["lost-payload", "corrupt-read", "redirect", "encoded"] {
        let f = Fixture::new().await;
        f.objects.lock().unwrap().mode = mode.into();
        assert_eq!(
            f.publish().await.0,
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "{mode}"
        );
        assert_eq!(
            f.catalog().await.0,
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "{mode}"
        );
        {
            let state = f.objects.lock().unwrap();
            assert!(!state.data.keys().any(|key| key.ends_with("manifest.json")));
            assert!(!state
                .requests
                .iter()
                .any(|key| key.contains("unconfigured-target")));
        }
        f.close().await;
    }
}
#[tokio::test]
async fn rust_remote_checkpoint_source_change_between_payload_and_manifest_revokes_publication() {
    let f = Fixture::new().await;
    f.objects.lock().unwrap().mode = "pause-payload".into();
    let parser = f.parser.clone();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", parser.config.token).parse().unwrap(),
    );
    let pending = tokio::spawn(publish(State(parser), headers));
    tokio::time::timeout(Duration::from_secs(2), f.entered.notified())
        .await
        .unwrap();
    {
        let mut state = f.parser.stores[&f.key].lock().unwrap();
        let mut snapshot = state.current().unwrap();
        snapshot.reference.online_generation += 1;
        snapshot.reference.journal_seq += 1;
        state
            .prepare(&Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot,
            })
            .unwrap();
        state
            .apply_prepared(Cursor {
                topic: "canonical.solana-localnet.v1".into(),
                partition: 0,
                offset: 9007199254741021,
            })
            .unwrap();
    }
    f.release.notify_one();
    assert_eq!(
        pending.await.unwrap().unwrap_err().0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    {
        let state = f.objects.lock().unwrap();
        assert_eq!(state.data.len(), 1);
        assert!(!state.data.keys().any(|key| key.ends_with("manifest.json")));
    }
    assert_eq!(
        f.catalog().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.close().await;
}
#[tokio::test]
async fn rust_remote_checkpoint_close_interrupts_pending_http_before_manifest_and_drains_gate() {
    let f = Fixture::new().await;
    f.objects.lock().unwrap().mode = "hold".into();
    let parser = f.parser.clone();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", parser.config.token).parse().unwrap(),
    );
    let pending = tokio::spawn(publish(State(parser), headers));
    tokio::time::timeout(Duration::from_secs(2), f.entered.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_millis(500), f.parser.close())
        .await
        .expect("Close must cancel HTTP without waiting request timeout");
    assert_eq!(
        pending.await.unwrap().unwrap_err().0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(f.objects.lock().unwrap().data.is_empty());
    f.release.notify_waiters();
    f.close().await;
}
