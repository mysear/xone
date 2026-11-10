use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use xone_config::{Chain, ConfigSnapshot};
use xone_control::{Lease, Postgres};
use xone_domain::{err, hash, Result, Snapshot, SnapshotRef};
use xone_kafka_transport::{Cursor, KafkaProducer, Unit};
pub struct Writer {
    pub lease: Lease,
    producer: Option<KafkaProducer>,
    floor: SnapshotRef,
    takeover: bool,
    renewed: Instant,
}
impl Writer {
    pub async fn open(
        pg: &Postgres,
        cfg: &Arc<ConfigSnapshot>,
        chain: &Chain,
        previous: Option<SnapshotRef>,
        pending: Option<Unit>,
    ) -> Result<(Self, Option<Cursor>)> {
        let budget = Duration::from_millis(cfg.config.control.renewal_ms);
        tokio::time::timeout(
            budget,
            pg.project(
                &chain.key,
                &chain.owner_service,
                chain.assignment_version,
                &cfg.public_hash,
                &cfg.release,
            ),
        )
        .await
        .map_err(|_| err("CONTROL_DEADLINE"))??;
        let acquire_started = Instant::now();
        let lease = tokio::time::timeout(
            budget,
            pg.acquire(
                &chain.key,
                &chain.owner_service,
                chain.assignment_version,
                &cfg.config.control.instance_id,
                cfg.config.control.lease_ttl_ms,
            ),
        )
        .await
        .map_err(|_| err("CONTROL_DEADLINE"))??;
        let config = cfg.clone();
        let ch = chain.clone();
        let opened = tokio::task::spawn_blocking(move || {
            let k = &config.config.journal.kafka;
            let timeout = Duration::from_millis(k.transaction_timeout_ms);
            let topic = k
                .topics
                .get(&ch.key)
                .ok_or_else(|| err("KAFKA_TOPIC_CONFIG"))?;
            let producer = KafkaProducer::open(
                &k.brokers,
                topic,
                &format!("xone.canonical.{}", ch.key),
                k.chunk_bytes,
                config.config.journal.max_payload_bytes,
                timeout,
            )?;
            let (latest, cursor) = xone_kafka_transport::tail(
                &k.brokers,
                topic,
                config.config.journal.max_payload_bytes,
                k.max_tail_records,
                timeout,
            )?;
            Ok::<_, xone_domain::Error>((producer, latest, cursor))
        })
        .await
        .map_err(|_| err("KAFKA_INIT_TASK"))?;
        let (producer, latest, cursor) = match opened {
            Ok(v) => v,
            Err(e) => {
                let _ = pg.release(&lease).await;
                return Err(e);
            }
        };
        let floor = if let Some(u) = &latest {
            u.snapshot
                .validate(&chain.owner_service, chain.assignment_version)?;
            if u.snapshot.reference.block.chain_key != chain.key {
                return Err(err("JOURNAL_CHAIN_MISMATCH"));
            }
            u.snapshot.reference.clone()
        } else {
            previous
                .clone()
                .ok_or_else(|| err("EMPTY_JOURNAL_INITIAL_REFERENCE_REQUIRED"))?
        };
        let mut recovery = None;
        let mut retry = None;
        if let Some(p) = pending {
            p.snapshot
                .validate(&chain.owner_service, chain.assignment_version)?;
            if p.snapshot.reference.block.chain_key != chain.key {
                return Err(err("JOURNAL_CHAIN_MISMATCH"));
            }
            if let Some(actual) = latest {
                if p.snapshot.reference == actual.snapshot.reference {
                    if hash(&p)? != hash(&actual)? {
                        return Err(err("PREPARED_JOURNAL_CONFLICT"));
                    }
                    recovery = Some(cursor);
                } else if p.kind == "ONLINE_SCOPE_REVOKED" {
                    let mut predecessor = p.snapshot.clone();
                    predecessor.reference.journal_seq = predecessor
                        .reference
                        .journal_seq
                        .checked_sub(1)
                        .ok_or_else(|| err("SEQUENCE_OVERFLOW"))?;
                    if predecessor == actual.snapshot && actual.kind == "ONLINE_BASELINE_COMMIT" {
                        // Recover the exact immutable intent under the newly acquired lease
                        // and Kafka transactional fence. Its original reference must match
                        // the committed predecessor; it is not a new old-writer proposal.
                        retry = Some(p);
                    } else if actual.snapshot.reference.journal_seq
                        <= p.snapshot.reference.journal_seq
                        || actual.snapshot.reference.online_generation
                            <= p.snapshot.reference.online_generation
                    {
                        return Err(err("REVOCATION_FLOOR_CONFLICT"));
                    }
                }
            } else if p.kind == "ONLINE_SCOPE_REVOKED" {
                return Err(err("REVOCATION_BASELINE_MISSING"));
            }
        }
        let mut writer = Self {
            lease,
            producer: Some(producer),
            floor,
            takeover: true,
            renewed: acquire_started,
        };
        if let Some(intent) = retry {
            match writer.publish(pg, cfg, intent).await {
                Ok(cursor) => recovery = Some(cursor),
                Err(error) => {
                    writer.close(pg, cfg).await;
                    return Err(error);
                }
            }
        }
        Ok((writer, recovery))
    }
    pub async fn baseline(
        &mut self,
        pg: &Postgres,
        cfg: &ConfigSnapshot,
        snapshot: &mut Snapshot,
    ) -> Result<()> {
        let mut floor = self.floor.clone();
        if !self.takeover {
            floor.writer_epoch = 0;
        }
        if snapshot.reference.canonical_epoch > floor.canonical_epoch {
            floor.canonical_epoch = snapshot.reference.canonical_epoch;
        }
        let lease = tokio::time::timeout(
            Duration::from_millis(cfg.config.control.renewal_ms),
            pg.allocate_baseline(&self.lease, &floor),
        )
        .await
        .map_err(|_| err("CONTROL_DEADLINE"))??;
        snapshot.reference.writer_epoch = lease.writer_epoch;
        snapshot.reference.online_generation = lease.generation;
        snapshot.reference.canonical_epoch = lease.canonical_epoch;
        snapshot.reference.journal_seq = lease.journal_seq + 1;
        self.lease = lease;
        self.takeover = false;
        Ok(())
    }
    pub fn revocation(&self, old: &Snapshot) -> Result<Unit> {
        if old.reference != self.floor || old.reference.writer_epoch != self.lease.writer_epoch {
            return Err(err("REVOCATION_FLOOR_MISMATCH"));
        }
        let mut revoked = old.clone();
        revoked.reference.journal_seq = revoked
            .reference
            .journal_seq
            .checked_add(1)
            .ok_or_else(|| err("SEQUENCE_OVERFLOW"))?;
        Ok(Unit {
            kind: "ONLINE_SCOPE_REVOKED".into(),
            snapshot: revoked,
            normalized_facts: None,
        })
    }
    pub async fn heartbeat(&mut self, pg: &Postgres, cfg: &ConfigSnapshot) -> Result<()> {
        if self.renewed.elapsed() < Duration::from_millis(cfg.config.control.renewal_ms) {
            return Ok(());
        }
        let renew_started = Instant::now();
        self.lease = tokio::time::timeout(
            Duration::from_millis(cfg.config.control.renewal_ms),
            pg.renew(&self.lease, cfg.config.control.lease_ttl_ms),
        )
        .await
        .map_err(|_| err("CONTROL_DEADLINE"))??;
        self.renewed = renew_started;
        Ok(())
    }
    pub async fn publish(
        &mut self,
        pg: &Postgres,
        cfg: &Arc<ConfigSnapshot>,
        unit: Unit,
    ) -> Result<Cursor> {
        let mut producer = self
            .producer
            .take()
            .ok_or_else(|| err("KAFKA_SESSION_SEALED"))?;
        let handle = tokio::runtime::Handle::current();
        let control = pg.clone();
        let lease = self.lease.clone();
        let config = cfg.clone();
        let work_unit = unit.clone();
        let (producer, result) = tokio::task::spawn_blocking(move || {
            let result = producer.publish(&work_unit, || {
                handle.block_on(async {
                    tokio::time::timeout(
                        Duration::from_millis(config.config.control.renewal_ms),
                        control.check_owner_fence(
                            &lease,
                            &work_unit.snapshot.reference,
                            config.config.control.min_commit_remaining_ms,
                        ),
                    )
                    .await
                    .map_err(|_| err("CONTROL_DEADLINE"))?
                })
            });
            (producer, result)
        })
        .await
        .map_err(|_| err("KAFKA_PUBLISH_TASK"))?;
        self.producer = Some(producer);
        let cursor = result?;
        tokio::time::timeout(
            Duration::from_millis(cfg.config.control.renewal_ms),
            pg.check_owner_fence(&self.lease, &unit.snapshot.reference, 0),
        )
        .await
        .map_err(|_| err("CONTROL_DEADLINE"))??;
        tokio::time::timeout(
            Duration::from_millis(cfg.config.control.renewal_ms),
            pg.committed(&self.lease, &unit.snapshot.reference),
        )
        .await
        .map_err(|_| err("CONTROL_DEADLINE"))??;
        self.floor = unit.snapshot.reference;
        Ok(cursor)
    }
    pub fn deadline(&self, cfg: &ConfigSnapshot) -> Instant {
        self.renewed + Duration::from_millis(cfg.config.control.lease_ttl_ms)
    }
    pub async fn close(&mut self, pg: &Postgres, cfg: &ConfigSnapshot) {
        self.producer.take();
        let _ = tokio::time::timeout(
            Duration::from_millis(cfg.config.control.renewal_ms),
            pg.release(&self.lease),
        )
        .await;
    }
}
