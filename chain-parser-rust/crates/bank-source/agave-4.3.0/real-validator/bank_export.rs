//! Included as a child module of the real Agave AccountsUpdateNotifierImpl.
//! Every notification and frozen read comes from AccountsDb / Bank itself.
use super::*;
use solana_account::Account;
use solana_hash::Hash;
use solana_runtime::{
    bank::Bank, bank_forks::BankForks, genesis_utils::create_genesis_config_with_leader,
    installed_scheduler_pool::BankWithScheduler, runtime_config::RuntimeConfig,
};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, atomic::AtomicBool},
};
use xone_bank_source::{BankManifest, SourceConfig};

// A transparent trace tap: forward every native callback to the production
// notifier unchanged, then retain its actual scoped arguments for provenance.
#[derive(Debug)]
struct TracedNotifier {
    inner: AccountsUpdateNotifierImpl,
    watched: [Pubkey; 2],
    events: Mutex<Vec<serde_json::Value>>,
}
impl AccountsUpdateNotifierInterface for TracedNotifier {
    fn snapshot_notifications_enabled(&self) -> bool {
        self.inner.snapshot_notifications_enabled()
    }
    fn notify_account_update(
        &self,
        slot: Slot,
        bank_id: BankId,
        account: &AccountSharedData,
        txn: &Option<&SanitizedTransaction>,
        pubkey: &Pubkey,
        write_version: u64,
    ) {
        self.inner
            .notify_account_update(slot, bank_id, account, txn, pubkey, write_version);
        if self.watched.contains(pubkey) {
            self.events.lock().unwrap().push(serde_json::json!({
                "slot":slot,"bank_id":bank_id,"write_version":write_version,
                "pubkey":pubkey.to_string(),"owner":account.owner().to_string(),
                "lamports":account.lamports(),"executable":account.executable(),
                "rent_epoch":account.rent_epoch(),"raw_data":account.data(),
                "signature_present":txn.is_some(),
            }));
        }
    }
    fn notify_account_restore_from_snapshot(
        &self,
        slot: Slot,
        write_version: u64,
        account: &AccountForGeyser<'_>,
    ) {
        self.inner
            .notify_account_restore_from_snapshot(slot, write_version, account);
    }
    fn notify_end_of_restore_from_snapshot(&self) {
        self.inner.notify_end_of_restore_from_snapshot();
    }
}
fn save_native_trace(root: &Path, notifier: &TracedNotifier) {
    std::fs::write(
        root.join("native-notifications.json"),
        serde_json::to_vec_pretty(&*notifier.events.lock().unwrap()).unwrap(),
    )
    .unwrap();
}

