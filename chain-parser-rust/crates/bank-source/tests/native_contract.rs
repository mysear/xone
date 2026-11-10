use serde_json::json;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use xone_bank_source::*;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "xone-native-contract-{}-{}",
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
fn key(n: u8) -> String {
    bs58::encode([n; 32]).into_string()
}
fn request(root: &std::path::Path, session: &str) {
    std::fs::write(root.join(format!("{session}.request")),json!({"version":1,"chain_key":"solana:localnet","genesis_hash":"localnet","source_id":"public","session_id":session,"catalog_identity":"ab".repeat(32),"catalog_revision":1,"scope_accounts":[key(1)],"expires_ms":u64::MAX}).to_string()).unwrap();
}
fn observer(root: &std::path::Path) -> Observer {
    let catalog = root.join("catalog");
    std::fs::write(
        &catalog,
        json!({"version":1,"revision":1,"chain_key":"solana:localnet","genesis_hash":"localnet"})
            .to_string(),
    )
    .unwrap();
    request(root, "first");
    let mut o = Observer::new(SourceConfig {
        chain_key: "solana:localnet".into(),
        genesis_hash: "localnet".into(),
        source_id: "public".into(),
        catalog_file: catalog,
        manifest_directory: root.into(),
        max_bytes: 1000000,
        max_updates: 100,
        max_banks: 8,
        max_sessions: 4,
        refs_endpoint: None,
        refs_token_file: None,
    })
    .unwrap();
    o.genesis("localnet").unwrap();
    o
}
fn write(slot: u64) -> ManifestAccount {
    ManifestAccount {
        pubkey: key(1),
        owner: key(2),
        lamports: 1,
        executable: false,
        rent_epoch: 0,
        write_slot: slot,
        write_bank_id: slot,
        write_version: slot,
        data_base64: "AA==".into(),
        txn_signature_base64: None,
    }
}
fn frozen(o: &mut Observer, slot: u64) -> Vec<BankManifest> {
    o.frozen(
        NativeIdentity {
            slot,
            bank_id: slot,
            blockhash: key(slot as u8),
            state_hash: key(90),
            parent_bank_id: if slot == 10 { None } else { Some(slot - 1) },
            timestamp_ms: 1000 + slot,
        },
        |id| {
            Ok(FrozenAccount {
                pubkey: id.into(),
                owner: key(2),
                lamports: 1,
                executable: false,
                rent_epoch: 0,
                data_base64: "AA==".into(),
            })
        },
    )
    .unwrap()
}
#[test]
fn frozen_native_bank_can_be_rematerialized_for_a_new_receipt_session() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    o.observe(write(11)).unwrap();
    frozen(&mut o, 11);
    request(&dir.0, "second");
    o.observe(write(12)).unwrap();
    let exports = frozen(&mut o, 12);
    assert!(exports.iter().any(|m|m.bank.slot==11 && m.session_id=="second"),"a finalized reconnect needs retained real native bank evidence, not invented cross-session versions");
}
#[test]
fn native_write_after_freeze_poisoning_never_creates_another_certificate() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    let mut late = write(10);
    late.write_version = 99;
    assert!(
        o.observe(late).is_err(),
        "a write after native frozen completion invalidates that producer authority"
    );
}

#[test]
fn abandoned_unfrozen_forks_release_pending_budget() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    for slot in 11..=35 {
        o.observe(write(1000 + slot)).unwrap();
        o.drop_bank(1000 + slot);
        o.drop_bank(1000 + slot); // repeated native cleanup is harmless
        o.observe(write(slot))
            .expect("abandoned forks must not exhaust pending bank budget");
        frozen(&mut o, slot);
    }
}

