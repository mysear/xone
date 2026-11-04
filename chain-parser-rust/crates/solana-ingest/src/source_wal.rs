//! Durable raw JSON-RPC reception evidence. It never grants online readiness.
use rocksdb::{DBRecoveryMode, IteratorMode, Options, WriteBatch, WriteOptions, DB};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use xone_domain::{err, Result};

const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const META: &[u8] = b"meta";
#[derive(Clone, Copy, Debug)]
pub struct WalLimits {
    pub max_response_bytes: usize,
    pub max_records: usize,
}
impl WalLimits {
    fn record_bytes(self) -> Result<usize> {
        self.max_response_bytes
            .checked_mul(2)
            .and_then(|v| v.checked_add(2048))
            .ok_or_else(|| err("SOURCE_WAL_LIMITS"))
    }
    fn total_bytes(self) -> Result<usize> {
        self.record_bytes()?
            .checked_mul(self.max_records)
            .ok_or_else(|| err("SOURCE_WAL_LIMITS"))
    }
}
#[derive(Clone, Debug)]
pub struct CapturedResponse {
    pub sequence: u64,
    pub session_id: u64,
    pub chain_key: String,
    pub source_id: String,
    pub receive_timestamp_ms: u64,
    pub method: String,
    pub params: Value,
    pub response: Vec<u8>,
    pub payload_sha256: String,
}
pub struct SourceWal {
    db: DB,
    path: PathBuf,
    poisoned: bool,
    chain: String,
    limits: WalLimits,
    session: u64,
    sequence: u64,
    bytes: usize,
    last_hash: String,
    last: BTreeMap<String, (String, u64)>,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn corrupt() -> xone_domain::Error {
    err("SOURCE_WAL_CORRUPT")
}
fn key(sequence: u64) -> String {
    format!("record/{sequence:020}")
}
fn u64_field(v: &Value, name: &str) -> Result<u64> {
    v[name].as_u64().ok_or_else(corrupt)
}
fn string_field(v: &Value, name: &str) -> Result<String> {
    v[name].as_str().map(str::to_owned).ok_or_else(corrupt)
}
fn bounded_head_bytes(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    reader
        .take(2049)
        .read_to_end(&mut raw)
        .map_err(|_| corrupt())?;
    if raw.len() > 2048 {
        return Err(corrupt());
    }
    Ok(raw)
}

impl SourceWal {
    pub fn open(path: &Path, chain: &str, limits: WalLimits) -> Result<Self> {
        if !chain.starts_with("solana:")
            || chain.len() == 7
            || limits.max_response_bytes == 0
            || limits.max_records == 0
        {
            return Err(err("SOURCE_WAL_LIMITS"));
        }
        limits.total_bytes()?;
        let mut options = Options::default();
        options.create_if_missing(true);
        options.set_wal_recovery_mode(DBRecoveryMode::AbsoluteConsistency);
        let db = DB::open(&options, path).map_err(|_| err("SOURCE_WAL_OPEN"))?;
        let meta = db.get(META).map_err(|_| err("SOURCE_WAL_READ"))?;
        let (session, sequence, bytes, last_hash) = if let Some(raw) = meta {
            let v: Value = serde_json::from_slice(&raw).map_err(|_| corrupt())?;
            if v["schema"] != 1 || v["chain_key"] != chain {
                return Err(err("SOURCE_WAL_IDENTITY"));
            }
            let sequence = u64_field(&v, "sequence")?;
            let bytes = usize::try_from(u64_field(&v, "bytes")?).map_err(|_| corrupt())?;
            if sequence > limits.max_records as u64 || bytes > limits.total_bytes()? {
                return Err(err("SOURCE_WAL_CAPACITY"));
            }
            (
                u64_field(&v, "session")?,
                sequence,
                bytes,
                string_field(&v, "last_hash")?,
            )
        } else {
            (0, 0, 0, ZERO_HASH.into())
        };
        let mut wal = Self {
            db,
            path: path.to_owned(),
            poisoned: false,
            chain: chain.into(),
            limits,
            session,
            sequence,
            bytes,
            last_hash,
            last: BTreeMap::new(),
        };
        wal.verify()?;
        wal.verify_durable_head()?;
        wal.session = wal
            .session
            .checked_add(1)
            .ok_or_else(|| err("SOURCE_WAL_SESSION_OVERFLOW"))?;
        wal.commit(None)?;
        Ok(wal)
    }
    pub fn status(&self) -> Value {
        json!({"chain_key":self.chain,"session_id":self.session,"records":self.sequence,"retained_bytes":self.bytes,"max_records":self.limits.max_records,"max_response_bytes":self.limits.max_response_bytes,"max_retained_bytes":self.limits.total_bytes().ok(),"last_record_sha256":self.last_hash,"evidence_only":true,"confers_readiness":false,"write_poisoned":self.poisoned})
    }
    pub fn len(&self) -> u64 {
        self.sequence
    }
    pub fn is_empty(&self) -> bool {
        self.sequence == 0
    }
    fn metadata(&self) -> Value {
        json!({"schema":1,"chain_key":self.chain,"session":self.session,"sequence":self.sequence,"bytes":self.bytes,"last_hash":self.last_hash})
    }
    fn verify_durable_head(&self) -> Result<()> {
        let head = self.path.join("SOURCE_HEAD");
        let mut file = match std::fs::File::open(&head) {
            Ok(file) => file,
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && self.session == 0
                    && self.sequence == 0 =>
            {
                return Ok(())
            }
            Err(_) => return Err(corrupt()),
        };
        let raw = bounded_head_bytes(&mut file)?;
        let mut v: Value = serde_json::from_slice(&raw).map_err(|_| corrupt())?;
        let checksum = v
            .as_object_mut()
            .and_then(|v| v.remove("checksum"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(corrupt)?;
        if sha(&serde_json::to_vec(&v).map_err(|_| corrupt())?) != checksum {
            return Err(corrupt());
        }
        if v["schema"] != 1 || v["chain_key"] != self.chain {
            return Err(corrupt());
        }
        let acknowledged = u64_field(&v, "sequence")?;
        let head_session = u64_field(&v, "session")?;
        if acknowledged > self.sequence || head_session == 0 || head_session > self.session {
            return Err(corrupt());
        }
        let expected = if acknowledged == 0 {
            ZERO_HASH.to_owned()
        } else {
            let bytes = self
                .db
                .get(key(acknowledged))
                .map_err(|_| corrupt())?
                .ok_or_else(corrupt)?;
            let (record, _) = self.decode(acknowledged, &bytes)?;
            if record.session_id > head_session {
                return Err(corrupt());
            }
            sha(&bytes)
        };
        if string_field(&v, "last_hash")? != expected {
            return Err(corrupt());
        }
        Ok(())
    }
    fn persist_head(&self) -> Result<()> {
        let mut marker = json!({"schema":1,"chain_key":self.chain,"session":self.session,"sequence":self.sequence,"last_hash":self.last_hash});
        let checksum = sha(&serde_json::to_vec(&marker).map_err(|_| err("SOURCE_WAL_ENCODING"))?);
        marker["checksum"] = json!(checksum);
        let raw = serde_json::to_vec(&marker).map_err(|_| err("SOURCE_WAL_ENCODING"))?;
        let temporary = self.path.join("SOURCE_HEAD.tmp");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|_| err("SOURCE_WAL_WRITE"))?;
        file.write_all(&raw)
            .and_then(|_| file.sync_all())
            .map_err(|_| err("SOURCE_WAL_WRITE"))?;
        std::fs::rename(&temporary, self.path.join("SOURCE_HEAD"))
            .map_err(|_| err("SOURCE_WAL_WRITE"))?;
        std::fs::File::open(&self.path)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| err("SOURCE_WAL_WRITE"))
    }
    fn commit(&self, record: Option<(u64, &[u8])>) -> Result<()> {
        let mut batch = WriteBatch::default();
        if let Some((sequence, bytes)) = record {
            batch.put(key(sequence), bytes);
        }
        batch.put(
            META,
            serde_json::to_vec(&self.metadata()).map_err(|_| err("SOURCE_WAL_ENCODING"))?,
        );
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db
            .write_opt(batch, &options)
            .map_err(|_| err("SOURCE_WAL_WRITE"))?;
        self.persist_head()
    }
    pub fn append(
        &mut self,
        url: &str,
        method: &str,
        params: &Value,
        response: &[u8],
        at: u64,
    ) -> Result<u64> {
        if self.poisoned {
            return Err(err("SOURCE_WAL_REOPEN_REQUIRED"));
        }
        if !matches!(
            method,
            "getGenesisHash" | "getMultipleAccounts" | "getBlock"
        ) {
            return Err(err("SOURCE_WAL_METHOD"));
        }
        if response.len() > self.limits.max_response_bytes {
            return Err(err("SOURCE_WAL_RESPONSE_LIMIT"));
        }
        let request = serde_json::to_vec(params).map_err(|_| err("SOURCE_WAL_ENCODING"))?;
        if request.len() > self.limits.max_response_bytes {
            return Err(err("SOURCE_WAL_REQUEST_LIMIT"));
        }
        let source = sha(url.as_bytes());
        let payload_hash = sha(response);
        let mut h = Sha256::new();
        h.update((request.len() as u64).to_be_bytes());
        h.update(&request);
        h.update(response);
        let fingerprint = format!("{:x}", h.finalize());
        let dedup = format!("{source}/{method}");
        if let Some((old, sequence)) = self.last.get(&dedup) {
            if old == &fingerprint {
                return Ok(*sequence);
            }
        }
        if self.sequence >= self.limits.max_records as u64 {
            return Err(err("SOURCE_WAL_CAPACITY"));
        }
        let next = self.sequence.checked_add(1).ok_or_else(corrupt)?;
        let header = json!({"schema":1,"chain_key":self.chain,"sequence":next,"session":self.session,"source_id":source,"source_kind":"jsonrpc-current-baseline","provider_version":"unattested","receive_timestamp_ms":at,"method":method,"params":params,"response_len":response.len(),"payload_sha256":payload_hash,"previous_hash":self.last_hash});
        let header = serde_json::to_vec(&header).map_err(|_| err("SOURCE_WAL_ENCODING"))?;
        let mut encoded = Vec::with_capacity(4 + header.len() + response.len() + 32);
        encoded.extend(
            u32::try_from(header.len())
                .map_err(|_| err("SOURCE_WAL_REQUEST_LIMIT"))?
                .to_be_bytes(),
        );
        encoded.extend(header);
        encoded.extend(response);
        let hash = Sha256::digest(&encoded);
        encoded.extend(hash);
        let bytes = self
            .bytes
            .checked_add(encoded.len())
            .ok_or_else(|| err("SOURCE_WAL_CAPACITY"))?;
        if encoded.len() > self.limits.record_bytes()? || bytes > self.limits.total_bytes()? {
            return Err(err("SOURCE_WAL_CAPACITY"));
        }
        let previous = (self.sequence, self.bytes, self.last_hash.clone());
        self.sequence = next;
        self.bytes = bytes;
        self.last_hash = sha(&encoded);
        if let Err(e) = self.commit(Some((next, &encoded))) {
            // A failed fsync/marker write has an unknown durability outcome.
            // Even an old dedup hit must not bypass a verified reopen.
            self.poisoned = true;
            self.sequence = previous.0;
            self.bytes = previous.1;
            self.last_hash = previous.2;
            return Err(e);
        }
        self.last.insert(dedup, (fingerprint, next));
        Ok(next)
    }
    fn decode(&self, sequence: u64, raw: &[u8]) -> Result<(CapturedResponse, String)> {
        if raw.len() < 36 || raw.len() > self.limits.record_bytes()? {
            return Err(corrupt());
        }
        let split = raw.len() - 32;
        if Sha256::digest(&raw[..split])[..] != raw[split..] {
            return Err(corrupt());
        }
        let header_len = u32::from_be_bytes(raw[..4].try_into().map_err(|_| corrupt())?) as usize;
        let end = 4usize
            .checked_add(header_len)
            .filter(|end| *end <= split)
            .ok_or_else(corrupt)?;
        let header: Value = serde_json::from_slice(&raw[4..end]).map_err(|_| corrupt())?;
        if header["schema"] != 1
            || header["chain_key"] != self.chain
            || u64_field(&header, "sequence")? != sequence
        {
            return Err(corrupt());
        }
        let response = &raw[end..split];
        let payload_hash = string_field(&header, "payload_sha256")?;
        if response.len() > self.limits.max_response_bytes
            || response.len() as u64 != u64_field(&header, "response_len")?
            || sha(response) != payload_hash
        {
            return Err(corrupt());
        }
        let record = CapturedResponse {
            sequence,
            session_id: u64_field(&header, "session")?,
            chain_key: self.chain.clone(),
            source_id: string_field(&header, "source_id")?,
            receive_timestamp_ms: u64_field(&header, "receive_timestamp_ms")?,
            method: string_field(&header, "method")?,
            params: header["params"].clone(),
            response: response.to_vec(),
            payload_sha256: payload_hash,
        };
        Ok((record, string_field(&header, "previous_hash")?))
    }
    pub fn read(&self, sequence: u64) -> Result<CapturedResponse> {
        let raw = self
            .db
            .get(key(sequence))
            .map_err(|_| err("SOURCE_WAL_READ"))?
            .ok_or_else(|| err("SOURCE_WAL_NOT_FOUND"))?;
        self.decode(sequence, &raw).map(|(record, _)| record)
    }
    fn verify(&self) -> Result<()> {
        let mut count = 0u64;
        let mut bytes = 0usize;
        let mut previous = ZERO_HASH.to_owned();
        let mut session = 0u64;
        for entry in self.db.iterator(IteratorMode::Start) {
            let (k, v) = entry.map_err(|_| corrupt())?;
            if k.as_ref() == META {
                continue;
            }
            count = count.checked_add(1).ok_or_else(corrupt)?;
            if count > self.sequence || k.as_ref() != key(count).as_bytes() {
                return Err(corrupt());
            }
            let (record, parent) = self.decode(count, &v)?;
            if parent != previous
                || record.session_id < session
                || record.session_id == 0
                || record.session_id > self.session
            {
                return Err(corrupt());
            }
            session = record.session_id;
            previous = sha(&v);
            bytes = bytes.checked_add(v.len()).ok_or_else(corrupt)?;
        }
        if count != self.sequence || bytes != self.bytes || previous != self.last_hash {
            return Err(corrupt());
        }
        Ok(())
    }
}

/// The recorder is deliberately synchronous: no response may escape to a
/// protocol decoder until the local RocksDB write-ahead evidence is durable.
#[derive(Clone)]
pub struct Recorder(Arc<Mutex<SourceWal>>, Arc<tokio::sync::Semaphore>);
impl Recorder {
    pub fn new(wal: SourceWal) -> Self {
        Self(
            Arc::new(Mutex::new(wal)),
            Arc::new(tokio::sync::Semaphore::new(1)),
        )
    }
    pub fn capture(&self, url: &str, method: &str, params: &Value, response: &[u8]) -> Result<u64> {
        self.0.lock().map_err(|_| err("SOURCE_WAL_LOCK"))?.append(
            url,
            method,
            params,
            response,
            xone_observability::now_ms(),
        )
    }
    pub async fn capture_async(
        &self,
        url: &str,
        method: &str,
        params: &Value,
        response: &[u8],
    ) -> Result<u64> {
        // Capture reception time before queueing disk work; filesystem/lock
        // latency must not be presented as the response's arrival time.
        let received_at = xone_observability::now_ms();
        // Admit before copying response bytes or spawning work. The owned
        // permit lives inside the actual blocking task, including after an
        // outer baseline timeout drops its JoinHandle.
        let permit = self
            .1
            .clone()
            .try_acquire_owned()
            .map_err(|_| err("SOURCE_WAL_BUSY"))?;
        let recorder = self.clone();
        let url = url.to_owned();
        let method = method.to_owned();
        let params = params.clone();
        let response = response.to_vec();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            recorder
                .0
                .lock()
                .map_err(|_| err("SOURCE_WAL_LOCK"))?
                .append(&url, &method, &params, &response, received_at)
        })
        .await
        .map_err(|_| err("SOURCE_WAL_TASK"))?
    }
    pub fn read(&self, sequence: u64) -> Result<CapturedResponse> {
        self.0
            .lock()
            .map_err(|_| err("SOURCE_WAL_LOCK"))?
            .read(sequence)
    }
    pub fn status(&self) -> Result<Value> {
        // A slow evidence fsync must not block the async status/health server.
        let wal = self.0.try_lock().map_err(|e| match e {
            std::sync::TryLockError::WouldBlock => err("SOURCE_WAL_BUSY"),
            std::sync::TryLockError::Poisoned(_) => err("SOURCE_WAL_LOCK"),
        })?;
        Ok(wal.status())
    }
    pub fn len(&self) -> Result<u64> {
        Ok(self.0.lock().map_err(|_| err("SOURCE_WAL_LOCK"))?.len())
    }
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    struct Release(Option<std::sync::mpsc::Sender<()>>);
    impl Drop for Release {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timed_out_captures_do_not_accumulate_blocking_jobs_or_change_arrival_time() {
        let path = std::env::temp_dir().join(format!("xone-wal-admission-{}", std::process::id()));
        let recorder = Recorder::new(
            SourceWal::open(
                &path,
                "solana:localnet",
                WalLimits {
                    max_response_bytes: 4096,
                    max_records: 8,
                },
            )
            .unwrap(),
        );
        let other = recorder.clone();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release = Release(Some(release_tx));
        let thread = std::thread::spawn(move || {
            let _lock = other.0.lock().unwrap();
            locked_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        locked_rx.recv().unwrap();
        let observed = recorder.clone();
        let mut first = tokio::spawn(async move {
            observed
                .capture_async("rpc", "getBlock", &json!([1]), b"one")
                .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut first)
                .await
                .is_err()
        );
        first.abort();
        assert_eq!(recorder.status().unwrap_err().0, "SOURCE_WAL_BUSY");
        let received_before_release = xone_observability::now_ms();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(30),
            recorder.capture_async("rpc", "getBlock", &json!([2]), b"two"),
        )
        .await;
        drop(release);
        thread.join().unwrap();
        assert!(
            matches!(result,Ok(Err(ref e))if e.0=="SOURCE_WAL_BUSY"),
            "{result:?}"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if recorder.len().unwrap() == 1 {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            tokio::task::yield_now().await;
        }
        assert!(recorder.read(1).unwrap().receive_timestamp_ms <= received_before_release);
        drop(recorder);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[cfg(test)]
mod marker_budget_tests {
    use super::*;
    struct Oversized {
        remaining: usize,
        consumed: usize,
    }
    impl std::io::Read for Oversized {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let size = self.remaining.min(buffer.len());
            buffer[..size].fill(b'x');
            self.remaining -= size;
            self.consumed += size;
            Ok(size)
        }
    }
    #[test]
    fn oversized_head_reads_only_limit_plus_one_before_rejecting() {
        let mut source = Oversized {
            remaining: 1024 * 1024 * 1024,
            consumed: 0,
        };
        assert!(bounded_head_bytes(&mut source).is_err());
        assert_eq!(source.consumed, 2049);
    }
}
