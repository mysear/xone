use prost::Message as ProstMessage;
use rdkafka::{
    consumer::{BaseConsumer, Consumer},
    producer::{BaseProducer, BaseRecord, DeliveryResult, Producer, ProducerContext},
    util::Timeout,
    ClientConfig, Message, Offset, TopicPartitionList,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};
use xone_domain::{err, Result, Snapshot, SnapshotRef};
use xone_domain::{wire, wire_conversion as wc};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_facts: Option<xone_domain::facts::FactBatch>,
    pub kind: String,
    pub snapshot: Snapshot,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    pub snapshot: Snapshot,
    pub cursor: Cursor,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub schema_version: u32,
    pub unit_id: String,
    pub kind: String,
    pub reference: SnapshotRef,
    pub payload_sha256: String,
    pub payload_bytes: u64,
    pub chunk_count: u32,
    pub chunk_index: u32,
    pub data_hex: String,
}
fn valid_kind(kind: &str) -> bool {
    [
        "ONLINE_BASELINE_COMMIT",
        "BLOCK_COMMIT",
        "REORG_COMMIT",
        "REORG_BEGIN",
        "ONLINE_SCOPE_REVOKED",
    ]
    .contains(&kind)
}
pub fn encode(unit: &Unit, chunk_bytes: usize) -> Result<Vec<Frame>> {
    if !(1024..=1048576).contains(&chunk_bytes) || !valid_kind(&unit.kind) {
        return Err(err("JOURNAL_CHUNK_OR_KIND"));
    }
    unit.snapshot.validate(
        &unit.snapshot.reference.owner_service,
        unit.snapshot.reference.owner_assignment_version,
    )?;
    let b = encode_unit(unit)?;
    if b.len() > 64 << 20 {
        return Err(err("JOURNAL_UNIT_LIMIT"));
    }
    let r = &unit.snapshot.reference;
    let count = b.len().div_ceil(chunk_bytes);
    let base = Frame {
        schema_version: 1,
        unit_id: format!(
            "{}/{}/{}/{}",
            r.block.chain_key, r.owner_assignment_version, r.writer_epoch, r.journal_seq
        ),
        kind: "BEGIN".into(),
        reference: r.clone(),
        payload_sha256: hex::encode(Sha256::digest(&b)),
        payload_bytes: b.len() as u64,
        chunk_count: count as u32,
        chunk_index: 0,
        data_hex: String::new(),
    };
    let mut frames = vec![base.clone()];
    for (i, part) in b.chunks(chunk_bytes).enumerate() {
        let mut f = base.clone();
        f.kind = "CHUNK".into();
        f.chunk_index = i as u32;
        f.data_hex = hex::encode(part);
        frames.push(f);
    }
    let mut commit = base;
    commit.kind = "COMMIT".into();
    frames.push(commit);
    Ok(frames)
}
pub struct Assembler {
    max_bytes: usize,
    begin: Option<Frame>,
    bytes: Vec<u8>,
    next: u32,
}
impl Assembler {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            begin: None,
            bytes: Vec::new(),
            next: 0,
        }
    }
    pub fn in_progress(&self) -> bool {
        self.begin.is_some()
    }
    pub fn reset(&mut self) {
        self.begin = None;
        self.bytes.clear();
        self.next = 0;
    }
    pub fn push(&mut self, f: Frame) -> Result<Option<Unit>> {
        let r = self.push_inner(f);
        if r.is_err() {
            self.reset();
        }
        r
    }
    fn push_inner(&mut self, f: Frame) -> Result<Option<Unit>> {
        if f.schema_version != 1
            || f.payload_bytes == 0
            || f.payload_bytes > self.max_bytes as u64
            || f.chunk_count == 0
            || f.chunk_count > 65536
        {
            return Err(err("JOURNAL_FRAME_INVALID"));
        }
        if f.kind == "BEGIN" {
            if self.begin.is_some() || !f.data_hex.is_empty() || f.chunk_index != 0 {
                return Err(err("JOURNAL_INTERLEAVED_BEGIN"));
            }
            self.begin = Some(f);
            return Ok(None);
        }
        let b = self
            .begin
            .as_ref()
            .ok_or_else(|| err("JOURNAL_BEGIN_REQUIRED"))?;
        if f.unit_id != b.unit_id
            || f.reference != b.reference
            || f.payload_sha256 != b.payload_sha256
            || f.payload_bytes != b.payload_bytes
            || f.chunk_count != b.chunk_count
        {
            return Err(err("JOURNAL_FRAME_IDENTITY"));
        }
        match f.kind.as_str() {
            "CHUNK" => {
                if f.chunk_index != self.next || self.next >= f.chunk_count {
                    return Err(err("JOURNAL_CHUNK_SEQUENCE"));
                }
                if f.data_hex.len() > 2 * self.max_bytes {
                    return Err(err("JOURNAL_CHUNK_SIZE"));
                }
                let raw = hex::decode(&f.data_hex).map_err(|_| err("JOURNAL_CHUNK_HEX"))?;
                if raw.is_empty() || self.bytes.len() + raw.len() > f.payload_bytes as usize {
                    return Err(err("JOURNAL_CHUNK_SIZE"));
                }
                self.bytes.extend(raw);
                self.next += 1;
                Ok(None)
            }
            "COMMIT" => {
                if self.next != f.chunk_count
                    || self.bytes.len() != f.payload_bytes as usize
                    || !f.data_hex.is_empty()
                {
                    return Err(err("JOURNAL_INCOMPLETE_COMMIT"));
                }
                if hex::encode(Sha256::digest(&self.bytes)) != f.payload_sha256 {
                    return Err(err("JOURNAL_PAYLOAD_CHECKSUM"));
                }
                let unit = decode_unit(&self.bytes)?;
                if unit.snapshot.reference != f.reference || !valid_kind(&unit.kind) {
                    return Err(err("JOURNAL_PAYLOAD_IDENTITY"));
                }
                unit.snapshot.validate(
                    &f.reference.owner_service,
                    f.reference.owner_assignment_version,
                )?;
                self.reset();
                Ok(Some(unit))
            }
            _ => Err(err("JOURNAL_FRAME_KIND")),
        }
    }
}
/// Dedicated producer/partition. Any publication failure seals the session; only a new lease may reopen it.
#[derive(Clone, Default)]
struct Delivery {
    last: Arc<AtomicI64>,
    failed: Arc<AtomicBool>,
}
impl rdkafka::client::ClientContext for Delivery {}
impl ProducerContext for Delivery {
    type DeliveryOpaque = ();
    fn delivery(&self, result: &DeliveryResult, _: ()) {
        match result {
            Ok(message) => {
                self.last.fetch_max(message.offset(), Ordering::Relaxed);
            }
            Err(_) => {
                self.failed.store(true, Ordering::Relaxed);
            }
        }
    }
}
pub struct KafkaProducer {
    producer: BaseProducer<Delivery>,
    delivery: Delivery,
    topic: String,
    chunk_bytes: usize,
    max_bytes: usize,
    timeout: Duration,
    sealed: bool,
}
impl KafkaProducer {
    pub fn open(
        brokers: &[String],
        topic: &str,
        transaction_id: &str,
        chunk_bytes: usize,
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<Self> {
        let delivery = Delivery::default();
        let producer: BaseProducer<Delivery> = ClientConfig::new()
            .set("bootstrap.servers", brokers.join(","))
            .set("transactional.id", transaction_id)
            .set("transaction.timeout.ms", timeout.as_millis().to_string())
            .set("enable.idempotence", "true")
            .set("acks", "all")
            .set("compression.type", "none")
            .set("allow.auto.create.topics", "false")
            .create_with_context(delivery.clone())
            .map_err(|_| err("KAFKA_CLIENT_CONFIG"))?;
        validate_topic(producer.client(), topic, timeout)?;
        producer
            .init_transactions(Timeout::After(timeout))
            .map_err(|_| err("KAFKA_INIT_TRANSACTIONS"))?;
        Ok(Self {
            producer,
            delivery,
            topic: topic.into(),
            chunk_bytes,
            max_bytes,
            timeout,
            sealed: false,
        })
    }
    pub fn publish(
        &mut self,
        unit: &Unit,
        before_commit: impl Fn() -> Result<()>,
    ) -> Result<Cursor> {
        if self.sealed {
            return Err(err("KAFKA_SESSION_SEALED"));
        }
        before_commit()?;
        validate_topic(self.producer.client(), &self.topic, self.timeout)?;
        let frames = encode(unit, self.chunk_bytes)?;
        let boot = encode_bootstrap(&Bootstrap {
            snapshot: unit.snapshot.clone(),
            cursor: Cursor {
                topic: self.topic.clone(),
                partition: 0,
                offset: i64::MAX,
            },
        })?;
        if frames[0].payload_bytes > self.max_bytes as u64 || boot.len() > self.max_bytes {
            return Err(err("JOURNAL_UNIT_LIMIT"));
        }
        self.delivery.last.store(-1, Ordering::Relaxed);
        self.delivery.failed.store(false, Ordering::Relaxed);
        self.producer
            .begin_transaction()
            .map_err(|_| err("KAFKA_BEGIN_TRANSACTION"))?;
        for f in frames {
            let bytes = encode_frame(&f)?;
            if self
                .producer
                .send(
                    BaseRecord::to(&self.topic)
                        .partition(0)
                        .key(&f.reference.block.chain_key)
                        .payload(&bytes),
                )
                .is_err()
            {
                self.sealed = true;
                return Err(err("KAFKA_QUEUE_FAILED"));
            }
        }
        if self.producer.flush(Timeout::After(self.timeout)).is_err()
            || self.delivery.failed.load(Ordering::Relaxed)
        {
            self.sealed = true;
            return Err(err("KAFKA_PRODUCE_FAILED"));
        }
        if let Err(e) = before_commit() {
            self.sealed = true;
            return Err(e);
        }
        if self
            .producer
            .commit_transaction(Timeout::After(self.timeout))
            .is_err()
        {
            self.sealed = true;
            return Err(err("KAFKA_COMMIT_OUTCOME_UNKNOWN"));
        }
        let offset = self.delivery.last.load(Ordering::Relaxed);
        if offset < 0 {
            self.sealed = true;
            return Err(err("KAFKA_COMMIT_CURSOR"));
        }
        Ok(Cursor {
            topic: self.topic.clone(),
            partition: 0,
            offset: offset + 1,
        })
    }
}
fn validate_topic<C: rdkafka::client::ClientContext>(
    client: &rdkafka::client::Client<C>,
    topic: &str,
    timeout: Duration,
) -> Result<()> {
    let metadata = client
        .fetch_metadata(Some(topic), Timeout::After(timeout))
        .map_err(|_| err("KAFKA_METADATA"))?;
    let topics = metadata.topics();
    if topics.len() != 1
        || topics[0].error().is_some()
        || topics[0].partitions().len() != 1
        || topics[0].partitions()[0].id() != 0
    {
        return Err(err("KAFKA_REQUIRES_SINGLE_PARTITION"));
    }
    Ok(())
}
pub struct Reader {
    consumer: BaseConsumer,
    topic: String,
    assembler: Assembler,
    next_offset: i64,
    max_tail: i64,
    pub last_sequence: u64,
}
impl Reader {
    pub fn open(
        brokers: &[String],
        group: &str,
        cursor: &Cursor,
        max_bytes: usize,
        max_tail: usize,
        timeout: Duration,
    ) -> Result<Self> {
        if cursor.partition != 0 || cursor.offset < 0 {
            return Err(err("JOURNAL_CURSOR_INVALID"));
        }
        let consumer: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers.join(","))
            .set("group.id", group)
            .set("isolation.level", "read_committed")
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("enable.partition.eof", "true")
            .set("allow.auto.create.topics", "false")
            .set("fetch.message.max.bytes", max_bytes.to_string())
            .create()
            .map_err(|_| err("KAFKA_CLIENT_CONFIG"))?;
        validate_topic(consumer.client(), &cursor.topic, timeout)?;
        let (low, high) = consumer
            .fetch_watermarks(&cursor.topic, 0, Timeout::After(timeout))
            .map_err(|_| err("KAFKA_WATERMARKS"))?;
        if cursor.offset < low || cursor.offset > high || high - cursor.offset > max_tail as i64 {
            return Err(err("KAFKA_REANCHOR_REQUIRED"));
        }
        let mut assignment = TopicPartitionList::new();
        assignment
            .add_partition_offset(&cursor.topic, 0, Offset::Offset(cursor.offset))
            .map_err(|_| err("JOURNAL_CURSOR_INVALID"))?;
        consumer
            .assign(&assignment)
            .map_err(|_| err("KAFKA_ASSIGN"))?;
        Ok(Self {
            consumer,
            topic: cursor.topic.clone(),
            assembler: Assembler::new(max_bytes),
            next_offset: cursor.offset,
            max_tail: max_tail as i64,
            last_sequence: 0,
        })
    }
    pub fn poll(&mut self, timeout: Duration) -> Result<Option<Unit>> {
        let result = self.consumer.poll(timeout);
        let Some(message) = result else {
            return Ok(None);
        };
        let message = match message {
            Ok(m) => m,
            Err(rdkafka::error::KafkaError::PartitionEOF(_)) => return Ok(None),
            Err(_) => return Err(err("KAFKA_CONSUME")),
        };
        if message.partition() != 0 || message.topic() != self.topic {
            return Err(err("KAFKA_PARTITION_IDENTITY"));
        }
        let payload = message.payload().ok_or_else(|| err("KAFKA_EMPTY_RECORD"))?;
        let frame = decode_frame(payload)?;
        self.next_offset = message.offset() + 1;
        let unit = self.assembler.push(frame)?;
        if let Some(u) = &unit {
            if self.last_sequence != 0 && u.snapshot.reference.journal_seq != self.last_sequence + 1
            {
                return Err(err("JOURNAL_SEQUENCE_GAP"));
            }
            self.last_sequence = u.snapshot.reference.journal_seq;
        }
        Ok(unit)
    }
    pub fn cursor(&self) -> Cursor {
        Cursor {
            topic: self.topic.clone(),
            partition: 0,
            offset: self.next_offset,
        }
    }
    pub fn check_lag(&self, timeout: Duration) -> Result<()> {
        let (low, high) = self
            .consumer
            .fetch_watermarks(&self.topic, 0, Timeout::After(timeout))
            .map_err(|_| err("KAFKA_WATERMARKS"))?;
        if self.next_offset < low || high - self.next_offset > self.max_tail {
            return Err(err("KAFKA_REANCHOR_REQUIRED"));
        }
        Ok(())
    }
}
pub fn tail(
    brokers: &[String],
    topic: &str,
    max_bytes: usize,
    max_records: usize,
    timeout: Duration,
) -> Result<(Option<Unit>, Cursor)> {
    let consumer: BaseConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers.join(","))
        .set("group.id", "xone-owner-bootstrap-tail")
        .set("isolation.level", "read_committed")
        .set("enable.auto.commit", "false")
        .set("enable.partition.eof", "true")
        .set("allow.auto.create.topics", "false")
        .create()
        .map_err(|_| err("KAFKA_CLIENT_CONFIG"))?;
    validate_topic(consumer.client(), topic, timeout)?;
    let (low, high) = consumer
        .fetch_watermarks(topic, 0, Timeout::After(timeout))
        .map_err(|_| err("KAFKA_WATERMARKS"))?;
    let start = low.max(high - max_records as i64);
    let mut list = TopicPartitionList::new();
    list.add_partition_offset(topic, 0, Offset::Offset(start))
        .map_err(|_| err("JOURNAL_CURSOR_INVALID"))?;
    consumer.assign(&list).map_err(|_| err("KAFKA_ASSIGN"))?;
    let deadline = std::time::Instant::now() + timeout;
    let mut assembler = Assembler::new(max_bytes);
    let mut latest = None;
    let mut cursor = Cursor {
        topic: topic.into(),
        partition: 0,
        offset: high,
    };
    if low == high {
        return Ok((None, cursor));
    }
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(err("KAFKA_TAIL_DEADLINE"));
        }
        match consumer.poll(Duration::from_millis(100)) {
            None => {}
            Some(Err(rdkafka::error::KafkaError::PartitionEOF(_))) => {
                let position = consumer.position().map_err(|_| err("KAFKA_POSITION"))?;
                if position
                    .find_partition(topic, 0)
                    .is_some_and(|p| matches!(p.offset(),Offset::Offset(n) if n>=high))
                {
                    if assembler.in_progress() {
                        return Err(err("KAFKA_INCOMPLETE_TRANSACTION_UNIT"));
                    }
                    if latest.is_none() {
                        return Err(err("KAFKA_TAIL_BASELINE_NOT_FOUND"));
                    }
                    return Ok((latest, cursor));
                }
            }
            Some(Err(_)) => return Err(err("KAFKA_TAIL_FETCH")),
            Some(Ok(m)) => {
                if m.offset() >= high {
                    continue;
                }
                let f = decode_frame(m.payload().ok_or_else(|| err("KAFKA_EMPTY_RECORD"))?)?;
                if !assembler.in_progress() && f.kind != "BEGIN" {
                    continue;
                }
                if let Some(u) = assembler.push(f)? {
                    latest = Some(u);
                    cursor.offset = m.offset() + 1;
                }
            }
        }
    }
}