#[test]
fn drop_keeps_frozen_baselines_and_integrity_failure() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    o.drop_bank(10);
    o.observe(write(11)).unwrap();
    assert!(
        !frozen(&mut o, 11).is_empty(),
        "a dropped parent remains a frozen baseline"
    );
    let mut late = write(11);
    late.write_version = 99;
    assert!(o.observe(late).is_err());
    o.drop_bank(11);
    assert!(
        o.observe(write(12)).is_err(),
        "drop cannot erase integrity poison"
    );
}

#[test]
fn quality_expired_session_directory_blocks_gc() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    // All entries here are expired receiver requests left by ungraceful exits.
    for i in 0..66 {
        std::fs::write(dir.0.join(format!("expired{i}.request")),json!({"version":1,"chain_key":"solana:localnet","genesis_hash":"localnet","source_id":"public","session_id":format!("expired{i}"),"catalog_identity":"ab".repeat(32),"catalog_revision":1,"scope_accounts":[key(1)],"expires_ms":1}).to_string()).unwrap();
    }
    o.observe(write(11)).unwrap();
    let result = o.frozen(
        NativeIdentity {
            slot: 11,
            bank_id: 11,
            blockhash: key(11),
            state_hash: key(90),
            parent_bank_id: Some(10),
            timestamp_ms: 1011,
        },
        |id| {
            Ok(FrozenAccount {
                pubkey: id.into(),
                owner: key(2),
                lamports: 1,
                executable: false,
                rent_epoch: 0,
                data_base64: "AA==".into(),
            })
        },
    );
    println!(
        "after expired requests: {:?}",
        result.as_ref().map(|m| m.len())
    );
    assert!(
        result.is_ok(),
        "expired requests should permit a new live bank"
    );
}

#[test]
fn expired_cleanup_preserves_live_foreign_and_unknown_files() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    let live_path = dir.0.join("first.request");
    let live = std::fs::read(&live_path).unwrap();
    let mut expired: serde_json::Value = serde_json::from_slice(&live).unwrap();
    expired["session_id"] = json!("retired");
    expired["expires_ms"] = json!(1);
    std::fs::write(dir.0.join("retired.request"), expired.to_string()).unwrap();
    expired["session_id"] = json!("foreign");
    expired["chain_key"] = json!("another-chain");
    let foreign = expired.to_string();
    std::fs::write(dir.0.join("foreign.request"), &foreign).unwrap();
    std::fs::write(dir.0.join("unknown.request"), b"unknown request bytes").unwrap();
    std::fs::write(dir.0.join("protected.json"), b"unknown protected proof").unwrap();
    o.observe(write(11)).unwrap();
    let result = o.frozen(
        NativeIdentity {
            slot: 11,
            bank_id: 11,
            blockhash: key(11),
            state_hash: key(90),
            parent_bank_id: Some(10),
            timestamp_ms: 1011,
        },
        |id| {
            Ok(FrozenAccount {
                pubkey: id.into(),
                owner: key(2),
                lamports: 1,
                executable: false,
                rent_epoch: 0,
                data_base64: "AA==".into(),
            })
        },
    );
    assert!(result.is_err(), "an unknown request must still fail closed");
    assert!(!dir.0.join("retired.request").exists());
    assert_eq!(std::fs::read(live_path).unwrap(), live);
    assert_eq!(
        std::fs::read_to_string(dir.0.join("foreign.request")).unwrap(),
        foreign
    );
    assert_eq!(
        std::fs::read(dir.0.join("unknown.request")).unwrap(),
        b"unknown request bytes"
    );
    assert_eq!(
        std::fs::read(dir.0.join("protected.json")).unwrap(),
        b"unknown protected proof"
    );
}

