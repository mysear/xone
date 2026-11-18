//! Fsynced local receipt journal; a send intent is durable before any RPC submission.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
use xone_domain::{err, hash, Result};
use xone_execution_plan::Context;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum State {
    Submitting,
    Submitted,
    Unknown,
    Processed,
    ReceiptPending,
    Confirmed,
    Finalized,
    Failed,
    Expired,
    SettlementMismatch,
}
impl State {
    pub fn finished(&self) -> bool {
        matches!(
            self,
            Self::Finalized | Self::Failed | Self::Expired | Self::SettlementMismatch
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: u32,
    pub signature: String,
    pub transaction_sha256: String,
    pub signed_transaction: String,
    pub context: Context,
    pub genesis_hash: String,
    pub quote_id: String,
    pub preparation: Value,
    pub evidence_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_preflight: Option<Value>,
    pub state: State,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub confirmation_status: Option<String>,
    pub slot: Option<u64>,
    pub settlement: Option<Value>,
    pub error: Option<Value>,
}
impl Receipt {
    fn identity(&self) -> Result<String> {
        let mut identity = json!({"schemaVersion":self.schema_version,"signature":self.signature,"transactionSha256":self.transaction_sha256,"signedTransaction":self.signed_transaction,"context":self.context,"genesisHash":self.genesis_hash,"quoteId":self.quote_id,"preparation":self.preparation,"evidenceHash":self.evidence_hash,"createdAtMs":self.created_at_ms});
        // Absent on old v1 journals: keep their identity and entry hashes exact.
        if let Some(witness) = &self.fee_preflight {
            identity["feePreflight"] = witness.clone();
        }
        hash(&identity)
    }
    pub fn public(&self) -> Value {
        json!({"signature":self.signature,"transactionSha256":self.transaction_sha256,"chainKey":self.context.chain_key,"caller":self.context.caller,"recipient":self.context.recipient,"sellToken":self.context.sell_token,"buyToken":self.context.buy_token,"quoteId":self.quote_id,"preparationHash":self.preparation["preparationHash"],"evidenceHash":self.evidence_hash,"feePreflight":self.fee_preflight,"state":self.state,"createdAtMs":self.created_at_ms,"updatedAtMs":self.updated_at_ms,"confirmationStatus":self.confirmation_status,"slot":self.slot,"settlement":self.settlement,"error":self.error,"signed":true,"sendAttempted":true,"productionReady":false})
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    sequence: u64,
    previous_hash: String,
    receipt: Receipt,
    entry_hash: String,
}
pub struct Journal {
    file: File,
    records: BTreeMap<String, Receipt>,
    sequence: u64,
    last_hash: String,
    max_pending: usize,
    bytes: u64,
    healthy: bool,
}
impl Journal {
    pub fn open(path: &Path, max_pending: usize) -> Result<Self> {
        if !(1..=256).contains(&max_pending) {
            return Err(err("RECEIPT_JOURNAL_LIMIT"));
        }
        if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(err("RECEIPT_JOURNAL_PATH"));
        }
        if std::fs::metadata(path).is_ok_and(|m| !m.is_file()) {
            return Err(err("RECEIPT_JOURNAL_PATH"));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let existed = path.exists();
        let mut file = options
            .open(path)
            .map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
        if !existed {
            file.sync_all()
                .map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
            if let Some(parent) = path.parent() {
                for directory in parent.ancestors() {
                    if directory.as_os_str().is_empty() {
                        break;
                    }
                    File::open(directory)
                        .and_then(|d| d.sync_all())
                        .map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
                }
            }
        }
        file.try_lock().map_err(|_| err("RECEIPT_JOURNAL_IN_USE"))?;
        let metadata = file
            .metadata()
            .map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
        if !metadata.is_file() {
            return Err(err("RECEIPT_JOURNAL_PATH"));
        }
        let size = metadata.len();
        if size > 16 * 1024 * 1024 {
            return Err(err("RECEIPT_JOURNAL_LIMIT"));
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(err("RECEIPT_JOURNAL_LIMIT"));
        }
        let mut journal = Self {
            file,
            records: BTreeMap::new(),
            sequence: 0,
            last_hash: "0".repeat(64),
            max_pending,
            bytes: 0,
            healthy: true,
        };
        let mut offset = 0;
        while offset < bytes.len() {
            let Some(end) = bytes[offset..].iter().position(|b| *b == b'\n') else {
                journal
                    .file
                    .set_len(offset as u64)
                    .and_then(|_| journal.file.sync_all())
                    .map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
                break;
            };
            let line = &bytes[offset..offset + end];
            if line.len() > 65536 {
                return Err(err("RECEIPT_JOURNAL_LIMIT"));
            }
            let entry: Entry =
                serde_json::from_slice(line).map_err(|_| err("RECEIPT_JOURNAL_CORRUPT"))?;
            if entry.sequence != journal.sequence + 1
                || entry.previous_hash != journal.last_hash
                || entry.entry_hash
                    != hash(&json!([entry.sequence, entry.previous_hash, entry.receipt]))?
            {
                return Err(err("RECEIPT_JOURNAL_CORRUPT"));
            }
            journal.validate(&entry.receipt)?;
            journal.sequence = entry.sequence;
            journal.last_hash = entry.entry_hash;
            journal
                .records
                .insert(entry.receipt.signature.clone(), entry.receipt);
            offset += end + 1;
        }
        journal.bytes = offset as u64;
        journal
            .file
            .seek(SeekFrom::End(0))
            .map_err(|_| err("RECEIPT_JOURNAL_UNAVAILABLE"))?;
        Ok(journal)
    }
    fn validate(&self, r: &Receipt) -> Result<()> {
        if r.schema_version != 1
            || r.signature.len() > 88
            || !bs58::decode(&r.signature)
                .into_vec()
                .is_ok_and(|v| v.len() == 64)
            || r.signed_transaction.len() > 1644
            || r.updated_at_ms < r.created_at_ms
        {
            return Err(err("RECEIPT_JOURNAL_CORRUPT"));
        }
        if let Some(old) = self.records.get(&r.signature) {
            if old.identity()? != r.identity()?
                || r.updated_at_ms < old.updated_at_ms
                || (matches!(
                    old.state,
                    State::Finalized | State::Failed | State::SettlementMismatch
                ) && old.state != r.state)
            {
                return Err(err("RECEIPT_TRANSITION_INVALID"));
            }
        } else if r.state != State::Submitting {
            return Err(err("RECEIPT_TRANSITION_INVALID"));
        }
        Ok(())
    }
    pub fn get(&self, signature: &str) -> Option<Receipt> {
        self.records.get(signature).cloned()
    }
    pub fn reserve(&mut self, receipt: Receipt) -> Result<(Receipt, bool)> {
        if let Some(existing) = self.records.get(&receipt.signature) {
            let mut incoming = receipt.clone();
            incoming.created_at_ms = existing.created_at_ms;
            // Concurrent repeat preflights can observe newer slots/timestamps.
            // Keep the original durable intent's witness for the same transaction.
            incoming.fee_preflight = existing.fee_preflight.clone();
            if existing.identity()? != incoming.identity()? {
                return Err(err("RECEIPT_TRANSACTION_MISMATCH"));
            }
            return Ok((existing.clone(), false));
        }
        if self.records.len() >= 4096
            || self
                .records
                .values()
                .filter(|r| !r.state.finished())
                .count()
                >= self.max_pending
        {
            return Err(err("SUBMISSION_BUSY"));
        }
        self.update(receipt.clone())?;
        Ok((receipt, true))
    }
    pub fn update(&mut self, receipt: Receipt) -> Result<()> {
        if !self.healthy {
            return Err(err("RECEIPT_JOURNAL_UNAVAILABLE"));
        }
        self.validate(&receipt)?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| err("RECEIPT_JOURNAL_LIMIT"))?;
        let digest = hash(&json!([sequence, self.last_hash, receipt]))?;
        let entry = Entry {
            sequence,
            previous_hash: self.last_hash.clone(),
            receipt: receipt.clone(),
            entry_hash: digest.clone(),
        };
        let mut bytes = serde_json::to_vec(&entry).map_err(|_| err("RECEIPT_JOURNAL_CORRUPT"))?;
        bytes.push(b'\n');
        if bytes.len() > 65536 || self.bytes + bytes.len() as u64 > 16 * 1024 * 1024 {
            return Err(err("RECEIPT_JOURNAL_LIMIT"));
        }
        if self
            .file
            .write_all(&bytes)
            .and_then(|_| self.file.sync_all())
            .is_err()
        {
            self.healthy = false;
            return Err(err("RECEIPT_JOURNAL_UNAVAILABLE"));
        }
        self.records.insert(receipt.signature.clone(), receipt);
        self.sequence = sequence;
        self.last_hash = digest;
        self.bytes += bytes.len() as u64;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn receipt() -> Receipt {
        Receipt {
            schema_version: 1,
            signature: bs58::encode([7u8; 64]).into_string(),
            transaction_sha256: "a".repeat(64),
            signed_transaction: "wire".into(),
            context: Context {
                chain_key: "solana:mainnet-beta".into(),
                caller: "caller".into(),
                recipient: "recipient".into(),
                sell_token: "a".into(),
                buy_token: "b".into(),
                slippage_bps: 50,
                deadline_ms: 100,
            },
            genesis_hash: "chain".into(),
            quote_id: "quote".into(),
            preparation: json!({"preparationHash":"p"}),
            evidence_hash: "e".into(),
            fee_preflight: None,
            state: State::Submitting,
            created_at_ms: 1,
            updated_at_ms: 1,
            confirmation_status: None,
            slot: None,
            settlement: None,
            error: None,
        }
    }
    #[test]
    fn receipt_send_intent_is_durable_exclusive_and_restored_without_resend() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipts");
        let mut journal = Journal::open(&path, 2).unwrap();
        let value = receipt();
        assert!(journal.reserve(value.clone()).unwrap().1);
        assert!(!journal.reserve(value.clone()).unwrap().1);
        assert!(Journal::open(&path, 2).is_err());
        drop(journal);
        let journal = Journal::open(&path, 2).unwrap();
        assert_eq!(
            journal.get(&value.signature).unwrap().state,
            State::Submitting
        );
    }
    #[test]
    fn receipt_journal_recovers_incomplete_tail_and_rejects_complete_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("receipts");
        {
            let mut journal = Journal::open(&path, 2).unwrap();
            journal.reserve(receipt()).unwrap();
        }
        let size = std::fs::metadata(&path).unwrap().len();
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{partial")
            .unwrap();
        {
            let j = Journal::open(&path, 2).unwrap();
            assert_eq!(j.records.len(), 1);
        }
        assert_eq!(std::fs::metadata(&path).unwrap().len(), size);
        let mut raw = std::fs::read(&path).unwrap();
        raw[20] ^= 1;
        std::fs::write(&path, raw).unwrap();
        assert!(Journal::open(&path, 2).is_err());
    }
    #[test]
    fn fee_preflight_is_immutable_durable_and_old_v1_journals_remain_readable() {
        let dir = tempfile::tempdir().unwrap();
        let legacy_path = dir.path().join("legacy");
        let old = receipt();
        let wire = serde_json::to_value(&old).unwrap();
        assert!(wire.get("feePreflight").is_none());
        let legacy_identity = hash(&json!({"schemaVersion":old.schema_version,"signature":old.signature,"transactionSha256":old.transaction_sha256,"signedTransaction":old.signed_transaction,"context":old.context,"genesisHash":old.genesis_hash,"quoteId":old.quote_id,"preparation":old.preparation,"evidenceHash":old.evidence_hash,"createdAtMs":old.created_at_ms})).unwrap();
        assert_eq!(old.identity().unwrap(), legacy_identity);
        let previous = "0".repeat(64);
        // Construct the original v1 record independently, with no new field.
        let digest = hash(&json!([1, previous, wire])).unwrap();
        std::fs::write(
            &legacy_path,
            format!(
                "{}\n",
                json!({"sequence":1,"previousHash":previous,"receipt":wire,"entryHash":digest})
            ),
        )
        .unwrap();
        let mut journal = Journal::open(&legacy_path, 2).unwrap();
        let mut loaded = journal.get(&old.signature).unwrap();
        assert!(loaded.fee_preflight.is_none());
        loaded.state = State::Submitted;
        journal.update(loaded).unwrap();
        drop(journal);
        assert_eq!(
            Journal::open(&legacy_path, 2)
                .unwrap()
                .get(&old.signature)
                .unwrap()
                .state,
            State::Submitted
        );

        let path = dir.path().join("new");
        let mut value = old;
        value.fee_preflight =
            Some(json!({"minimumSendContextSlot":42,"feeMessageSha256":"message"}));
        let mut journal = Journal::open(&path, 2).unwrap();
        journal.reserve(value.clone()).unwrap();
        let mut repeated = value.clone();
        repeated.fee_preflight.as_mut().unwrap()["minimumSendContextSlot"] = 43.into();
        let (existing, reserved) = journal.reserve(repeated.clone()).unwrap();
        assert!(!reserved);
        assert_eq!(existing.fee_preflight, value.fee_preflight);
        repeated.evidence_hash = "changed".into();
        assert!(journal.reserve(repeated).is_err());
        let mut changed = value.clone();
        changed.fee_preflight.as_mut().unwrap()["minimumSendContextSlot"] = 41.into();
        assert!(journal.update(changed).is_err());
        let mut removed = value.clone();
        removed.fee_preflight = None;
        assert!(journal.update(removed).is_err());
        value.state = State::Submitted;
        journal.update(value.clone()).unwrap();
        drop(journal);
        let restored = Journal::open(&path, 2)
            .unwrap()
            .get(&value.signature)
            .unwrap();
        assert_eq!(restored.fee_preflight, value.fee_preflight);
        assert_eq!(
            restored.public()["feePreflight"],
            value.fee_preflight.unwrap()
        );
    }
}
