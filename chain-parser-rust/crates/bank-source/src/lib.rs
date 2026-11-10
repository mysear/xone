//! Native self-hosted Agave observer, NOT a Geyser block reconstruction proof.
//! The supplied pinned patch calls observe_write synchronously at the notifier
//! and freezes a complete scope through Bank::get_account after freeze's lock.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
#[derive(Debug)]
pub struct SourceError(String);
impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SourceError {}
pub type Result<T> = std::result::Result<T, SourceError>;
pub fn source_error(reason: &str) -> SourceError {
    SourceError(reason.into())
}
fn err(reason: &str) -> SourceError {
    source_error(reason)
}
fn key(value: &str) -> Result<[u8; 32]> {
    bs58::decode(value)
        .into_vec()
        .map_err(|_| err("NATIVE_SOURCE_PUBKEY"))?
        .try_into()
        .map_err(|_| err("NATIVE_SOURCE_PUBKEY"))
}
fn read_bounded<T: serde::de::DeserializeOwned>(path: &std::path::Path, max: usize) -> Result<T> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|_| err("NATIVE_SOURCE_IO"))?;
    let mut b = Vec::new();
    f.take(max as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| err("NATIVE_SOURCE_IO"))?;
    if b.len() > max {
        return Err(err("NATIVE_SOURCE_BYTE_BUDGET"));
    }
    serde_json::from_slice(&b).map_err(|_| err("NATIVE_SOURCE_SCHEMA"))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ManifestAccount {
    pub pubkey: String,
    pub owner: String,
    pub lamports: u64,
    pub executable: bool,
    pub rent_epoch: u64,
    pub write_slot: u64,
    pub write_bank_id: u64,
    pub write_version: u64,
    pub data_base64: String,
    pub txn_signature_base64: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FinalVersion {
    pub pubkey: String,
    pub write_version: u64,
    pub account_sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ManifestAnchor {
    pub slot: u64,
    pub bank_id: u64,
    pub blockhash: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct BankManifest {
    pub version: u32,
    pub chain_key: String,
    pub genesis_hash: String,
    pub source_id: String,
    pub session_id: String,
    pub catalog_identity: String,
    pub parent: ManifestAnchor,
    pub bank: ManifestAnchor,
    pub timestamp_ms: u64,
    pub parent_accounts: Vec<ManifestAccount>,
    pub expected_account_updates: u64,
    pub final_versions: Vec<FinalVersion>,
    pub skipped_slots: Vec<u64>,
    #[serde(default)]
    pub native_writes: Vec<ManifestAccount>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Catalog {
    version: u32,
    revision: u64,
    chain_key: String,
    genesis_hash: String,
}
fn fingerprint(a: &ManifestAccount) -> Result<String> {
    let mut h = Sha256::new();
    h.update(b"xone/solana/account-v1\0");
    h.update(a.write_slot.to_be_bytes());
    h.update(a.write_bank_id.to_be_bytes());
    h.update(key(&a.pubkey)?);
    h.update(key(&a.owner)?);
    h.update(a.lamports.to_be_bytes());
    h.update([u8::from(a.executable), 0]);
    h.update(a.rent_epoch.to_be_bytes());
    h.update(a.write_version.to_be_bytes());
    let data = STANDARD
        .decode(&a.data_base64)
        .map_err(|_| err("NATIVE_SOURCE_DATA"))?;
    h.update((data.len() as u64).to_be_bytes());
    h.update(data);
    if let Some(s) = &a.txn_signature_base64 {
        let s = STANDARD
            .decode(s)
            .map_err(|_| err("NATIVE_SOURCE_SIGNATURE"))?;
        if s.len() != 64 {
            return Err(err("NATIVE_SOURCE_SIGNATURE"));
        }
        h.update([1]);
        h.update(s);
    } else {
        h.update([0]);
    }
    Ok(format!("{:x}", h.finalize()))
}
pub const AGAVE_TAG: &str = "v4.3.0";
pub const YELLOWSTONE_PIN: &str = "10f0af69d150637d8e3eaaf83b179d640988dd3c";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeIdentity {
    pub slot: u64,
    pub bank_id: u64,
    pub blockhash: String,
    pub state_hash: String,
    pub parent_bank_id: Option<u64>,
    pub timestamp_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenAccount {
    pub pubkey: String,
    pub owner: String,
    pub lamports: u64,
    pub executable: bool,
    pub rent_epoch: u64,
    pub data_base64: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub chain_key: String,
    pub genesis_hash: String,
    pub source_id: String,
    pub catalog_file: PathBuf,
    pub manifest_directory: PathBuf,
    pub max_bytes: usize,
    pub max_updates: usize,
    pub max_banks: usize,
    pub max_sessions: usize,
    #[serde(default)]
    pub refs_endpoint: Option<String>,
    #[serde(default)]
    pub refs_token_file: Option<PathBuf>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    pub version: u32,
    pub chain_key: String,
    pub genesis_hash: String,
    pub source_id: String,
    pub session_id: String,
    pub catalog_identity: String,
    pub catalog_revision: u64,
    pub scope_accounts: Vec<String>,
    pub expires_ms: u64,
}
#[derive(Clone)]
struct Frozen {
    writes: Vec<ManifestAccount>,
    identity: NativeIdentity,
    rows: BTreeMap<String, ManifestAccount>,
}
struct Writes {
    rows: BTreeMap<(String, u64), ManifestAccount>,
    bytes: usize,
}
pub struct Observer {
    config: SourceConfig,
    catalog: Catalog,
    scope: BTreeSet<String>,
    scope_bytes: BTreeSet<[u8; 32]>,
    genesis: Option<String>,
    notifier_live: bool,
    writes: BTreeMap<u64, Writes>,
    banks: BTreeMap<u64, Frozen>,
    failed: Option<String>,
}
impl Observer {
    pub fn new(config: SourceConfig) -> Result<Self> {
        if config.max_bytes == 0
            || config.max_updates == 0
            || config.max_banks < 2
            || config.max_sessions == 0
        {
            return Err(err("NATIVE_SOURCE_BUDGET"));
        }
        let catalog: Catalog = read_bounded(&config.catalog_file, config.max_bytes)?;
        if catalog.version != 1
            || catalog.chain_key != config.chain_key
            || catalog.genesis_hash != config.genesis_hash
        {
            return Err(err("NATIVE_SOURCE_CHAIN"));
        }
        let scope = BTreeSet::new();
        Ok(Self {
            config,
            catalog,
            scope,
            scope_bytes: BTreeSet::new(),
            genesis: None,
            notifier_live: false,
            writes: BTreeMap::new(),
            banks: BTreeMap::new(),
            failed: None,
        })
    }
    pub fn genesis(&mut self, actual: &str) -> Result<()> {
        if actual != self.config.genesis_hash {
            self.failed = Some("NATIVE_SOURCE_GENESIS".into());
            return Err(err("NATIVE_SOURCE_GENESIS"));
        }
        self.genesis = Some(actual.into());
        Ok(())
    }
    pub fn keys(&mut self) -> Vec<String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_millis() as u64);
        let _ = self.refresh_scope(now);
        self.scope.iter().cloned().collect()
    }
    fn refresh_scope(&mut self, now: u64) -> Result<()> {
        // Native freeze is gated by configured_keys(), so crash cleanup must
        // also be reachable before this Observer has ever admitted a scope.
        self.clean_expired_requests(now)?;
        let requests = self.sessions(now)?;
        self.scope_from_requests(&requests)
    }
    fn scope_from_requests(&mut self, requests: &[SessionRequest]) -> Result<()> {
        let mut scope = BTreeSet::new();
        for r in requests {
            for id in &r.scope_accounts {
                key(id)?;
                scope.insert(id.clone());
                if scope.len() > self.config.max_updates {
                    return self.poison("NATIVE_SOURCE_SCOPE_BUDGET");
                }
            }
        }
        self.scope_bytes = scope
            .iter()
            .map(|id| key(id))
            .collect::<Result<BTreeSet<_>>>()?;
        self.scope = scope;
        Ok(())
    }
    pub fn observe(&mut self, row: ManifestAccount) -> Result<()> {
        self.notifier_live = true;
        if self.scope.is_empty() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |t| t.as_millis() as u64);
            self.refresh_scope(now)?;
        }
        if let Some(error) = &self.failed {
            return Err(err(error));
        }
        if self.banks.contains_key(&row.write_bank_id) {
            return self.poison("NATIVE_SOURCE_WRITE_AFTER_FREEZE");
        }
        if !self.scope.contains(&row.pubkey) {
            return Ok(());
        }
        key(&row.pubkey)?;
        key(&row.owner)?;
        // Agave starts its actual notifier counter at zero. Presence in this
        // callback, source/bank identity and conflict checks establish an
        // observed write; the numeric value is not a missing-metadata marker.
        if self.writes.len() >= self.config.max_banks
            && !self.writes.contains_key(&row.write_bank_id)
        {
            return self.poison("NATIVE_SOURCE_BANK_BUDGET");
        }
        let bytes = serde_json::to_vec(&row)
            .map_err(|_| err("NATIVE_SOURCE_ENCODING"))?
            .len();
        let total = self.writes.values().map(|w| w.bytes).sum::<usize>();
        if total
            .checked_add(bytes)
            .is_none_or(|n| n > self.config.max_bytes)
        {
            return self.poison("NATIVE_SOURCE_BYTE_BUDGET");
        }
        let w = self
            .writes
            .entry(row.write_bank_id)
            .or_insert_with(|| Writes {
                rows: BTreeMap::new(),
                bytes: 0,
            });
        if let Some(old) = w.rows.get(&(row.pubkey.clone(), row.write_version)) {
            if serde_json::to_vec(old).ok() != serde_json::to_vec(&row).ok() {
                return self.poison("NATIVE_SOURCE_WRITE_CONFLICT");
            }
            return Ok(());
        }
        if w.rows.len() >= self.config.max_updates {
            return self.poison("NATIVE_SOURCE_UPDATE_BUDGET");
        }
        w.bytes += bytes;
        w.rows.insert((row.pubkey.clone(), row.write_version), row);
        Ok(())
    }
    fn poison<T>(&mut self, reason: &str) -> Result<T> {
        self.failed = Some(reason.into());
        Err(err(reason))
    }
    /// Called only by the native Bank destructor for this exact bank identity.
    /// Frozen evidence may still anchor live children or retained owner references.
    /// Cleanup does not reset any integrity or resource failure.
    pub fn drop_bank(&mut self, bank_id: u64) {
        self.writes.remove(&bank_id);
    }

    /// `read` MUST be the patched native frozen Bank read, not RPC or stream bytes.
    pub fn frozen(
        &mut self,
        id: NativeIdentity,
        read: impl Fn(&str) -> Result<FrozenAccount>,
    ) -> Result<Vec<BankManifest>> {
        if let Some(reason) = self.failed.as_deref() {
            return Err(err(reason));
        }
        if self.genesis.as_deref() != Some(&self.config.genesis_hash) || !self.notifier_live {
            return Err(err("NATIVE_SOURCE_AUTHORITY_UNAVAILABLE"));
        }
        if let Some(old) = self.banks.get(&id.bank_id) {
            if old.identity.state_hash == id.state_hash && old.identity.slot == id.slot {
                return Ok(vec![]);
            }
            return self.poison("NATIVE_SOURCE_BANK_ID_REUSE");
        }
        if id.state_hash.is_empty() || key(&id.state_hash).is_err() || key(&id.blockhash).is_err() {
            return Err(err("NATIVE_SOURCE_BANK_IDENTITY"));
        }
        let fresh: Catalog = read_bounded(&self.config.catalog_file, self.config.max_bytes)?;
        if fresh.chain_key != self.catalog.chain_key
            || fresh.genesis_hash != self.catalog.genesis_hash
            || fresh.revision < self.catalog.revision
        {
            return Err(err("NATIVE_SOURCE_CATALOG_FENCE"));
        }
        self.catalog = fresh;
        let control_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |t| t.as_millis() as u64);
        // Reuse one complete inventory for scope, export and re-attestation.
        // One bounded maintenance pass + one admission pass replaces the three
        // former directory scans; neither the scan nor per-file byte cap grows.
        self.clean_expired_requests(control_now)?;
        let requests = self.sessions(control_now)?;
        self.scope_from_requests(&requests)?;
        let parent = id.parent_bank_id.and_then(|p| self.banks.get(&p)).cloned();
        let updates = self.writes.remove(&id.bank_id).unwrap_or(Writes {
            rows: BTreeMap::new(),
            bytes: 0,
        });
        let mut rows = BTreeMap::new();
        let mut finals = vec![];
        let mut bytes = 0usize;
        let mut can_export = parent.is_some();
        for pubkey in &self.scope {
            let frozen = read(pubkey)?;
            if frozen.pubkey != *pubkey {
                return Err(err("NATIVE_SOURCE_READ_KEY"));
            }
            let latest = updates
                .rows
                .iter()
                .filter(|((k, _), _)| k == pubkey)
                .next_back()
                .map(|(_, r)| r);
            let observed = latest.or_else(|| parent.as_ref().and_then(|p| p.rows.get(pubkey)));
            let row = if let Some(row) = observed {
                if !same_state(row, &frozen) {
                    return self.poison("NATIVE_SOURCE_FINAL_BYTES_MISMATCH");
                }
                row.clone()
            } else {
                // Explicit frozen-bank baseline state, version 0; this is never presented
                // as a historical native write. The consumer gets it only as parent state.
                can_export = false;
                ManifestAccount {
                    pubkey: pubkey.clone(),
                    owner: frozen.owner,
                    lamports: frozen.lamports,
                    executable: frozen.executable,
                    rent_epoch: frozen.rent_epoch,
                    write_slot: id.slot,
                    write_bank_id: id.bank_id,
                    write_version: 0,
                    data_base64: frozen.data_base64,
                    txn_signature_base64: None,
                }
            };
            bytes = bytes
                .checked_add(
                    serde_json::to_vec(&row)
                        .map_err(|_| err("NATIVE_SOURCE_ENCODING"))?
                        .len(),
                )
                .ok_or_else(|| err("NATIVE_SOURCE_BYTE_BUDGET"))?;
            if bytes > self.config.max_bytes {
                return self.poison("NATIVE_SOURCE_BYTE_BUDGET");
            }
            finals.push(FinalVersion {
                pubkey: pubkey.clone(),
                write_version: row.write_version,
                account_sha256: fingerprint(&row)?,
            });
            rows.insert(pubkey.clone(), row);
        }
        let mut manifests = vec![];
        if can_export {
            let p = parent.ok_or_else(|| err("NATIVE_SOURCE_PARENT"))?;
            // A skipped range is accepted only from actual parent/child Bank identities.
            if id.slot <= p.identity.slot
                || id.slot - p.identity.slot - 1 > self.config.max_banks as u64
            {
                return Err(err("NATIVE_SOURCE_SKIP_BUDGET"));
            }
            let skipped = (p.identity.slot + 1..id.slot).collect::<Vec<_>>();
            self.gc_manifests()?;
            for request in &requests {
                let m = BankManifest {
                    version: 2,
                    chain_key: self.config.chain_key.clone(),
                    genesis_hash: self.config.genesis_hash.clone(),
                    source_id: self.config.source_id.clone(),
                    session_id: request.session_id.clone(),
                    catalog_identity: request.catalog_identity.clone(),
                    parent: ManifestAnchor {
                        slot: p.identity.slot,
                        bank_id: p.identity.bank_id,
                        blockhash: p.identity.blockhash.clone(),
                    },
                    bank: ManifestAnchor {
                        slot: id.slot,
                        bank_id: id.bank_id,
                        blockhash: id.blockhash.clone(),
                    },
                    timestamp_ms: id.timestamp_ms,
                    parent_accounts: p.rows.values().cloned().collect(),
                    expected_account_updates: updates
                        .rows
                        .iter()
                        .filter(|((id, _), _)| self.scope.contains(id))
                        .count() as u64,
                    final_versions: finals.clone(),
                    skipped_slots: skipped.clone(),
                    native_writes: updates
                        .rows
                        .values()
                        .filter(|r| self.scope.contains(&r.pubkey))
                        .cloned()
                        .collect(),
                };
                atomic_manifest(&self.config.manifest_directory, &m, self.config.max_bytes)?;
                manifests.push(m);
            }
        }
        let cache_bytes = bytes.saturating_add(updates.bytes);
        if cache_bytes > self.config.max_bytes {
            return self.poison("NATIVE_SOURCE_BYTE_BUDGET");
        }
        while !self.banks.is_empty()
            && self
                .banks
                .values()
                .map(|b| {
                    b.rows
                        .values()
                        .map(|r| r.data_base64.len() + 512)
                        .sum::<usize>()
                        .saturating_add(
                            b.writes
                                .iter()
                                .map(|r| r.data_base64.len() + 512)
                                .sum::<usize>(),
                        )
                })
                .sum::<usize>()
                .saturating_add(cache_bytes)
                > self.config.max_bytes
        {
            let old = *self
                .banks
                .keys()
                .next()
                .ok_or_else(|| err("NATIVE_SOURCE_BANK_BUDGET"))?;
            self.banks.remove(&old);
        }
        self.banks.insert(
            id.bank_id,
            Frozen {
                identity: id,
                rows,
                writes: updates
                    .rows
                    .into_values()
                    .filter(|r| self.scope.contains(&r.pubkey))
                    .collect(),
            },
        );
        while self.banks.len() > self.config.max_banks {
            let old = *self
                .banks
                .keys()
                .next()
                .ok_or_else(|| err("NATIVE_SOURCE_BANK_BUDGET"))?;
            self.banks.remove(&old);
        }
        // Re-attest retained immutable native banks for a new receiver session.
        // This is bounded replay from real Bank observations, never a comparison
        // of write versions across receiver connections.
        for request in &requests {
            for bank in self.banks.values() {
                let Some(parent) = bank
                    .identity
                    .parent_bank_id
                    .and_then(|id| self.banks.get(&id))
                else {
                    continue;
                };
                if !bank.rows.keys().eq(self.scope.iter())
                    || !parent.rows.keys().eq(self.scope.iter())
                {
                    continue;
                }
                let path = self.config.manifest_directory.join(format!(
                    "{}-{}-{}.json",
                    request.session_id, bank.identity.slot, bank.identity.bank_id
                ));
                if path.exists() {
                    continue;
                }
                if bank.identity.slot <= parent.identity.slot
                    || bank.identity.slot - parent.identity.slot - 1 > self.config.max_banks as u64
                {
                    return Err(err("NATIVE_SOURCE_SKIP_BUDGET"));
                }
                let skipped = (parent.identity.slot + 1..bank.identity.slot).collect::<Vec<_>>();
                let m = BankManifest {
                    version: 2,
                    chain_key: self.config.chain_key.clone(),
                    genesis_hash: self.config.genesis_hash.clone(),
                    source_id: self.config.source_id.clone(),
                    session_id: request.session_id.clone(),
                    catalog_identity: request.catalog_identity.clone(),
                    parent: ManifestAnchor {
                        slot: parent.identity.slot,
                        bank_id: parent.identity.bank_id,
                        blockhash: parent.identity.blockhash.clone(),
                    },
                    bank: ManifestAnchor {
                        slot: bank.identity.slot,
                        bank_id: bank.identity.bank_id,
                        blockhash: bank.identity.blockhash.clone(),
                    },
                    timestamp_ms: bank.identity.timestamp_ms,
                    parent_accounts: parent.rows.values().cloned().collect(),
                    expected_account_updates: bank.writes.len() as u64,
                    final_versions: bank
                        .rows
                        .values()
                        .map(|r| {
                            Ok(FinalVersion {
                                pubkey: r.pubkey.clone(),
                                write_version: r.write_version,
                                account_sha256: fingerprint(r)?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                    skipped_slots: skipped,
                    native_writes: bank.writes.clone(),
                };
                atomic_manifest(&self.config.manifest_directory, &m, self.config.max_bytes)?;
                manifests.push(m);
            }
        }
        Ok(manifests)
    }
    fn gc_manifests(&self) -> Result<()> {
        let Some(endpoint) = &self.config.refs_endpoint else {
            return Ok(());
        };
        let refs = live_refs(
            endpoint,
            self.config.refs_token_file.as_deref(),
            self.config.max_bytes,
        )?;
        if refs.chain_key != self.config.chain_key || !refs.gc_permitted {
            return Err(err("NATIVE_SOURCE_REFS_UNAVAILABLE"));
        }
        let retained = self
            .banks
            .values()
            .map(|b| b.identity.blockhash.as_str())
            .collect::<BTreeSet<_>>();
        for entry in std::fs::read_dir(&self.config.manifest_directory)
            .map_err(|_| err("NATIVE_SOURCE_IO"))?
        {
            let path = entry.map_err(|_| err("NATIVE_SOURCE_IO"))?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let manifest: BankManifest = read_bounded(&path, self.config.max_bytes)?;
            if manifest.bank.slot <= refs.committed_slot
                && !retained.contains(manifest.bank.blockhash.as_str())
                && !refs
                    .protected_blockhashes
                    .contains(&manifest.bank.blockhash)
            {
                std::fs::remove_file(path).map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))?;
            }
        }
        std::fs::File::open(&self.config.manifest_directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))
    }
    fn directory_budget(&self) -> Result<usize> {
        self.config
            .max_sessions
            .checked_mul(8)
            .and_then(|n| {
                self.config
                    .max_banks
                    .checked_mul(self.config.max_sessions)
                    .and_then(|m| n.checked_add(m))
            })
            .ok_or_else(|| err("NATIVE_SOURCE_DIRECTORY_BUDGET"))
    }
    fn owned_request(&self, path: &std::path::Path, r: &SessionRequest) -> bool {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let filename = name == format!("{}.request", r.session_id)
            || (name.starts_with(&format!(".xone-expired-{}-", r.session_id))
                && name.ends_with(".request"));
        filename
            && r.version == 1
            && r.chain_key == self.config.chain_key
            && r.genesis_hash == self.config.genesis_hash
            && r.source_id == self.config.source_id
            && r.catalog_revision > 0
            && r.catalog_identity.len() == 64
            && r.catalog_identity.bytes().all(|c| c.is_ascii_hexdigit())
            && !r.session_id.is_empty()
            && r.session_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            && !r.scope_accounts.is_empty()
            && r.scope_accounts.len() <= self.config.max_updates
            && r.scope_accounts.iter().all(|id| key(id).is_ok())
    }
    fn restore_request(&self, staged: &std::path::Path, original: &std::path::Path) -> Result<()> {
        // create-only restoration cannot overwrite a receiver renewal. Ambiguous
        // staging remains retained rather than deleting any possibly live bytes.
        std::fs::hard_link(staged, original).map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))?;
        std::fs::remove_file(staged).map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))
    }
    fn clean_expired_requests(&self, now: u64) -> Result<()> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = &self.config.manifest_directory;
        std::fs::create_dir_all(directory).map_err(|_| err("NATIVE_SOURCE_IO"))?;
        let mut changed = false;
        let entries = std::fs::read_dir(directory).map_err(|_| err("NATIVE_SOURCE_IO"))?;
        for entry in entries.take(self.directory_budget()?) {
            let path = entry.map_err(|_| err("NATIVE_SOURCE_IO"))?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("request") {
                continue;
            }
            if !std::fs::symlink_metadata(&path)
                .map_err(|_| err("NATIVE_SOURCE_IO"))?
                .file_type()
                .is_file()
            {
                continue;
            }
            let Ok(r) =
                read_bounded::<SessionRequest>(&path, self.config.max_bytes.min(4 * 1024 * 1024))
            else {
                continue;
            };
            if !self.owned_request(&path, &r) {
                continue;
            }
            let staged = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".xone-expired-"));
            if staged {
                if r.expires_ms <= now {
                    std::fs::remove_file(&path).map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))?;
                } else {
                    self.restore_request(
                        &path,
                        &directory.join(format!("{}.request", r.session_id)),
                    )?;
                }
                changed = true;
                continue;
            }
            if r.expires_ms > now {
                continue;
            }
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |t| t.as_nanos());
            let staged = directory.join(format!(
                ".xone-expired-{}-{}-{nonce}-{}.request",
                r.session_id,
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            // Claim the unique destination, then move the actual directory entry.
            let stage = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged)
                .map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))?;
            drop(stage);
            if let Err(e) = std::fs::rename(&path, &staged) {
                std::fs::remove_file(&staged).map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))?;
                if e.kind() == std::io::ErrorKind::NotFound {
                    continue;
                }
                return Err(err("NATIVE_SOURCE_GC_UNKNOWN"));
            }
            // A renewal can replace the path between initial read and rename.
            // Re-read the moved inode; never unlink a renewed/unknown request.
            let moved =
                read_bounded::<SessionRequest>(&staged, self.config.max_bytes.min(4 * 1024 * 1024));
            match moved {
                Ok(ref moved) if self.owned_request(&staged, moved) && moved.expires_ms <= now => {
                    std::fs::remove_file(&staged).map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))?;
                }
                _ => {
                    self.restore_request(&staged, &path)?;
                }
            }
            changed = true;
        }
        if changed {
            std::fs::File::open(directory)
                .and_then(|f| f.sync_all())
                .map_err(|_| err("NATIVE_SOURCE_GC_UNKNOWN"))?;
        }
        Ok(())
    }
    fn sessions(&self, now: u64) -> Result<Vec<SessionRequest>> {
        std::fs::create_dir_all(&self.config.manifest_directory)
            .map_err(|_| err("NATIVE_SOURCE_IO"))?;
        let mut requests = vec![];
        let mut examined = 0;
        for row in std::fs::read_dir(&self.config.manifest_directory)
            .map_err(|_| err("NATIVE_SOURCE_IO"))?
        {
            let path = row.map_err(|_| err("NATIVE_SOURCE_IO"))?.path();
            examined += 1;
            if examined > self.directory_budget()? {
                return Err(err("NATIVE_SOURCE_DIRECTORY_BUDGET"));
            }
            if path.extension().and_then(|s| s.to_str()) != Some("request") {
                continue;
            }
            let r: SessionRequest =
                read_bounded(&path, self.config.max_bytes.min(4 * 1024 * 1024))?;
            if r.expires_ms <= now {
                continue;
            }
            if r.version != 1
                || r.chain_key != self.config.chain_key
                || r.genesis_hash != self.config.genesis_hash
                || r.source_id != self.config.source_id
                || r.catalog_revision != self.catalog.revision
                || r.catalog_identity.len() != 64
                || !r
                    .session_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            {
                return Err(err("NATIVE_SOURCE_REQUEST"));
            }
            if requests.len() >= self.config.max_sessions {
                return Err(err("NATIVE_SOURCE_SESSION_BUDGET"));
            }
            requests.push(r);
        }
        Ok(requests)
    }
}
fn same_state(a: &ManifestAccount, b: &FrozenAccount) -> bool {
    a.pubkey == b.pubkey
        && a.owner == b.owner
        && a.lamports == b.lamports
        && a.executable == b.executable
        && a.rent_epoch == b.rent_epoch
        && a.data_base64 == b.data_base64
}
fn atomic_manifest(root: &std::path::Path, m: &BankManifest, max: usize) -> Result<()> {
    let bytes = serde_json::to_vec(m).map_err(|_| err("NATIVE_SOURCE_ENCODING"))?;
    if bytes.len() > max {
        return Err(err("NATIVE_SOURCE_BYTE_BUDGET"));
    }
    let path = root.join(format!(
        "{}-{}-{}.json",
        m.session_id, m.bank.slot, m.bank.bank_id
    ));
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp).map_err(|_| err("NATIVE_SOURCE_IO"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| err("NATIVE_SOURCE_IO"))?;
    std::fs::rename(tmp, path).map_err(|_| err("NATIVE_SOURCE_IO"))?;
    std::fs::File::open(root)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("NATIVE_SOURCE_IO"))
}
static OBSERVER: OnceLock<Mutex<Option<Observer>>> = OnceLock::new();
fn global() -> &'static Mutex<Option<Observer>> {
    OBSERVER.get_or_init(|| Mutex::new(None))
}
pub fn install(config: SourceConfig, genesis: &str) -> Result<()> {
    let mut observer = Observer::new(config)?;
    observer.genesis(genesis)?;
    *global().lock().map_err(|_| err("NATIVE_SOURCE_LOCK"))? = Some(observer);
    Ok(())
}
pub fn initialize(genesis: &str) {
    if let Some(path) = std::env::var_os("XONE_BANK_SOURCE_CONFIG") {
        let result = read_bounded::<SourceConfig>(&PathBuf::from(path), 4 * 1024 * 1024)
            .and_then(|c| install(c, genesis));
        if result.is_err() {
            eprintln!("xone native bank source initialization failed");
        }
    }
}
pub fn configured_keys() -> Vec<String> {
    global()
        .lock()
        .ok()
        .and_then(|mut g| g.as_mut().map(Observer::keys))
        .unwrap_or_default()
}
pub struct AccountNotification<'a> {
    pub slot: u64,
    pub bank_id: u64,
    pub pubkey: &'a [u8],
    pub owner: &'a [u8],
    pub lamports: u64,
    pub executable: bool,
    pub rent_epoch: u64,
    pub write_version: u64,
    pub data: &'a [u8],
    pub signature: Option<&'a [u8]>,
}
pub fn observe_account(n: AccountNotification<'_>) {
    if let Ok(mut g) = global().lock() {
        if let Some(observer) = g.as_mut() {
            observer.notifier_live = true;
            let Ok(pubkey_bytes) = <&[u8; 32]>::try_from(n.pubkey) else {
                return;
            };
            if !observer.scope_bytes.contains(pubkey_bytes) {
                return;
            }
            let pubkey = bs58::encode(n.pubkey).into_string();
            if n.data.len() > observer.config.max_bytes {
                let _ = observer.poison::<()>("NATIVE_SOURCE_BYTE_BUDGET");
                return;
            }
            let _ = observer.observe(ManifestAccount {
                pubkey,
                owner: bs58::encode(n.owner).into_string(),
                lamports: n.lamports,
                executable: n.executable,
                rent_epoch: n.rent_epoch,
                write_slot: n.slot,
                write_bank_id: n.bank_id,
                write_version: n.write_version,
                data_base64: STANDARD.encode(n.data),
                txn_signature_base64: n.signature.map(|v| STANDARD.encode(v)),
            });
        }
    }
}