pub fn encode_unit(u: &Unit) -> Result<Vec<u8>> {
    if let Some(f) = &u.normalized_facts {
        f.validate(&u.kind, &u.snapshot)?;
    }
    Ok(wire::JournalUnit {
        normalized_facts: u
            .normalized_facts
            .as_ref()
            .map(|f| f.to_proto())
            .transpose()?,
        kind: u.kind.clone(),
        snapshot: Some(wc::snapshot_to_proto(&u.snapshot)?),
    }
    .encode_to_vec())
}
pub fn decode_unit(raw: &[u8]) -> Result<Unit> {
    if raw.len() > 64 << 20 {
        return Err(err("JOURNAL_UNIT_LIMIT"));
    }
    let u = wire::JournalUnit::decode(raw).map_err(|_| err("JOURNAL_PROTOBUF_INVALID"))?;
    let unit = Unit {
        normalized_facts: u
            .normalized_facts
            .map(xone_domain::facts::FactBatch::from_proto)
            .transpose()?,
        kind: u.kind,
        snapshot: wc::snapshot_from_proto(
            u.snapshot.ok_or_else(|| err("WIRE_SNAPSHOT_REQUIRED"))?,
        )?,
    };
    if let Some(f) = &unit.normalized_facts {
        f.validate(&unit.kind, &unit.snapshot)?;
    }
    Ok(unit)
}
pub fn encode_frame(f: &Frame) -> Result<Vec<u8>> {
    let hash = hex::decode(&f.payload_sha256).map_err(|_| err("JOURNAL_PAYLOAD_CHECKSUM"))?;
    if hash.len() != 32 {
        return Err(err("JOURNAL_PAYLOAD_CHECKSUM"));
    }
    Ok(wire::JournalFrame {
        schema_version: f.schema_version,
        unit_id: f.unit_id.clone(),
        kind: f.kind.clone(),
        reference: Some(wc::ref_to_proto(&f.reference)?),
        payload_sha256: hash,
        payload_bytes: f.payload_bytes,
        chunk_count: f.chunk_count,
        chunk_index: f.chunk_index,
        data: hex::decode(&f.data_hex).map_err(|_| err("JOURNAL_CHUNK_HEX"))?,
    }
    .encode_to_vec())
}
pub fn decode_frame(raw: &[u8]) -> Result<Frame> {
    if raw.len() > 64 << 20 {
        return Err(err("JOURNAL_FRAME_LIMIT"));
    }
    let f = wire::JournalFrame::decode(raw).map_err(|_| err("JOURNAL_PROTOBUF_FRAME"))?;
    if f.schema_version != 1 || f.payload_sha256.len() != 32 {
        return Err(err("JOURNAL_PROTOBUF_FRAME"));
    }
    Ok(Frame {
        schema_version: f.schema_version,
        unit_id: f.unit_id,
        kind: f.kind,
        reference: wc::ref_from_proto(f.reference.ok_or_else(|| err("WIRE_REFERENCE_REQUIRED"))?)?,
        payload_sha256: hex::encode(f.payload_sha256),
        payload_bytes: f.payload_bytes,
        chunk_count: f.chunk_count,
        chunk_index: f.chunk_index,
        data_hex: hex::encode(f.data),
    })
}

