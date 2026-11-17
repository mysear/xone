//! Read-only evidence-bound retention selection. A plan never authorizes deletion.
use super::{decode, local, parse_manifest, ManifestCursor, RemotePublication};
use crate::Bootstrap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use xone_domain::{err, Result, SnapshotRef};
fn sha(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && s.bytes().any(|c| c != b'0')
}
fn empty(s: &str) -> bool {
    s.is_empty()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub keep_latest: u64,
    pub min_age_ms: u64,
    pub max_remove: u64,
    pub max_objects: u64,
    pub max_entries: u64,
    pub max_read_bytes: u64,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.keep_latest < 2
            || self.keep_latest > 1000
            || self.min_age_ms == 0
            || self.min_age_ms > 31536000000
            || self.max_remove == 0
            || self.max_remove > 64
            || self.max_objects < self.keep_latest
            || self.max_objects > 4096
            || self.max_entries < self.max_objects
            || self.max_entries > 100000
            || self.max_read_bytes == 0
            || self.max_read_bytes > 256 << 20
        {
            return Err(err("CHECKPOINT_RETENTION_CONFIG"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub content_coverage: String,
    pub object_sha256: String,
    #[serde(default, skip_serializing_if = "empty")]
    pub manifest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<SnapshotRef>,
    pub bytes: u64,
    pub files: u64,
    pub reason: String,
    pub selected: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub namespace_digest_basis: String,
    pub schema_version: u32,
    pub plan_id: String,
    pub namespace_sha256: String,
    pub store_sha256: String,
    pub reference: SnapshotRef,
    pub cursor: ManifestCursor,
    pub policy: Policy,
    pub observed_at_ms: i64,
    pub scanned_entries: u64,
    pub read_bytes: u64,
    pub selected_bytes: u64,
    pub selected_objects: u64,
    pub objects: Vec<Object>,
    pub applied: bool,
}
#[derive(Serialize)]
struct Entry {
    path: String,
    kind: String,
    size: i64,
    modified_ns: i64,
    #[serde(skip_serializing_if = "empty")]
    sha256: String,
}
struct ObservedObject {
    name: String,
    files: BTreeMap<String, fs::Metadata>,
    modified_ns: i64,
    row: Object,
}
struct Reader<'a, F> {
    policy: &'a Policy,
    guard: &'a F,
    entries: u64,
    read_bytes: u64,
    inventory: BTreeMap<String, Entry>,
}
fn nanos(time: SystemTime) -> Result<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).map_err(|_| err("CHECKPOINT_RETENTION_CLOCK")),
        Err(e) => i64::try_from(e.duration().as_nanos())
            .map(|n| -n)
            .map_err(|_| err("CHECKPOINT_RETENTION_CLOCK")),
    }
}
impl<F: Fn() -> Result<()>> Reader<'_, F> {
    fn list(&mut self, path: &Path, prefix: &str) -> Result<BTreeMap<String, fs::Metadata>> {
        let mut result = BTreeMap::new();
        for entry in fs::read_dir(path).map_err(|_| err("CHECKPOINT_RETENTION_LAYOUT"))? {
            (self.guard)()?;
            self.entries += 1;
            if self.entries > self.policy.max_entries {
                return Err(err("CHECKPOINT_RETENTION_SCAN_BUDGET"));
            }
            let entry = entry.map_err(|_| err("CHECKPOINT_RETENTION_LAYOUT"))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| err("CHECKPOINT_RETENTION_LAYOUT"))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| err("CHECKPOINT_RETENTION_LAYOUT"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() && !metadata.is_dir() {
                return Err(err("CHECKPOINT_RETENTION_LAYOUT"));
            }
            let path = format!("{prefix}{name}");
            let modified_ns = nanos(
                metadata
                    .modified()
                    .map_err(|_| err("CHECKPOINT_RETENTION_LAYOUT"))?,
            )?;
            self.inventory.insert(
                path.clone(),
                Entry {
                    path,
                    kind: if metadata.is_dir() {
                        "directory".into()
                    } else {
                        "file".into()
                    },
                    size: i64::try_from(metadata.len())
                        .map_err(|_| err("CHECKPOINT_RETENTION_SIZE_OVERFLOW"))?,
                    modified_ns,
                    sha256: String::new(),
                },
            );
            result.insert(name, metadata);
        }
        Ok(result)
    }
    fn read(&mut self, path: &Path, metadata: &fs::Metadata, maximum: usize) -> Result<Vec<u8>> {
        (self.guard)()?;
        if metadata.len() > maximum as u64
            || metadata.len() > self.policy.max_read_bytes - self.read_bytes
        {
            return Err(err("CHECKPOINT_RETENTION_READ_BUDGET"));
        }
        let raw = local::read_regular(path, maximum)?;
        if raw.len() as u64 > self.policy.max_read_bytes - self.read_bytes {
            return Err(err("CHECKPOINT_RETENTION_READ_BUDGET"));
        }
        self.read_bytes += raw.len() as u64;
        Ok(raw)
    }
}
pub fn plan(
    root: &Path,
    boot: &Bootstrap,
    maximum: usize,
    base: &str,
    policy: &Policy,
    observed_at_ms: i64,
    guard: impl Fn() -> Result<()>,
) -> Result<Plan> {
    policy.validate()?;
    guard()?;
    let (_, _, payload) = local::encode(boot, maximum)?;
    let current = sha(&payload);
    let mut protected = BTreeMap::from([(current, "CURRENT_STATE".to_owned())]);
    let namespace = root.join("public-v2");
    let metadata =
        fs::symlink_metadata(&namespace).map_err(|_| err("CHECKPOINT_RETENTION_LAYOUT"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(err("CHECKPOINT_RETENTION_LAYOUT"));
    }
    let mut reader = Reader {
        policy,
        guard: &guard,
        entries: 0,
        read_bytes: 0,
        inventory: BTreeMap::new(),
    };
    let top = reader.list(&namespace, "")?;
    let mut objects = BTreeMap::<String, ObservedObject>::new();
    for (name, metadata) in top {
        if metadata.is_dir() {
            if !digest(&name) {
                return Err(err("CHECKPOINT_RETENTION_LAYOUT"));
            }
            if objects.len() as u64 >= policy.max_objects {
                return Err(err("CHECKPOINT_RETENTION_OBJECT_BUDGET"));
            }
            let files = reader.list(&namespace.join(&name), &format!("{name}/"))?;
            let mut bytes = 0u64;
            for meta in files.values() {
                if !meta.is_file() {
                    return Err(err("CHECKPOINT_RETENTION_LAYOUT"));
                }
                bytes = bytes
                    .checked_add(meta.len())
                    .ok_or_else(|| err("CHECKPOINT_RETENTION_SIZE_OVERFLOW"))?;
            }
            let row = Object {
                content_coverage: "METADATA_ONLY".into(),
                object_sha256: name.clone(),
                manifest_sha256: String::new(),
                reference: None,
                bytes,
                files: files.len() as u64,
                reason: "INCOMPLETE".into(),
                selected: false,
            };
            objects.insert(
                name.clone(),
                ObservedObject {
                    name,
                    files,
                    modified_ns: 0,
                    row,
                },
            );
        } else if name.starts_with("candidate-") {
            let id = name
                .strip_prefix("candidate-")
                .and_then(|s| s.strip_suffix(".json"))
                .filter(|s| digest(s))
                .ok_or_else(|| err("CHECKPOINT_RETENTION_CANDIDATE"))?;
            let _ = id;
            let raw = reader.read(&namespace.join(&name), &metadata, 65536)?;
            let p: RemotePublication =
                serde_json::from_slice(&raw).map_err(|_| err("CHECKPOINT_RETENTION_CANDIDATE"))?;
            if serde_json::to_vec(&p).map_err(|_| err("CHECKPOINT_RETENTION_CANDIDATE"))? != raw {
                return Err(err("CHECKPOINT_RETENTION_CANDIDATE"));
            }
            let manifest = serde_json::to_vec(&p.manifest)
                .map_err(|_| err("CHECKPOINT_RETENTION_CANDIDATE"))?;
            parse_manifest(&manifest, &p.manifest_sha256, maximum)?;
            protected
                .entry(p.manifest.sha256)
                .or_insert_with(|| "CANDIDATE_REFERENCE".into());
            reader
                .inventory
                .get_mut(&name)
                .ok_or_else(|| err("CHECKPOINT_RETENTION_LAYOUT"))?
                .sha256 = sha(&raw);
        }
    }
    let certificate_name = format!("remote-{}.json", sha(base.as_bytes()));
    let mut qualified = Vec::new();
    for object in objects.values_mut() {
        let directory = namespace.join(&object.name);
        let known = object.files.keys().all(|name| {
            name == "manifest.json"
                || name == "bootstrap.pb"
                || name
                    .strip_prefix("remote-")
                    .and_then(|s| s.strip_suffix(".json"))
                    .is_some_and(digest)
        });
        if !known {
            object.row.reason = "UNKNOWN_FILES".into();
            continue;
        }
        if ["manifest.json", "bootstrap.pb", &certificate_name]
            .iter()
            .any(|name| !object.files.contains_key(*name))
        {
            continue;
        }
        let manifest = reader.read(
            &directory.join("manifest.json"),
            &object.files["manifest.json"],
            65536,
        )?;
        let pin = sha(&manifest);
        let m = parse_manifest(&manifest, &pin, maximum)?;
        if m.sha256 != object.name {
            return Err(err("CHECKPOINT_RETENTION_MANIFEST"));
        }
        if m.reference.block.chain_key != boot.snapshot.reference.block.chain_key
            || m.reference.owner_service != boot.snapshot.reference.owner_service
        {
            object.row.reason = "OTHER_OWNER".into();
            continue;
        }
        let mut bytes = BTreeMap::new();
        for (name, metadata) in &object.files {
            let max = if name == "bootstrap.pb" {
                m.size as usize
            } else {
                65536
            };
            let raw = reader.read(&directory.join(name), metadata, max)?;
            bytes.insert(name.clone(), raw);
        }
        decode(&manifest, &bytes["bootstrap.pb"], &pin, maximum)?;
        let expected = RemotePublication {
            manifest_url: format!("{base}/public-v2/{}/manifest.json", m.sha256),
            manifest_sha256: pin.clone(),
            manifest: m.clone(),
        };
        if serde_json::to_vec(&expected).map_err(|_| err("CHECKPOINT_RETENTION_CERTIFICATE"))?
            != bytes[&certificate_name]
        {
            return Err(err("CHECKPOINT_RETENTION_CERTIFICATE"));
        }
        object.modified_ns = nanos(
            object.files["manifest.json"]
                .modified()
                .map_err(|_| err("CHECKPOINT_RETENTION_LAYOUT"))?,
        )?;
        object.row.content_coverage = "FULL_DECODE_AND_STORE_CERTIFICATE".into();
        object.row.manifest_sha256 = pin;
        object.row.reference = Some(m.reference);
        object.row.reason = "AGE_PROTECTED".into();
        qualified.push(object.name.clone());
        for (name, raw) in bytes {
            reader
                .inventory
                .get_mut(&format!("{}/{name}", object.name))
                .ok_or_else(|| err("CHECKPOINT_RETENTION_LAYOUT"))?
                .sha256 = sha(&raw);
        }
    }
    qualified.sort_by(|a, b| {
        objects[b]
            .row
            .reference
            .as_ref()
            .map(|r| r.journal_seq)
            .cmp(&objects[a].row.reference.as_ref().map(|r| r.journal_seq))
            .then_with(|| a.cmp(b))
    });
    let mut rank = 0u64;
    let cutoff = i128::from(observed_at_ms) - i128::from(policy.min_age_ms);
    for name in &qualified {
        let object = objects
            .get_mut(name)
            .ok_or_else(|| err("CHECKPOINT_RETENTION_LAYOUT"))?;
        let reference = object
            .row
            .reference
            .as_ref()
            .ok_or_else(|| err("CHECKPOINT_RETENTION_LAYOUT"))?;
        let head = &boot.snapshot.reference;
        if reference.writer_epoch > head.writer_epoch
            || reference.canonical_epoch > head.canonical_epoch
            || reference.owner_assignment_version > head.owner_assignment_version
            || reference.journal_seq > head.journal_seq
        {
            object.row.reason = "FUTURE_IDENTITY".into();
            continue;
        }
        let latest = rank < policy.keep_latest;
        rank += 1;
        if let Some(reason) = protected.get(name) {
            object.row.reason = reason.clone();
            continue;
        }
        if latest {
            object.row.reason = "LATEST_BACKUP".into();
            continue;
        }
        if cutoff < 0
            || i128::from(reference.block.timestamp_ms) > cutoff
            || i128::from(object.modified_ns) > cutoff * 1000000
        {
            continue;
        }
        object.row.reason = "RETENTION_ELIGIBLE".into();
    }
    let mut selected = 0;
    let mut selected_bytes = 0;
    for name in qualified.iter().rev() {
        let object = objects
            .get_mut(name)
            .ok_or_else(|| err("CHECKPOINT_RETENTION_LAYOUT"))?;
        if object.row.reason == "RETENTION_ELIGIBLE" && selected < policy.max_remove {
            object.row.selected = true;
            selected += 1;
            selected_bytes += object.row.bytes;
        }
    }
    let inventory = reader.inventory.values().collect::<Vec<_>>();
    let namespace_sha256 =
        sha(&serde_json::to_vec(&inventory).map_err(|_| err("CHECKPOINT_RETENTION_ENCODING"))?);
    let mut plan = Plan {
        namespace_digest_basis: "FILE_METADATA_AND_VERIFIED_KNOWN_OBJECT_BYTES".into(),
        schema_version: 1,
        plan_id: String::new(),
        namespace_sha256,
        store_sha256: sha(base.as_bytes()),
        reference: boot.snapshot.reference.clone(),
        cursor: ManifestCursor {
            topic: boot.cursor.topic.clone(),
            partition: boot.cursor.partition,
            next_offset: boot.cursor.offset.to_string(),
        },
        policy: policy.clone(),
        observed_at_ms,
        scanned_entries: reader.entries,
        read_bytes: reader.read_bytes,
        selected_bytes,
        selected_objects: selected,
        objects: objects.into_values().map(|object| object.row).collect(),
        applied: false,
    };
    plan.plan_id =
        sha(&serde_json::to_vec(&plan).map_err(|_| err("CHECKPOINT_RETENTION_ENCODING"))?);
    guard()?;
    Ok(plan)
}