/// Release pending observations when the real native Bank is destroyed.
/// Unknown/repeated IDs are harmless; immutable frozen evidence remains retained.
pub fn drop_bank(bank_id: u64) {
    if let Ok(mut g) = global().lock() {
        if let Some(observer) = g.as_mut() {
            observer.drop_bank(bank_id);
        }
    }
}

pub fn frozen_bank(identity: NativeIdentity, read: impl Fn(&str) -> Result<FrozenAccount>) {
    if let Ok(mut g) = global().lock() {
        if let Some(observer) = g.as_mut() {
            if let Err(error) = observer.frozen(identity, read) {
                eprintln!("xone native bank scope export refused: {error}");
            }
        }
    }
}

#[derive(Deserialize)]
struct LiveRefs {
    chain_key: String,
    protected_blockhashes: BTreeSet<String>,
    gc_permitted: bool,
    #[serde(default)]
    committed_slot: u64,
}
fn live_refs(endpoint: &str, token: Option<&std::path::Path>, max: usize) -> Result<LiveRefs> {
    use std::io::Read;
    let raw = endpoint
        .strip_prefix("http://127.0.0.1:")
        .ok_or_else(|| err("NATIVE_SOURCE_REFS_ENDPOINT"))?;
    let (port, path) = raw
        .split_once('/')
        .ok_or_else(|| err("NATIVE_SOURCE_REFS_ENDPOINT"))?;
    if path.bytes().any(|b| b.is_ascii_control()) {
        return Err(err("NATIVE_SOURCE_REFS_ENDPOINT"));
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| err("NATIVE_SOURCE_REFS_ENDPOINT"))?;
    let mut socket = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(100),
    )
    .map_err(|_| err("NATIVE_SOURCE_REFS_UNAVAILABLE"))?;
    socket
        .set_read_timeout(Some(std::time::Duration::from_millis(100)))
        .map_err(|_| err("NATIVE_SOURCE_REFS_UNAVAILABLE"))?;
    let auth = if let Some(path) = token {
        let mut bytes = String::new();
        std::fs::File::open(path)
            .map_err(|_| err("NATIVE_SOURCE_REFS_TOKEN"))?
            .take(4097)
            .read_to_string(&mut bytes)
            .map_err(|_| err("NATIVE_SOURCE_REFS_TOKEN"))?;
        let bytes = bytes.trim();
        if bytes.len() > 4096 || bytes.bytes().any(|b| b.is_ascii_control()) {
            return Err(err("NATIVE_SOURCE_REFS_TOKEN"));
        }
        format!("Authorization: Bearer {bytes}\r\n")
    } else {
        String::new()
    };
    let request = format!(
        "GET /{path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Connection: close\r\n\r\n"
    );
    socket
        .write_all(request.as_bytes())
        .map_err(|_| err("NATIVE_SOURCE_REFS_UNAVAILABLE"))?;
    let mut raw = Vec::new();
    socket
        .take(max as u64 + 16385)
        .read_to_end(&mut raw)
        .map_err(|_| err("NATIVE_SOURCE_REFS_UNAVAILABLE"))?;
    if raw.len() > max + 16384 {
        return Err(err("NATIVE_SOURCE_REFS_BUDGET"));
    }
    let split = raw
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .ok_or_else(|| err("NATIVE_SOURCE_REFS_HTTP"))?;
    if !raw.starts_with(b"HTTP/1.1 200 ") || split > 16384 {
        return Err(err("NATIVE_SOURCE_REFS_HTTP"));
    }
    serde_json::from_slice(&raw[split + 4..]).map_err(|_| err("NATIVE_SOURCE_REFS_SCHEMA"))
}
