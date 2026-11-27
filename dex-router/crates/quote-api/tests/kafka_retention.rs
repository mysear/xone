use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, StatusCode},
    routing, Router,
};
use rdkafka::{
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    client::DefaultClientContext,
    producer::{FutureProducer, FutureRecord},
    ClientConfig, Offset, TopicPartitionList,
};
use serde_json::{json, Value};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tower::ServiceExt;
use xone_domain::{hash, scope_hash, BlockRef, Gap, Pool, Snapshot, SnapshotRef, SCHEMA_VERSION};
use xone_kafka_transport::{
    encode, encode_bootstrap, encode_frame, Bootstrap, KafkaProducer, Unit,
};
use xone_observability::now_ms;
use xone_quote_api::Api;

const CHAIN: &str = "evm:31337";
const TIMEOUT: Duration = Duration::from_secs(10);
type OwnerResponse = Arc<RwLock<Vec<u8>>>;

fn snapshot(sequence: u64) -> Snapshot {
    let pool: Pool = serde_json::from_value(json!({
        "chain_key": CHAIN,
        "pool_id": "0x0000000000000000000000000000000000000003",
        "protocol": "uniswap-v2",
        "token0": "0x0000000000000000000000000000000000000001",
        "token1": "0x0000000000000000000000000000000000000002",
        "reserve0": "1000000",
        "reserve1": "2000000",
        "fee_numerator": 3,
        "fee_denominator": 1000,
        "adapter_version": "uniswap-v2/1",
        "token_behavior": "standard"
    }))
    .unwrap();
    let adapter_set_hash = hash(&[&pool.adapter_version]).unwrap();
    let pools = vec![pool];
    Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: CHAIN.into(),
                number_or_slot: sequence * 100,
                hash: format!("0x{sequence:064x}"),
                parent_hash: format!("0x{:064x}", sequence - 1),
                timestamp_ms: now_ms(),
            },
            canonical_epoch: 1,
            journal_seq: sequence,
            schema_version: SCHEMA_VERSION,
            adapter_set_hash,
            online_generation: sequence,
            scope_hash: scope_hash(&pools).unwrap(),
            writer_epoch: 1,
            owner_service: "chain-parser-go".into(),
            owner_assignment_version: 1,
        },
        pools,
        source: "retention-test-owner".into(),
        history_gaps: Vec::new(),
    }
}

fn publish(producer: &mut KafkaProducer, kind: &str, snapshot: Snapshot) -> Bootstrap {
    let unit = Unit {
        kind: kind.into(),
        snapshot,
        normalized_facts: None,
    };
    let cursor = producer.publish(&unit, || Ok(())).unwrap();
    Bootstrap {
        snapshot: unit.snapshot,
        cursor,
    }
}

