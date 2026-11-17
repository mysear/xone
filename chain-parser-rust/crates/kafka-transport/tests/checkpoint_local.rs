use serde_json::json;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use xone_domain::{err, Snapshot};
use xone_kafka_transport::{checkpoint::local, Bootstrap, Cursor};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "xone-public-checkpoint-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Bootstrap {
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    Bootstrap {
        snapshot,
        cursor: Cursor {
            topic: "canonical.evm-31337.v1".into(),
            partition: 0,
            offset: 9_007_199_254_741_009,
        },
    }
}
#[test]
fn rust_local_checkpoint_immutable_roundtrip_and_concurrent_retry() {
    let root = Temp::new();
    let boot = fixture();
    let first = local::write(&root.0, &boot, 8 << 20, || Ok(())).unwrap();
    let got = local::read(&first.manifest_path, &first.manifest_sha256, 8 << 20).unwrap();
    assert_eq!(got.cursor, boot.cursor);
    assert_eq!(got.snapshot.reference, boot.snapshot.reference);
    let handles = (0..8)
        .map(|_| {
            let boot = boot.clone();
            let path = root.0.clone();
            std::thread::spawn(move || local::write(&path, &boot, 8 << 20, || Ok(())).unwrap())
        })
        .collect::<Vec<_>>();
    for handle in handles {
        let p = handle.join().unwrap();
        assert_eq!(p.manifest_sha256, first.manifest_sha256);
        assert_eq!(p.manifest_path, first.manifest_path);
    }
    std::fs::write(
        first.manifest_path.with_file_name("bootstrap.pb"),
        b"corrupt",
    )
    .unwrap();
    assert!(local::write(&root.0, &boot, 8 << 20, || Ok(())).is_err());
    assert_eq!(
        std::fs::read(first.manifest_path.with_file_name("bootstrap.pb")).unwrap(),
        b"corrupt"
    );
}
#[test]
fn rust_local_checkpoint_partial_publication_and_revocation_are_not_qualified() {
    let root = Temp::new();
    let boot = fixture();
    let count = std::cell::Cell::new(0);
    assert!(local::write(&root.0, &boot, 8 << 20, || {
        count.set(count.get() + 1);
        if count.get() == 3 {
            Err(err("OWNER_REVOKED"))
        } else {
            Ok(())
        }
    })
    .is_err());
    let (m, _, _) = local::encode(&boot, 8 << 20).unwrap();
    let directory = root.0.join("public-v2").join(m.sha256);
    assert!(directory.join("bootstrap.pb").is_file());
    assert!(!directory.join("manifest.json").exists());
    let original = std::fs::metadata(directory.join("bootstrap.pb")).unwrap();
    let p = local::write(&root.0, &boot, 8 << 20, || Ok(())).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            original.ino(),
            std::fs::metadata(p.manifest_path.with_file_name("bootstrap.pb"))
                .unwrap()
                .ino()
        );
    }
    let count = std::cell::Cell::new(0);
    assert!(local::write(&root.0, &boot, 8 << 20, || {
        count.set(count.get() + 1);
        if count.get() == 4 {
            Err(err("OWNER_REVOKED"))
        } else {
            Ok(())
        }
    })
    .is_err());
    assert!(
        p.manifest_path.is_file(),
        "revocation does not promise deleting physical objects"
    );
}
#[test]
fn rust_local_checkpoint_authenticates_metadata_before_payload_io() {
    let root = Temp::new();
    let boot = fixture();
    let p = local::write(&root.0, &boot, 8 << 20, || Ok(())).unwrap();
    std::fs::remove_file(p.manifest_path.with_file_name("bootstrap.pb")).unwrap();
    assert_eq!(
        local::read(&p.manifest_path, &"a".repeat(64), 8 << 20)
            .unwrap_err()
            .0,
        "CHECKPOINT_MANIFEST_CHECKSUM"
    );
    assert!(local::read(&p.manifest_path, &p.manifest_sha256, 8 << 20).is_err());
    let mut boot = boot;
    boot.cursor.offset = 0;
    assert!(local::encode(&boot, 8 << 20).is_err());
    for max in [0, 1, usize::MAX] {
        assert!(local::encode(&fixture(), max).is_err());
    }
}
#[cfg(unix)]
#[test]
fn rust_local_checkpoint_rejects_symlink_namespaces_and_objects() {
    use std::os::unix::fs::symlink;
    for namespace in [true, false] {
        let root = Temp::new();
        let outside = Temp::new();
        let boot = fixture();
        let (m, _, _) = local::encode(&boot, 8 << 20).unwrap();
        let mut target = root.0.join("public-v2");
        if !namespace {
            std::fs::create_dir(&target).unwrap();
            target = target.join(m.sha256);
        }
        symlink(&outside.0, &target).unwrap();
        assert!(local::write(&root.0, &boot, 8 << 20, || Ok(())).is_err());
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
    }
    let root = Temp::new();
    let boot = fixture();
    let p = local::write(&root.0, &boot, 8 << 20, || Ok(())).unwrap();
    let payload = p.manifest_path.with_file_name("bootstrap.pb");
    let outside = Temp::new();
    let raw = std::fs::read(&payload).unwrap();
    std::fs::write(outside.0.join("payload"), raw).unwrap();
    std::fs::remove_file(&payload).unwrap();
    symlink(outside.0.join("payload"), &payload).unwrap();
    assert!(local::read(&p.manifest_path, &p.manifest_sha256, 8 << 20).is_err());
    assert!(local::write(&root.0, &boot, 8 << 20, || Ok(())).is_err());
}
#[test]
fn rust_local_checkpoint_exports_current_vectors_for_independent_go_decoder() {
    let temporary = Temp::new();
    let out = std::env::var_os("XONE_RUST_CHECKPOINT_VECTORS_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.0.clone());
    std::fs::create_dir_all(&out).unwrap();
    let mut vectors = Vec::new();
    for i in 0..3 {
        let mut boot = fixture();
        if i == 1 {
            boot.cursor.offset = i64::MAX - 1;
            boot.snapshot.reference.journal_seq = (1u64 << 63) + 7;
        }
        if i == 2 {
            boot.snapshot.pools[0].reserve0 = serde_json::from_value(json!(
                "1606938044258990275541962092341162602522202993782792835301376"
            ))
            .unwrap();
        }
        let p = local::write(&out.join("objects"), &boot, 8 << 20, || Ok(())).unwrap();
        let manifest = std::fs::read_to_string(&p.manifest_path).unwrap();
        let payload = std::fs::read(p.manifest_path.with_file_name("bootstrap.pb")).unwrap();
        vectors.push(json!({"manifest":manifest,"payload_hex":hex::encode(payload),"manifest_sha256":p.manifest_sha256,"reference":boot.snapshot.reference,"cursor":boot.cursor}));
    }
    std::fs::write(
        out.join("vectors.json"),
        serde_json::to_vec_pretty(&json!({"schema_version":1,"vectors":vectors})).unwrap(),
    )
    .unwrap();
}

#[test]
fn rust_local_checkpoint_certificates_are_bounded_immutable_and_pinned() {
    let root = Temp::new();
    let p = local::write(&root.0, &fixture(), 8 << 20, || Ok(())).unwrap();
    let tag = format!("remote-{}.json", "a".repeat(64));
    let raw = b"{\"confirmed\":true}";
    local::certify(&p, 8 << 20, &tag, raw, || Ok(())).unwrap();
    assert_eq!(local::certificate(&p.manifest_path, &tag).unwrap(), raw);
    local::certify(&p, 8 << 20, &tag, raw, || Ok(())).unwrap();
    assert!(local::certify(&p, 8 << 20, &tag, b"conflict", || Ok(())).is_err());
    for name in ["../escape", "remote-anything.json", "manifest.json"] {
        assert!(local::certify(&p, 8 << 20, name, raw, || Ok(())).is_err());
        assert!(local::certificate(&p.manifest_path, name).is_err());
    }
    assert!(local::certify(
        &p,
        8 << 20,
        &format!("remote-{}.json", "b".repeat(64)),
        &vec![0; 65537],
        || Ok(())
    )
    .is_err());
    let mut changed = p.clone();
    changed.manifest_sha256 = "b".repeat(64);
    assert!(local::certify(&changed, 8 << 20, &tag, raw, || Ok(())).is_err());
    let mut changed = p.clone();
    changed.manifest.cursor.next_offset = "123".into();
    assert!(local::certify(&changed, 8 << 20, &tag, raw, || Ok(())).is_err());
}

#[test]
fn rust_local_checkpoint_candidate_is_a_bounded_replaceable_hint_not_a_certificate() {
    let root = Temp::new();
    let p = local::write(&root.0, &fixture(), 8 << 20, || Ok(())).unwrap();
    let name = format!("candidate-{}.json", "a".repeat(64));
    local::set_candidate(&p, &name, b"first", || Ok(())).unwrap();
    assert_eq!(local::candidate(&root.0, &name).unwrap(), b"first");
    local::set_candidate(&p, &name, b"second", || Ok(())).unwrap();
    assert_eq!(local::candidate(&root.0, &name).unwrap(), b"second");
    assert!(local::set_candidate(&p, "../escape", b"bad", || Ok(())).is_err());
    assert!(local::set_candidate(&p, &name, &vec![0; 65537], || Ok(())).is_err());
    assert!(local::set_candidate(&p, &name, b"denied", || Err(err("SOURCE_REVOKED"))).is_err());
    assert_eq!(local::candidate(&root.0, &name).unwrap(), b"second");
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let path = root.0.join("public-v2").join(&name);
        std::fs::remove_file(&path).unwrap();
        let outside = Temp::new();
        let destination = outside.0.join("unchanged");
        std::fs::write(&destination, b"outside").unwrap();
        symlink(&destination, &path).unwrap();
        assert!(local::set_candidate(&p, &name, b"wrong", || Ok(())).is_err());
        assert!(local::candidate(&root.0, &name).is_err());
        assert_eq!(std::fs::read(destination).unwrap(), b"outside");
    }
}