fn read_manifest(root: &Path, bank: &Bank) -> BankManifest {
    let path = root.join(format!("real-bank-{}-{}.json", bank.slot(), bank.bank_id()));
    serde_json::from_slice(
        &std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .unwrap()
}

fn account_evidence(bank: &Bank, pubkey: &Pubkey) -> serde_json::Value {
    let account = bank.get_account(pubkey).unwrap();
    serde_json::json!({
        "pubkey":pubkey.to_string(), "owner":account.owner().to_string(),
        "lamports":account.lamports(), "executable":account.executable(),
        "rent_epoch":account.rent_epoch(), "raw_data":account.data(),
    })
}

fn complete_and_freeze(bank: &Bank) {
    let scheduler = BankWithScheduler::no_scheduler_available();
    while bank.tick_height() < bank.max_tick_height() {
        bank.register_tick(&Hash::new_unique(), &scheduler);
    }
    assert!(bank.is_complete());
    bank.freeze();
    assert!(bank.is_frozen());
}

#[test]
fn xone_real_bank_freeze_accounts_export() {
    let root = PathBuf::from(
        std::env::var_os("XONE_REAL_BANK_OUTPUT").expect("runner must retain evidence"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let manifests = root.join("manifests");
    std::fs::create_dir_all(&manifests).unwrap();
    assert!(
        !root.join("catalog.json").exists(),
        "use a fresh output directory"
    );
    let changed = Pubkey::new_unique();
    let inherited = Pubkey::new_unique();
    let owner = Pubkey::new_unique();
    let validator = Pubkey::new_unique();
    let mut genesis =
        create_genesis_config_with_leader(1_000_000_000, &validator, 1_000_000_000).genesis_config;
    for (key, data) in [
        (changed, b"parent".to_vec()),
        (inherited, b"inherit".to_vec()),
    ] {
        genesis.accounts.insert(
            key,
            Account {
                lamports: 10_000_000,
                data,
                owner,
                executable: false,
                rent_epoch: 0,
            },
        );
    }
    let genesis_hash = genesis.hash().to_string();
    std::fs::write(
        root.join("catalog.json"),
        serde_json::json!({
            "version":1,"revision":1,"chain_key":"solana:localnet","genesis_hash":genesis_hash,
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        manifests.join("real-bank.request"),
        serde_json::json!({
            "version":1,"chain_key":"solana:localnet","genesis_hash":genesis_hash,
            "source_id":"real-agave-v4.3.0","session_id":"real-bank",
            "catalog_identity":"ab".repeat(32),"catalog_revision":1,
            "scope_accounts":[changed.to_string(),inherited.to_string()],"expires_ms":u64::MAX,
        })
        .to_string(),
    )
    .unwrap();
    let config = SourceConfig {
        chain_key: "solana:localnet".into(),
        genesis_hash: genesis_hash.clone(),
        source_id: "real-agave-v4.3.0".into(),
        catalog_file: root.join("catalog.json"),
        manifest_directory: manifests.clone(),
        max_bytes: 4 * 1024 * 1024,
        max_updates: 1000,
        max_banks: 8,
        max_sessions: 4,
        refs_endpoint: None,
        refs_token_file: None,
    };
    let config_path =
        PathBuf::from(std::env::var_os("XONE_BANK_SOURCE_CONFIG").expect("runner config path"));
    assert_eq!(config_path, root.join("source-config.json"));
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    // Exercise the same env/config initializer called by the patched ledger.
    xone_bank_source::initialize(&genesis_hash);
    assert_eq!(xone_bank_source::configured_keys().len(), 2);
    let notifier = Arc::new(TracedNotifier {
        inner: AccountsUpdateNotifierImpl::new(
            Arc::new(ArcSwap::from_pointee(GeyserPluginManager::default())),
            false,
        ),
        watched: [changed, inherited],
        events: Mutex::new(Vec::new()),
    });
    let root_bank = Bank::new_from_genesis(
        &genesis,
        Arc::new(RuntimeConfig::default()),
        vec![],
        None,
        Default::default(),
        Some(notifier.clone()),
        None,
        Arc::new(AtomicBool::new(false)),
        None,
        None,
    );
    // Production Banks share a real fork graph through their program cache.
    let bank_forks = BankForks::new_rw_arc(root_bank);
    let parent = bank_forks.read().unwrap().root_bank();
    complete_and_freeze(&parent);
    save_native_trace(&root, &notifier);
    assert!(parent.is_frozen());
    {
        let events = notifier.events.lock().unwrap();
        let first = events.first().expect("native genesis callback");
        assert_eq!(first["slot"], 0);
        assert_eq!(first["bank_id"], parent.bank_id());
        assert_eq!(first["pubkey"], changed.to_string());
        assert_eq!(
            first["write_version"], 0,
            "preserve actual upstream zero; no renumbering"
        );
    }
    // More abandoned, genuinely notified Banks than max_banks must not poison
    // the next healthy child. Cleanup must arrive through Bank::drop itself.
    for slot in 10..26 {
        let abandoned = Bank::new_from_parent(parent.clone(), *parent.leader(), slot);
        let mut discarded = abandoned.get_account(&changed).unwrap();
        discarded.set_data_from_slice(b"abandoned");
        abandoned.store_account(&changed, &discarded);
        drop(abandoned);
    }
    let child = Bank::new_from_parent(parent.clone(), *parent.leader(), 1);
    let child = bank_forks
        .write()
        .unwrap()
        .insert(child)
        .clone_without_scheduler();
    let mut account = child.get_account(&changed).unwrap();
    account.set_data_from_slice(b"first");
    child.store_account(&changed, &account);
    account.set_data_from_slice(b"final");
    child.store_account(&changed, &account);
    complete_and_freeze(&child);
    save_native_trace(&root, &notifier);
    let manifest = read_manifest(&manifests, &child);
    assert_eq!(manifest.bank.slot, child.slot());
    assert_eq!(manifest.bank.bank_id, child.bank_id());
    assert_eq!(manifest.bank.blockhash, child.last_blockhash().to_string());
    assert_ne!(manifest.bank.blockhash, manifest.parent.blockhash);
    assert_eq!(manifest.parent.bank_id, parent.bank_id());
    assert_eq!(manifest.parent_accounts.len(), 2);
    let native_zero_parent = manifest
        .parent_accounts
        .iter()
        .find(|a| a.pubkey == changed.to_string())
        .unwrap();
    assert_eq!(native_zero_parent.write_version, 0);
    assert_eq!(native_zero_parent.write_bank_id, parent.bank_id());
    assert_eq!(native_zero_parent.data_base64, "cGFyZW50");
    assert_eq!(manifest.final_versions.len(), 2);
    assert_eq!(manifest.expected_account_updates, 2);
    assert_eq!(manifest.native_writes.len(), 2);
    let writes = &manifest.native_writes;
    assert!(
        writes
            .iter()
            .all(|w| w.pubkey == changed.to_string() && w.write_bank_id == child.bank_id())
    );
    assert_eq!(writes[0].data_base64, "Zmlyc3Q=");
    assert_eq!(writes[1].data_base64, "ZmluYWw=");
    assert!(writes[0].write_version < writes[1].write_version);
    assert_eq!(writes[1].owner, owner.to_string());
    assert_eq!(
        writes[1].executable,
        child.get_account(&changed).unwrap().executable()
    );
    assert_eq!(
        writes[1].rent_epoch,
        child.get_account(&changed).unwrap().rent_epoch()
    );
    assert_eq!(
        writes[1].lamports,
        child.get_account(&changed).unwrap().lamports()
    );
    assert_eq!(writes[1].txn_signature_base64, None); // direct Bank stores have no transaction
    assert!(
        manifest
            .parent_accounts
            .iter()
            .any(|a| a.pubkey == inherited.to_string() && a.data_base64 == "aW5oZXJpdA==")
    );
    assert_eq!(
        manifest
            .final_versions
            .iter()
            .find(|v| v.pubkey == changed.to_string())
            .unwrap()
            .write_version,
        writes[1].write_version
    );
    let path = manifests.join(format!(
        "real-bank-{}-{}.json",
        child.slot(),
        child.bank_id()
    ));
    let before = std::fs::read(&path).unwrap();
    child.freeze(); // regression: freeze's hash write lock must be released before hash()/is_frozen()
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // AccountsDb caches by slot; use legal divergent child slots for live fork isolation.
    let sibling = Bank::new_from_parent(parent.clone(), *parent.leader(), 2);
    let sibling = bank_forks
        .write()
        .unwrap()
        .insert(sibling)
        .clone_without_scheduler();
    assert_ne!(child.bank_id(), sibling.bank_id());
    let mut fork_account = sibling.get_account(&changed).unwrap();
    fork_account.set_data_from_slice(b"fork");
    sibling.store_account(&changed, &fork_account);
    complete_and_freeze(&sibling);
    save_native_trace(&root, &notifier);
    let fork = read_manifest(&manifests, &sibling);
    assert_eq!(fork.parent.bank_id, parent.bank_id());
    assert_eq!(fork.bank.bank_id, sibling.bank_id());
    assert_eq!(fork.expected_account_updates, 1);
    assert_eq!(fork.native_writes[0].data_base64, "Zm9yaw==");
    assert_eq!(child.get_account(&changed).unwrap().data(), b"final");
    assert_eq!(sibling.get_account(&changed).unwrap().data(), b"fork");
    assert_eq!(parent.get_account(&changed).unwrap().data(), b"parent");
    std::fs::write(root.join("real-bank-result.json"), serde_json::json!({
        "status":"passed", "agave":"v4.3.0", "genesis_hash":genesis_hash,
        "parent":{"slot":parent.slot(),"bank_id":parent.bank_id(),"state_hash":parent.hash().to_string(),"blockhash":parent.last_blockhash().to_string(),"complete":parent.is_complete(),"frozen":parent.is_frozen()},
        "child":{"slot":child.slot(),"bank_id":child.bank_id(),"state_hash":child.hash().to_string(),"blockhash":child.last_blockhash().to_string(),"complete":child.is_complete(),"frozen":child.is_frozen()},
        "sibling":{"slot":sibling.slot(),"bank_id":sibling.bank_id(),"state_hash":sibling.hash().to_string(),"blockhash":sibling.last_blockhash().to_string(),"complete":sibling.is_complete(),"frozen":sibling.is_frozen()},
        "changed":changed.to_string(),"inherited":inherited.to_string(),"exported_native_updates":3,
        "bank_reads":{"parent_changed":account_evidence(&parent,&changed),"child_changed":account_evidence(&child,&changed),"sibling_changed":account_evidence(&sibling,&changed),"child_inherited":account_evidence(&child,&inherited)},
        "repeated_freeze":true,"bank_ticks_complete":true,"env_initializer":true,"native_zero_preserved":true,"fork_isolation":true,"scope_accounts":2,"abandoned_unfrozen_banks_dropped":16,
    }).to_string()).unwrap();
}
