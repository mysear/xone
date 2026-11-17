use super::tests::{boot, fixture};
use super::*;
use serde_json::{json, Value};
use std::sync::Arc;
use xone_kafka_transport::{Cursor, KafkaProducer, Unit};
fn catalog(boot: &Bootstrap, head: xone_domain::SnapshotRef, cursor: Cursor) -> RecoveryCatalog {
    let payload = xone_kafka_transport::encode_bootstrap(boot).unwrap();
    use sha2::{Digest, Sha256};
    let m = checkpoint::Manifest {
        schema_version: 2,
        codec: checkpoint::CODEC.into(),
        reference: boot.snapshot.reference.clone(),
        cursor: checkpoint::ManifestCursor {
            topic: boot.cursor.topic.clone(),
            partition: 0,
            next_offset: boot.cursor.offset.to_string(),
        },
        payload: "bootstrap.pb".into(),
        sha256: hex::encode(Sha256::digest(&payload)),
        size: payload.len() as u64,
    };
    let raw = serde_json::to_vec(&m).unwrap();
    RecoveryCatalog {
        schema_version: 1,
        checkpoint: Catalog {
            manifest_url: String::new(),
            manifest_sha256: hex::encode(Sha256::digest(&raw)),
            manifest: m,
        },
        head_reference: head,
        head_cursor: checkpoint::ManifestCursor {
            topic: cursor.topic,
            partition: 0,
            next_offset: cursor.offset.to_string(),
        },
    }
}
#[test]
fn recovery_catalog_target_preserves_wide_cursor_and_rejects_era_and_head_confusion() {
    let boot = boot();
    let mut head = boot.snapshot.reference.clone();
    head.journal_seq += 1;
    head.block.number_or_slot += 1;
    head.block.parent_hash = head.block.hash.clone();
    head.block.hash = format!("0x{}", "3".repeat(64));
    let mut cursor = boot.cursor.clone();
    cursor.offset += 4;
    let c = catalog(&boot, head, cursor.clone());
    assert_eq!(recovery_target(&c, 8 << 20, 100).unwrap(), cursor);
    for mode in [
        "writer",
        "canonical",
        "generation",
        "scope",
        "adapter",
        "owner",
        "assignment",
        "topic",
        "offset",
        "over-budget",
        "seq",
        "same-height",
        "pin",
        "hash",
    ] {
        let mut c = c.clone();
        let mut limit = 100;
        match mode {
            "writer" => c.head_reference.writer_epoch += 1,
            "canonical" => c.head_reference.canonical_epoch += 1,
            "generation" => c.head_reference.online_generation += 1,
            "scope" => c.head_reference.scope_hash = "a".repeat(64),
            "adapter" => c.head_reference.adapter_set_hash = "a".repeat(64),
            "owner" => c.head_reference.owner_service = "chain-parser-rust".into(),
            "assignment" => c.head_reference.owner_assignment_version += 1,
            "topic" => c.head_cursor.topic = "wrong.topic".into(),
            "offset" => c.head_cursor.next_offset = boot.cursor.offset.to_string(),
            "over-budget" => limit = 3,
            "seq" => c.head_reference.journal_seq = boot.snapshot.reference.journal_seq,
            "same-height" => {
                c.head_reference.block.number_or_slot = boot.snapshot.reference.block.number_or_slot
            }
            "pin" => c.checkpoint.manifest_sha256 = "a".repeat(64),
            _ => c.head_reference.block.hash = "not-a-hash".into(),
        }
        assert!(recovery_target(&c, 8 << 20, limit).is_err(), "{mode}");
    }
}
#[tokio::test]
#[ignore = "requires explicitly owned Kafka and exercises bounded replay before any public activation"]
async fn older_remote_checkpoint_replays_actual_kafka_privately_then_publishes_only_attested_head()
{
    let broker = std::env::var("XONE_TEST_KAFKA_BROKER").expect("owned Kafka required");
    let topic = "test.remote-checkpoint-replay";
    let mut base = boot();
    base.cursor.topic = topic.into();
    let mut producer = KafkaProducer::open(
        std::slice::from_ref(&broker),
        topic,
        "checkpoint-bounded-replay-writer",
        1024,
        8 << 20,
        Duration::from_secs(5),
    )
    .unwrap();
    base.cursor = producer
        .publish(
            &Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: base.snapshot.clone(),
            },
            || Ok(()),
        )
        .unwrap();
    let mut final_snapshot = base.snapshot.clone();
    let mut target = base.cursor.clone();
    for n in 0..2u64 {
        final_snapshot.reference.journal_seq += 1;
        final_snapshot.reference.block.number_or_slot += 1;
        final_snapshot.reference.block.parent_hash = final_snapshot.reference.block.hash.clone();
        final_snapshot.reference.block.hash = format!("0x{:064x}", 0x3030 + n);
        final_snapshot.reference.block.timestamp_ms += 1;
        final_snapshot.pools[0].reserve0 =
            xone_domain::Amount::checked((1000100u64 + n).into()).unwrap();
        target = producer
            .publish(
                &Unit {
                    normalized_facts: None,
                    kind: "BLOCK_COMMIT".into(),
                    snapshot: final_snapshot.clone(),
                },
                || Ok(()),
            )
            .unwrap();
    }
    let mut f = fixture(base.clone()).await;
    let (store_base, mut good) = {
        let s = f.state.lock().unwrap();
        let original: Catalog = serde_json::from_slice(&s.catalog).unwrap();
        let mut c = catalog(&base, final_snapshot.reference.clone(), target.clone());
        c.checkpoint = original;
        (
            f.api
                .config
                .config
                .journal
                .checkpoint_store
                .as_ref()
                .unwrap()
                .base_url
                .clone(),
            c,
        )
    };
    {
        let cfg = &mut Arc::get_mut(&mut f.api.config).unwrap().config;
        cfg.chains.retain(|c| c.key == "evm:31337");
        cfg.journal.transport = "kafka".into();
        cfg.journal.kafka.brokers = vec![broker];
    }
    {
        let mut s = f.state.lock().unwrap();
        s.recovery = Some(serde_json::to_vec(&good).unwrap());
    }
    // Missing attested boundary cannot expose the downloaded baseline or first delta.
    {
        let mut s = f.state.lock().unwrap();
        s.mode = "replay-revoked".into();
    }
    assert!(f
        .api
        .replay_remote_checkpoint(&f.owner, "evm:31337")
        .await
        .is_err());
    assert!(f.api.replica.view("evm:31337").is_err());
    {
        let mut s = f.state.lock().unwrap();
        s.mode.clear();
        s.recovery_reads = 0;
    }
    let (staged, reader) = f
        .api
        .replay_remote_checkpoint(&f.owner, "evm:31337")
        .await
        .unwrap();
    assert_eq!(staged.snapshot.reference, final_snapshot.reference);
    assert_eq!(staged.cursor, target);
    assert_eq!(staged.snapshot.pools, final_snapshot.pools);
    assert!(reader.at_committed_boundary());
    assert!(
        f.api.replica.view("evm:31337").is_err(),
        "staging leaked intermediate state"
    );
    drop(reader);
    // A lower configured frame budget cannot run an unbounded recovery tail.
    let old_budget = f.api.config.config.journal.kafka.max_tail_records;
    Arc::get_mut(&mut f.api.config)
        .unwrap()
        .config
        .journal
        .kafka
        .max_tail_records = 1;
    assert!(f
        .api
        .replay_remote_checkpoint(&f.owner, "evm:31337")
        .await
        .is_err());
    assert!(f.api.replica.view("evm:31337").is_err());
    Arc::get_mut(&mut f.api.config)
        .unwrap()
        .config
        .journal
        .kafka
        .max_tail_records = old_budget;
    // A target in the middle of an atomic unit is rejected, even with an otherwise valid catalogue.
    good.head_cursor.next_offset = (target.offset - 1).to_string();
    {
        let mut s = f.state.lock().unwrap();
        s.recovery = Some(serde_json::to_vec(&good).unwrap());
        s.recovery_reads = 0;
    }
    assert!(f
        .api
        .replay_remote_checkpoint(&f.owner, "evm:31337")
        .await
        .is_err());
    assert!(f.api.replica.view("evm:31337").is_err());
    good.head_cursor.next_offset = target.offset.to_string();
    {
        let mut s = f.state.lock().unwrap();
        s.recovery = Some(serde_json::to_vec(&good).unwrap());
        s.recovery_reads = 0;
    }
    f.api.sync_sources().await;
    assert_eq!(
        f.api.replica.view("evm:31337").unwrap().reference,
        final_snapshot.reference
    );
    assert_eq!(
        f.api
            .applied_cursors
            .lock()
            .unwrap()
            .get("evm:31337")
            .unwrap()
            .1,
        target
    );
    use tower::ServiceExt;
    let request = json!({"chainKey":"evm:31337","kind":"EXACT_IN","sellToken":"A","buyToken":"B","sellAmount":"1000","caller":"alice","recipient":"alice","slippageBps":100,"verification":"FAST"});
    let response = f
        .api
        .handler()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/quote")
                .header("authorization", format!("Bearer {}", f.api.config.token))
                .body(axum::body::Body::from(
                    serde_json::to_vec(&request).unwrap(),
                ))
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
    assert_eq!(status, axum::http::StatusCode::OK, "{quote}");
    assert_eq!(
        quote["snapshotRef"]["journal_seq"],
        json!(final_snapshot.reference.journal_seq)
    );
    if let Ok(directory) = std::env::var("XONE_REMOTE_CHECKPOINT_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("bounded-kafka-replay.json"),serde_json::to_vec_pretty(&json!({"checkpoint_ref":base.snapshot.reference,"checkpoint_cursor":base.cursor,"target_ref":final_snapshot.reference,"target_cursor":target,"quote":quote,"store":store_base,"checks":["private_stage_no_public_view","revoked_final_catalog_rejected","partial_unit_boundary_rejected","normal_sync_and_http_quote"]})).unwrap()).unwrap();
    }
}
