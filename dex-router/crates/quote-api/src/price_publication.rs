//! Local durable high-water marks for authorized price books. This component
//! assumes a persistent filesystem; it is not a consensus or anti-disk-rollback
//! authority. Runtime configuration and publication admission remain separate.
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use xone_domain::{
    err, hash,
    native_valuation::{block_hash, canonical_hex, NativeValuationConfig},
    Result,
};
use xone_execution_preparation::native_valuation::{PriceAnchor, PriceBook};

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_LINE: usize = 16 * 1024;
const DOMAIN: &str = "xone-price-publication-journal/1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HighWater {
    schema_version: u32,
    policy_hash: String,
    revision: u64,
    book_hash: String,
    anchor: PriceAnchor,
    tokens: Vec<String>,
}

/// Callers cannot construct an authorized publication from worker/HTTP output.
#[derive(Clone, Debug)]
pub struct AuthorizedPublication(HighWater);
impl AuthorizedPublication {
    pub fn authorize(
        policy: &NativeValuationConfig,
        book: &PriceBook,
        anchor: &PriceAnchor,
        at_ms: u64,
    ) -> Result<Self> {
        policy.validate()?;
        let bytes = serde_json::to_vec(book).map_err(|_| err("PRICE_PUBLICATION_BOOK"))?;
        if bytes.len() > 1024 * 1024 || book.prices.iter().any(|p| &p.observation.anchor != anchor)
        {
            return Err(err("PRICE_PUBLICATION_BOOK"));
        }
        let prices = book.authorize(&policy.policies)?;
        let revision = prices[0].publication_revision();
        if revision == 0 {
            return Err(err("PRICE_PUBLICATION_POSITIVE_REVISION_REQUIRED"));
        }
        for price in &prices {
            price.validate_at(anchor, at_ms)?;
        }
        let tokens = prices
            .iter()
            .map(|p| p.token().to_owned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let water = HighWater {
            schema_version: 1,
            policy_hash: hash(&policy.policies)?,
            revision,
            book_hash: hash(book)?,
            anchor: anchor.clone(),
            tokens,
        };
        Binding::new(policy)?.validate(&water)?;
        Ok(Self(water))
    }
    pub fn high_water(&self) -> &HighWater {
        &self.0
    }
}
impl HighWater {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn book_hash(&self) -> &str {
        &self.book_hash
    }
}

#[derive(Debug)]
struct Binding {
    policy_hash: String,
    allowed: BTreeSet<(String, String, String)>,
}
impl Binding {
    fn new(policy: &NativeValuationConfig) -> Result<Self> {
        policy.validate()?;
        Ok(Self {
            policy_hash: hash(&policy.policies)?,
            allowed: policy
                .policies
                .iter()
                .map(|p| (p.chain_key.clone(), p.genesis_hash.clone(), p.token.clone()))
                .collect(),
        })
    }
    fn validate(&self, water: &HighWater) -> Result<()> {
        if water.schema_version != 1
            || water.policy_hash != self.policy_hash
            || water.revision == 0
            || !canonical_hex(&water.book_hash, 32)
            || !block_hash(&water.anchor.block_hash)
            || !block_hash(&water.anchor.parent_hash)
            || water.anchor.timestamp_ms == 0
            || water.tokens.is_empty()
            || water.tokens.len() > 128
            || water.tokens.windows(2).any(|p| p[0] >= p[1])
            || water.tokens.iter().any(|token| {
                !self.allowed.contains(&(
                    water.anchor.chain_key.clone(),
                    water.anchor.genesis_hash.clone(),
                    token.clone(),
                ))
            })
        {
            return Err(err("PRICE_PUBLICATION_JOURNAL_BINDING"));
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    schema_version: u32,
    sequence: u64,
    previous_hash: String,
    high_water: HighWater,
    entry_hash: String,
}
fn entry_hash(sequence: u64, previous: &str, water: &HighWater) -> Result<String> {
    hash(
        &json!({"domain":DOMAIN,"schema_version":1,"sequence":sequence,
        "previous_hash":previous,"high_water":water}),
    )
}
fn transition(old: &HighWater, next: &HighWater) -> Result<bool> {
    if old == next {
        return Ok(false);
    }
    if old.policy_hash != next.policy_hash
        || old.tokens != next.tokens
        || old.anchor.chain_key != next.anchor.chain_key
        || old.anchor.genesis_hash != next.anchor.genesis_hash
    {
        return Err(err("PRICE_PUBLICATION_JOURNAL_BINDING"));
    }
    if next.revision <= old.revision {
        return Err(err("PRICE_PUBLICATION_JOURNAL_ROLLBACK_OR_CONFLICT"));
    }
    Ok(true)
}

#[derive(Debug, Serialize)]
pub struct DurableReceipt {
    pub changed: bool,
    pub sequence: u64,
    pub entry_hash: String,
    pub high_water: HighWater,
    pub fsync_confirmed: bool,
}
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    file: File,
    binding: Binding,
    head: Option<HighWater>,
    sequence: u64,
    last_hash: String,
    bytes: u64,
    healthy: bool,
    #[cfg(test)]
    fault: Option<Fault>,
}
impl Journal {
    /// Explicit bootstrap only; existing or missing journals are never replaced.
    pub fn initialize(
        path: &Path,
        policy: &NativeValuationConfig,
        publication: &AuthorizedPublication,
    ) -> Result<Self> {
        let binding = Binding::new(policy)?;
        binding.validate(&publication.0)?;
        let file = open_file(path, true)?;
        let mut journal = Self {
            path: path.to_owned(),
            file,
            binding,
            head: None,
            sequence: 0,
            last_hash: "0".repeat(64),
            bytes: 0,
            healthy: true,
            #[cfg(test)]
            fault: None,
        };
        journal.append(&publication.0)?;
        // The parent already exists: bootstrap does not create a directory tree.
        if sync_parent(path).is_err() || check_path(path, &journal.file, journal.bytes).is_err() {
            journal.healthy = false;
            return Err(err("PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"));
        }
        Ok(journal)
    }
    pub fn open(path: &Path, policy: &NativeValuationConfig) -> Result<Self> {
        let binding = Binding::new(policy)?;
        let mut file = open_file(path, false)?;
        let mut raw = Vec::new();
        (&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut raw)
            .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_UNAVAILABLE"))?;
        if raw.len() as u64 > MAX_BYTES {
            return Err(err("PRICE_PUBLICATION_JOURNAL_LIMIT"));
        }
        let mut journal = Self {
            path: path.to_owned(),
            file,
            binding,
            head: None,
            sequence: 0,
            last_hash: "0".repeat(64),
            bytes: 0,
            healthy: true,
            #[cfg(test)]
            fault: None,
        };
        let mut offset = 0;
        while offset < raw.len() {
            let Some(end) = raw[offset..].iter().position(|b| *b == b'\n') else {
                if journal.head.is_none() || raw.len() - offset > MAX_LINE {
                    return Err(err("PRICE_PUBLICATION_JOURNAL_CORRUPT"));
                }
                check_path(path, &journal.file, raw.len() as u64)?;
                journal
                    .file
                    .set_len(offset as u64)
                    .and_then(|_| journal.file.sync_all())
                    .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"))?;
                break;
            };
            if end > MAX_LINE {
                return Err(err("PRICE_PUBLICATION_JOURNAL_LIMIT"));
            }
            let entry: Entry = serde_json::from_slice(&raw[offset..offset + end])
                .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_CORRUPT"))?;
            journal.binding.validate(&entry.high_water)?;
            let expected = journal
                .sequence
                .checked_add(1)
                .ok_or_else(|| err("PRICE_PUBLICATION_JOURNAL_LIMIT"))?;
            if entry.schema_version != 1
                || entry.sequence != expected
                || entry.previous_hash != journal.last_hash
                || entry.entry_hash
                    != entry_hash(entry.sequence, &entry.previous_hash, &entry.high_water)?
            {
                return Err(err("PRICE_PUBLICATION_JOURNAL_CORRUPT"));
            }
            if let Some(old) = &journal.head {
                if !transition(old, &entry.high_water)? {
                    return Err(err("PRICE_PUBLICATION_JOURNAL_CORRUPT"));
                }
            }
            journal.sequence = entry.sequence;
            journal.last_hash = entry.entry_hash;
            journal.head = Some(entry.high_water);
            offset += end + 1;
            journal.bytes = offset as u64;
        }
        if journal.head.is_none() {
            return Err(err("PRICE_PUBLICATION_JOURNAL_UNINITIALIZED"));
        }
        check_path(&journal.path, &journal.file, journal.bytes)?;
        // A complete final record may have been written before a failed fsync.
        // Confirm what recovery actually observed before reporting durability.
        journal
            .file
            .sync_all()
            .and_then(|_| sync_parent(path))
            .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"))?;
        check_path(&journal.path, &journal.file, journal.bytes)?;
        journal
            .file
            .seek(SeekFrom::End(0))
            .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_UNAVAILABLE"))?;
        Ok(journal)
    }
    pub fn high_water(&self) -> &HighWater {
        self.head.as_ref().expect("initialized journal")
    }
    pub fn check(&self, publication: &AuthorizedPublication) -> Result<DurableReceipt> {
        if !self.healthy {
            return Err(err("PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"));
        }
        check_path(&self.path, &self.file, self.bytes)?;
        self.binding.validate(&publication.0)?;
        if transition(self.high_water(), &publication.0)? {
            return Err(err("PRICE_PUBLICATION_JOURNAL_ADVANCE_REQUIRED"));
        }
        Ok(self.receipt(false))
    }
    pub fn advance(&mut self, publication: &AuthorizedPublication) -> Result<DurableReceipt> {
        if !self.healthy {
            return Err(err("PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"));
        }
        self.binding.validate(&publication.0)?;
        if check_path(&self.path, &self.file, self.bytes).is_err() {
            self.healthy = false;
            return Err(err("PRICE_PUBLICATION_JOURNAL_REPLACED"));
        }
        if !transition(self.high_water(), &publication.0)? {
            return Ok(self.receipt(false));
        }
        self.append(&publication.0)?;
        Ok(self.receipt(true))
    }
    fn receipt(&self, changed: bool) -> DurableReceipt {
        DurableReceipt {
            changed,
            sequence: self.sequence,
            entry_hash: self.last_hash.clone(),
            high_water: self.high_water().clone(),
            fsync_confirmed: true,
        }
    }
    fn append(&mut self, water: &HighWater) -> Result<()> {
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| err("PRICE_PUBLICATION_JOURNAL_LIMIT"))?;
        let digest = entry_hash(sequence, &self.last_hash, water)?;
        let entry = Entry {
            schema_version: 1,
            sequence,
            previous_hash: self.last_hash.clone(),
            high_water: water.clone(),
            entry_hash: digest.clone(),
        };
        let mut bytes =
            serde_json::to_vec(&entry).map_err(|_| err("PRICE_PUBLICATION_JOURNAL_CORRUPT"))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_LINE || self.bytes + bytes.len() as u64 > MAX_BYTES {
            return Err(err("PRICE_PUBLICATION_JOURNAL_LIMIT"));
        }
        if self.write_record(&bytes).is_err()
            || check_path(&self.path, &self.file, self.bytes + bytes.len() as u64).is_err()
        {
            self.healthy = false;
            return Err(err("PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"));
        }
        self.sequence = sequence;
        self.last_hash = digest;
        self.head = Some(water.clone());
        self.bytes += bytes.len() as u64;
        Ok(())
    }
    fn write_record(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(fault) = self.fault.take() {
            match fault {
                Fault::BeforeWrite => (),
                Fault::PartialWrite => self.file.write_all(&bytes[..bytes.len() / 2])?,
                Fault::BeforeSync => self.file.write_all(bytes)?,
            }
            return Err(std::io::Error::other("controlled publication writer fault"));
        }
        self.file
            .write_all(bytes)
            .and_then(|_| self.file.sync_all())
    }
}

fn sync_parent(path: &Path) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent).and_then(|directory| directory.sync_all())
}

