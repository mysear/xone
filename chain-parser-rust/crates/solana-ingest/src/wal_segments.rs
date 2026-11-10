//! Whole-segment retention. Rotation is allowed only at a complete owner-committed
//! boundary with no staged bank refs. The sole writer is CLOSED before rename.
use crate::owner_scope::CommittedWal;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io::Write, path::Path};
use xone_domain::{err, Result};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    commit: serde_json::Value,
    sessions: BTreeSet<String>,
    bytes: u64,
    checksum: String,
    #[serde(default)]
    native_refs: Vec<crate::owner_scope::NativeManifestRef>,
}
fn size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut entries = 0usize;
    for item in std::fs::read_dir(path).map_err(|_| err("WAL_SEGMENT_READ"))? {
        let item = item.map_err(|_| err("WAL_SEGMENT_READ"))?;
        let kind = item.file_type().map_err(|_| err("WAL_SEGMENT_READ"))?;
        if kind.is_file() {
            total = total
                .checked_add(item.metadata().map_err(|_| err("WAL_SEGMENT_READ"))?.len())
                .ok_or_else(|| err("WAL_SEGMENT_BYTE_LIMIT"))?;
            entries += 1;
        } else if kind.is_dir() && item.file_name() == "NATIVE_MANIFESTS" {
            for native in std::fs::read_dir(item.path()).map_err(|_| err("WAL_SEGMENT_READ"))? {
                let native = native.map_err(|_| err("WAL_SEGMENT_READ"))?;
                if !native
                    .file_type()
                    .map_err(|_| err("WAL_SEGMENT_READ"))?
                    .is_file()
                {
                    return Err(err("WAL_SEGMENT_LAYOUT"));
                }
                total = total
                    .checked_add(
                        native
                            .metadata()
                            .map_err(|_| err("WAL_SEGMENT_READ"))?
                            .len(),
                    )
                    .ok_or_else(|| err("WAL_SEGMENT_BYTE_LIMIT"))?;
                entries += 1;
                if entries > 10000 {
                    return Err(err("WAL_SEGMENT_FILE_LIMIT"));
                }
            }
        } else {
            return Err(err("WAL_SEGMENT_LAYOUT"));
        }
        if entries > 10000 {
            return Err(err("WAL_SEGMENT_FILE_LIMIT"));
        }
    }
    Ok(total)
}
/// Caller supplies live references read from the SAME owner ledger after commit.
/// Protected segments are never evicted to make a budget pass.
pub(crate) fn rotate(
    active: &Path,
    proof: CommittedWal,
    protected: &BTreeSet<String>,
    max_segments: usize,
    max_bytes: u64,
    native_directory: &Path,
    native_refs: &[crate::owner_scope::NativeManifestRef],
) -> Result<()> {
    if max_segments == 0 || max_bytes == 0 || proof.cursor.sequence == 0 {
        return Err(err("WAL_SEGMENT_BUDGET"));
    }
    let root = active.with_extension("archives");
    std::fs::create_dir_all(&root).map_err(|_| err("WAL_SEGMENT_WRITE"))?;
    let native_archive = active.join("NATIVE_MANIFESTS");
    std::fs::create_dir_all(&native_archive).map_err(|_| err("WAL_NATIVE_ARCHIVE_WRITE"))?;
    for reference in native_refs {
        if Path::new(&reference.filename)
            .file_name()
            .and_then(|s| s.to_str())
            != Some(reference.filename.as_str())
        {
            return Err(err("WAL_NATIVE_MANIFEST_PATH"));
        }
        let source = native_directory.join(&reference.filename);
        let manifest: crate::bank_manifest::BankManifest = crate::bank_manifest::read_bounded(
            &source,
            usize::try_from(max_bytes).map_err(|_| err("WAL_SEGMENT_BYTE_LIMIT"))?,
        )?;
        if xone_domain::hash(&manifest)? != reference.sha256 {
            return Err(err("WAL_NATIVE_MANIFEST_DIGEST"));
        }
        let data =
            serde_json::to_vec(&manifest).map_err(|_| err("WAL_NATIVE_MANIFEST_ENCODING"))?;
        let mut file = std::fs::File::create(native_archive.join(&reference.filename))
            .map_err(|_| err("WAL_NATIVE_ARCHIVE_WRITE"))?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|_| err("WAL_NATIVE_ARCHIVE_WRITE"))?;
    }
    std::fs::File::open(&native_archive)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("WAL_NATIVE_ARCHIVE_WRITE"))?;
    let bytes = size(active)?;
    if bytes > max_bytes {
        return Err(err("WAL_SEGMENT_BYTE_LIMIT"));
    }
    let mut old = vec![];
    let mut total = 0u64;
    for entry in std::fs::read_dir(&root).map_err(|_| err("WAL_SEGMENT_READ"))? {
        let p = entry.map_err(|_| err("WAL_SEGMENT_READ"))?.path();
        if !p.is_dir() {
            return Err(err("WAL_SEGMENT_LAYOUT"));
        }
        let mut m: Manifest = crate::bank_manifest::read_bounded(
            &p.join("OWNER_SEGMENT.json"),
            usize::try_from(max_bytes.min(4 * 1024 * 1024))
                .map_err(|_| err("WAL_SEGMENT_BYTE_LIMIT"))?,
        )?;
        let checksum = std::mem::take(&mut m.checksum);
        if xone_domain::hash(&m)? != checksum {
            return Err(err("WAL_SEGMENT_CHECKSUM"));
        }
        m.checksum = checksum;
        if m.version != 1 {
            return Err(err("WAL_SEGMENT_SCHEMA"));
        }
        let stored = size(&p)?;
        total = total
            .checked_add(stored)
            .ok_or_else(|| err("WAL_SEGMENT_BYTE_LIMIT"))?;
        old.push((p, m, stored));
        if old.len() > max_segments {
            return Err(err("WAL_SEGMENT_COUNT_LIMIT"));
        }
    }
    old.sort_by(|a, b| a.0.cmp(&b.0));
    while old.len() >= max_segments || total.checked_add(bytes).is_none_or(|n| n > max_bytes) {
        let index = old
            .iter()
            .position(|(_, m, _)| m.sessions.is_disjoint(protected))
            .ok_or_else(|| err("WAL_SEGMENT_REFERENCES_PROTECTED"))?;
        let (p, _, n) = old.remove(index);
        std::fs::remove_dir_all(&p).map_err(|_| err("WAL_SEGMENT_GC_UNKNOWN"))?;
        std::fs::File::open(&root)
            .and_then(|f| f.sync_all())
            .map_err(|_| err("WAL_SEGMENT_GC_UNKNOWN"))?;
        total -= n;
    }
    // Exact source-session inventory is read only after the capture worker released
    // the lock; this opens a verification generation and cannot grant live authority.
    let mut sessions = BTreeSet::new();
    {
        let wal = crate::stream::FrameWal::open(
            active,
            &proof.chain,
            crate::stream::FrameWalLimits {
                max_frame_bytes: 32 * 1024 * 1024,
                max_total_bytes: usize::try_from(max_bytes)
                    .map_err(|_| err("WAL_SEGMENT_BYTE_LIMIT"))?,
                max_records: 1_000_000,
            },
        )?;
        let receipt = wal.read(proof.cursor.sequence)?;
        if receipt.frame.source.source_id != proof.source_id
            || receipt.frame.source.session_id != proof.session_id
            || receipt.wal_generation != proof.wal_generation
            || receipt.cursor != proof.cursor
        {
            return Err(err("WAL_SEGMENT_OWNER_BINDING"));
        }
        if wal.len() != proof.cursor.sequence || wal.recovery_cursor() != proof.cursor {
            return Err(err("WAL_SEGMENT_UNCOMMITTED_TAIL"));
        }
        for sequence in 1..=wal.len() {
            sessions.insert(wal.read(sequence)?.frame.source.session_id);
        }
    }
    let mut manifest = Manifest {
        version: 1,
        commit: serde_json::to_value(&proof).map_err(|_| err("WAL_SEGMENT_ENCODING"))?,
        sessions,
        bytes,
        checksum: String::new(),
        native_refs: native_refs.to_vec(),
    };
    manifest.checksum = xone_domain::hash(&manifest)?;
    let data = serde_json::to_vec(&manifest).map_err(|_| err("WAL_SEGMENT_ENCODING"))?;
    let temp = active.join("OWNER_SEGMENT.tmp");
    let mut file = std::fs::File::create(&temp).map_err(|_| err("WAL_SEGMENT_WRITE"))?;
    file.write_all(&data)
        .and_then(|_| file.sync_all())
        .map_err(|_| err("WAL_SEGMENT_WRITE"))?;
    std::fs::rename(&temp, active.join("OWNER_SEGMENT.json"))
        .map_err(|_| err("WAL_SEGMENT_WRITE"))?;
    std::fs::File::open(active)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("WAL_SEGMENT_WRITE"))?;
    if size(active)?
        .checked_add(total)
        .is_none_or(|n| n > max_bytes)
    {
        return Err(err("WAL_SEGMENT_BYTE_LIMIT"));
    }
    let destination = root.join(format!("{}-{}", proof.session_id, proof.cursor.sequence));
    if destination.exists() {
        return Err(err("WAL_SEGMENT_ALREADY_EXISTS"));
    }
    std::fs::rename(active, &destination).map_err(|_| err("WAL_SEGMENT_RENAME_UNKNOWN"))?;
    std::fs::File::open(&root)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("WAL_SEGMENT_RENAME_UNKNOWN"))?;
    if let Some(parent) = active.parent() {
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| err("WAL_SEGMENT_RENAME_UNKNOWN"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "xone-segment-refs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn seal(path: &Path, session: &str) -> CommittedWal {
        let mut wal = FrameWal::open(
            path,
            "solana:localnet",
            FrameWalLimits {
                max_frame_bytes: 1024,
                max_total_bytes: 1000000,
                max_records: 10,
            },
        )
        .unwrap();
        let generation = wal.generation();
        let cursor = wal
            .capture(RawFrame {
                source: SourceSession {
                    source_id: "public".into(),
                    session_id: session.into(),
                },
                sequence: 1,
                receive_timestamp_ms: 1000,
                protobuf: vec![1, 2, 3],
            })
            .unwrap();
        wal.checkpoint(&cursor).unwrap();
        CommittedWal {
            chain: "solana:localnet".into(),
            source_id: "public".into(),
            session_id: session.into(),
            wal_generation: generation,
            cursor,
            bank: BankId {
                slot: 10,
                bank_id: 10,
            },
            blockhash: "bank".into(),
            commit_id: "controlled-owner-commit".into(),
        }
    }
    #[test]
    fn archive_revalidates_actual_record_source_binding_after_writer_close() {
        let dir = Temp::new();
        let active = dir.0.join("active");
        let mut proof = seal(&active, "actual");
        proof.session_id = "wrong-session".into();
        assert!(
            rotate(&active, proof, &BTreeSet::new(), 4, 1000000, &dir.0, &[]).is_err(),
            "a matching cursor digest cannot authorize a different source session"
        );
        assert!(active.exists());
    }
    #[test]
    fn protected_archive_never_evicted_to_make_a_budget_pass() {
        let dir = Temp::new();
        let active = dir.0.join("active");
        let first = seal(&active, "first");
        rotate(
            &active,
            first,
            &BTreeSet::from(["first".into()]),
            1,
            1000000,
            &dir.0,
            &[],
        )
        .unwrap();
        let second = seal(&active, "second");
        let protected = BTreeSet::from(["first".into(), "second".into()]);
        assert!(rotate(&active, second.clone(), &protected, 1, 1000000, &dir.0, &[]).is_err());
        assert!(active.exists());
        rotate(
            &active,
            second,
            &BTreeSet::from(["second".into()]),
            1,
            1000000,
            &dir.0,
            &[],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_dir(active.with_extension("archives"))
                .unwrap()
                .count(),
            1
        );
    }
}
