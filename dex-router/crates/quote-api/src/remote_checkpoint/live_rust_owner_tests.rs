use super::*;
use serde_json::{json, Value};
use std::path::PathBuf;
#[tokio::test]
#[ignore = "requires this run's live normal Rust owner, object HTTP API and owned Kafka"]
async fn actual_rust_owner_remote_checkpoint_replays_into_independent_router_and_quotes_attested_head(
) {
    let directory = PathBuf::from(
        std::env::var("XONE_RUST_LIVE_RECOVERY_DIRECTORY").expect("current live owner required"),
    );
    let ready: Value =
        serde_json::from_slice(&std::fs::read(directory.join("ready.json")).unwrap()).unwrap();
    let source = ready["owner_url"].as_str().unwrap();
    let chain = ready["chain_key"].as_str().unwrap();
    let object_base = ready["object_base"].as_str().unwrap();
    let target: xone_domain::SnapshotRef =
        serde_json::from_value(ready["target_reference"].clone()).unwrap();
    let cursor: xone_kafka_transport::Cursor =
        serde_json::from_value(ready["target_cursor"].clone()).unwrap();
    let checkpoint: xone_domain::SnapshotRef =
        serde_json::from_value(ready["checkpoint_reference"].clone()).unwrap();
    assert!(
        target.journal_seq > checkpoint.journal_seq,
        "live fixture must require tail replay"
    );
    let mut cfg = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    cfg.config.chains.retain(|c| c.key == chain);
    cfg.config.journal.transport = "kafka".into();
    cfg.config.journal.kafka.brokers = vec![ready["broker"].as_str().unwrap().into()];
    cfg.config
        .journal
        .kafka
        .topics
        .insert(chain.into(), ready["topic"].as_str().unwrap().into());
    cfg.config.journal.source_urls = vec![source.into()];
    cfg.config.journal.checkpoint_store = Some(xone_config::CheckpointStore {
        base_url: object_base.into(),
        allow_plaintext_development: true,
        publish_interval_ms: 0,
    });
    cfg.config
        .journal
        .source_auth_tokens
        .insert(source.into(), ready["owner_token"].as_str().unwrap().into());
    cfg.config.journal.source_auth_tokens.insert(
        object_base.into(),
        ready["object_token"].as_str().unwrap().into(),
    );
    let api = Api::new(cfg).unwrap();
    assert!(api.replica.view(chain).is_err());
    let (staged, reader) = api.replay_remote_checkpoint(source, chain).await.unwrap();
    assert_eq!(staged.snapshot.reference, target);
    assert_eq!(staged.cursor, cursor);
    assert!(reader.at_committed_boundary());
    assert!(api.replica.view(chain).is_err());
    drop(reader);
    api.sync_sources().await;
    assert_eq!(api.replica.view(chain).unwrap().reference, target);
    assert_eq!(
        api.applied_cursors.lock().unwrap().get(chain).unwrap().1,
        cursor
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/quote", listener.local_addr().unwrap());
    let routes = api.handler();
    let server = tokio::spawn(async move {
        axum::serve(listener, routes).await.unwrap();
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let response=client.post(&url).bearer_auth(&api.config.token).json(&json!({"chainKey":chain,"kind":"EXACT_IN","sellToken":"A","buyToken":"B","sellAmount":"1000","caller":"alice","recipient":"alice","slippageBps":100,"verification":"FAST"})).send().await.unwrap();
    let status = response.status();
    let quote: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK, "{quote}");
    assert_eq!(quote["snapshotRef"], serde_json::to_value(&target).unwrap());
    assert_eq!(quote["verification"]["level"], json!("INDICATIVE"));
    server.abort();
    let _ = server.await;
    // Exact-current remote catalogue must be unavailable for the intentionally old object.
    assert!(api.remote_bootstrap(source, chain).await.is_err());
    // Removing source credentials prevents recovery; a downloaded file by itself has no authority.
    let mut isolated = api.config.as_ref().clone();
    isolated.config.journal.source_auth_tokens.clear();
    let isolated = Api::new(isolated).unwrap();
    isolated.sync_sources().await;
    assert!(isolated.replica.view(chain).is_err());
    std::fs::write(directory.join("independent-router.json"),serde_json::to_vec_pretty(&json!({"checkpoint_reference":checkpoint,"target_reference":target,"target_cursor":cursor,"quote":quote,"checks":["actual_normal_rust_owner_http","actual_separate_object_http","actual_kafka_two_baseline_tail","private_stage_unpublished","normal_sync_sources_final_head","normal_router_http_quote","old_object_not_exact_current","missing_owner_credentials_rejected"],"verification_scope":"TWO_INDEPENDENT_TEST_PROCESSES_NORMAL_RUST_OWNER_AND_ROUTER_OWNED_KAFKA_CONTROLLED_SOLANA_SOURCE"})).unwrap()).unwrap();
}
