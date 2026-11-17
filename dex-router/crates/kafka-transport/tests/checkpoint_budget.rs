use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use xone_domain::{err, Snapshot};
use xone_kafka_transport::{
    checkpoint::{
        budget::{self, Budget},
        local,
    },
    Bootstrap, Cursor,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "xone-budget-{}-{}",
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
fn boot() -> Bootstrap {
    let snapshot: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    Bootstrap {
        snapshot,
        cursor: Cursor {
            topic: "canonical.evm-31337.v1".into(),
            partition: 0,
            offset: 9007199254741009,
        },
    }
}
#[test]
fn public_checkpoint_budget_preflights_exact_bytes_files_and_existing_retries() {
    let root = Temp::new();
    let boot = boot();
    let high = Budget {
        max_bytes: 1 << 20,
        max_files: 32,
        max_entries: 64,
    };
    let expected = budget::admit(&root.0, &boot, 8 << 20, "", &high, || Ok(())).unwrap();
    assert_eq!(expected.files, 2);
    assert_eq!(expected.entries, 3);
    let exact = Budget {
        max_bytes: expected.bytes,
        max_files: expected.files,
        max_entries: expected.entries,
    };
    for mode in 0..3 {
        let mut limit = exact.clone();
        match mode {
            0 => limit.max_bytes -= 1,
            1 => limit.max_files -= 1,
            _ => limit.max_entries -= 1,
        };
        assert!(budget::admit(&root.0, &boot, 8 << 20, "", &limit, || Ok(())).is_err());
    }
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
    budget::admit(&root.0, &boot, 8 << 20, "", &exact, || Ok(())).unwrap();
    let p = local::write(&root.0, &boot, 8 << 20, || Ok(())).unwrap();
    assert_eq!(
        budget::admit(&root.0, &boot, 8 << 20, "", &exact, || Ok(())).unwrap(),
        expected
    );
    let mut next = boot.clone();
    next.snapshot.reference.online_generation += 1;
    next.snapshot.reference.journal_seq += 1;
    next.cursor.offset += 4;
    assert!(budget::admit(&root.0, &next, 8 << 20, "", &exact, || Ok(())).is_err());
    assert_eq!(
        local::read(&p.manifest_path, &p.manifest_sha256, 8 << 20)
            .unwrap()
            .cursor,
        boot.cursor
    );
    let remote = budget::admit(
        &Temp::new().0,
        &boot,
        8 << 20,
        "https://objects.example/bucket",
        &high,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(remote.files, 4);
    assert_eq!(remote.entries, 5);
}
#[test]
fn public_checkpoint_budget_counts_unknown_files_and_bounds_depth_and_entry_scan() {
    let boot = boot();
    let high = Budget {
        max_bytes: 1 << 20,
        max_files: 32,
        max_entries: 32,
    };
    for mode in ["depth", "entries", "bytes", "files"] {
        let root = Temp::new();
        let namespace = root.0.join("public-v2");
        std::fs::create_dir(&namespace).unwrap();
        let mut limit = high.clone();
        match mode {
            "depth" => std::fs::create_dir_all(namespace.join("a/b/c/d/e/f")).unwrap(),
            "entries" => {
                for i in 0..33 {
                    std::fs::create_dir(namespace.join(format!("{i}"))).unwrap();
                }
            }
            "bytes" => {
                limit.max_bytes = 1;
                std::fs::write(namespace.join("unknown"), b"full").unwrap();
            }
            _ => {
                limit.max_files = 1;
                std::fs::write(namespace.join("one"), []).unwrap();
                std::fs::write(namespace.join("two"), []).unwrap();
            }
        }
        assert!(
            budget::admit(&root.0, &boot, 8 << 20, "", &limit, || Ok(())).is_err(),
            "{mode}"
        );
    }
    assert_eq!(
        budget::admit(&Temp::new().0, &boot, 8 << 20, "", &high, || Err(err(
            "SOURCE_REVOKED"
        )))
        .unwrap_err()
        .0,
        "SOURCE_REVOKED"
    );
}
#[cfg(unix)]
#[test]
fn public_checkpoint_budget_rejects_symlink_namespace_and_entries() {
    use std::os::unix::fs::symlink;
    let high = Budget {
        max_bytes: 1 << 20,
        max_files: 32,
        max_entries: 64,
    };
    for namespace_link in [true, false] {
        let root = Temp::new();
        let outside = Temp::new();
        let namespace = root.0.join("public-v2");
        if namespace_link {
            symlink(&outside.0, &namespace).unwrap();
        } else {
            std::fs::create_dir(&namespace).unwrap();
            symlink(&outside.0, namespace.join("link")).unwrap();
        }
        assert!(budget::admit(&root.0, &boot(), 8 << 20, "", &high, || Ok(())).is_err());
        assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
    }
}

#[test]
#[ignore = "requires the current Go budget projection and actual public objects"]
fn current_go_checkpoint_budget_projection_and_actual_objects_match() {
    let directory = std::path::PathBuf::from(
        std::env::var("XONE_CHECKPOINT_BUDGET_VECTORS_OUTPUT")
            .expect("current Go budget export required"),
    );
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("budget-vector.json")).unwrap())
            .unwrap();
    let raw = value["manifest"].as_str().unwrap().as_bytes();
    let payload = hex::decode(value["payload_hex"].as_str().unwrap()).unwrap();
    let boot = xone_kafka_transport::checkpoint::decode(
        raw,
        &payload,
        value["manifest_sha256"].as_str().unwrap(),
        8 << 20,
    )
    .unwrap();
    let limit: Budget = serde_json::from_value(value["budget"].clone()).unwrap();
    let expected: budget::Usage = serde_json::from_value(value["usage"].clone()).unwrap();
    let actual = budget::admit(
        &directory.join("objects"),
        &boot,
        8 << 20,
        value["base_url"].as_str().unwrap(),
        &limit,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(actual, expected);
    let blank = Temp::new();
    let fresh = budget::admit(
        &blank.0,
        &boot,
        8 << 20,
        value["base_url"].as_str().unwrap(),
        &limit,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(fresh, expected);
}