async fn get(api: &Api, uri: &str) -> (StatusCode, String) {
    let response = api
        .handler()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn assert_unready(api: &Api) {
    api.sync_sources().await;
    assert!(api.replica.last_reference(CHAIN).unwrap().is_none());
    assert_eq!(
        get(api, "/health/ready").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
#[ignore = "requires an isolated Kafka broker in XONE_TEST_KAFKA_BROKER"]
async fn expired_cursor_waits_for_current_owner_checkpoint() {
    let broker = std::env::var("XONE_TEST_KAFKA_BROKER").expect("isolated Kafka required");
    let topic = format!("quote-retention-{}-{}", std::process::id(), now_ms());
    let brokers = [broker.clone()];
    let kafka = ClientConfig::new()
        .set("bootstrap.servers", &broker)
        .clone();
    let admin: AdminClient<DefaultClientContext> = kafka.create().unwrap();
    let admin_options = AdminOptions::new().request_timeout(Some(TIMEOUT));
    admin
        .create_topics(
            [&NewTopic::new(&topic, 1, TopicReplication::Fixed(1))],
            &admin_options,
        )
        .await
        .unwrap()
        .into_iter()
        .for_each(|result| {
            result.unwrap();
        });
    let mut producer =
        KafkaProducer::open(&brokers, &topic, &topic, 1024, 1 << 20, TIMEOUT).unwrap();
    let old = publish(&mut producer, "ONLINE_BASELINE_COMMIT", snapshot(1));
    let deleted = publish(&mut producer, "ONLINE_BASELINE_COMMIT", snapshot(2));
    // Include Kafka's transaction control record after the application COMMIT.
    let mut offsets = TopicPartitionList::new();
    offsets
        .add_partition_offset(&topic, 0, Offset::Offset(deleted.cursor.offset + 1))
        .unwrap();
    let result = admin
        .delete_records(&offsets, &admin_options)
        .await
        .unwrap();
    let partition = result.find_partition(&topic, 0).unwrap();
    partition.error().unwrap();
    let Offset::Offset(low) = partition.offset() else {
        panic!("expected an absolute low watermark");
    };
    assert!(low > old.cursor.offset);
    println!("expired_cursor={} low_watermark={low}", old.cursor.offset);

    let owner: OwnerResponse = Arc::new(RwLock::new(encode_bootstrap(&old).unwrap()));
    let app = Router::new()
        .route(
            "/internal/chains/evm:31337/bootstrap",
            routing::get(|State(owner): State<OwnerResponse>| async move {
                owner.read().unwrap().clone()
            }),
        )
        .with_state(owner.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let mut cfg = xone_config::load("../../config/config-test.yaml", "dex-router").unwrap();
    cfg.config.chains.retain(|chain| chain.key == CHAIN);
    cfg.config.journal.transport = "kafka".into();
    cfg.config.journal.kafka.brokers = brokers.to_vec();
    cfg.config
        .journal
        .kafka
        .topics
        .insert(CHAIN.into(), topic.clone());
    cfg.config.journal.source_urls = vec![source];
    cfg.config.startup.max_baseline_age_ms = 60_000;
    let api = Api::new(cfg).unwrap();

    assert_unready(&api).await;
    for (sequence, kind) in [(3, "ONLINE_SCOPE_REVOKED"), (4, "REORG_BEGIN")] {
        publish(&mut producer, kind, snapshot(sequence));
        assert_unready(&api).await;
    }
    publish(&mut producer, "ONLINE_BASELINE_COMMIT", snapshot(5));
    assert_unready(&api).await;

    let raw_producer: FutureProducer = kafka.create().unwrap();
    let incomplete = encode(
        &Unit {
            kind: "ONLINE_BASELINE_COMMIT".into(),
            snapshot: snapshot(6),
            normalized_facts: None,
        },
        1024,
    )
    .unwrap();
    let bytes = encode_frame(&incomplete[0]).unwrap();
    raw_producer
        .send(
            FutureRecord::to(&topic)
                .partition(0)
                .key(CHAIN)
                .payload(&bytes),
            TIMEOUT,
        )
        .await
        .unwrap();
    assert_unready(&api).await;

    let mut current = snapshot(7);
    current.history_gaps.push(Gap {
        from: 101,
        to: 699,
        reason: "RETENTION_EXPIRED".into(),
    });
    let current = publish(&mut producer, "ONLINE_BASELINE_COMMIT", current);
    *owner.write().unwrap() = encode_bootstrap(&current).unwrap();
    let cold = Api::new(api.config.as_ref().clone()).unwrap();
    cold.sync_sources().await;
    assert_eq!(get(&cold, "/health/ready").await.0, StatusCode::OK);
    let view = cold.replica.view(CHAIN).unwrap();
    assert_eq!(view.reference, current.snapshot.reference);
    let pool = &view.pools[0];
    let response = cold
        .handler()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/quote")
                .header("authorization", format!("Bearer {}", cold.config.token))
                .body(Body::from(
                    json!({
                        "chainKey": CHAIN, "kind": "EXACT_IN",
                        "sellToken": pool.token0, "buyToken": pool.token1, "sellAmount": "1000",
                        "caller": pool.token0, "recipient": pool.token0,
                        "slippageBps": 50, "verification": "FAST"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["quality"]["historyIndependent"], true);
    assert_eq!(
        body["quality"]["historyGaps"],
        json!(current.snapshot.history_gaps)
    );
    let (_, metrics) = get(&cold, "/metrics").await;
    for metric in [
        "xone_first_qualified_snapshot_seconds",
        "xone_first_successful_quote_seconds",
    ] {
        let label = format!("{metric}{{chain=\"{CHAIN}\"}}");
        assert!(
            metrics.lines().any(|line| line.starts_with(&label)),
            "{metrics}"
        );
        for line in metrics.lines().filter(|line| line.starts_with(&label)) {
            println!("{line}");
        }
    }
    let mut next = current.snapshot.clone();
    next.reference.journal_seq += 1;
    next.reference.block.number_or_slot += 1;
    next.reference.block.parent_hash = next.reference.block.hash;
    next.reference.block.hash = format!("0x{:064x}", next.reference.journal_seq);
    let next = publish(&mut producer, "BLOCK_COMMIT", next);
    tokio::time::timeout(TIMEOUT, async {
        loop {
            cold.sync_sources().await;
            let view = cold.replica.view(CHAIN).unwrap();
            if view.reference == next.snapshot.reference {
                assert_eq!(view.history_gaps, current.snapshot.history_gaps);
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("live journal continued from the owner checkpoint");
    server.abort();
}