fn open_file(path: &Path, initialize: bool) -> Result<File> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err(err("PRICE_PUBLICATION_JOURNAL_PATH"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(initialize);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_UNAVAILABLE"))?;
    file.try_lock()
        .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_IN_USE"))?;
    let metadata = file
        .metadata()
        .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_UNAVAILABLE"))?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(err("PRICE_PUBLICATION_JOURNAL_LIMIT"));
    }
    check_path(path, &file, metadata.len())?;
    Ok(file)
}
fn check_path(path: &Path, file: &File, bytes: u64) -> Result<()> {
    let actual = file
        .metadata()
        .map_err(|_| err("PRICE_PUBLICATION_JOURNAL_UNAVAILABLE"))?;
    let named =
        std::fs::symlink_metadata(path).map_err(|_| err("PRICE_PUBLICATION_JOURNAL_REPLACED"))?;
    if !actual.is_file()
        || !named.is_file()
        || named.file_type().is_symlink()
        || actual.len() != bytes
    {
        return Err(err("PRICE_PUBLICATION_JOURNAL_REPLACED"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if actual.dev() != named.dev() || actual.ino() != named.ino() {
            return Err(err("PRICE_PUBLICATION_JOURNAL_REPLACED"));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        Err(err("PRICE_PUBLICATION_JOURNAL_PLATFORM"))
    }
}

#[cfg(test)]
#[derive(Debug)]
enum Fault {
    BeforeWrite,
    PartialWrite,
    BeforeSync,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inputs(name: &str) -> (NativeValuationConfig, PriceBook, PriceAnchor) {
        let v: serde_json::Value = serde_json::from_str(include_str!(
            "../../../test/contracts/native-valuation-publications.json"
        ))
        .unwrap();
        (
            serde_json::from_value(
                json!({"observations_file":"prices.json","policies":v["policies"]}),
            )
            .unwrap(),
            serde_json::from_value(v["books"][name].clone()).unwrap(),
            serde_json::from_value(v["anchor"].clone()).unwrap(),
        )
    }
    fn publication(name: &str) -> Result<AuthorizedPublication> {
        let (policy, book, anchor) = inputs(name);
        AuthorizedPublication::authorize(&policy, &book, &anchor, anchor.timestamp_ms)
    }
    #[test]
    fn journal_publications_require_fresh_signed_positive_same_anchor_books() {
        for name in ["0", "stale", "future", "mixed", "tampered"] {
            assert!(publication(name).is_err(), "{name}");
        }
        let (policy, book, mut anchor) = inputs("1");
        anchor.block_hash = format!("0x{}", "77".repeat(32));
        assert!(
            AuthorizedPublication::authorize(&policy, &book, &anchor, anchor.timestamp_ms).is_err()
        );
        assert_eq!(publication("1").unwrap().high_water().revision(), 1);
    }
    #[test]
    fn durable_high_water_is_exclusive_monotonic_and_bound_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prices.journal");
        let (policy, _, _) = inputs("1");
        let one = publication("1").unwrap();
        let two = publication("2").unwrap();
        let mut j = Journal::initialize(&path, &policy, &one).unwrap();
        assert!(!j.check(&one).unwrap().changed);
        assert_eq!(
            Journal::open(&path, &policy).unwrap_err().0,
            "PRICE_PUBLICATION_JOURNAL_IN_USE"
        );
        let receipt = j.advance(&two).unwrap();
        assert!(receipt.changed && receipt.fsync_confirmed);
        assert_eq!(receipt.sequence, 2);
        for name in ["1", "conflict", "missing"] {
            assert!(j.advance(&publication(name).unwrap()).is_err(), "{name}");
        }
        assert_eq!(
            j.check(&publication("3").unwrap()).unwrap_err().0,
            "PRICE_PUBLICATION_JOURNAL_ADVANCE_REQUIRED"
        );
        assert!(!j.advance(&two).unwrap().changed);
        drop(j);
        let mut j = Journal::open(&path, &policy).unwrap();
        assert_eq!(j.high_water(), two.high_water());
        assert!(j.advance(&one).is_err());
        assert_eq!(j.advance(&publication("3").unwrap()).unwrap().sequence, 3);
        drop(j);
        let mut wrong_policy = policy.clone();
        wrong_policy.policies[0].source_id.push_str("-rotated");
        assert!(Journal::open(&path, &wrong_policy).is_err());
        assert_eq!(
            Journal::open(&path, &policy)
                .unwrap()
                .high_water()
                .revision(),
            3
        );
    }
    #[test]
    fn torn_tail_recovers_but_complete_corruption_and_missing_bootstrap_fail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prices.journal");
        let (policy, _, _) = inputs("1");
        assert!(Journal::open(&path, &policy).is_err());
        assert!(!path.exists());
        let mut j = Journal::initialize(&path, &policy, &publication("1").unwrap()).unwrap();
        j.advance(&publication("2").unwrap()).unwrap();
        drop(j);
        let good = std::fs::read(&path).unwrap();
        for tail in [b"{partial".as_slice(), b" "] {
            let mut raw = good.clone();
            raw.extend_from_slice(tail);
            std::fs::write(&path, &raw).unwrap();
            let j = Journal::open(&path, &policy).unwrap();
            assert_eq!(j.high_water().revision(), 2);
            drop(j);
            assert_eq!(std::fs::read(&path).unwrap(), good);
        }
        for mut raw in [
            Vec::new(),
            b"{bootstrap".to_vec(),
            b"{}\n".to_vec(),
            good.clone(),
        ] {
            if raw == good {
                raw[20] ^= 1;
            }
            std::fs::write(&path, &raw).unwrap();
            assert!(Journal::open(&path, &policy).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), raw);
        }
        std::fs::write(&path, &good).unwrap();
        let mut raw = good.clone();
        raw.extend_from_slice(b"{}\n");
        std::fs::write(&path, &raw).unwrap();
        assert!(Journal::open(&path, &policy).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        assert!(Journal::initialize(&path, &policy, &publication("1").unwrap()).is_err());
    }
    #[test]
    fn uncertain_writes_close_the_writer_and_recovery_observes_actual_file_state() {
        for (fault, recovered) in [
            (Fault::BeforeWrite, 1),
            (Fault::PartialWrite, 1),
            (Fault::BeforeSync, 2),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("prices.journal");
            let (policy, _, _) = inputs("1");
            let mut j = Journal::initialize(&path, &policy, &publication("1").unwrap()).unwrap();
            j.fault = Some(fault);
            assert_eq!(
                j.advance(&publication("2").unwrap()).unwrap_err().0,
                "PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"
            );
            assert_eq!(j.high_water().revision(), 1);
            assert!(j.check(&publication("1").unwrap()).is_err());
            assert_eq!(
                j.advance(&publication("3").unwrap()).unwrap_err().0,
                "PRICE_PUBLICATION_JOURNAL_COMMIT_UNCERTAIN"
            );
            drop(j);
            let mut j = Journal::open(&path, &policy).unwrap();
            assert_eq!(j.high_water().revision(), recovered);
            assert_eq!(
                j.advance(&publication("3").unwrap())
                    .unwrap()
                    .high_water
                    .revision(),
                3
            );
        }
    }
    #[test]
    fn journal_replacement_nonregular_handles_and_capacity_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prices.journal");
        let (policy, _, _) = inputs("1");
        assert!(Journal::open(dir.path(), &policy).is_err());
        let mut j = Journal::initialize(&path, &policy, &publication("1").unwrap()).unwrap();
        std::fs::rename(&path, dir.path().join("old.journal")).unwrap();
        let mut replacement =
            Journal::initialize(&path, &policy, &publication("1").unwrap()).unwrap();
        assert_eq!(
            j.advance(&publication("2").unwrap()).unwrap_err().0,
            "PRICE_PUBLICATION_JOURNAL_REPLACED"
        );
        replacement.advance(&publication("2").unwrap()).unwrap();
        drop(replacement);
        drop(j);
        #[cfg(unix)]
        {
            let link = dir.path().join("symlink");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(Journal::open(&link, &policy).is_err());
            assert!(Journal::initialize(&link, &policy, &publication("1").unwrap()).is_err());
        }
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert_eq!(
            Journal::open(&path, &policy).unwrap_err().0,
            "PRICE_PUBLICATION_JOURNAL_LIMIT"
        );
    }
    #[test]
    fn committed_high_water_survives_process_exit_without_drop() {
        let (policy, _, _) = inputs("1");
        const CHILD: &str = "XONE_PRICE_PUBLICATION_UNIT_CHILD";
        if let Some(path) = std::env::var_os(CHILD) {
            let mut j = Journal::open(Path::new(&path), &policy).unwrap();
            j.advance(&publication("2").unwrap()).unwrap();
            std::process::exit(17);
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prices.journal");
        drop(Journal::initialize(&path, &policy, &publication("1").unwrap()).unwrap());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "price_publication::tests::committed_high_water_survives_process_exit_without_drop",
                "--nocapture",
            ])
            .env(CHILD, &path)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(17), "{:?}", output);
        let mut j = Journal::open(&path, &policy).unwrap();
        assert_eq!(j.high_water().revision(), 2);
        assert!(j.advance(&publication("1").unwrap()).is_err());
    }
}
