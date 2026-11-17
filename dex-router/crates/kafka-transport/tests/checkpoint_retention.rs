use serde_json::Value;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use xone_domain::err;
use xone_kafka_transport::{
    checkpoint::{
        local,
        retention::{self, Policy},
    },
    Bootstrap,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "xone-retention-plan-{}-{}",
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
fn policy() -> Policy {
    Policy {
        auto_reclaim: false,
        reclaim_enabled: false,
        plan_ttl_ms: 0,
        keep_latest: 2,
        min_age_ms: 3600000,
        max_remove: 2,
        max_objects: 32,
        max_entries: 256,
        max_read_bytes: 1 << 20,
    }
}
#[test]
#[ignore = "requires current Go retention directory and the exact observed plan"]
fn current_go_retention_directory_and_entire_plan_match_rust_without_mutation() {
    let directory = PathBuf::from(
        std::env::var("XONE_CHECKPOINT_RETENTION_VECTORS_OUTPUT")
            .expect("current Go retention export required"),
    );
    let value: Value =
        serde_json::from_slice(&std::fs::read(directory.join("retention-vector.json")).unwrap())
            .unwrap();
    let boot: Bootstrap = serde_json::from_value(value["bootstrap"].clone()).unwrap();
    let policy: Policy = serde_json::from_value(value["policy"].clone()).unwrap();
    let base = value["base_url"].as_str().unwrap();
    let now = value["observed_at_ms"].as_i64().unwrap();
    let actual = retention::plan(
        &directory.join("objects"),
        &boot,
        8 << 20,
        base,
        &policy,
        now,
        || Ok(()),
    )
    .unwrap();
    let raw = serde_json::to_value(&actual).unwrap();
    assert_eq!(
        raw, value["plan"],
        "Go/Rust must agree on every protection, count, file digest and plan id"
    );
    let repeat = retention::plan(
        &directory.join("objects"),
        &boot,
        8 << 20,
        base,
        &policy,
        now,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(actual.namespace_sha256, repeat.namespace_sha256);
    assert_eq!(actual.plan_id, repeat.plan_id);
    assert!(!actual.applied);
    assert_eq!(actual.selected_objects, 2);
    let mut read_limited = policy.clone();
    read_limited.max_read_bytes = 1;
    assert!(retention::plan(
        &directory.join("objects"),
        &boot,
        8 << 20,
        base,
        &read_limited,
        now,
        || Ok(())
    )
    .is_err());
    let mut object_limited = policy.clone();
    object_limited.max_objects = 2;
    assert!(retention::plan(
        &directory.join("objects"),
        &boot,
        8 << 20,
        base,
        &object_limited,
        now,
        || Ok(())
    )
    .is_err());
    assert_eq!(
        retention::plan(
            &directory.join("objects"),
            &boot,
            8 << 20,
            base,
            &policy,
            now,
            || Err(err("OWNER_REVOKED"))
        )
        .unwrap_err()
        .0,
        "OWNER_REVOKED"
    );
}
#[test]
fn rust_retention_plan_preserves_current_and_uncertified_objects() {
    let root = Temp::new();
    let snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let boot = Bootstrap {
        snapshot,
        cursor: xone_kafka_transport::Cursor {
            topic: "canonical.evm-31337.v1".into(),
            partition: 0,
            offset: 7,
        },
    };
    let p = local::write(&root.0, &boot, 8 << 20, || Ok(())).unwrap();
    let plan = retention::plan(
        &root.0,
        &boot,
        8 << 20,
        "https://objects.example/bucket",
        &policy(),
        1795000000000,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(plan.selected_objects, 0);
    assert_eq!(plan.objects.len(), 1);
    assert_eq!(plan.objects[0].reason, "INCOMPLETE");
    assert_eq!(plan.objects[0].content_coverage, "METADATA_ONLY");
    assert_eq!(
        local::read(&p.manifest_path, &p.manifest_sha256, 8 << 20)
            .unwrap()
            .cursor,
        boot.cursor
    );
    #[cfg(unix)]
    {
        let namespace = root.0.join("public-v2");
        std::os::unix::fs::symlink(&root.0, namespace.join("link")).unwrap();
        assert!(retention::plan(
            &root.0,
            &boot,
            8 << 20,
            "https://objects.example/bucket",
            &policy(),
            1795000000000,
            || Ok(())
        )
        .is_err());
    }
}
