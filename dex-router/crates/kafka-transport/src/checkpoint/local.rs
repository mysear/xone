//! Immutable local public objects, independent of an owner's KV engine.
//! The configured root and ancestors are operator controlled. Newly created
//! object directories are private; untrusted processes must not mutate them.
use super::{decode, parse_manifest, Manifest, ManifestCursor, CODEC, MAX_BYTES};
use crate::Bootstrap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use xone_domain::{err, Result};
static NEXT: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub manifest_path: PathBuf,
    pub manifest_sha256: String,
    pub manifest: Manifest,
}
pub fn encode(boot: &Bootstrap, max_bytes: usize) -> Result<(Manifest, Vec<u8>, Vec<u8>)> {
    if !(1..=MAX_BYTES).contains(&max_bytes) {
        return Err(err("CHECKPOINT_SIZE_OR_PIN"));
    }
    boot.snapshot.validate(
        &boot.snapshot.reference.owner_service,
        boot.snapshot.reference.owner_assignment_version,
    )?;
    let payload = crate::encode_bootstrap(boot)?;
    if payload.len() > max_bytes {
        return Err(err("CHECKPOINT_SIZE_OR_PIN"));
    }
    let manifest = Manifest {
        schema_version: 2,
        codec: CODEC.into(),
        reference: boot.snapshot.reference.clone(),
        cursor: ManifestCursor {
            topic: boot.cursor.topic.clone(),
            partition: boot.cursor.partition,
            next_offset: boot.cursor.offset.to_string(),
        },
        payload: "bootstrap.pb".into(),
        sha256: hex::encode(Sha256::digest(&payload)),
        size: payload.len() as u64,
    };
    let raw = serde_json::to_vec(&manifest).map_err(|_| err("CHECKPOINT_MANIFEST_INVALID"))?;
    // Validate topic, canonical metadata, exact protobuf and every reference now.
    decode(
        &raw,
        &payload,
        &hex::encode(Sha256::digest(&raw)),
        max_bytes,
    )?;
    Ok((manifest, raw, payload))
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("CHECKPOINT_DIRECTORY_SYNC"))
}
fn private_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(err("CHECKPOINT_DIRECTORY")),
    }
    let meta = fs::symlink_metadata(path).map_err(|_| err("CHECKPOINT_DIRECTORY"))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(err("CHECKPOINT_DIRECTORY"));
    }
    if let Some(parent) = path.parent() {
        sync_dir(parent)?;
    }
    sync_dir(path)
}
fn root(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| err("CHECKPOINT_DIRECTORY"))?
            .join(path)
    };
    let path = absolute.as_path();
    let mut missing = Vec::new();
    let mut parent = path;
    while !parent.exists() {
        missing.push(parent.to_path_buf());
        parent = parent.parent().ok_or_else(|| err("CHECKPOINT_DIRECTORY"))?;
    }
    fs::create_dir_all(path).map_err(|_| err("CHECKPOINT_DIRECTORY"))?;
    for p in missing.iter().rev() {
        if let Some(parent) = p.parent() {
            sync_dir(parent)?;
        }
    }
    let resolved = fs::canonicalize(path).map_err(|_| err("CHECKPOINT_DIRECTORY"))?;
    sync_dir(&resolved)?;
    Ok(resolved)
}
fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let before = fs::symlink_metadata(path).map_err(|_| err("CHECKPOINT_FILE"))?;
    if !before.is_file() || before.file_type().is_symlink() || before.len() > limit as u64 {
        return Err(err("CHECKPOINT_FILE_LIMIT"));
    }
    let f = File::open(path).map_err(|_| err("CHECKPOINT_FILE"))?;
    let actual = f.metadata().map_err(|_| err("CHECKPOINT_FILE"))?;
    if !actual.is_file() || actual.len() > limit as u64 {
        return Err(err("CHECKPOINT_FILE_IDENTITY"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != actual.dev() || before.ino() != actual.ino() {
            return Err(err("CHECKPOINT_FILE_IDENTITY"));
        }
    }
    let mut raw = Vec::new();
    f.take(limit as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| err("CHECKPOINT_FILE"))?;
    if raw.len() > limit {
        return Err(err("CHECKPOINT_FILE_LIMIT"));
    }
    Ok(raw)
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn immutable(directory: &Path, name: &str, raw: &[u8]) -> Result<()> {
    let mut selected = None;
    for _ in 0..64 {
        let path = directory.join(format!(
            ".checkpoint-staging-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(f) => {
                selected = Some((Temporary(path), f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(err("CHECKPOINT_STAGING")),
        }
    }
    let (temp, mut f) = selected.ok_or_else(|| err("CHECKPOINT_STAGING"))?;
    f.write_all(raw)
        .and_then(|_| f.sync_all())
        .map_err(|_| err("CHECKPOINT_FILE_SYNC"))?;
    drop(f);
    let destination = directory.join(name);
    match fs::hard_link(&temp.0, &destination) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_regular(&destination, raw.len())? != raw {
                return Err(err("CHECKPOINT_IMMUTABLE_CONFLICT"));
            }
        }
        Err(_) => return Err(err("CHECKPOINT_FILE_PUBLICATION")),
    }
    sync_dir(directory)
}
pub fn write(
    root_path: &Path,
    boot: &Bootstrap,
    max_bytes: usize,
    guard: impl Fn() -> Result<()>,
) -> Result<Publication> {
    guard()?;
    let (manifest, raw, payload) = encode(boot, max_bytes)?;
    guard()?;
    let directory = root(root_path)?.join("public-v2");
    private_directory(&directory)?;
    let directory = directory.join(&manifest.sha256);
    private_directory(&directory)?;
    immutable(&directory, "bootstrap.pb", &payload)?;
    guard()?;
    immutable(&directory, "manifest.json", &raw)?;
    guard()?;
    Ok(Publication {
        manifest_path: directory.join("manifest.json"),
        manifest_sha256: hex::encode(Sha256::digest(&raw)),
        manifest,
    })
}
pub fn read(path: &Path, pin: &str, max_bytes: usize) -> Result<Bootstrap> {
    let (_, manifest, payload) = objects(path, pin, max_bytes)?;
    decode(&manifest, &payload, pin, max_bytes)
}
fn certificate_name(name: &str) -> Result<()> {
    let digest = name
        .strip_prefix("remote-")
        .and_then(|s| s.strip_suffix(".json"))
        .ok_or_else(|| err("CHECKPOINT_CERTIFICATE_NAME"))?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(err("CHECKPOINT_CERTIFICATE_NAME"));
    }
    Ok(())
}
pub fn objects(path: &Path, pin: &str, max_bytes: usize) -> Result<(Manifest, Vec<u8>, Vec<u8>)> {
    if path.file_name().and_then(|n| n.to_str()) != Some("manifest.json") {
        return Err(err("CHECKPOINT_MANIFEST_PATH"));
    }
    let manifest = read_regular(path, 65536)?;
    let m = parse_manifest(&manifest, pin, max_bytes)?;
    let payload = read_regular(&path.with_file_name("bootstrap.pb"), m.size as usize)?;
    decode(&manifest, &payload, pin, max_bytes)?;
    Ok((m, manifest, payload))
}
pub fn certify(
    p: &Publication,
    max_bytes: usize,
    name: &str,
    raw: &[u8],
    guard: impl Fn() -> Result<()>,
) -> Result<()> {
    certificate_name(name)?;
    if raw.len() > 65536 {
        return Err(err("CHECKPOINT_CERTIFICATE_LIMIT"));
    }
    guard()?;
    let (m, _, _) = objects(&p.manifest_path, &p.manifest_sha256, max_bytes)?;
    if serde_json::to_vec(&m).map_err(|_| err("CHECKPOINT_MANIFEST_INVALID"))?
        != serde_json::to_vec(&p.manifest).map_err(|_| err("CHECKPOINT_MANIFEST_INVALID"))?
    {
        return Err(err("CHECKPOINT_CERTIFICATE_SOURCE"));
    }
    guard()?;
    immutable(
        p.manifest_path
            .parent()
            .ok_or_else(|| err("CHECKPOINT_MANIFEST_PATH"))?,
        name,
        raw,
    )?;
    guard()
}
pub fn certificate(path: &Path, name: &str) -> Result<Vec<u8>> {
    certificate_name(name)?;
    if path.file_name().and_then(|n| n.to_str()) != Some("manifest.json") {
        return Err(err("CHECKPOINT_MANIFEST_PATH"));
    }
    read_regular(&path.with_file_name(name), 65536)
}
