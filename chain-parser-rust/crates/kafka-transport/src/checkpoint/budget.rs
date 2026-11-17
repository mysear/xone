//! Logical public-file admission under one owner's publication gate. This does
//! not delete old objects, reserve filesystem blocks or coordinate other writers.
use crate::Bootstrap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use xone_domain::{err, Result};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub max_bytes: u64,
    pub max_files: u64,
    pub max_entries: u64,
}
impl Budget {
    pub fn validate(&self) -> Result<()> {
        if self.max_bytes == 0
            || self.max_bytes > 1 << 40
            || self.max_files == 0
            || self.max_files > 100000
            || self.max_entries < self.max_files
            || self.max_entries > 1000000
        {
            return Err(err("CHECKPOINT_LOCAL_BUDGET_CONFIG"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub bytes: u64,
    pub files: u64,
    pub entries: u64,
}
struct Scan<'a, F> {
    limit: &'a Budget,
    guard: &'a F,
    usage: Usage,
    files: BTreeMap<String, u64>,
    directories: BTreeSet<String>,
}
impl<F: Fn() -> Result<()>> Scan<'_, F> {
    fn directory(&mut self, path: &Path, prefix: &str, depth: usize) -> Result<()> {
        if depth > 4 {
            return Err(err("CHECKPOINT_LOCAL_BUDGET_DEPTH"));
        }
        for entry in fs::read_dir(path).map_err(|_| err("CHECKPOINT_LOCAL_BUDGET_ENTRY"))? {
            (self.guard)()?;
            self.usage.entries += 1;
            if self.usage.entries > self.limit.max_entries {
                return Err(err("CHECKPOINT_LOCAL_BUDGET_SCAN"));
            }
            let entry = entry.map_err(|_| err("CHECKPOINT_LOCAL_BUDGET_ENTRY"))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| err("CHECKPOINT_LOCAL_BUDGET_ENTRY"))?;
            let relative = format!("{prefix}{name}");
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| err("CHECKPOINT_LOCAL_BUDGET_ENTRY"))?;
            if metadata.file_type().is_symlink() {
                return Err(err("CHECKPOINT_LOCAL_BUDGET_ENTRY"));
            }
            if metadata.is_dir() {
                self.directories.insert(relative.clone());
                self.directory(&entry.path(), &format!("{relative}/"), depth + 1)?;
            } else if metadata.is_file() {
                self.usage.bytes = self
                    .usage
                    .bytes
                    .checked_add(metadata.len())
                    .ok_or_else(|| err("CHECKPOINT_LOCAL_BUDGET_EXCEEDED"))?;
                self.usage.files += 1;
                if self.usage.bytes > self.limit.max_bytes
                    || self.usage.files > self.limit.max_files
                {
                    return Err(err("CHECKPOINT_LOCAL_BUDGET_EXCEEDED"));
                }
                self.files.insert(relative, metadata.len());
            } else {
                return Err(err("CHECKPOINT_LOCAL_BUDGET_ENTRY"));
            }
        }
        Ok(())
    }
}
pub fn admit(
    root: &Path,
    boot: &Bootstrap,
    maximum: usize,
    base: &str,
    limit: &Budget,
    guard: impl Fn() -> Result<()>,
) -> Result<Usage> {
    limit.validate()?;
    guard()?;
    let (manifest, raw, payload) = super::local::encode(boot, maximum)?;
    let mut planned = BTreeMap::from([
        (
            format!("{}/bootstrap.pb", manifest.sha256),
            payload.len() as u64,
        ),
        (
            format!("{}/manifest.json", manifest.sha256),
            raw.len() as u64,
        ),
    ]);
    if !base.is_empty() {
        let certificate = serde_json::to_vec(&crate::checkpoint::RemotePublication {
            manifest_url: format!("{base}/public-v2/{}/manifest.json", manifest.sha256),
            manifest_sha256: hex::encode(Sha256::digest(&raw)),
            manifest: manifest.clone(),
        })
        .map_err(|_| err("CHECKPOINT_CERTIFICATE_ENCODING"))?;
        if certificate.len() > 65536 {
            return Err(err("CHECKPOINT_CERTIFICATE_LIMIT"));
        }
        let id = hex::encode(Sha256::digest(base.as_bytes()));
        planned.insert(
            format!("{}/remote-{id}.json", manifest.sha256),
            certificate.len() as u64,
        );
        planned.insert(format!("candidate-{id}.json"), certificate.len() as u64);
    }
    let mut scan = Scan {
        limit,
        guard: &guard,
        usage: Usage::default(),
        files: BTreeMap::new(),
        directories: BTreeSet::new(),
    };
    let namespace = root.join("public-v2");
    match fs::symlink_metadata(&namespace) {
        Ok(m) => {
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err(err("CHECKPOINT_LOCAL_BUDGET_ENTRY"));
            }
            scan.directory(&namespace, "", 0)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(err("CHECKPOINT_LOCAL_BUDGET_ENTRY")),
    }
    for (path, size) in planned {
        let existing = scan.files.get(&path).copied();
        if existing.is_none() {
            scan.usage.files += 1;
            scan.usage.entries += 1;
            let mut parents = path.rsplit_once('/').map(|(parent, _)| parent.to_owned());
            while let Some(parent) = parents {
                if scan.directories.insert(parent.clone()) {
                    scan.usage.entries += 1;
                }
                parents = parent.rsplit_once('/').map(|(parent, _)| parent.to_owned());
            }
        }
        scan.usage.bytes = scan
            .usage
            .bytes
            .checked_add(size.saturating_sub(existing.unwrap_or(0)))
            .ok_or_else(|| err("CHECKPOINT_LOCAL_BUDGET_EXCEEDED"))?;
        if scan.usage.bytes > limit.max_bytes
            || scan.usage.files > limit.max_files
            || scan.usage.entries > limit.max_entries
        {
            return Err(err("CHECKPOINT_LOCAL_BUDGET_EXCEEDED"));
        }
    }
    guard()?;
    Ok(scan.usage)
}
