use rocksdb::{Direction, IteratorMode, Options, WriteBatch, WriteOptions, DB};
use std::path::Path;
use xone_domain::{err, hash, Record, Result, Snapshot, SnapshotRef};
use xone_kafka_transport::{Bootstrap, Cursor, Unit};
pub struct Store {
    db: DB,
    owner: String,
    assignment: u64,
    current: Option<Snapshot>,
    previous: Option<SnapshotRef>,
    cursor: Option<Cursor>,
    authority_deadline: Option<std::time::Instant>,
}
impl Store {
    pub fn open(path: &Path, owner: &str, assignment: u64) -> Result<Self> {
        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, path).map_err(|_| err("ROCKSDB_OPEN"))?;
        let previous = match db.get(b"online/current").map_err(|_| err("ROCKSDB_READ"))? {
            Some(b) => Some(
                serde_json::from_slice::<Snapshot>(&b)
                    .map_err(|_| err("ROCKSDB_STATE_CORRUPT"))?
                    .reference,
            ),
            None => None,
        };
        let cursor = db
            .get(b"online/cursor")
            .map_err(|_| err("ROCKSDB_READ"))?
            .map(|b| serde_json::from_slice(&b).map_err(|_| err("JOURNAL_CURSOR_CORRUPT")))
            .transpose()?;
        Ok(Self {
            cursor,
            authority_deadline: None,
            db,
            owner: owner.into(),
            assignment,
            current: None,
            previous,
        })
    }
    pub fn current(&self) -> Option<Snapshot> {
        if self
            .authority_deadline
            .is_some_and(|d| std::time::Instant::now() >= d)
        {
            None
        } else {
            self.current.clone()
        }
    }
    pub fn previous(&self) -> Option<SnapshotRef> {
        self.previous.clone()
    }
    pub fn set_authority_deadline(&mut self, deadline: std::time::Instant) {
        self.authority_deadline = Some(deadline);
    }
    pub fn suspend(&mut self) {
        self.current = None;
    }
    pub fn publish_baseline(&mut self, s: Snapshot) -> Result<()> {
        self.apply(s, None, "ONLINE_BASELINE_COMMIT")
    }
    fn apply(&mut self, s: Snapshot, cursor: Option<Cursor>, kind: &str) -> Result<()> {
        self.validate_unit(&s, kind)?;
        let record = Record {
            reference: s.reference.clone(),
            kind: kind.into(),
            record_id: format!(
                "{}/{}/{}",
                s.reference.block.chain_key, s.reference.online_generation, s.reference.journal_seq
            ),
            payload_hash: hash(&s.pools)?,
            pools: s.pools.clone(),
        };
        let mut batch = WriteBatch::default();
        batch.put(
            b"online/current",
            serde_json::to_vec(&s).map_err(|_| err("STATE_ENCODING"))?,
        );
        batch.put(
            format!("journal/{:020}", s.reference.journal_seq),
            serde_json::to_vec(&record).map_err(|_| err("STATE_ENCODING"))?,
        );
        if let Some(c) = &cursor {
            batch.put(
                b"online/cursor",
                serde_json::to_vec(c).map_err(|_| err("STATE_ENCODING"))?,
            );
            batch.delete(b"online/prepared");
        }
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db
            .write_opt(batch, &options)
            .map_err(|_| err("ROCKSDB_COMMIT"))?;
        if cursor.is_some() {
            self.cursor = cursor;
        }
        self.previous = Some(s.reference.clone());
        self.current = if kind == "ONLINE_SCOPE_REVOKED" {
            None
        } else {
            Some(s)
        };
        Ok(())
    }
    fn validate_unit(&self, s: &Snapshot, kind: &str) -> Result<()> {
        s.validate(&self.owner, self.assignment)?;
        if kind == "ONLINE_SCOPE_REVOKED" {
            let old: Snapshot = serde_json::from_slice(
                &self
                    .db
                    .get(b"online/current")
                    .map_err(|_| err("ROCKSDB_READ"))?
                    .ok_or_else(|| err("BASELINE_REQUIRED"))?,
            )
            .map_err(|_| err("ROCKSDB_STATE_CORRUPT"))?;
            let mut expected = old.clone();
            expected.reference.journal_seq = expected
                .reference
                .journal_seq
                .checked_add(1)
                .ok_or_else(|| err("SEQUENCE_OVERFLOW"))?;
            if s != &expected {
                return Err(err("REVOCATION_SCOPE_MISMATCH"));
            }
            return Ok(());
        }
        if kind != "ONLINE_BASELINE_COMMIT" {
            return Err(err("PREPARED_KIND_UNSUPPORTED"));
        }
        if let Some(p) = &self.previous {
            if s.reference.online_generation <= p.online_generation
                || s.reference.writer_epoch < p.writer_epoch
                || s.reference.canonical_epoch < p.canonical_epoch
                || s.reference.journal_seq <= p.journal_seq
            {
                return Err(err("STALE_BASELINE"));
            }
        }
        Ok(())
    }
    pub fn revoke_local(&mut self, old: Snapshot) -> Result<()> {
        let mut revoked = old;
        revoked.reference.journal_seq = revoked
            .reference
            .journal_seq
            .checked_add(1)
            .ok_or_else(|| err("SEQUENCE_OVERFLOW"))?;
        self.apply(revoked, None, "ONLINE_SCOPE_REVOKED")
    }
    pub fn prepare(&mut self, u: &Unit) -> Result<()> {
        self.validate_unit(&u.snapshot, &u.kind)?;
        let bytes = serde_json::to_vec(u).map_err(|_| err("STATE_ENCODING"))?;
        if let Some(old) = self
            .db
            .get(b"online/prepared")
            .map_err(|_| err("ROCKSDB_READ"))?
        {
            if old != bytes {
                return Err(err("PREPARED_CONFLICT"));
            }
            return Ok(());
        }
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db
            .put_opt(b"online/prepared", bytes, &options)
            .map_err(|_| err("ROCKSDB_PREPARE"))?;
        if u.kind == "ONLINE_SCOPE_REVOKED" {
            self.suspend();
        }
        Ok(())
    }
    pub fn prepared(&self) -> Result<Option<Unit>> {
        self.db
            .get(b"online/prepared")
            .map_err(|_| err("ROCKSDB_READ"))?
            .map(|b| serde_json::from_slice(&b).map_err(|_| err("PREPARED_CORRUPT")))
            .transpose()
    }
    pub fn discard_prepared(&self) -> Result<()> {
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db
            .delete_opt(b"online/prepared", &options)
            .map_err(|_| err("ROCKSDB_PREPARE"))
    }
    pub fn apply_prepared(&mut self, cursor: Cursor) -> Result<()> {
        let u = self.prepared()?.ok_or_else(|| err("PREPARED_REQUIRED"))?;
        if cursor.partition != 0 || cursor.offset < 1 || cursor.topic.is_empty() {
            return Err(err("JOURNAL_CURSOR_INVALID"));
        }
        self.apply(u.snapshot, Some(cursor), &u.kind)
    }
    pub fn bootstrap(&self) -> Result<Bootstrap> {
        Ok(Bootstrap {
            snapshot: self.current().ok_or_else(|| err("CHAIN_NOT_READY"))?,
            cursor: self
                .cursor
                .clone()
                .ok_or_else(|| err("BOOTSTRAP_NOT_READY"))?,
        })
    }
    pub fn journal(&self, after: u64, limit: usize) -> Result<Vec<Record>> {
        if limit == 0 || limit > 1000 {
            return Err(err("JOURNAL_LIMIT"));
        }
        let key = format!("journal/{:020}", after.saturating_add(1));
        let mut out = Vec::new();
        for item in self
            .db
            .iterator(IteratorMode::From(key.as_bytes(), Direction::Forward))
        {
            let (k, v) = item.map_err(|_| err("ROCKSDB_READ"))?;
            if !k.starts_with(b"journal/") || out.len() >= limit {
                break;
            }
            out.push(serde_json::from_slice(&v).map_err(|_| err("JOURNAL_CORRUPT"))?);
        }
        Ok(out)
    }
    pub fn history(&self, s: &Snapshot) -> Result<()> {
        self.db
            .put(
                format!("history/{}", s.reference.block.hash),
                serde_json::to_vec(s).map_err(|_| err("STATE_ENCODING"))?,
            )
            .map_err(|_| err("ROCKSDB_HISTORY_WRITE"))
    }
}