pub fn encode_bootstrap(b: &Bootstrap) -> Result<Vec<u8>> {
    if b.cursor.partition != 0 || b.cursor.topic.is_empty() || b.cursor.offset < 1 {
        return Err(err("BOOTSTRAP_CURSOR_IDENTITY"));
    }
    Ok(wire::OwnerBootstrap {
        snapshot: Some(wc::snapshot_to_proto(&b.snapshot)?),
        cursor: Some(wire::JournalCursor {
            topic: b.cursor.topic.clone(),
            partition: b.cursor.partition,
            next_offset: b.cursor.offset,
        }),
    }
    .encode_to_vec())
}
pub fn decode_bootstrap(raw: &[u8]) -> Result<Bootstrap> {
    if raw.len() > 64 << 20 {
        return Err(err("BOOTSTRAP_SIZE"));
    }
    let b = wire::OwnerBootstrap::decode(raw).map_err(|_| err("BOOTSTRAP_PROTOBUF"))?;
    let c = b.cursor.ok_or_else(|| err("BOOTSTRAP_CURSOR_REQUIRED"))?;
    if c.partition != 0 || c.topic.is_empty() || c.next_offset < 1 {
        return Err(err("BOOTSTRAP_CURSOR_IDENTITY"));
    }
    Ok(Bootstrap {
        snapshot: wc::snapshot_from_proto(
            b.snapshot.ok_or_else(|| err("WIRE_SNAPSHOT_REQUIRED"))?,
        )?,
        cursor: Cursor {
            topic: c.topic,
            partition: c.partition,
            offset: c.next_offset,
        },
    })
}