#[test]
fn expired_cleanup_keeps_scan_bound_and_makes_retry_progress() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    let mut template: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.0.join("first.request")).unwrap()).unwrap();
    template["expires_ms"] = json!(1);
    for i in 0..200 {
        let id = format!("retired{i}");
        template["session_id"] = json!(id);
        std::fs::write(dir.0.join(format!("{id}.request")), template.to_string()).unwrap();
    }
    o.observe(write(11)).unwrap();
    let attempt = |o: &mut Observer| {
        o.frozen(
            NativeIdentity {
                slot: 11,
                bank_id: 11,
                blockhash: key(11),
                state_hash: key(90),
                parent_bank_id: Some(10),
                timestamp_ms: 1011,
            },
            |id| {
                Ok(FrozenAccount {
                    pubkey: id.into(),
                    owner: key(2),
                    lamports: 1,
                    executable: false,
                    rent_epoch: 0,
                    data_base64: "AA==".into(),
                })
            },
        )
    };
    assert!(
        attempt(&mut o).is_err(),
        "200 stale entries cannot all be scanned in a 64-entry maintenance pass"
    );
    let remaining = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("retired"))
        .count();
    assert!(
        (136..200).contains(&remaining),
        "cleanup cannot exceed the original 64-entry scan cap; remaining={remaining}"
    );
    let mut recovered = false;
    for _ in 0..4 {
        if attempt(&mut o).is_ok() {
            recovered = true;
            break;
        }
    }
    assert!(
        recovered,
        "each bounded attempt must retire stale entries without poisoning the pending native bank"
    );
}

#[test]
fn cleanup_restores_renewed_request_from_interrupted_quarantine() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    let original = dir.0.join("first.request");
    let bytes = std::fs::read(&original).unwrap();
    let staged = dir.0.join(".xone-expired-first-interrupted.request");
    std::fs::rename(&original, &staged).unwrap();
    o.observe(write(11)).unwrap();
    frozen(&mut o, 11);
    assert_eq!(
        std::fs::read(original).unwrap(),
        bytes,
        "an unexpired moved request must be restored, never GCed"
    );
    assert!(!staged.exists());
}

#[test]
fn cleanup_never_overwrites_concurrent_renewal() {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    let original = dir.0.join("first.request");
    let bytes = std::fs::read(&original).unwrap();
    let staged = dir.0.join(".xone-expired-first-interrupted.request");
    std::fs::write(&staged, &bytes).unwrap();
    o.observe(write(11)).unwrap();
    let result = o.frozen(
        NativeIdentity {
            slot: 11,
            bank_id: 11,
            blockhash: key(11),
            state_hash: key(90),
            parent_bank_id: Some(10),
            timestamp_ms: 1011,
        },
        |id| {
            Ok(FrozenAccount {
                pubkey: id.into(),
                owner: key(2),
                lamports: 1,
                executable: false,
                rent_epoch: 0,
                data_base64: "AA==".into(),
            })
        },
    );
    assert!(
        result.is_err(),
        "ambiguous live quarantine must remain fail closed"
    );
    assert_eq!(std::fs::read(original).unwrap(), bytes);
    assert_eq!(std::fs::read(staged).unwrap(), bytes);
}

