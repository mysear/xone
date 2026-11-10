//! Bounded stream reconstruction and durable reception of exact protobuf message bytes.
//!
//! Transport must capture BEFORE prost decoding (a tonic typed message re-encoding is
//! not raw reception evidence). Local sequence numbers count every received frame,
//! including heartbeat/unsupported messages; they are not a server resume token.
//! `block_meta` counts transactions, not account updates. An independent, exact
//! versioned-bank account manifest is required to seal the subscribed scope. Native
//! Yellowstone metadata alone MUST map to `UnknownYellowstoneMetadata`, never to
//! `VersionedBankManifest`. This module supplies no such external bank authority.
use rocksdb::{DBRecoveryMode, IteratorMode, Options, WriteBatch, WriteOptions, DB};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use xone_domain::{err, Result};

pub type Pubkey = [u8; 32];
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceSession {
    /// Stable configured provider identity; never an endpoint containing credentials.
    pub source_id: String,
    /// Must change on every connection/reconnect, including within one process.
    pub session_id: String,
}
impl SourceSession {
    fn valid(&self) -> bool {
        [&self.source_id, &self.session_id]
            .iter()
            .all(|s| !s.is_empty() && s.len() <= 128)
    }
}
pub fn compare_write_versions(
    left: &SourceSession,
    a: u64,
    right: &SourceSession,
    b: u64,
) -> Option<Ordering> {
    (left.valid() && left == right).then(|| a.cmp(&b))
}
/// Native bank identity is local to SourceSession, never comparable across reconnects.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct BankId {
    pub slot: u64,
    pub bank_id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentBank {
    pub slot: u64,
    pub blockhash: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BankAnchor {
    pub bank: BankId,
    pub blockhash: String,
}
fn valid_hash(hash: &str) -> bool {
    !hash.is_empty() && hash.len() <= 128
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountUpdate {
    pub bank: BankId,
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub executable: bool,
    pub rent_epoch: u64,
    pub write_version: u64,
    pub is_startup: bool,
    pub txn_signature: Option<[u8; 64]>,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionUpdate {
    pub bank: BankId,
    pub index: u64,
    pub is_vote: bool,
    pub signature: [u8; 64],
    /// Exact nested transaction protobuf bytes, preserving fields outside this projection.
    pub transaction: Vec<u8>,
    /// Exact nested TransactionStatusMeta protobuf bytes (failed transactions included).
    pub meta: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockMetadata {
    pub bank: BankId,
    pub parent: ParentBank,
    pub blockhash: String,
    /// Only meaningful for a complete unfiltered transaction subscription.
    pub executed_transaction_count: u64,
    /// Retained evidence; not an account-completion proof.
    pub entry_count: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SlotStatus {
    CreatedBank,
    Processed,
    Confirmed,
    Finalized,
    Dead,
    Skipped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BanklessSlotStatus {
    FirstShredReceived,
    Completed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotUpdate {
    pub bank: BankId,
    pub parent_slot: Option<u64>,
    pub status: SlotStatus,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedAccountVersion {
    pub write_version: u64,
    pub account_sha256: [u8; 32],
}
impl ExpectedAccountVersion {
    /// Populate from the independent bank authority. Computing this from the
    /// same unverified stream does not provide an independent completion proof.
    pub fn of(account: &AccountUpdate) -> Self {
        Self {
            write_version: account.write_version,
            account_sha256: account_fingerprint(account),
        }
    }
}
/// Canonical identity of all projected account fields, including its native bank.
pub fn account_fingerprint(account: &AccountUpdate) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"xone/solana/account-v1\0");
    h.update(account.bank.slot.to_be_bytes());
    h.update(account.bank.bank_id.to_be_bytes());
    h.update(account.pubkey);
    h.update(account.owner);
    h.update(account.lamports.to_be_bytes());
    h.update([u8::from(account.executable), u8::from(account.is_startup)]);
    h.update(account.rent_epoch.to_be_bytes());
    h.update(account.write_version.to_be_bytes());
    h.update((account.data.len() as u64).to_be_bytes());
    h.update(&account.data);
    match account.txn_signature {
        Some(signature) => {
            h.update([1]);
            h.update(signature);
        }
        None => h.update([0]),
    };
    h.finalize().into()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountCompletion {
    /// A trusted adapter's independently verified versioned bank manifest. The
    /// subscription must contain every write for exactly this fixed account scope.
    /// All scope keys appear, and every final version is from this source/session.
    /// Without trusted parent accounts every scope key needs an observed update.
    /// With a complete trusted parent, keys with no writes may retain their original
    /// write bank, provided this complete manifest independently confirms them.
    /// Neither getMultipleAccounts/minContextSlot nor getBlock supplies this proof.
    VersionedBankManifest {
        bank: BankId,
        parent: ParentBank,
        blockhash: String,
        expected_account_updates: u64,
        final_versions: BTreeMap<Pubkey, ExpectedAccountVersion>,
    },
    UnknownYellowstoneMetadata {
        bank: BankId,
    },
}
/// External versioned account state. Rows retain the bank of their last write;
/// the independent manifest below binds that complete state to this exact anchor.
#[derive(Clone, Debug)]
pub struct VersionedAccountSnapshot {
    pub source: SourceSession,
    pub anchor: BankAnchor,
    pub accounts: BTreeMap<Pubkey, Arc<AccountUpdate>>,
}
/// Authenticity is the caller's authority boundary. A manifest computed from the
/// same unverified provider block is not an independent snapshot proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountSnapshotManifest {
    pub source: SourceSession,
    pub anchor: BankAnchor,
    pub final_versions: BTreeMap<Pubkey, ExpectedAccountVersion>,
}
/// Caller-verified skipped slots on one exact source/session and canonical parent.
/// Every slot strictly between parent and target must appear once, in order.
/// This is never constructed from null RPC responses, silence or numeric gaps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedRangeProof {
    pub source: SourceSession,
    pub parent: BankAnchor,
    pub target: BankAnchor,
    pub skipped_slots: Vec<u64>,
}
/// Consistent provider-returned event list; this is NOT an independent bank proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderBlock {
    pub metadata: BlockMetadata,
    pub updated_account_count: u64,
    pub accounts: Vec<AccountUpdate>,
    pub transactions: Vec<TransactionUpdate>,
    /// Exact nested entry messages; count checked before admitting the event set.
    pub entries: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum YellowstoneEvent {
    BanklessSlot {
        slot: u64,
        parent_slot: Option<u64>,
        status: BanklessSlotStatus,
    },
    ProviderBlock(ProviderBlock),
    Account(AccountUpdate),
    Transaction(TransactionUpdate),
    Metadata(BlockMetadata),
    Slot(SlotUpdate),
    /// Application completion input; native projection never emits this variant.
    /// Prefer seal_accounts() for external proofs so receipt sequences stay intact.
    AccountsComplete(AccountCompletion),
    /// Ping/pong/control frames still consume a source sequence.
    Heartbeat,
    /// A connection change is a hard boundary, even if slot numbers look contiguous.
    Reconnect,
    Unsupported {
        description: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedEvent {
    pub source: SourceSession,
    pub sequence: u64,
    pub event: YellowstoneEvent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Live { wal_generation: u64 },
    Replay,
}
#[derive(Clone, Copy, Debug)]
pub struct StreamLimits {
    /// Retained payload capacities, account/scope/manifest keys and conservative
    /// allocation overhead; anchor verification also budgets the supplied manifest.
    pub max_bytes: usize,
    /// Staged banks plus every individually proved skipped slot. A pending skip
    /// range also reserves capacity for its bound actual target bank.
    pub max_slots: usize,
    /// Trusted parent rows, staged records and skip proof claims. This is separate
    /// from the source receipt sequence, which only raw Geyser frames consume.
    pub max_updates: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamUsage {
    pub bytes: usize,
    pub slots: usize,
    pub updates: usize,
}
#[derive(Default)]
struct SlotBuffer {
    versions: BTreeMap<(Pubkey, u64), Arc<AccountUpdate>>,
    transactions: BTreeMap<u64, TransactionUpdate>,
    metadata: Option<BlockMetadata>,
    completion: Option<AccountCompletion>,
    status: Option<SlotUpdate>,
    bytes: usize,
    updates: usize,
}
#[derive(Debug)]
pub struct CommittedBank {
    pub bank: BankId,
    pub blockhash: String,
    pub parent: BankAnchor,
    pub accounts: BTreeMap<Pubkey, Arc<AccountUpdate>>,
    pub account_versions: BTreeMap<(Pubkey, u64), Arc<AccountUpdate>>,
    pub transactions: BTreeMap<u64, TransactionUpdate>,
}
/// Single active source/session; mixing provider versions is never reconciliation.
/// A new anchor must be an independently verified baseline, not a WAL cursor.
/// CommittedBank is a local reconstruction result; external owner/fencing/publication
/// checks and the authority of VersionedBankManifest belong to the caller.
pub struct StreamAssembler {
    limits: StreamLimits,
    usage: StreamUsage,
    base_bytes: usize,
    parent_accounts: BTreeMap<Pubkey, Arc<AccountUpdate>>,
    parent_bytes: usize,
    skipped: Option<SkippedRangeProof>,
    skipped_bytes: usize,
    slots: BTreeMap<BankId, SlotBuffer>,
    tip: Option<BankAnchor>,
    source: Option<SourceSession>,
    scope: BTreeSet<Pubkey>,
    next_sequence: u64,
    generation: u64,
    mode: Option<Admission>,
    reanchor: bool,
    committed: bool,
}
impl StreamAssembler {
    pub fn new(limits: StreamLimits) -> Result<Self> {
        if limits.max_bytes == 0 || limits.max_slots == 0 || limits.max_updates == 0 {
            return Err(err("STREAM_LIMITS"));
        }
        Ok(Self {
            limits,
            usage: StreamUsage::default(),
            base_bytes: 0,
            parent_accounts: BTreeMap::new(),
            parent_bytes: 0,
            skipped: None,
            skipped_bytes: 0,
            slots: BTreeMap::new(),
            tip: None,
            source: None,
            scope: BTreeSet::new(),
            next_sequence: 0,
            generation: 0,
            mode: None,
            reanchor: true,
            committed: false,
        })
    }
    pub fn anchor(
        &mut self,
        anchor: BankAnchor,
        source: SourceSession,
        next_sequence: u64,
        scope: BTreeSet<Pubkey>,
        wal_generation: u64,
    ) -> Result<()> {
        self.invalidate();
        if !valid_hash(&anchor.blockhash)
            || !source.valid()
            || scope.is_empty()
            || scope.len() > self.limits.max_updates
            || next_sequence == 0
            || wal_generation == 0
        {
            return Err(err("STREAM_ANCHOR"));
        }
        let base_bytes = scope
            .len()
            .checked_mul(96)
            .and_then(|n| n.checked_add(1024))
            .and_then(|n| n.checked_add(anchor.blockhash.capacity().max(128)))
            .and_then(|n| n.checked_add(source.source_id.capacity()))
            .and_then(|n| n.checked_add(source.session_id.capacity()))
            .filter(|n| *n <= self.limits.max_bytes)
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        self.base_bytes = base_bytes;
        self.usage.bytes = base_bytes;
        self.tip = Some(anchor);
        self.source = Some(source);
        self.scope = scope;
        self.next_sequence = next_sequence;
        self.generation = wal_generation;
        self.mode = None;
        self.reanchor = false;
        Ok(())
    }
    /// Establish a complete externally verified scope. This is an additive API;
    /// anchor() still starts without parent account state. Neither grants ready.
    pub fn anchor_versioned(
        &mut self,
        snapshot: VersionedAccountSnapshot,
        manifest: AccountSnapshotManifest,
        next_sequence: u64,
        scope: BTreeSet<Pubkey>,
        wal_generation: u64,
    ) -> Result<()> {
        let result =
            self.anchor_versioned_inner(snapshot, manifest, next_sequence, scope, wal_generation);
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    pub fn anchor_accounts(
        &mut self,
        snapshot: VersionedAccountSnapshot,
        manifest: AccountSnapshotManifest,
        next_sequence: u64,
        scope: BTreeSet<Pubkey>,
        wal_generation: u64,
    ) -> Result<()> {
        self.anchor_versioned(snapshot, manifest, next_sequence, scope, wal_generation)
    }
    fn anchor_versioned_inner(
        &mut self,
        snapshot: VersionedAccountSnapshot,
        manifest: AccountSnapshotManifest,
        next_sequence: u64,
        scope: BTreeSet<Pubkey>,
        wal_generation: u64,
    ) -> Result<()> {
        self.anchor(
            snapshot.anchor,
            snapshot.source,
            next_sequence,
            scope,
            wal_generation,
        )?;
        if self.source.as_ref() != Some(&manifest.source)
            || self.tip.as_ref() != Some(&manifest.anchor)
            || !snapshot.accounts.keys().eq(self.scope.iter())
            || !manifest.final_versions.keys().eq(self.scope.iter())
        {
            return Err(err("STREAM_SNAPSHOT_MANIFEST"));
        }
        let parent_bytes = account_map_bytes(&snapshot.accounts)?;
        // Include both external key sets and all string/vector capacities before
        // validating hashes. The manifest is discarded after verification.
        let proof_bytes = manifest
            .final_versions
            .len()
            .checked_mul(128)
            .and_then(|n| n.checked_add(manifest.source.source_id.capacity()))
            .and_then(|n| n.checked_add(manifest.source.session_id.capacity()))
            .and_then(|n| n.checked_add(manifest.anchor.blockhash.capacity()))
            .and_then(|n| n.checked_add(512))
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        self.base_bytes
            .checked_add(parent_bytes)
            .and_then(|n| n.checked_add(proof_bytes))
            .filter(|n| *n <= self.limits.max_bytes)
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        let tip = self.tip.as_ref().ok_or_else(|| err("STREAM_ANCHOR"))?;
        for (key, account) in &snapshot.accounts {
            let expected = &manifest.final_versions[key];
            if account.pubkey != *key
                || account.is_startup
                || account.bank.slot > tip.bank.slot
                || (account.bank.slot == tip.bank.slot && account.bank != tip.bank)
                || account.write_version != expected.write_version
                || account_fingerprint(account) != expected.account_sha256
            {
                return Err(err("STREAM_SNAPSHOT_ACCOUNT"));
            }
        }
        self.parent_accounts = snapshot.accounts;
        self.parent_bytes = parent_bytes;
        self.usage = self.measured_usage()?;
        Ok(())
    }
    fn skipped_count(&self) -> usize {
        self.skipped
            .as_ref()
            .map_or(0, |proof| proof.skipped_slots.len())
    }
    fn measured_usage(&self) -> Result<StreamUsage> {
        let failure = || err("STREAM_BUFFER_LIMIT");
        let mut bytes = self
            .base_bytes
            .checked_add(self.parent_bytes)
            .and_then(|n| n.checked_add(self.skipped_bytes))
            .ok_or_else(failure)?;
        let mut updates = self
            .parent_accounts
            .len()
            .checked_add(
                self.skipped
                    .as_ref()
                    .map_or(0, |p| p.skipped_slots.len() + 1),
            )
            .ok_or_else(failure)?;
        for b in self.slots.values() {
            bytes = bytes.checked_add(b.bytes).ok_or_else(failure)?;
            updates = updates.checked_add(b.updates).ok_or_else(failure)?;
        }
        let slots = self
            .slots
            .len()
            .checked_add(self.skipped_count())
            .ok_or_else(failure)?;
        if bytes > self.limits.max_bytes
            || updates > self.limits.max_updates
            || slots > self.limits.max_slots
        {
            return Err(failure());
        }
        Ok(StreamUsage {
            bytes,
            updates,
            slots,
        })
    }
    /// Admit an independently verified complete skip interval without inventing a
    /// Geyser sequence. A second different claim requires reanchor. The target is
    /// the next actual bank, including its native identity and blockhash. Persist
    /// this external proof with the matching checkpoint for subsequent replay.
    pub fn prove_skipped(&mut self, proof: SkippedRangeProof) -> Result<()> {
        let result = self.prove_skipped_inner(proof);
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    fn prove_skipped_inner(&mut self, proof: SkippedRangeProof) -> Result<()> {
        if self.reanchor {
            return Err(err("REANCHOR_REQUIRED"));
        }
        if self.source.as_ref() != Some(&proof.source) {
            return Err(err("STREAM_SOURCE_SESSION_CHANGED"));
        }
        if self.tip.as_ref() != Some(&proof.parent) || !valid_hash(&proof.target.blockhash) {
            return Err(err("STREAM_SKIP_ANCHOR"));
        }
        let count = proof
            .target
            .bank
            .slot
            .checked_sub(proof.parent.bank.slot)
            .and_then(|n| n.checked_sub(1))
            .filter(|n| *n > 0)
            .ok_or_else(|| err("STREAM_SKIP_RANGE"))?;
        if count > self.limits.max_slots as u64
            || usize::try_from(count).ok() != Some(proof.skipped_slots.len())
            || proof
                .skipped_slots
                .iter()
                .enumerate()
                .any(|(i, slot)| proof.parent.bank.slot.checked_add(i as u64 + 1) != Some(*slot))
        {
            return Err(err("STREAM_SKIP_RANGE"));
        }
        self.slots
            .len()
            .checked_add(proof.skipped_slots.len())
            .and_then(|n| n.checked_add(usize::from(!self.slots.contains_key(&proof.target.bank))))
            .filter(|n| *n <= self.limits.max_slots)
            .ok_or_else(|| err("STREAM_SLOT_LIMIT"))?;
        let bytes = proof
            .skipped_slots
            .capacity()
            .checked_mul(std::mem::size_of::<u64>())
            .and_then(|n| n.checked_add(proof.source.source_id.capacity()))
            .and_then(|n| n.checked_add(proof.source.session_id.capacity()))
            .and_then(|n| n.checked_add(proof.parent.blockhash.capacity()))
            .and_then(|n| n.checked_add(proof.target.blockhash.capacity()))
            .and_then(|n| n.checked_add(512))
            .filter(|n| *n <= self.limits.max_bytes)
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        if let Some(old) = &self.skipped {
            if old == &proof {
                return Ok(());
            }
            return Err(err("STREAM_SKIP_CONFLICT"));
        }
        for (bank, b) in &self.slots {
            check_skip_binding(&proof, bank, None, None)?;
            if let Some(m) = &b.metadata {
                check_skip_binding(&proof, bank, Some(&m.blockhash), Some(&m.parent))?;
            }
            if let Some(AccountCompletion::VersionedBankManifest {
                blockhash, parent, ..
            }) = &b.completion
            {
                check_skip_binding(&proof, bank, Some(blockhash), Some(parent))?;
            }
            if let Some(status) = &b.status {
                check_skip_parent_slot(&proof, bank, status.parent_slot)?;
            }
        }
        self.skipped = Some(proof);
        self.skipped_bytes = bytes;
        self.usage = self.measured_usage()?;
        Ok(())
    }
    pub fn invalidate(&mut self) {
        self.slots.clear();
        self.parent_accounts.clear();
        self.parent_bytes = 0;
        self.skipped = None;
        self.skipped_bytes = 0;
        // Keep only the small last-known identity for gap reporting, not a large
        // spare-capacity anchor allocation after invalidation.
        self.tip = self.tip.clone();
        self.scope.clear();
        self.source = None;
        self.base_bytes = 0;
        self.usage = StreamUsage::default();
        self.reanchor = true;
        self.committed = false;
    }
    pub fn needs_reanchor(&self) -> bool {
        self.reanchor
    }
    /// Live caught-up eligibility, separate from the last complete read position.
    /// Every known higher finalized bank and explicit recovery target must pass
    /// its independent barriers and ordered commit first. A proven empty skip
    /// interval alone does not complete its actual target bank. Merely processed
    /// future frames may coexist with the tagged old tip; freshness is caller-owned.
    pub fn ready(&self) -> bool {
        !self.reanchor
            && self.committed
            && matches!(self.mode, Some(Admission::Live { .. }))
            && !self.pending_barrier()
    }
    fn pending_barrier(&self) -> bool {
        self.skipped.is_some()
            || self.tip.as_ref().is_some_and(|tip| {
                self.slots.iter().any(|(bank, b)| {
                    bank.slot > tip.bank.slot
                        && b.status
                            .as_ref()
                            .is_some_and(|status| status.status == SlotStatus::Finalized)
                })
            })
            || self.unproven_tip_gap()
    }
    fn unproven_tip_gap(&self) -> bool {
        if self.skipped.is_some() {
            return false;
        }
        let Some(tip) = &self.tip else {
            return false;
        };
        let Some(next) = tip.bank.slot.checked_add(1) else {
            return false;
        };
        self.slots.iter().any(|(bank,b)|bank.slot>next && (
            b.metadata.as_ref().is_some_and(|m|m.parent.slot==tip.bank.slot && m.parent.blockhash==tip.blockhash)
            || matches!(&b.completion,Some(AccountCompletion::VersionedBankManifest {parent,..}) if parent.slot==tip.bank.slot && parent.blockhash==tip.blockhash)
            || b.status.as_ref().and_then(|s|s.parent_slot)==Some(tip.bank.slot)))
    }
    /// Anchor or last completely committed bank identity. Pending recovery keeps
    /// this position intact even while ready() is false; already returned account
    /// Arcs remain immutable and explicitly tagged with their original write bank.
    pub fn tip(&self) -> Option<&BankId> {
        self.tip.as_ref().map(|tip| &tip.bank)
    }
    pub fn usage(&self) -> StreamUsage {
        self.usage
    }
    pub fn admit(&mut self, event: ProjectedEvent, admission: Admission) -> Result<()> {
        let result = self.admit_inner(event, admission);
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    fn admit_inner(&mut self, projected: ProjectedEvent, admission: Admission) -> Result<()> {
        if self.reanchor {
            return Err(err("REANCHOR_REQUIRED"));
        }
        if self.source.as_ref() != Some(&projected.source) {
            return Err(err("STREAM_SOURCE_SESSION_CHANGED"));
        }
        if projected.sequence != self.next_sequence {
            return Err(err("STREAM_SEQUENCE_GAP"));
        }
        if matches!(admission,Admission::Live {wal_generation} if wal_generation != self.generation)
            || self.mode.is_some_and(|mode| mode != admission)
        {
            return Err(err("STREAM_REPLAY_NOT_LIVE"));
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| err("STREAM_SEQUENCE_OVERFLOW"))?;
        self.mode = Some(admission);
        self.admit_event(projected.event)
    }
    fn admit_event(&mut self, event: YellowstoneEvent) -> Result<()> {
        if let YellowstoneEvent::ProviderBlock(block) = event {
            return self.admit_provider_block(block);
        }
        let bank = match &event {
            YellowstoneEvent::ProviderBlock(_) => unreachable!(),
            YellowstoneEvent::Account(v) => {
                if v.is_startup || !self.scope.contains(&v.pubkey) {
                    return Err(err("STREAM_ACCOUNT_SCOPE"));
                }
                if self
                    .parent_accounts
                    .get(&v.pubkey)
                    .is_some_and(|parent| v.write_version <= parent.write_version)
                {
                    return Err(err("STREAM_STALE_ACCOUNT_VERSION"));
                }
                &v.bank
            }
            YellowstoneEvent::Transaction(v) => &v.bank,
            YellowstoneEvent::Metadata(v) => {
                self.check_parent(&v.bank, &v.parent)?;
                if !valid_hash(&v.blockhash) {
                    return Err(err("STREAM_BANK_HASH"));
                }
                &v.bank
            }
            YellowstoneEvent::Slot(v) => {
                if matches!(v.status, SlotStatus::Dead | SlotStatus::Skipped) {
                    return Err(err("STREAM_FORK_OR_SKIP"));
                }
                let parent_slot = v.parent_slot.ok_or_else(|| err("STREAM_PARENT_UNKNOWN"))?;
                if parent_slot >= v.bank.slot {
                    return Err(err("STREAM_PARENT_MISMATCH"));
                }
                &v.bank
            }
            YellowstoneEvent::AccountsComplete(AccountCompletion::UnknownYellowstoneMetadata {
                ..
            }) => return Err(err("STREAM_ACCOUNT_COMPLETION_UNPROVEN")),
            YellowstoneEvent::AccountsComplete(AccountCompletion::VersionedBankManifest {
                bank,
                parent,
                final_versions,
                expected_account_updates,
                blockhash,
            }) => {
                self.check_parent(bank, parent)?;
                if !valid_hash(blockhash) {
                    return Err(err("STREAM_BANK_HASH"));
                }
                if final_versions.len() != self.scope.len()
                    || final_versions.keys().any(|k| !self.scope.contains(k))
                    || *expected_account_updates > self.limits.max_updates as u64
                {
                    return Err(err("STREAM_ACCOUNT_MANIFEST"));
                }
                bank
            }
            YellowstoneEvent::Heartbeat | YellowstoneEvent::BanklessSlot { .. } => return Ok(()),
            YellowstoneEvent::Reconnect => return Err(err("STREAM_RECONNECT_GAP")),
            YellowstoneEvent::Unsupported { .. } => return Err(err("STREAM_UNSUPPORTED_EVENT")),
        }
        .clone();
        if bank.slot
            <= self
                .tip
                .as_ref()
                .ok_or_else(|| err("STREAM_ANCHOR"))?
                .bank
                .slot
        {
            return Err(err("STREAM_BANK_STALE_OR_UNKNOWN"));
        }
        if let Some(proof) = &self.skipped {
            let (hash, parent) = match &event {
                YellowstoneEvent::Metadata(m) => (Some(m.blockhash.as_str()), Some(&m.parent)),
                YellowstoneEvent::AccountsComplete(AccountCompletion::VersionedBankManifest {
                    blockhash,
                    parent,
                    ..
                }) => (Some(blockhash.as_str()), Some(parent)),
                _ => (None, None),
            };
            check_skip_binding(proof, &bank, hash, parent)?;
            if let YellowstoneEvent::Slot(status) = &event {
                check_skip_parent_slot(proof, &bank, status.parent_slot)?;
            }
        }
        if !self.slots.contains_key(&bank) {
            let reserve_target = self.skipped.as_ref().is_some_and(|proof| {
                proof.target.bank != bank && !self.slots.contains_key(&proof.target.bank)
            });
            self.usage
                .slots
                .checked_add(1)
                .and_then(|n| n.checked_add(usize::from(reserve_target)))
                .filter(|n| *n <= self.limits.max_slots)
                .ok_or_else(|| err("STREAM_SLOT_LIMIT"))?;
        }
        // Two independently finalized banks at the same slot cannot both be canonical.
        if matches!(&event,YellowstoneEvent::Slot(v) if v.status == SlotStatus::Finalized)
            && self.slots.iter().any(|(id, b)| {
                id.slot == bank.slot
                    && id != &bank
                    && b.status
                        .as_ref()
                        .is_some_and(|s| s.status == SlotStatus::Finalized)
            })
        {
            return Err(err("STREAM_FINALIZED_FORK"));
        }
        let b = self.slots.entry(bank.clone()).or_default();
        let parent = match &event {
            YellowstoneEvent::Metadata(v) => Some(&v.parent),
            YellowstoneEvent::AccountsComplete(AccountCompletion::VersionedBankManifest {
                parent,
                ..
            }) => Some(parent),
            _ => None,
        };
        let known_parent = b
            .metadata
            .as_ref()
            .map(|m| &m.parent)
            .or(match &b.completion {
                Some(AccountCompletion::VersionedBankManifest { parent, .. }) => Some(parent),
                _ => None,
            });
        if let Some(parent) = parent {
            if known_parent.is_some_and(|known| known != parent)
                || b.status
                    .as_ref()
                    .and_then(|s| s.parent_slot)
                    .is_some_and(|slot| slot != parent.slot)
            {
                return Err(err("STREAM_PARENT_MISMATCH"));
            }
        }
        if let YellowstoneEvent::Slot(status) = &event {
            if known_parent.is_some_and(|p| Some(p.slot) != status.parent_slot) {
                return Err(err("STREAM_PARENT_MISMATCH"));
            }
        }
        let hash = match &event {
            YellowstoneEvent::Metadata(m) => Some(&m.blockhash),
            YellowstoneEvent::AccountsComplete(AccountCompletion::VersionedBankManifest {
                blockhash,
                ..
            }) => Some(blockhash),
            _ => None,
        };
        let known_hash = b
            .metadata
            .as_ref()
            .map(|m| &m.blockhash)
            .or(match &b.completion {
                Some(AccountCompletion::VersionedBankManifest { blockhash, .. }) => Some(blockhash),
                _ => None,
            });
        if hash.zip(known_hash).is_some_and(|(a, b)| a != b) {
            return Err(err("STREAM_BANK_HASH_CONFLICT"));
        }
        // Exact repeats are idempotent; a repeated identifier with different payload
        // is corruption, regardless of arrival order or which source is newer.
        let duplicate = match &event {
            YellowstoneEvent::Account(v) => match b.versions.get(&(v.pubkey, v.write_version)) {
                Some(old) if old.as_ref() != v => return Err(err("STREAM_WRITE_VERSION_CONFLICT")),
                Some(_) => true,
                None => false,
            },
            YellowstoneEvent::Transaction(v) => {
                if b.transactions
                    .values()
                    .any(|old| old.signature == v.signature && old.index != v.index)
                {
                    return Err(err("STREAM_TRANSACTION_CONFLICT"));
                }
                match b.transactions.get(&v.index) {
                    Some(old) if old != v => return Err(err("STREAM_TRANSACTION_CONFLICT")),
                    Some(_) => true,
                    None => false,
                }
            }
            YellowstoneEvent::Metadata(v) => match &b.metadata {
                Some(old) if old != v => return Err(err("STREAM_METADATA_CONFLICT")),
                Some(_) => true,
                None => false,
            },
            YellowstoneEvent::AccountsComplete(v) => match &b.completion {
                Some(old) if old != v => return Err(err("STREAM_COMPLETION_CONFLICT")),
                Some(_) => true,
                None => false,
            },
            YellowstoneEvent::Slot(v) => {
                b.status.as_ref().is_some_and(|old| old.status >= v.status)
            }
            _ => false,
        };
        if duplicate {
            return Ok(());
        }
        let charge = retained_bytes(&event)?;
        // Status is replaced, not accumulated. Accounting remains conservatively
        // bounded by charging every progression until this bank leaves the window.
        if self
            .usage
            .bytes
            .checked_add(charge)
            .is_none_or(|n| n > self.limits.max_bytes)
            || self.usage.updates >= self.limits.max_updates
        {
            return Err(err("STREAM_BUFFER_LIMIT"));
        }
        b.bytes += charge;
        b.updates += 1;
        self.usage.bytes += charge;
        self.usage.updates += 1;
        match event {
            YellowstoneEvent::Account(v) => {
                b.versions.insert((v.pubkey, v.write_version), Arc::new(v));
            }
            YellowstoneEvent::Transaction(v) => {
                b.transactions.insert(v.index, v);
            }
            YellowstoneEvent::Metadata(v) => b.metadata = Some(v),
            YellowstoneEvent::AccountsComplete(v) => b.completion = Some(v),
            YellowstoneEvent::Slot(v) => b.status = Some(v),
            _ => unreachable!(),
        }
        self.usage.slots = self.slots.len() + self.skipped_count();
        Ok(())
    }
    fn admit_provider_block(&mut self, block: ProviderBlock) -> Result<()> {
        if block.updated_account_count != block.accounts.len() as u64
            || block.metadata.executed_transaction_count != block.transactions.len() as u64
            || block.metadata.entry_count != block.entries.len() as u64
        {
            return Err(err("STREAM_PROVIDER_BLOCK_COUNT"));
        }
        let count = block
            .accounts
            .len()
            .checked_add(block.transactions.len())
            .and_then(|n| n.checked_add(block.entries.len()))
            .filter(|n| *n <= self.limits.max_updates)
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        let mut bytes = count
            .checked_mul(1024)
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        let mut accounts = BTreeSet::new();
        let mut indexes = BTreeSet::new();
        let mut signatures = BTreeSet::new();
        for account in &block.accounts {
            if account.bank != block.metadata.bank
                || account.is_startup
                || !accounts.insert((account.pubkey, account.write_version))
            {
                return Err(err("STREAM_PROVIDER_BLOCK_CONFLICT"));
            }
            bytes = bytes
                .checked_add(account.data.capacity())
                .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        }
        for tx in &block.transactions {
            if tx.bank != block.metadata.bank
                || tx.index >= block.metadata.executed_transaction_count
                || !indexes.insert(tx.index)
                || !signatures.insert(tx.signature)
            {
                return Err(err("STREAM_PROVIDER_BLOCK_CONFLICT"));
            }
            bytes = bytes
                .checked_add(tx.transaction.capacity())
                .and_then(|n| n.checked_add(tx.meta.capacity()))
                .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        }
        for entry in &block.entries {
            bytes = bytes
                .checked_add(entry.capacity())
                .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?;
        }
        if bytes > self.limits.max_bytes {
            return Err(err("STREAM_BUFFER_LIMIT"));
        }
        self.admit_event(YellowstoneEvent::Metadata(block.metadata))?;
        for tx in block.transactions {
            self.admit_event(YellowstoneEvent::Transaction(tx))?;
        }
        for account in block.accounts {
            if self.scope.contains(&account.pubkey) {
                self.admit_event(YellowstoneEvent::Account(account))?;
            }
        }
        Ok(())
    }
    /// Use with FrameWal::read / Subscription::capture_next; it preserves the WAL
    /// generation and receipt session instead of inventing a new live envelope.
    pub fn admit_captured(
        &mut self,
        record: &CapturedFrame,
        event: YellowstoneEvent,
    ) -> Result<()> {
        self.admit(
            ProjectedEvent {
                source: record.frame.source.clone(),
                sequence: record.frame.sequence,
                event,
            },
            Admission::Live {
                wal_generation: record.wal_generation,
            },
        )
    }
    fn check_parent(&self, bank: &BankId, parent: &ParentBank) -> Result<()> {
        if !valid_hash(&parent.blockhash) || parent.slot >= bank.slot {
            return Err(err("STREAM_PARENT_MISMATCH"));
        }
        if let Some(tip) = &self.tip {
            if parent.slot <= tip.bank.slot
                && (parent.slot != tip.bank.slot || parent.blockhash != tip.blockhash)
            {
                return Err(err("STREAM_PARENT_MISMATCH"));
            }
        }
        Ok(())
    }
    /// Application-side proof from the independent versioned-bank authority.
    /// It does not consume or invent a Yellowstone receipt sequence. Persist and
    /// restore that authority's manifest alongside the external state checkpoint;
    /// the raw protobuf WAL cannot recreate an absent independent bank proof.
    pub fn seal_accounts(
        &mut self,
        source: &SourceSession,
        proof: AccountCompletion,
    ) -> Result<()> {
        let result = if self.reanchor {
            Err(err("REANCHOR_REQUIRED"))
        } else if self.source.as_ref() != Some(source) {
            Err(err("STREAM_SOURCE_SESSION_CHANGED"))
        } else {
            self.admit_event(YellowstoneEvent::AccountsComplete(proof))
        };
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    /// Native notifier observations are separate independent authority evidence,
    /// not fabricated reception frames. Receipt sequence/mode remain unchanged.
    pub fn seal_native_versions(
        &mut self,
        source: &SourceSession,
        versions: Vec<AccountUpdate>,
    ) -> Result<()> {
        let result = (|| {
            if self.reanchor || self.source.as_ref() != Some(source) {
                return Err(err("STREAM_SOURCE_SESSION_CHANGED"));
            }
            for account in versions {
                self.admit_event(YellowstoneEvent::Account(account))?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    pub fn commit_next(&mut self) -> Result<Option<CommittedBank>> {
        let result = self.commit_inner();
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    fn commit_inner(&mut self) -> Result<Option<CommittedBank>> {
        if self.reanchor {
            return Err(err("REANCHOR_REQUIRED"));
        }
        let tip = self.tip.as_ref().ok_or_else(|| err("STREAM_ANCHOR"))?;
        let next = match &self.skipped {
            Some(proof) => proof.target.bank.slot,
            None => tip
                .bank
                .slot
                .checked_add(1)
                .ok_or_else(|| err("STREAM_SLOT_OVERFLOW"))?,
        };
        let candidate = self
            .slots
            .iter()
            .find(|(id, b)| {
                id.slot == next
                    && b.status
                        .as_ref()
                        .is_some_and(|s| s.status == SlotStatus::Finalized)
            })
            .map(|(id, _)| id.clone());
        if candidate.is_none()
            && self.skipped.is_none()
            && self.slots.iter().any(|(id, b)| {
                id.slot > next
                    && b.status
                        .as_ref()
                        .is_some_and(|s| s.status == SlotStatus::Finalized)
                    && b.metadata.as_ref().is_some_and(|m| {
                        m.parent.slot == tip.bank.slot && m.parent.blockhash == tip.blockhash
                    })
            })
        {
            return Err(err("STREAM_SKIP_PROOF_REQUIRED"));
        }
        let Some(bank) = candidate else {
            return Ok(None);
        };
        let b = &self.slots[&bank];
        let (
            Some(metadata),
            Some(AccountCompletion::VersionedBankManifest {
                parent,
                expected_account_updates,
                final_versions,
                ..
            }),
        ) = (&b.metadata, &b.completion)
        else {
            return Ok(None);
        };
        if metadata.parent.slot != tip.bank.slot
            || metadata.parent.blockhash != tip.blockhash
            || parent != &metadata.parent
        {
            return Err(err("STREAM_PARENT_MISMATCH"));
        }
        let transactions = metadata.executed_transaction_count;
        if transactions > self.limits.max_updates as u64 {
            return Err(err("STREAM_TRANSACTION_COUNT"));
        }
        if b.transactions.len() as u64 > transactions
            || b.versions.len() as u64 > *expected_account_updates
        {
            return Err(err("STREAM_COUNT_CONFLICT"));
        }
        if b.transactions.keys().any(|index| *index >= transactions) {
            return Err(err("STREAM_TRANSACTION_INDEX"));
        }
        if (b.transactions.len() as u64) < transactions
            || (b.versions.len() as u64) < *expected_account_updates
        {
            return Ok(None);
        }
        let mut accounts = self.parent_accounts.clone();
        for ((key, _), update) in &b.versions {
            if self
                .parent_accounts
                .get(key)
                .is_some_and(|old| update.write_version <= old.write_version)
            {
                return Err(err("STREAM_STALE_ACCOUNT_VERSION"));
            }
            accounts.insert(*key, update.clone());
        }
        if accounts.len() != self.scope.len()
            || final_versions.iter().any(|(key, version)| {
                accounts.get(key).is_none_or(|a| {
                    a.write_version != version.write_version
                        || account_fingerprint(a) != version.account_sha256
                })
            })
        {
            return Err(err("STREAM_FINAL_VERSION_MISMATCH"));
        }
        let parent_bytes = account_map_bytes(&accounts)?;
        if let Some(proof) = &self.skipped {
            if proof.target.bank != bank || proof.target.blockhash != metadata.blockhash {
                return Err(err("STREAM_SKIP_CONFLICT"));
            }
        } else if tip.bank.slot.checked_add(1) != Some(bank.slot) {
            return Err(err("STREAM_SKIP_PROOF_REQUIRED"));
        }
        for (id, future) in &self.slots {
            if id.slot > bank.slot {
                let compatible = |parent: &ParentBank| {
                    parent.slot > bank.slot
                        || (parent.slot == bank.slot && parent.blockhash == metadata.blockhash)
                };
                if future
                    .metadata
                    .as_ref()
                    .is_some_and(|m| !compatible(&m.parent))
                    || matches!(&future.completion,Some(AccountCompletion::VersionedBankManifest {parent,..}) if !compatible(parent))
                    || future
                        .status
                        .as_ref()
                        .and_then(|s| s.parent_slot)
                        .is_some_and(|parent| parent < bank.slot)
                {
                    return Err(err("STREAM_PARENT_MISMATCH"));
                }
            }
            if id.slot > bank.slot
                && future.versions.iter().any(|((key, _), value)| {
                    accounts
                        .get(key)
                        .is_some_and(|parent| value.write_version <= parent.write_version)
                })
            {
                return Err(err("STREAM_STALE_ACCOUNT_VERSION"));
            }
        }
        let losing_hashes: BTreeSet<_> = self
            .slots
            .iter()
            .filter(|(id, _)| id.slot <= bank.slot && *id != &bank)
            .filter_map(|(_, b)| {
                b.metadata
                    .as_ref()
                    .map(|m| (m.bank.slot, m.blockhash.clone()))
                    .or(match &b.completion {
                        Some(AccountCompletion::VersionedBankManifest {
                            bank, blockhash, ..
                        }) => Some((bank.slot, blockhash.clone())),
                        _ => None,
                    })
            })
            .collect();
        if self.slots.values().any(|b| b.metadata.as_ref().is_some_and(|m|losing_hashes.contains(&(m.parent.slot,m.parent.blockhash.clone())))
            || matches!(&b.completion,Some(AccountCompletion::VersionedBankManifest {parent,..}) if losing_hashes.contains(&(parent.slot,parent.blockhash.clone())))) {
            return Err(err("STREAM_DEAD_FORK_DESCENDANT"));
        }
        let blockhash = metadata.blockhash.clone();
        let b = self.slots.remove(&bank).expect("candidate exists");
        self.parent_accounts = accounts.clone();
        self.parent_bytes = parent_bytes;
        let result = CommittedBank {
            bank: bank.clone(),
            blockhash: blockhash.clone(),
            parent: tip.clone(),
            accounts,
            account_versions: b.versions,
            transactions: b.transactions,
        };
        self.tip = Some(BankAnchor {
            bank: bank.clone(),
            blockhash,
        });
        self.committed = true;
        self.slots.retain(|id, _| id.slot > bank.slot);
        self.skipped = None;
        self.skipped_bytes = 0;
        self.usage = self.measured_usage()?;
        Ok(Some(result))
    }
    /// Transport calls this at a deadline/end-of-window. Absence of messages is
    /// never converted into a completion proof or an implicit skipped-slot proof.
    pub fn finish_window(&mut self) -> Result<()> {
        if self.reanchor || !self.slots.is_empty() || self.skipped.is_some() {
            self.invalidate();
            return Err(err("STREAM_INCOMPLETE_WINDOW"));
        }
        Ok(())
    }
}
fn account_map_bytes(accounts: &BTreeMap<Pubkey, Arc<AccountUpdate>>) -> Result<usize> {
    accounts.values().try_fold(0usize, |total, account| {
        total
            .checked_add(account.data.capacity())
            .and_then(|n| n.checked_add(1024))
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))
    })
}
fn check_skip_parent_slot(
    proof: &SkippedRangeProof,
    bank: &BankId,
    parent_slot: Option<u64>,
) -> Result<()> {
    if let Some(slot) = parent_slot {
        if (bank.slot == proof.target.bank.slot && slot != proof.parent.bank.slot)
            || (bank.slot > proof.target.bank.slot && slot < proof.target.bank.slot)
        {
            return Err(err("STREAM_SKIP_CONFLICT"));
        }
    }
    Ok(())
}
fn check_skip_binding(
    proof: &SkippedRangeProof,
    bank: &BankId,
    hash: Option<&str>,
    parent: Option<&ParentBank>,
) -> Result<()> {
    if (bank.slot > proof.parent.bank.slot && bank.slot < proof.target.bank.slot)
        || (bank.slot == proof.target.bank.slot && bank != &proof.target.bank)
        || (bank == &proof.target.bank && hash.is_some_and(|h| h != proof.target.blockhash))
    {
        return Err(err("STREAM_SKIP_CONFLICT"));
    }
    if let Some(parent) = parent {
        check_skip_parent_slot(proof, bank, Some(parent.slot))?;
        if (bank.slot == proof.target.bank.slot && parent.blockhash != proof.parent.blockhash)
            || (parent.slot == proof.target.bank.slot && parent.blockhash != proof.target.blockhash)
        {
            return Err(err("STREAM_SKIP_CONFLICT"));
        }
    }
    Ok(())
}
fn retained_bytes(event: &YellowstoneEvent) -> Result<usize> {
    let payload: usize = match event {
        YellowstoneEvent::Account(v) => v.data.capacity(),
        YellowstoneEvent::Transaction(v) => v
            .transaction
            .capacity()
            .checked_add(v.meta.capacity())
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?,
        YellowstoneEvent::Metadata(v) => v
            .blockhash
            .capacity()
            .checked_add(v.parent.blockhash.capacity())
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?,
        YellowstoneEvent::AccountsComplete(AccountCompletion::VersionedBankManifest {
            blockhash,
            parent,
            final_versions,
            ..
        }) => final_versions
            .len()
            .checked_mul(128)
            .and_then(|n| n.checked_add(blockhash.capacity()))
            .and_then(|n| n.checked_add(parent.blockhash.capacity()))
            .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))?,
        _ => 0,
    };
    payload
        .checked_add(1024)
        .ok_or_else(|| err("STREAM_BUFFER_LIMIT"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalGap {
    pub from_slot: u64,
    pub through_slot: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GapPlan {
    NoGap,
    ReplayWindow { from_slot: u64, through_slot: u64 },
    HeadFirst { history: HistoricalGap },
}
/// Plans only. A replay window is usable only if the provider ALSO proves exact
/// session/sequence, bank ancestry and account scope continuity. RPC getBlock or
/// minContextSlot cannot repair the missing account versions. Failure => HEAD_FIRST.
pub fn plan_gap(
    last_slot: u64,
    head_slot: u64,
    window_first_slot: Option<u64>,
    max_short_gap: u64,
) -> Result<GapPlan> {
    if head_slot < last_slot {
        return Err(err("STREAM_HEAD_REGRESSION"));
    }
    if head_slot == last_slot {
        return Ok(GapPlan::NoGap);
    }
    let from_slot = last_slot
        .checked_add(1)
        .ok_or_else(|| err("STREAM_SLOT_OVERFLOW"))?;
    if head_slot - last_slot <= max_short_gap
        && window_first_slot.is_some_and(|first| first <= from_slot)
    {
        Ok(GapPlan::ReplayWindow {
            from_slot,
            through_slot: head_slot,
        })
    } else {
        Ok(GapPlan::HeadFirst {
            history: HistoricalGap {
                from_slot,
                through_slot: head_slot,
            },
        })
    }
}

const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const WAL_META: &[u8] = b"stream-meta";
#[derive(Clone, Copy, Debug)]
pub struct FrameWalLimits {
    pub max_frame_bytes: usize,
    pub max_total_bytes: usize,
    pub max_records: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawFrame {
    pub source: SourceSession,
    pub sequence: u64,
    pub receive_timestamp_ms: u64,
    pub protobuf: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WalCursor {
    pub sequence: u64,
    pub record_sha256: String,
}
impl WalCursor {
    pub fn start() -> Self {
        Self {
            sequence: 0,
            record_sha256: ZERO_HASH.into(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct CapturedFrame {
    pub cursor: WalCursor,
    pub wal_generation: u64,
    pub frame: RawFrame,
}
/// Append-only, synchronous durable evidence WAL. No rotation is performed; a full
/// WAL rejects ingestion. Budgets count logical stored records (RocksDB physical
/// amplification requires a separate disk quota). Every arrival is retained.
pub struct FrameWal {
    db: DB,
    path: PathBuf,
    chain: String,
    limits: FrameWalLimits,
    generation: u64,
    sequence: u64,
    bytes: usize,
    last_hash: String,
    cursor: WalCursor,
    poisoned: bool,
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn wal_key(sequence: u64) -> String {
    format!("frame/{sequence:020}")
}
fn corrupt() -> xone_domain::Error {
    err("STREAM_WAL_CORRUPT")
}
fn number(v: &Value, key: &str) -> Result<u64> {
    v[key].as_u64().ok_or_else(corrupt)
}
fn text(v: &Value, key: &str) -> Result<String> {
    v[key].as_str().map(str::to_owned).ok_or_else(corrupt)
}
impl FrameWal {
    pub fn open(path: &Path, chain: &str, limits: FrameWalLimits) -> Result<Self> {
        if !chain.starts_with("solana:")
            || chain.len() <= 7
            || chain.len() > 128
            || limits.max_frame_bytes == 0
            || limits.max_total_bytes == 0
            || limits.max_records == 0
            || limits.max_frame_bytes.checked_add(4096).is_none()
        {
            return Err(err("STREAM_WAL_LIMITS"));
        }
        let mut options = Options::default();
        options.create_if_missing(true);
        options.set_wal_recovery_mode(DBRecoveryMode::AbsoluteConsistency);
        let db = DB::open(&options, path).map_err(|_| err("STREAM_WAL_OPEN"))?;
        let mut wal = Self {
            db,
            path: path.into(),
            chain: chain.into(),
            limits,
            generation: 0,
            sequence: 0,
            bytes: 0,
            last_hash: ZERO_HASH.into(),
            cursor: WalCursor::start(),
            poisoned: false,
        };
        if let Some(raw) = wal.db.get(WAL_META).map_err(|_| corrupt())? {
            if raw.len() > 2048 {
                return Err(corrupt());
            }
            let v: Value = serde_json::from_slice(&raw).map_err(|_| corrupt())?;
            if v["schema"] != 1 || v["chain"] != chain {
                return Err(corrupt());
            }
            wal.generation = number(&v, "generation")?;
            wal.sequence = number(&v, "sequence")?;
            wal.bytes = usize::try_from(number(&v, "bytes")?).map_err(|_| corrupt())?;
            wal.last_hash = text(&v, "last_hash")?;
            wal.cursor = WalCursor {
                sequence: number(&v, "cursor_sequence")?,
                record_sha256: text(&v, "cursor_hash")?,
            };
        }
        wal.verify()?;
        wal.verify_head()?;
        wal.generation = wal.generation.checked_add(1).ok_or_else(corrupt)?;
        wal.persist(None)?;
        Ok(wal)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn len(&self) -> u64 {
        self.sequence
    }
    pub fn is_empty(&self) -> bool {
        self.sequence == 0
    }
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }
    pub fn recovery_cursor(&self) -> WalCursor {
        self.cursor.clone()
    }
    /// Checks WAL-open generation only, not the active source session or readiness.
    pub fn is_current(&self, record: &CapturedFrame) -> bool {
        record.wal_generation == self.generation
    }
    fn metadata(&self) -> Value {
        json!({"schema":1,"chain":self.chain,"generation":self.generation,"sequence":self.sequence,"bytes":self.bytes,"last_hash":self.last_hash,"cursor_sequence":self.cursor.sequence,"cursor_hash":self.cursor.record_sha256})
    }
    fn persist(&self, record: Option<(u64, &[u8])>) -> Result<()> {
        let mut batch = WriteBatch::default();
        if let Some((seq, raw)) = record {
            batch.put(wal_key(seq), raw);
        }
        batch.put(
            WAL_META,
            serde_json::to_vec(&self.metadata()).map_err(|_| err("STREAM_WAL_ENCODING"))?,
        );
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db
            .write_opt(batch, &options)
            .map_err(|_| err("STREAM_WAL_WRITE"))?;
        let mut head = self.metadata();
        let checksum = digest(&serde_json::to_vec(&head).map_err(|_| corrupt())?);
        head["checksum"] = json!(checksum);
        let raw = serde_json::to_vec(&head).map_err(|_| corrupt())?;
        let temp = self.path.join("STREAM_HEAD.tmp");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp)
            .map_err(|_| err("STREAM_WAL_WRITE"))?;
        file.write_all(&raw)
            .and_then(|_| file.sync_all())
            .map_err(|_| err("STREAM_WAL_WRITE"))?;
        std::fs::rename(temp, self.path.join("STREAM_HEAD"))
            .map_err(|_| err("STREAM_WAL_WRITE"))?;
        std::fs::File::open(&self.path)
            .and_then(|d| d.sync_all())
            .map_err(|_| err("STREAM_WAL_WRITE"))
    }
    fn verify_head(&self) -> Result<()> {
        let path = self.path.join("STREAM_HEAD");
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && self.generation == 0
                    && self.sequence == 0 =>
            {
                return Ok(())
            }
            Err(_) => return Err(corrupt()),
        };
        let mut raw = Vec::new();
        file.take(2049)
            .read_to_end(&mut raw)
            .map_err(|_| corrupt())?;
        if raw.len() > 2048 {
            return Err(corrupt());
        }
        let mut v: Value = serde_json::from_slice(&raw).map_err(|_| corrupt())?;
        let checksum = v
            .as_object_mut()
            .and_then(|v| v.remove("checksum"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(corrupt)?;
        if digest(&serde_json::to_vec(&v).map_err(|_| corrupt())?) != checksum
            || v["schema"] != 1
            || v["chain"] != self.chain
        {
            return Err(corrupt());
        }
        let sequence = number(&v, "sequence")?;
        if sequence > self.sequence
            || number(&v, "generation")? > self.generation
            || number(&v, "generation")? == 0
        {
            return Err(corrupt());
        }
        self.validate_cursor(&WalCursor {
            sequence,
            record_sha256: text(&v, "last_hash")?,
        })?;
        let checkpoint = WalCursor {
            sequence: number(&v, "cursor_sequence")?,
            record_sha256: text(&v, "cursor_hash")?,
        };
        if checkpoint.sequence > sequence || checkpoint.sequence > self.cursor.sequence {
            return Err(corrupt());
        }
        self.validate_cursor(&checkpoint)
    }
    fn decode(&self, sequence: u64, raw: &[u8]) -> Result<(CapturedFrame, String)> {
        if raw.len()
            > self
                .limits
                .max_frame_bytes
                .checked_add(4096)
                .ok_or_else(corrupt)?
            || raw.len() < 4
        {
            return Err(corrupt());
        }
        let header_len = u32::from_be_bytes(raw[..4].try_into().map_err(|_| corrupt())?) as usize;
        if header_len > 4092 || header_len > raw.len() - 4 {
            return Err(corrupt());
        }
        let v: Value = serde_json::from_slice(&raw[4..4 + header_len]).map_err(|_| corrupt())?;
        let protobuf = &raw[4 + header_len..];
        if v["schema"] != 1
            || v["source_kind"] != "yellowstone-protobuf"
            || v["chain"] != self.chain
            || number(&v, "wal_sequence")? != sequence
            || number(&v, "payload_len")? != protobuf.len() as u64
            || text(&v, "payload_sha256")? != digest(protobuf)
            || protobuf.len() > self.limits.max_frame_bytes
        {
            return Err(corrupt());
        }
        let source = SourceSession {
            source_id: text(&v, "source_id")?,
            session_id: text(&v, "session_id")?,
        };
        let generation = number(&v, "generation")?;
        let source_sequence = number(&v, "source_sequence")?;
        if !source.valid()
            || source_sequence == 0
            || generation == 0
            || generation > self.generation
        {
            return Err(corrupt());
        }
        Ok((
            CapturedFrame {
                cursor: WalCursor {
                    sequence,
                    record_sha256: digest(raw),
                },
                wal_generation: generation,
                frame: RawFrame {
                    source,
                    sequence: source_sequence,
                    receive_timestamp_ms: number(&v, "receive_timestamp_ms")?,
                    protobuf: protobuf.to_vec(),
                },
            },
            text(&v, "previous_hash")?,
        ))
    }
    fn verify(&self) -> Result<()> {
        if self.sequence > self.limits.max_records as u64
            || self.bytes > self.limits.max_total_bytes
        {
            return Err(err("STREAM_WAL_CAPACITY"));
        }
        let (mut count, mut bytes, mut last_generation) = (0u64, 0usize, 0u64);
        let mut hash = ZERO_HASH.to_owned();
        for row in self.db.iterator(IteratorMode::Start) {
            let (key, raw) = row.map_err(|_| corrupt())?;
            if key.as_ref() == WAL_META {
                continue;
            }
            count = count.checked_add(1).ok_or_else(corrupt)?;
            if count > self.sequence || key.as_ref() != wal_key(count).as_bytes() {
                return Err(corrupt());
            }
            let (record, previous) = self.decode(count, &raw)?;
            if previous != hash || record.wal_generation < last_generation {
                return Err(corrupt());
            }
            last_generation = record.wal_generation;
            hash = record.cursor.record_sha256;
            bytes = bytes.checked_add(raw.len()).ok_or_else(corrupt)?;
            if bytes > self.limits.max_total_bytes {
                return Err(err("STREAM_WAL_CAPACITY"));
            }
        }
        if count != self.sequence || bytes != self.bytes || hash != self.last_hash {
            return Err(corrupt());
        }
        self.validate_cursor(&self.cursor)
    }
    pub fn capture(&mut self, frame: RawFrame) -> Result<WalCursor> {
        if self.poisoned {
            return Err(err("STREAM_WAL_REOPEN_REQUIRED"));
        }
        if !frame.source.valid() || frame.sequence == 0 {
            return Err(err("STREAM_WAL_IDENTITY"));
        }
        if frame.protobuf.len() > self.limits.max_frame_bytes {
            return Err(err("STREAM_WAL_FRAME_LIMIT"));
        }
        if self.sequence >= self.limits.max_records as u64 {
            return Err(err("STREAM_WAL_CAPACITY"));
        }
        let sequence = self.sequence.checked_add(1).ok_or_else(corrupt)?;
        let header=serde_json::to_vec(&json!({"schema":1,"source_kind":"yellowstone-protobuf","chain":self.chain,"generation":self.generation,"wal_sequence":sequence,"source_id":frame.source.source_id,"session_id":frame.source.session_id,"source_sequence":frame.sequence,"receive_timestamp_ms":frame.receive_timestamp_ms,"payload_len":frame.protobuf.len(),"payload_sha256":digest(&frame.protobuf),"previous_hash":self.last_hash})).map_err(|_|err("STREAM_WAL_ENCODING"))?;
        if header.len() > 4092 {
            return Err(err("STREAM_WAL_HEADER_LIMIT"));
        }
        let size = 4usize
            .checked_add(header.len())
            .and_then(|n| n.checked_add(frame.protobuf.len()))
            .ok_or_else(|| err("STREAM_WAL_CAPACITY"))?;
        let bytes = self
            .bytes
            .checked_add(size)
            .filter(|n| *n <= self.limits.max_total_bytes)
            .ok_or_else(|| err("STREAM_WAL_CAPACITY"))?;
        let mut raw = Vec::with_capacity(size);
        raw.extend_from_slice(&(header.len() as u32).to_be_bytes());
        raw.extend_from_slice(&header);
        raw.extend_from_slice(&frame.protobuf);
        self.sequence = sequence;
        self.bytes = bytes;
        self.last_hash = digest(&raw);
        // If the DB write or subsequent durable head fails, acknowledgement is
        // uncertain. Poison the writer and force full verification on reopen.
        if let Err(error) = self.persist(Some((sequence, &raw))) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(WalCursor {
            sequence,
            record_sha256: self.last_hash.clone(),
        })
    }
    pub fn read(&self, sequence: u64) -> Result<CapturedFrame> {
        if sequence == 0 || sequence > self.sequence {
            return Err(err("STREAM_WAL_CURSOR"));
        }
        let raw = self
            .db
            .get(wal_key(sequence))
            .map_err(|_| corrupt())?
            .ok_or_else(corrupt)?;
        self.decode(sequence, &raw).map(|(record, _)| record)
    }
    fn validate_cursor(&self, cursor: &WalCursor) -> Result<()> {
        let expected = if cursor.sequence == 0 {
            ZERO_HASH.to_owned()
        } else {
            self.read(cursor.sequence)?.cursor.record_sha256
        };
        if expected != cursor.record_sha256 {
            return Err(err("STREAM_WAL_CURSOR"));
        }
        Ok(())
    }
    /// Advance only after the complete corresponding downstream checkpoint is
    /// durable, including any pending future banks, proofs and source sequence.
    /// A committed bank alone does not cover later interleaved staged frames.
    /// Without that checkpoint, replay from zero; this method does not atomically
    /// commit application state or confer readiness.
    pub fn checkpoint(&mut self, cursor: &WalCursor) -> Result<()> {
        if self.poisoned {
            return Err(err("STREAM_WAL_REOPEN_REQUIRED"));
        }
        self.validate_cursor(cursor)?;
        if cursor.sequence < self.cursor.sequence {
            return Err(err("STREAM_WAL_CURSOR_REGRESSION"));
        }
        self.cursor = cursor.clone();
        if let Err(error) = self.persist(None) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
    pub(crate) fn commit_checkpoint(
        &mut self,
        proof: &crate::owner_scope::CommittedWal,
    ) -> Result<()> {
        if proof.chain != self.chain || proof.commit_id.is_empty() {
            return Err(err("STREAM_OWNER_COMMIT_REQUIRED"));
        }
        let record = self.read(proof.cursor.sequence)?;
        if record.cursor != proof.cursor
            || record.wal_generation != proof.wal_generation
            || record.frame.source.source_id != proof.source_id
            || record.frame.source.session_id != proof.session_id
        {
            return Err(err("STREAM_OWNER_WAL_BINDING"));
        }
        self.checkpoint(&proof.cursor)
    }
    /// Complete deterministic replay, with exact original session and generation.
    /// The callback must use Admission::Replay. A saved processing cursor is only
    /// usable with its matching external durable state; otherwise replay from zero.
    pub fn replay_from(
        &self,
        cursor: &WalCursor,
        mut visitor: impl FnMut(CapturedFrame) -> Result<()>,
    ) -> Result<()> {
        self.validate_cursor(cursor)?;
        for sequence in cursor.sequence.saturating_add(1)..=self.sequence {
            visitor(self.read(sequence)?)?;
        }
        Ok(())
    }
}
/// Synchronous capture/decode/admit boundary. Run on a bounded blocking worker;
/// callers must apply backpressure before accepting more network data. Decode
/// failure leaves the exact frame replayable and immediately invalidates online
/// state. No processing checkpoint is advanced here.
pub fn capture_decode_admit(
    wal: &mut FrameWal,
    assembler: &mut StreamAssembler,
    frame: RawFrame,
    decode: impl FnOnce(&CapturedFrame) -> Result<YellowstoneEvent>,
) -> Result<WalCursor> {
    let result = (|| {
        let cursor = wal.capture(frame)?;
        let record = wal.read(cursor.sequence)?;
        let event = decode(&record)?;
        assembler.admit(
            ProjectedEvent {
                source: record.frame.source,
                sequence: record.frame.sequence,
                event,
            },
            Admission::Live {
                wal_generation: record.wal_generation,
            },
        )?;
        Ok(cursor)
    })();
    if result.is_err() {
        assembler.invalidate();
    }
    result
}

/// Replay through the same decoder and assembler, always with replay authority.
/// Multiple source sessions require separate independently anchored replay runs;
/// encountering their boundary fails closed instead of comparing write versions.
pub fn replay_decode_admit(
    wal: &FrameWal,
    assembler: &mut StreamAssembler,
    cursor: &WalCursor,
    mut decode: impl FnMut(&CapturedFrame) -> Result<YellowstoneEvent>,
) -> Result<()> {
    let result = wal.replay_from(cursor, |record| {
        let event = decode(&record)?;
        assembler.admit(
            ProjectedEvent {
                source: record.frame.source,
                sequence: record.frame.sequence,
                event,
            },
            Admission::Replay,
        )
    });
    if result.is_err() {
        assembler.invalidate();
    }
    result
}

/// Field interpretation is pinned to integration/tools/reference/yellowstone.
/// This identifies a wire interface, not a deployed validator attestation.
pub const PROJECTION_SCHEMA_COMMIT: &str = "10f0af69d150637d8e3eaaf83b179d640988dd3c";
#[derive(Clone, Copy)]
enum Wire<'a> {
    Number(u64),
    Bytes(&'a [u8]),
    Fixed,
}
struct Fields<'a>(Vec<(u32, Wire<'a>)>);
fn proto_error() -> xone_domain::Error {
    err("STREAM_PROTO_INVALID")
}
fn varint(input: &mut &[u8]) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = input.split_first().ok_or_else(proto_error)?;
        *input = rest;
        if shift == 63 && byte > 1 {
            return Err(proto_error());
        }
        value |= ((byte & 127) as u64) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err(proto_error())
}
fn fields<'a>(mut input: &'a [u8], limits: StreamLimits) -> Result<Fields<'a>> {
    let limit = limits
        .max_updates
        .saturating_add(32)
        .min(limits.max_bytes / 32);
    let mut out = Vec::new();
    while !input.is_empty() {
        if out.len() >= limit {
            return Err(err("STREAM_PROJECTION_LIMIT"));
        }
        let tag = varint(&mut input)?;
        let number = tag >> 3;
        if number == 0 || number > 0x1fffffff {
            return Err(proto_error());
        }
        let value = match tag & 7 {
            0 => Wire::Number(varint(&mut input)?),
            2 => {
                let length = usize::try_from(varint(&mut input)?).map_err(|_| proto_error())?;
                if length > input.len() {
                    return Err(proto_error());
                }
                let (value, rest) = input.split_at(length);
                input = rest;
                Wire::Bytes(value)
            }
            1 | 5 => {
                let length = if tag & 7 == 1 { 8 } else { 4 };
                if length > input.len() {
                    return Err(proto_error());
                }
                input = &input[length..];
                Wire::Fixed
            }
            _ => return Err(proto_error()),
        };
        out.push((number as u32, value));
    }
    Ok(Fields(out))
}
impl<'a> Fields<'a> {
    fn allowed(&self, numbers: &[u32]) -> Result<()> {
        if self.0.iter().any(|(n, _)| !numbers.contains(n)) {
            return Err(err("STREAM_UNSUPPORTED_PROTO_FIELD"));
        }
        Ok(())
    }
    fn one(&self, number: u32) -> Result<Option<Wire<'a>>> {
        let mut found = None;
        for (n, value) in &self.0 {
            if *n == number {
                if found.is_some() {
                    return Err(proto_error());
                }
                found = Some(*value);
            }
        }
        Ok(found)
    }
    fn optional_number(&self, n: u32) -> Result<Option<u64>> {
        match self.one(n)? {
            None => Ok(None),
            Some(Wire::Number(value)) => Ok(Some(value)),
            _ => Err(proto_error()),
        }
    }
    fn number(&self, n: u32) -> Result<u64> {
        Ok(self.optional_number(n)?.unwrap_or(0))
    }
    fn bank_id(&self, n: u32) -> Result<u64> {
        self.optional_number(n)?
            .ok_or_else(|| err("STREAM_BANK_ID_UNAVAILABLE"))
    }
    fn bytes(&self, n: u32) -> Result<&'a [u8]> {
        match self.one(n)? {
            Some(Wire::Bytes(bytes)) => Ok(bytes),
            _ => Err(proto_error()),
        }
    }
    fn optional_bytes(&self, n: u32) -> Result<Option<&'a [u8]>> {
        match self.one(n)? {
            None => Ok(None),
            Some(Wire::Bytes(bytes)) => Ok(Some(bytes)),
            _ => Err(proto_error()),
        }
    }
    fn boolean(&self, n: u32) -> Result<bool> {
        match self.number(n)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(proto_error()),
        }
    }
    fn hash(&self, n: u32) -> Result<String> {
        let raw = self.bytes(n)?;
        if raw.len() > 128 {
            return Err(proto_error());
        }
        let value = std::str::from_utf8(raw).map_err(|_| proto_error())?;
        if bs58::decode(value)
            .into_vec()
            .map_err(|_| proto_error())?
            .len()
            != 32
        {
            return Err(proto_error());
        }
        Ok(value.to_owned())
    }
    fn repeated(&self, n: u32) -> impl Iterator<Item = Result<&'a [u8]>> + '_ {
        self.0
            .iter()
            .filter(move |(number, _)| *number == n)
            .map(|(_, v)| match v {
                Wire::Bytes(bytes) => Ok(*bytes),
                _ => Err(proto_error()),
            })
    }
}
fn project_account_info(
    raw: &[u8],
    bank: BankId,
    is_startup: bool,
    limits: StreamLimits,
) -> Result<AccountUpdate> {
    let f = fields(raw, limits)?;
    f.allowed(&[1, 2, 3, 4, 5, 6, 7, 8])?;
    Ok(AccountUpdate {
        bank,
        pubkey: f.bytes(1)?.try_into().map_err(|_| proto_error())?,
        owner: f.bytes(3)?.try_into().map_err(|_| proto_error())?,
        lamports: f.number(2)?,
        executable: f.boolean(4)?,
        rent_epoch: f.number(5)?,
        data: f.optional_bytes(6)?.unwrap_or(&[]).to_vec(),
        write_version: f.number(7)?,
        is_startup,
        txn_signature: f
            .optional_bytes(8)?
            .map(|b| b.try_into().map_err(|_| proto_error()))
            .transpose()?,
    })
}
fn project_tx_info(raw: &[u8], bank: BankId, limits: StreamLimits) -> Result<TransactionUpdate> {
    let f = fields(raw, limits)?;
    f.allowed(&[1, 2, 3, 4, 5])?;
    Ok(TransactionUpdate {
        bank,
        index: f.number(5)?,
        is_vote: f.boolean(2)?,
        signature: f.bytes(1)?.try_into().map_err(|_| proto_error())?,
        transaction: f.bytes(3)?.to_vec(),
        meta: f.bytes(4)?.to_vec(),
    })
}
fn project_block(raw: &[u8], limits: StreamLimits) -> Result<ProviderBlock> {
    let f = fields(raw, limits)?;
    f.allowed(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14])?;
    let count = f.0.iter().filter(|(n, _)| matches!(n, 6 | 11 | 13)).count();
    if count > limits.max_updates
        || count
            .checked_mul(1024)
            .and_then(|n| n.checked_add(raw.len()))
            .is_none_or(|n| n > limits.max_bytes)
    {
        return Err(err("STREAM_PROJECTION_LIMIT"));
    }
    let bank = BankId {
        slot: f.number(1)?,
        bank_id: f.bank_id(14)?,
    };
    let metadata = BlockMetadata {
        bank: bank.clone(),
        blockhash: f.hash(2)?,
        parent: ParentBank {
            slot: f.number(7)?,
            blockhash: f.hash(8)?,
        },
        executed_transaction_count: f.number(9)?,
        entry_count: f.number(12)?,
    };
    let updated_account_count = f.number(10)?;
    if f.repeated(6).count() as u64 != metadata.executed_transaction_count
        || f.repeated(11).count() as u64 != updated_account_count
        || f.repeated(13).count() as u64 != metadata.entry_count
    {
        return Err(err("STREAM_PROVIDER_BLOCK_COUNT"));
    }
    let transactions = f
        .repeated(6)
        .map(|raw| project_tx_info(raw?, bank.clone(), limits))
        .collect::<Result<Vec<_>>>()?;
    let accounts = f
        .repeated(11)
        .map(|raw| project_account_info(raw?, bank.clone(), false, limits))
        .collect::<Result<Vec<_>>>()?;
    let mut indexes = BTreeSet::new();
    let entries = f
        .repeated(13)
        .map(|raw| {
            let raw = raw?;
            let entry = fields(raw, limits)?;
            entry.allowed(&[1, 2, 3, 4, 5, 6, 7])?;
            if entry.number(1)? != bank.slot
                || entry.bank_id(7)? != bank.bank_id
                || entry.number(2)? >= metadata.entry_count
                || !indexes.insert(entry.number(2)?)
            {
                return Err(err("STREAM_PROVIDER_BLOCK_CONFLICT"));
            }
            Ok(raw.to_vec())
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ProviderBlock {
        metadata,
        updated_account_count,
        accounts,
        transactions,
        entries,
    })
}
/// Decode the pinned Yellowstone interface AFTER exact raw reception is durable.
/// A bounded wire reader checks list sizes before allocating projected messages,
/// preserves transaction/meta submessage bytes, rejects ambiguous oneofs and
/// missing native bank IDs, and never synthesizes an account completion manifest.
/// Full blocks describe the provider's returned event set only. Footer certificates,
/// transaction-status-only, entry-parent resets and unsupported fields fail closed.
pub fn project_yellowstone(
    record: &CapturedFrame,
    limits: StreamLimits,
) -> Result<YellowstoneEvent> {
    if limits.max_bytes == 0
        || limits.max_updates == 0
        || limits.max_slots == 0
        || record.frame.protobuf.len() > limits.max_bytes
    {
        return Err(err("STREAM_PROJECTION_LIMIT"));
    }
    let f = fields(&record.frame.protobuf, limits)?;
    f.allowed(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13])?;
    let updates =
        f.0.iter()
            .filter(|(n, _)| *n != 1 && *n != 11)
            .collect::<Vec<_>>();
    if updates.len() != 1 {
        return Err(proto_error());
    }
    let &(tag, Wire::Bytes(raw)) = updates[0] else {
        return Err(proto_error());
    };
    match tag {
        2 => {
            let a = fields(raw, limits)?;
            a.allowed(&[1, 2, 3, 4])?;
            if a.boolean(3)? {
                return Err(err("STREAM_STARTUP_ACCOUNT"));
            }
            let bank = BankId {
                slot: a.number(2)?,
                bank_id: a.bank_id(4)?,
            };
            Ok(YellowstoneEvent::Account(project_account_info(
                a.bytes(1)?,
                bank,
                false,
                limits,
            )?))
        }
        3 => {
            let slot = fields(raw, limits)?;
            slot.allowed(&[1, 2, 3, 4, 5])?;
            let status = slot.number(3)?;
            let parent_slot = slot.optional_number(2)?;
            let number = slot.number(1)?;
            if matches!(status, 3 | 4) {
                if slot.optional_number(5)?.is_some() {
                    return Err(err("STREAM_SLOT_BANK_AMBIGUITY"));
                }
                return Ok(YellowstoneEvent::BanklessSlot {
                    slot: number,
                    parent_slot,
                    status: if status == 3 {
                        BanklessSlotStatus::FirstShredReceived
                    } else {
                        BanklessSlotStatus::Completed
                    },
                });
            }
            let status = match status {
                0 => SlotStatus::Processed,
                1 => SlotStatus::Confirmed,
                2 => SlotStatus::Finalized,
                5 => SlotStatus::CreatedBank,
                6 => SlotStatus::Dead,
                _ => return Err(err("STREAM_UNKNOWN_SLOT_STATUS")),
            };
            Ok(YellowstoneEvent::Slot(SlotUpdate {
                bank: BankId {
                    slot: number,
                    bank_id: slot.bank_id(5)?,
                },
                parent_slot,
                status,
            }))
        }
        4 => {
            let t = fields(raw, limits)?;
            t.allowed(&[1, 2, 3])?;
            Ok(YellowstoneEvent::Transaction(project_tx_info(
                t.bytes(1)?,
                BankId {
                    slot: t.number(2)?,
                    bank_id: t.bank_id(3)?,
                },
                limits,
            )?))
        }
        5 => Ok(YellowstoneEvent::ProviderBlock(project_block(raw, limits)?)),
        7 => {
            let m = fields(raw, limits)?;
            m.allowed(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10])?;
            Ok(YellowstoneEvent::Metadata(BlockMetadata {
                bank: BankId {
                    slot: m.number(1)?,
                    bank_id: m.bank_id(10)?,
                },
                blockhash: m.hash(2)?,
                parent: ParentBank {
                    slot: m.number(6)?,
                    blockhash: m.hash(7)?,
                },
                executed_transaction_count: m.number(8)?,
                entry_count: m.number(9)?,
            }))
        }
        6 | 9 => {
            let control = fields(raw, limits)?;
            control.allowed(if tag == 6 { &[] } else { &[1] })?;
            if tag == 9 {
                control.number(1)?;
            }
            Ok(YellowstoneEvent::Heartbeat)
        }
        _ => Err(err("STREAM_UNSUPPORTED_EVENT")),
    }
}
/// Main's raw Subscription::capture_next may feed this directly. Projection or
/// admission failure invalidates readiness while the captured frame remains in WAL.
pub fn project_and_admit(
    assembler: &mut StreamAssembler,
    record: &CapturedFrame,
    limits: StreamLimits,
) -> Result<()> {
    let result = project_yellowstone(record, limits)
        .and_then(|event| assembler.admit_captured(record, event));
    if result.is_err() {
        assembler.invalidate();
    }
    result
}
