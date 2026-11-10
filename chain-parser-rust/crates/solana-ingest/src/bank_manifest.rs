//! Independent public bank export input. These files are authority inputs from a
//! caller-trusted bank exporter, never generated from the received Geyser block.
//! A standard RPC context or native block count cannot supply this contract.
use crate::{rpc, stream::*};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    sync::Arc,
};
use xone_config::{Chain, PoolConfig, YellowstoneConfig};
use xone_domain::{err, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmittedCatalog {
    pub version: u32,
    pub revision: u64,
    pub chain_key: String,
    pub genesis_hash: String,
    pub pools: Vec<PoolConfig>,
}
pub fn read_bounded<T: serde::de::DeserializeOwned>(path: &Path, max: usize) -> Result<T> {
    let file = std::fs::File::open(path).map_err(|_| err("BANK_AUTHORITY_UNAVAILABLE"))?;
    if !file
        .metadata()
        .map_err(|_| err("BANK_AUTHORITY_UNAVAILABLE"))?
        .is_file()
    {
        return Err(err("BANK_AUTHORITY_FILE"));
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("BANK_AUTHORITY_READ"))?;
    if bytes.len() > max {
        return Err(err("BANK_AUTHORITY_LIMIT"));
    }
    serde_json::from_slice(&bytes).map_err(|_| err("BANK_AUTHORITY_SCHEMA"))
}
impl AdmittedCatalog {
    pub fn load(path: &Path, chain: &Chain, cfg: &YellowstoneConfig) -> Result<Self> {
        let result: Self = read_bounded(path, cfg.max_manifest_bytes)?;
        if result.version != 1
            || result.revision == 0
            || result.chain_key != chain.key
            || result.genesis_hash != chain.genesis_hash
            || result.pools.is_empty()
            || result.pools.len() > cfg.max_catalog_pools
        {
            return Err(err("BANK_CATALOG_IDENTITY"));
        }
        let mut ids = BTreeSet::new();
        for pool in &result.pools {
            if !ids.insert(&pool.id) {
                return Err(err("BANK_CATALOG_DUPLICATE"));
            }
        }
        Ok(result)
    }
    pub fn identity(&self) -> Result<String> {
        xone_domain::hash(self)
    }
    pub fn chain(&self, original: &Chain) -> Chain {
        let mut c = original.clone();
        c.pools = self.pools.clone();
        c
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestAnchor {
    pub slot: u64,
    pub bank_id: u64,
    pub blockhash: String,
}
impl ManifestAnchor {
    pub fn anchor(&self) -> BankAnchor {
        BankAnchor {
            bank: BankId {
                slot: self.slot,
                bank_id: self.bank_id,
            },
            blockhash: self.blockhash.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
impl ManifestAccount {
    pub fn account(&self) -> Result<AccountUpdate> {
        Ok(AccountUpdate {
            bank: BankId {
                slot: self.write_slot,
                bank_id: self.write_bank_id,
            },
            pubkey: key(&self.pubkey)?,
            owner: key(&self.owner)?,
            lamports: self.lamports,
            executable: self.executable,
            rent_epoch: self.rent_epoch,
            write_version: self.write_version,
            is_startup: false,
            data: STANDARD
                .decode(&self.data_base64)
                .map_err(|_| err("BANK_ACCOUNT_DATA"))?,
            txn_signature: self
                .txn_signature_base64
                .as_ref()
                .map(|s| {
                    STANDARD
                        .decode(s)
                        .map_err(|_| err("BANK_ACCOUNT_SIGNATURE"))?
                        .try_into()
                        .map_err(|_| err("BANK_ACCOUNT_SIGNATURE"))
                })
                .transpose()?,
        })
    }
}
pub fn key(value: &str) -> Result<Pubkey> {
    bs58::decode(value)
        .into_vec()
        .map_err(|_| err("BANK_PUBKEY"))?
        .try_into()
        .map_err(|_| err("BANK_PUBKEY"))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalVersion {
    pub pubkey: String,
    pub write_version: u64,
    pub account_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
pub struct VerifiedManifest {
    pub retained_bytes: usize,
    pub source_bytes: usize,
    pub native_writes: Vec<AccountUpdate>,
    pub manifest: BankManifest,
    pub parent: VersionedAccountSnapshot,
    pub parent_proof: AccountSnapshotManifest,
    pub completion: AccountCompletion,
    pub scope: BTreeSet<Pubkey>,
}
impl BankManifest {
    pub fn load(
        path: &Path,
        config: &YellowstoneConfig,
        chain: &Chain,
        catalog: &AdmittedCatalog,
        source: &SourceSession,
        bank: &BankId,
    ) -> Result<VerifiedManifest> {
        let m: Self = read_bounded(path, config.max_manifest_bytes)?;
        if !matches!(m.version, 1 | 2)
            || m.chain_key != chain.key
            || m.genesis_hash != chain.genesis_hash
            || m.source_id != source.source_id
            || m.session_id != source.session_id
            || m.catalog_identity != catalog.identity()?
            || m.bank.anchor().bank != *bank
            || m.parent.slot >= m.bank.slot
            || m.bank.blockhash.is_empty()
            || m.parent.blockhash.is_empty()
            || m.parent_accounts.len() > config.max_stream_updates
            || m.final_versions.len() > config.max_stream_updates
            || m.expected_account_updates > config.max_stream_updates as u64
            || m.skipped_slots.len() > config.max_stream_slots
        {
            return Err(err("BANK_MANIFEST_IDENTITY"));
        }
        let requested = rpc::required_scope(chain)?
            .iter()
            .map(|id| key(id))
            .collect::<Result<BTreeSet<_>>>()?;
        // V2 names every native write, so its count can be projected exactly.
        // V1 only certifies one aggregate count: retain its explicitly declared
        // complete scope rather than inventing a per-key count. Neither version
        // requires accounts for unrequested catalog pools.
        let scope = if m.version == 1 {
            let declared = m
                .final_versions
                .iter()
                .map(|r| key(&r.pubkey))
                .collect::<Result<BTreeSet<_>>>()?;
            if !requested.is_subset(&declared) {
                return Err(err("BANK_SCOPE_INCOMPLETE"));
            }
            declared
        } else {
            requested
        };
        if scope.is_empty() || scope.len() > config.max_stream_updates {
            return Err(err("BANK_SCOPE_LIMIT"));
        }
        let mut accounts = BTreeMap::new();
        let mut finals = BTreeMap::new();
        for row in &m.parent_accounts {
            if m.version == 2 && !scope.contains(&key(&row.pubkey)?) {
                continue;
            }
            let a = row.account()?;
            let id = a.pubkey;
            if accounts.insert(id, Arc::new(a)).is_some() {
                return Err(err("BANK_MANIFEST_DUPLICATE"));
            }
        }
        for row in &m.final_versions {
            if m.version == 2 && !scope.contains(&key(&row.pubkey)?) {
                continue;
            }
            let bytes = (0..row.account_sha256.len())
                .step_by(2)
                .map(|i| {
                    row.account_sha256
                        .get(i..i + 2)
                        .and_then(|v| u8::from_str_radix(v, 16).ok())
                        .ok_or_else(|| err("BANK_MANIFEST_DIGEST"))
                })
                .collect::<Result<Vec<_>>>()?;
            let value = ExpectedAccountVersion {
                write_version: row.write_version,
                account_sha256: bytes.try_into().map_err(|_| err("BANK_MANIFEST_DIGEST"))?,
            };
            if finals.insert(key(&row.pubkey)?, value).is_some() {
                return Err(err("BANK_MANIFEST_DUPLICATE"));
            }
        }
        if !accounts.keys().eq(scope.iter()) || !finals.keys().eq(scope.iter()) {
            return Err(err("BANK_SCOPE_INCOMPLETE"));
        }
        let parent = VersionedAccountSnapshot {
            source: source.clone(),
            anchor: m.parent.anchor(),
            accounts,
        };
        let parent_proof = AccountSnapshotManifest {
            source: source.clone(),
            anchor: m.parent.anchor(),
            final_versions: parent
                .accounts
                .iter()
                .map(|(k, a)| (*k, ExpectedAccountVersion::of(a)))
                .collect(),
        };
        let mut native_writes = Vec::new();
        if m.version == 2 {
            if m.native_writes.len() > config.max_stream_updates
                || m.native_writes.len() as u64 != m.expected_account_updates
            {
                return Err(err("BANK_NATIVE_WRITE_COUNT"));
            }
            let mut seen = BTreeSet::new();
            let mut latest = BTreeMap::new();
            for row in &m.native_writes {
                if !scope.contains(&key(&row.pubkey)?) {
                    continue;
                }
                let account = row.account()?;
                if account.bank != *bank
                    || !scope.contains(&account.pubkey)
                    || !seen.insert((account.pubkey, account.write_version))
                {
                    return Err(err("BANK_NATIVE_WRITE_IDENTITY"));
                }
                latest
                    .entry(account.pubkey)
                    .and_modify(|old: &mut ExpectedAccountVersion| {
                        if old.write_version < account.write_version {
                            *old = ExpectedAccountVersion::of(&account);
                        }
                    })
                    .or_insert_with(|| ExpectedAccountVersion::of(&account));
                native_writes.push(account);
            }
            if latest
                .iter()
                .any(|(key, version)| finals.get(key) != Some(version))
            {
                return Err(err("BANK_NATIVE_FINAL_VERSION"));
            }
        } else if !m.native_writes.is_empty() {
            return Err(err("BANK_NATIVE_SCHEMA"));
        }
        let completion = AccountCompletion::VersionedBankManifest {
            bank: bank.clone(),
            parent: ParentBank {
                slot: m.parent.slot,
                blockhash: m.parent.blockhash.clone(),
            },
            blockhash: m.bank.blockhash.clone(),
            expected_account_updates: if m.version == 2 {
                native_writes.len() as u64
            } else {
                m.expected_account_updates
            },
            final_versions: finals,
        };
        let retained_bytes = serde_json::to_vec(&m)
            .map_err(|_| err("BANK_AUTHORITY_SCHEMA"))?
            .len()
            .checked_add(
                parent
                    .accounts
                    .values()
                    .map(|a| a.data.capacity() + 512)
                    .sum::<usize>(),
            )
            .and_then(|n| n.checked_add(scope.len() * 512 + 2048))
            .ok_or_else(|| err("BANK_MANIFEST_WINDOW_LIMIT"))?;
        if retained_bytes > config.max_stream_bytes {
            return Err(err("BANK_MANIFEST_WINDOW_LIMIT"));
        }
        Ok(VerifiedManifest {
            retained_bytes,
            source_bytes: usize::try_from(
                std::fs::metadata(path)
                    .map_err(|_| err("BANK_AUTHORITY_READ"))?
                    .len(),
            )
            .map_err(|_| err("BANK_AUTHORITY_LIMIT"))?,
            native_writes,
            manifest: m,
            parent,
            parent_proof,
            completion,
            scope,
        })
    }
}
