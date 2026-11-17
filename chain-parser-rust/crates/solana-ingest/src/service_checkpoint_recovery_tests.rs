use super::remote_tests::Fixture;
use super::*;
use xone_kafka_transport::{
    checkpoint::{recovery_target, RecoveryCatalog},
    Bootstrap, Unit,
};
fn next_block(base: &Bootstrap) -> Bootstrap {
    let mut boot = base.clone();
    boot.snapshot.reference.journal_seq += 1;
    boot.snapshot.reference.online_generation += 1;
    boot.snapshot.reference.block.number_or_slot += 1;
    boot.snapshot.reference.block.parent_hash = boot.snapshot.reference.block.hash.clone();
    boot.snapshot.reference.block.hash = bs58::encode([0x37; 32]).into_string();
    boot.snapshot.reference.block.timestamp_ms += 1;
    boot.snapshot.pools[0].reserve0 = xone_domain::Amount::checked(1000111u64.into()).unwrap();
    boot.cursor.offset += 4;
    boot
}
async fn get_catalog(f: &Fixture) -> (reqwest::StatusCode, Value) {
    let response = f
        .client
        .get(format!(
            "{}/internal/chains/{}/checkpoint-recovery-catalog",
            f.owner, f.key
        ))
        .bearer_auth(&f.parser.config.token)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let raw = response.bytes().await.unwrap();
    (status, serde_json::from_slice(&raw).unwrap())
}
#[tokio::test]
async fn normal_rust_recovery_catalog_attests_uploaded_old_object_and_committed_head() {
    let f = Fixture::new_configured(0, false, "").await;
    let before = f.parser.stores[&f.key].lock().unwrap().bootstrap().unwrap();
    assert_eq!(
        get_catalog(&f).await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    let (status, published) = f.publish().await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let boot = next_block(&before);
    {
        let mut store = f.parser.stores[&f.key].lock().unwrap();
        store
            .prepare(&Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: boot.snapshot.clone(),
            })
            .unwrap();
        store.apply_prepared(boot.cursor.clone()).unwrap();
        assert_eq!(
            store.bootstrap_head().unwrap(),
            (boot.snapshot.reference.clone(), boot.cursor.clone())
        );
    }
    assert_eq!(
        f.catalog().await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "exact-current catalogue must stay exact"
    );
    let (status, value) = get_catalog(&f).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{value}");
    let catalog: RecoveryCatalog = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        catalog.checkpoint.manifest.reference,
        before.snapshot.reference
    );
    assert_eq!(catalog.head_reference, boot.snapshot.reference);
    assert_eq!(
        recovery_target(
            &catalog,
            f.parser.config.config.journal.max_payload_bytes,
            100
        )
        .unwrap(),
        boot.cursor
    );
    assert_eq!(
        catalog.checkpoint.manifest_sha256,
        published["remote_checkpoints"][0]["manifest_sha256"]
            .as_str()
            .unwrap()
    );
    if let Ok(directory) = std::env::var("XONE_RUST_RECOVERY_CATALOG_OUTPUT") {
        std::fs::create_dir_all(&directory).unwrap();
        let publication: local::Publication =
            serde_json::from_value(published["public_checkpoints"][0].clone()).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("normal-rust-recovery-catalog.json"),serde_json::to_vec_pretty(&serde_json::json!({"catalog":catalog,"checkpoint_manifest_raw":std::fs::read_to_string(&publication.manifest_path).unwrap(),"checkpoint_payload_hex":hex::encode(std::fs::read(publication.manifest_path.with_file_name("bootstrap.pb")).unwrap()),"reference":boot.snapshot.reference,"cursor":boot.cursor,"verification_scope":"NORMAL_RUST_OWNER_OLD_OBJECT_CURRENT_HEAD_HTTP"})).unwrap()).unwrap();
    }
    // Crossing a writer/canonical domain requires a new qualified source.
    {
        let mut store = f.parser.stores[&f.key].lock().unwrap();
        let mut changed = boot.snapshot.clone();
        changed.reference.online_generation += 1;
        changed.reference.journal_seq += 1;
        changed.reference.writer_epoch += 1;
        store
            .prepare(&Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: changed,
            })
            .unwrap();
        let mut cursor = boot.cursor.clone();
        cursor.offset += 4;
        store.apply_prepared(cursor).unwrap();
    }
    assert_eq!(
        get_catalog(&f).await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.close().await;
}
#[tokio::test]
async fn rust_recovery_catalog_rejects_corrupt_hint_and_owner_head_expiration() {
    let f = Fixture::new_configured(0, false, "").await;
    assert_eq!(f.publish().await.0, reqwest::StatusCode::OK);
    let c = f
        .parser
        .config
        .config
        .journal
        .checkpoint_store
        .as_ref()
        .unwrap();
    let remote = Remote::new(c, "", Duration::from_secs(1)).unwrap();
    let root = f
        .parser
        .config
        .resolve(&f.parser.config.config.state.path)
        .join("checkpoints")
        .join(f.key.replace(':', "-"));
    let path = root.join("public-v2").join(remote.candidate_name());
    let original = std::fs::read(&path).unwrap();
    let mut changed: RemotePublication = serde_json::from_slice(&original).unwrap();
    changed.manifest_url = "https://unconfigured.example/manifest.json".into();
    std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(
        get_catalog(&f).await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    std::fs::write(&path, original).unwrap();
    assert_eq!(get_catalog(&f).await.0, reqwest::StatusCode::OK);
    f.parser.stores[&f.key]
        .lock()
        .unwrap()
        .set_authority_deadline(std::time::Instant::now() - Duration::from_secs(1));
    assert!(f.parser.stores[&f.key]
        .lock()
        .unwrap()
        .bootstrap_head()
        .is_err());
    assert_eq!(
        get_catalog(&f).await.0,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    f.close().await;
}
#[tokio::test]
#[ignore = "provider process stays live while the same run's independent router executes Kafka recovery"]
async fn actual_rust_owner_serves_uploaded_checkpoint_and_two_real_kafka_baselines_to_external_router(
) {
    let broker = std::env::var("XONE_TEST_KAFKA_BROKER").expect("owned Kafka required");
    let directory = std::path::PathBuf::from(
        std::env::var("XONE_RUST_LIVE_RECOVERY_DIRECTORY").expect("run output required"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    let f = Fixture::new_configured(0, false, "").await;
    let key = f.key.clone();
    let mut base = f.parser.stores[&key].lock().unwrap().bootstrap().unwrap();
    base.cursor.topic = "test.rust-owner-recovery".into();
    let mut producer = xone_kafka_transport::KafkaProducer::open(
        std::slice::from_ref(&broker),
        &base.cursor.topic,
        "live-rust-checkpoint-recovery-writer",
        1024,
        8 << 20,
        Duration::from_secs(5),
    )
    .unwrap();
    // First establish an actual committed baseline cursor in this source-backed normal owner.
    base.snapshot.reference.online_generation += 1;
    base.snapshot.reference.journal_seq += 1;
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
    {
        let mut store = f.parser.stores[&key].lock().unwrap();
        store
            .prepare(&Unit {
                normalized_facts: None,
                kind: "ONLINE_BASELINE_COMMIT".into(),
                snapshot: base.snapshot.clone(),
            })
            .unwrap();
        store.apply_prepared(base.cursor.clone()).unwrap();
    }
    assert_eq!(f.publish().await.0, reqwest::StatusCode::OK);
    let mut head = base.clone();
    for n in 0..2 {
        let mut next = next_block(&head);
        next.snapshot.reference.block.hash = bs58::encode([0x40 + n; 32]).into_string();
        next.snapshot.pools[0].reserve0 =
            xone_domain::Amount::checked((1000200u64 + u64::from(n)).into()).unwrap();
        let unit = Unit {
            normalized_facts: None,
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: next.snapshot.clone(),
        };
        next.cursor = producer.publish(&unit, || Ok(())).unwrap();
        {
            let mut store = f.parser.stores[&key].lock().unwrap();
            store.prepare(&unit).unwrap();
            store.apply_prepared(next.cursor.clone()).unwrap();
        }
        head = next;
    }
    let (status, value) = get_catalog(&f).await;
    assert_eq!(status, reqwest::StatusCode::OK);
    let ready = serde_json::json!({"owner_url":f.owner,"object_base":f.base,"owner_token":f.parser.config.token,"object_token":"rust-object-token-only-20261118","broker":broker,"chain_key":key,"topic":head.cursor.topic,"catalog":value,"target_reference":head.snapshot.reference,"target_cursor":head.cursor,"checkpoint_reference":base.snapshot.reference,"checkpoint_cursor":base.cursor,"verification_scope":"NORMAL_RUST_OWNER_CONTROLLED_SOURCE_ACTUAL_KAFKA_AND_HTTP_OBJECT_API"});
    let temp = directory.join("ready.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(&ready).unwrap()).unwrap();
    std::fs::rename(temp, directory.join("ready.json")).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while !directory.join("release").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "external router did not finish in budget"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    f.close().await;
}