fn cold_keys_reclaim_before_freeze_gate(expired: usize) {
    let dir = Temp::new();
    let mut o = observer(&dir.0);
    // No keys(), account callback or freeze may warm the Observer before the
    // crashed receivers' files are installed: this is the native cold gate.
    let live = std::fs::read(dir.0.join("first.request")).unwrap();
    let mut template: serde_json::Value = serde_json::from_slice(&live).unwrap();
    template["expires_ms"] = json!(1);
    for i in 0..expired {
        let id = format!("cold{i}");
        template["session_id"] = json!(id);
        std::fs::write(dir.0.join(format!("{id}.request")), template.to_string()).unwrap();
    }
    let proof = b"unknown protected proof";
    std::fs::write(dir.0.join("unknown-proof.json"), proof).unwrap();
    template["session_id"] = json!("foreign");
    template["chain_key"] = json!("another-chain");
    let foreign = template.to_string();
    std::fs::write(dir.0.join("foreign.request"), &foreign).unwrap();
    let mut ready = false;
    for retry in 0..5 {
        let keys = o.keys();
        let remaining = std::fs::read_dir(&dir.0)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("cold"))
            .count();
        println!(
            "cold retry={retry} expired={expired} keys={} remaining={remaining}",
            keys.len()
        );
        if retry == 0 && expired == 200 {
            assert!(
                keys.is_empty(),
                "an incomplete inventory cannot open the freeze gate"
            );
            assert!((136..200).contains(&remaining), "one maintenance pass must respect the original 64-entry cap: remaining={remaining}");
        }
        if !keys.is_empty() {
            assert_eq!(keys, vec![key(1)]);
            ready = true;
            break;
        }
    }
    assert!(
        ready,
        "cold configured_keys must reclaim expired receivers before the freeze gate"
    );
    assert_eq!(std::fs::read(dir.0.join("first.request")).unwrap(), live);
    assert_eq!(
        std::fs::read(dir.0.join("unknown-proof.json")).unwrap(),
        proof
    );
    assert_eq!(
        std::fs::read_to_string(dir.0.join("foreign.request")).unwrap(),
        foreign
    );
    o.observe(write(10)).unwrap();
    frozen(&mut o, 10);
    o.observe(write(11)).unwrap();
    assert!(
        !frozen(&mut o, 11).is_empty(),
        "actual native writes and parent completion still gate export"
    );
}

#[test]
fn cold_keys_reclaim_66_expired_requests_before_freeze_gate() {
    cold_keys_reclaim_before_freeze_gate(66);
}
#[test]
fn cold_keys_reclaim_200_expired_requests_in_bounded_attempts() {
    cold_keys_reclaim_before_freeze_gate(200);
}

#[test]
fn cold_keys_still_create_an_absent_manifest_directory() {
    let dir = Temp::new();
    let _seed = observer(&dir.0);
    let manifests = dir.0.join("new-manifests");
    let mut o = Observer::new(SourceConfig {
        chain_key: "solana:localnet".into(),
        genesis_hash: "localnet".into(),
        source_id: "public".into(),
        catalog_file: dir.0.join("catalog"),
        manifest_directory: manifests.clone(),
        max_bytes: 1000000,
        max_updates: 100,
        max_banks: 8,
        max_sessions: 4,
        refs_endpoint: None,
        refs_token_file: None,
    })
    .unwrap();
    o.genesis("localnet").unwrap();
    assert!(!manifests.exists());
    assert!(o.keys().is_empty());
    assert!(
        manifests.is_dir(),
        "maintenance must preserve the original cold directory initialization"
    );
    request(&manifests, "first");
    assert_eq!(o.keys(), vec![key(1)]);
}

#[test]
fn first_real_native_write_version_zero_is_retained_without_poisoning() {
    let directory = Temp::new();
    let mut source = observer(&directory.0);
    let mut row = write(10);
    row.write_version = 0;
    source
        .observe(row.clone())
        .expect("Agave's first notifier version is zero");
    source
        .observe(row.clone())
        .expect("identical callback remains idempotent");
    frozen(&mut source, 10);
    let manifests = frozen(&mut source, 11);
    let manifest = manifests.iter().find(|m| m.bank.slot == 11).unwrap();
    assert_eq!(manifest.parent_accounts[0].write_version, 0);
    assert_eq!(
        manifest.expected_account_updates, 0,
        "inherited version zero must not fabricate an observed child write"
    );
    assert!(manifest.native_writes.is_empty());
    let mut next = write(12);
    next.write_version = 1;
    source.observe(next).unwrap();
    assert!(frozen(&mut source, 12).iter().any(|m| m.bank.slot == 12));

    let conflict_directory = Temp::new();
    let mut conflict = observer(&conflict_directory.0);
    conflict.observe(row.clone()).unwrap();
    row.data_base64 = "AQ==".into();
    assert!(
        conflict.observe(row).is_err(),
        "conflicting duplicate version zero must poison the source"
    );
}
