//! Concrete recovery source for a bank already certified by StreamAssembler and
//! the independent manifest. It supplies exact current scope bytes. It never
//! turns an RPC context into historical state or acquires owner authority.
use crate::{
    rpc,
    scope::*,
    stream::{CapturedFrame, CommittedBank},
};
use std::collections::{BTreeMap, BTreeSet};
use xone_config::{Chain, YellowstoneConfig};
use xone_domain::{err, Result, Snapshot};

pub struct TailBaseline<'a> {
    pub bank: &'a CommittedBank,
    pub receipt: &'a CapturedFrame,
    pub timestamp_ms: u64,
}
struct VerifiedTail {
    anchor: ScopeAnchor,
    accounts: BTreeMap<String, ScopeAccount>,
    events: u64,
    bytes: u64,
}
pub struct ManifestRecoverySource {
    tail: Option<VerifiedTail>,
    source_session: crate::stream::SourceSession,
    wal_generation: u64,
    source_sequence: u64,
    parent: crate::stream::BankAnchor,
    chain_key: String,
    genesis_hash: String,
    source: String,
    buffer: StreamBuffer,
    anchor: ScopeAnchor,
    accounts: BTreeMap<String, ScopeAccount>,
    max_bytes: u64,
    max_shard: u64,
}
impl ManifestRecoverySource {
    pub fn from_committed(
        chain: &Chain,
        bank: &CommittedBank,
        timestamp_ms: u64,
        receipt: &CapturedFrame,
        max_bytes: u64,
        max_shard: u64,
    ) -> ScopeResult<Self> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        if receipt.cursor.sequence == 0
            || receipt.frame.sequence == 0
            || receipt.wal_generation == 0
            || receipt.cursor.record_sha256.len() != 64
            || !receipt
                .cursor
                .record_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || receipt.frame.source.source_id.is_empty()
            || receipt.frame.source.session_id.is_empty()
            || max_shard == 0
            || max_bytes == 0
        {
            return Err(ScopeError::Data);
        }
        let anchor = ScopeAnchor {
            height: bank.bank.slot,
            hash: bank.blockhash.clone(),
            timestamp_ms,
            cursor: receipt.cursor.sequence,
        };
        let mut accounts = BTreeMap::new();
        let mut total = 0u64;
        for (key, a) in &bank.accounts {
            let row = crate::bank_manifest::ManifestAccount {
                pubkey: bs58::encode(key).into_string(),
                owner: bs58::encode(a.owner).into_string(),
                lamports: a.lamports,
                executable: a.executable,
                rent_epoch: a.rent_epoch,
                write_slot: a.bank.slot,
                write_bank_id: a.bank.bank_id,
                write_version: a.write_version,
                data_base64: STANDARD.encode(&a.data),
                txn_signature_base64: a.txn_signature.map(|s| STANDARD.encode(s)),
            };
            let data = serde_json::to_vec(&row).map_err(|_| ScopeError::Data)?;
            total = total
                .checked_add(data.len() as u64)
                .ok_or(ScopeError::Budget)?;
            if total > max_bytes || max_shard == 0 {
                return Err(ScopeError::Budget);
            }
            accounts.insert(
                row.pubkey.clone(),
                ScopeAccount {
                    id: row.pubkey,
                    data,
                    anchor: anchor.clone(),
                },
            );
        }
        Ok(Self {
            tail: None,
            source_session: receipt.frame.source.clone(),
            wal_generation: receipt.wal_generation,
            source_sequence: receipt.frame.sequence,
            parent: bank.parent.clone(),
            chain_key: chain.key.clone(),
            genesis_hash: chain.genesis_hash.clone(),
            source: receipt.frame.source.source_id.clone(),
            buffer: StreamBuffer {
                id: format!(
                    "{}/{}/{}",
                    receipt.frame.source.session_id,
                    receipt.wal_generation,
                    receipt.cursor.record_sha256
                ),
                first_cursor: anchor.cursor,
                durable: true,
            },
            anchor,
            accounts,
            max_bytes,
            max_shard,
        })
    }
    pub fn set_tail(
        &mut self,
        chain: &Chain,
        base: TailBaseline<'_>,
        max_frame_bytes: usize,
    ) -> ScopeResult<()> {
        if base.receipt.frame.source != self.source_session
            || base.receipt.wal_generation != self.wal_generation
            || base.receipt.cursor.sequence >= self.anchor.cursor
            || self
                .source_sequence
                .saturating_sub(base.receipt.frame.sequence)
                != self.anchor.cursor - base.receipt.cursor.sequence
            || self.parent.bank != base.bank.bank
            || self.parent.blockhash != base.bank.blockhash
            || base.timestamp_ms > self.anchor.timestamp_ms
        {
            return Err(ScopeError::HeadFirst);
        }
        let baseline = Self::from_committed(
            chain,
            base.bank,
            base.timestamp_ms,
            base.receipt,
            self.max_bytes,
            self.max_shard,
        )?;
        if !baseline.accounts.keys().eq(self.accounts.keys()) {
            return Err(ScopeError::HeadFirst);
        }
        let events = self.anchor.cursor - base.receipt.cursor.sequence;
        let final_bytes = self
            .accounts
            .values()
            .map(|a| a.data.len() as u64)
            .sum::<u64>();
        let bytes = events
            .checked_mul(max_frame_bytes as u64 + 4096)
            .and_then(|n| n.checked_add(final_bytes))
            .ok_or(ScopeError::Budget)?;
        if bytes > self.max_bytes
            || baseline
                .accounts
                .values()
                .map(|a| a.data.len() as u64)
                .sum::<u64>()
                .saturating_add(final_bytes)
                > self.max_bytes
        {
            return Err(ScopeError::Budget);
        }
        self.buffer.first_cursor = baseline.anchor.cursor.saturating_add(1);
        self.tail = Some(VerifiedTail {
            anchor: baseline.anchor,
            accounts: baseline.accounts,
            events,
            bytes,
        });
        Ok(())
    }
    pub fn anchor(&self) -> ScopeAnchor {
        self.anchor.clone()
    }
    pub fn dependencies(&self) -> BTreeMap<String, ScopeDependency> {
        self.accounts
            .iter()
            .map(|(id, a)| {
                (
                    id.clone(),
                    ScopeDependency {
                        id: id.clone(),
                        max_bytes: (a.data.len() as u64).max(
                            self.tail
                                .as_ref()
                                .and_then(|t| t.accounts.get(id))
                                .map_or(0, |a| a.data.len() as u64),
                        ),
                    },
                )
            })
            .collect()
    }
    fn check(&self, cancel: &Cancellation, job: &ScopeJob) -> ScopeResult<()> {
        cancel.check()?;
        if job.fence.chain_key != self.chain_key || job.fence.chain_identity != self.genesis_hash {
            return Err(ScopeError::FenceChanged);
        }
        let keys = job
            .dependencies
            .iter()
            .map(|d| d.id.as_str())
            .collect::<BTreeSet<_>>();
        if keys.len() != self.accounts.len()
            || !keys
                .iter()
                .copied()
                .eq(self.accounts.keys().map(String::as_str))
        {
            return Err(ScopeError::Data);
        }
        if job.anchor != self.anchor
            || job.dependencies.len() != self.accounts.len()
            || job.dependencies.iter().any(|d| {
                self.accounts
                    .get(&d.id)
                    .is_none_or(|a| a.data.len() as u64 > d.max_bytes)
            })
        {
            return Err(ScopeError::Data);
        }
        let bytes = self
            .accounts
            .values()
            .map(|a| a.data.len() as u64)
            .sum::<u64>();
        if bytes > job.budget.max_account_bytes || bytes > self.max_bytes {
            return Err(ScopeError::Budget);
        }
        Ok(())
    }
    /// The ordinary service consumes this same concrete source before protocol
    /// decoding and owner prepare/publish/apply. Durable job/attempt CAS remains
    /// the online ScopeStore adapter's separate responsibility.
    pub fn verify_complete(&self, chain: &Chain) -> ScopeResult<()> {
        if chain.key != self.chain_key || chain.genesis_hash != self.genesis_hash {
            return Err(ScopeError::FenceChanged);
        }
        let expected = rpc::required_scope(chain).map_err(|_| ScopeError::Data)?;
        if !expected.iter().eq(self.accounts.keys()) {
            return Err(ScopeError::Data);
        }
        for ids in expected
            .into_iter()
            .collect::<Vec<_>>()
            .chunks(self.max_shard as usize)
        {
            for id in ids {
                let a = &self.accounts[id];
                let row: crate::bank_manifest::ManifestAccount =
                    serde_json::from_slice(&a.data).map_err(|_| ScopeError::Data)?;
                if row.pubkey != *id || row.account().is_err() || a.anchor != self.anchor {
                    return Err(ScopeError::Data);
                }
            }
        }
        Ok(())
    }
}
impl RecoverySource for ManifestRecoverySource {
    fn start_buffer(&mut self, c: &Cancellation, j: &ScopeJob) -> ScopeResult<StreamBuffer> {
        self.check(c, j)?;
        Ok(self.buffer.clone())
    }
    fn fix_anchor(
        &mut self,
        c: &Cancellation,
        j: &ScopeJob,
        b: &StreamBuffer,
    ) -> ScopeResult<ScopeAnchor> {
        self.check(c, j)?;
        if b.id != self.buffer.id {
            return Err(ScopeError::Data);
        }
        Ok(self.anchor.clone())
    }
    fn candidates(
        &mut self,
        c: &Cancellation,
        j: &ScopeJob,
        a: &ScopeAnchor,
    ) -> ScopeResult<Vec<BaselineCandidate>> {
        self.check(c, j)?;
        if *a != self.anchor {
            return Err(ScopeError::Data);
        }
        let baseline_anchor = self.tail.as_ref().map_or(&self.anchor, |t| &t.anchor);
        let baseline_accounts = self.tail.as_ref().map_or(&self.accounts, |t| &t.accounts);
        let mut b = BaselineCandidate {
            fence: j.fence.clone(),
            anchor: baseline_anchor.clone(),
            source: self.source.clone(),
            kind: if self.tail.is_some() {
                BaselineKind::RecentSnapshot
            } else {
                BaselineKind::CurrentChainState
            },
            pool_ids: j.pool_ids.clone(),
            dependencies: j.dependencies.clone(),
            account_hashes: baseline_accounts
                .iter()
                .map(|(k, a)| (k.clone(), account_hash(&a.data)))
                .collect(),
            content_hash: String::new(),
        };
        b.content_hash = baseline_hash(&b);
        Ok(vec![b])
    }
    fn restore_shard(
        &mut self,
        c: &Cancellation,
        j: &ScopeJob,
        b: &BaselineCandidate,
        ids: &[String],
    ) -> ScopeResult<Vec<ScopeAccount>> {
        self.check(c, j)?;
        if ids.len() as u64 > self.max_shard || ids.len() as u64 > j.budget.shard_accounts {
            return Err(ScopeError::Budget);
        }
        let baseline_anchor = self.tail.as_ref().map_or(&self.anchor, |t| &t.anchor);
        let baseline_accounts = self.tail.as_ref().map_or(&self.accounts, |t| &t.accounts);
        if b.anchor != *baseline_anchor
            || b.fence != j.fence
            || b.source != self.source
            || b.content_hash != baseline_hash(b)
        {
            return Err(ScopeError::Data);
        }
        let mut seen = BTreeSet::new();
        ids.iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err(ScopeError::Data);
                }
                let a = baseline_accounts.get(id).ok_or(ScopeError::Data)?;
                if b.account_hashes.get(id) != Some(&account_hash(&a.data)) {
                    return Err(ScopeError::Data);
                }
                Ok(a.clone())
            })
            .collect()
    }
    fn align_tail(
        &mut self,
        c: &Cancellation,
        j: &ScopeJob,
        buffer: &StreamBuffer,
        b: &BaselineCandidate,
        accounts: &[ScopeAccount],
    ) -> ScopeResult<ScopeTail> {
        self.check(c, j)?;
        let baseline_anchor = self.tail.as_ref().map_or(&self.anchor, |t| &t.anchor);
        let baseline_accounts = self.tail.as_ref().map_or(&self.accounts, |t| &t.accounts);
        if buffer.id != self.buffer.id
            || b.anchor != *baseline_anchor
            || b.content_hash != baseline_hash(b)
            || accounts.len() != self.accounts.len()
        {
            return Err(ScopeError::HeadFirst);
        }
        let mut seen = BTreeSet::new();
        let mut bytes = 0;
        for a in accounts {
            if !seen.insert(&a.id)
                || baseline_accounts
                    .get(&a.id)
                    .is_none_or(|v| v.data != a.data || a.anchor != v.anchor)
            {
                return Err(ScopeError::Data);
            }
            bytes += a.data.len() as u64;
        }
        Ok(ScopeTail {
            from: baseline_anchor.clone(),
            to: self.anchor.clone(),
            complete: true,
            events: self.tail.as_ref().map_or(0, |t| t.events),
            bytes: self.tail.as_ref().map_or(bytes, |t| t.bytes),
            accounts: if self.tail.is_some() {
                self.accounts.values().cloned().collect()
            } else {
                accounts.to_vec()
            },
        })
    }
    fn release_buffer(&mut self, b: &StreamBuffer) -> ScopeResult<()> {
        if b.id != self.buffer.id {
            return Err(ScopeError::Data);
        }
        Ok(())
    }
}
pub fn restore_snapshot(
    chain: &Chain,
    bank: &CommittedBank,
    timestamp_ms: u64,
    record: &CapturedFrame,
    cfg: &YellowstoneConfig,
) -> Result<Snapshot> {
    let source = ManifestRecoverySource::from_committed(
        chain,
        bank,
        timestamp_ms,
        record,
        cfg.max_stream_bytes as u64,
        100,
    )
    .map_err(|e| err(&e.to_string()))?;
    source
        .verify_complete(chain)
        .map_err(|e| err(&e.to_string()))?;
    rpc::snapshot_from_bank(chain, bank, timestamp_ms)
}
