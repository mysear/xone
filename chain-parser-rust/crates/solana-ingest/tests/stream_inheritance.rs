use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use xone_solana_ingest::stream::*;
fn source() -> SourceSession {
    SourceSession {
        source_id: "provider-a".into(),
        session_id: "s1".into(),
    }
}
fn bank(slot: u64) -> BankId {
    BankId {
        slot,
        bank_id: 100 + slot,
    }
}
fn anchor(slot: u64) -> BankAnchor {
    BankAnchor {
        bank: bank(slot),
        blockhash: format!("hash-{slot}"),
    }
}
fn parent(slot: u64) -> ParentBank {
    ParentBank {
        slot,
        blockhash: format!("hash-{slot}"),
    }
}
fn limits() -> StreamLimits {
    StreamLimits {
        max_bytes: 200_000,
        max_slots: 8,
        max_updates: 40,
    }
}
fn row(key: u8, slot: u64, version: u64) -> Arc<AccountUpdate> {
    Arc::new(AccountUpdate {
        bank: bank(slot),
        pubkey: [key; 32],
        owner: [9; 32],
        lamports: 100,
        executable: false,
        rent_epoch: 0,
        write_version: version,
        is_startup: false,
        txn_signature: None,
        data: vec![key, version as u8],
    })
}
fn scope() -> BTreeSet<Pubkey> {
    BTreeSet::from([[1; 32], [2; 32]])
}
fn rows() -> BTreeMap<Pubkey, Arc<AccountUpdate>> {
    BTreeMap::from([([1; 32], row(1, 10, 10)), ([2; 32], row(2, 8, 20))])
}
fn snapshot(accounts: BTreeMap<Pubkey, Arc<AccountUpdate>>) -> VersionedAccountSnapshot {
    VersionedAccountSnapshot {
        source: source(),
        anchor: anchor(10),
        accounts,
    }
}
fn snapshot_manifest(accounts: &BTreeMap<Pubkey, Arc<AccountUpdate>>) -> AccountSnapshotManifest {
    AccountSnapshotManifest {
        source: source(),
        anchor: anchor(10),
        final_versions: accounts
            .iter()
            .map(|(key, value)| (*key, ExpectedAccountVersion::of(value)))
            .collect(),
    }
}
fn fresh() -> StreamAssembler {
    let mut a = StreamAssembler::new(limits()).unwrap();
    let r = rows();
    a.anchor_versioned(snapshot(r.clone()), snapshot_manifest(&r), 1, scope(), 1)
        .unwrap();
    a
}
fn send(a: &mut StreamAssembler, seq: u64, event: YellowstoneEvent) -> xone_domain::Result<()> {
    a.admit(
        ProjectedEvent {
            source: source(),
            sequence: seq,
            event,
        },
        Admission::Live { wal_generation: 1 },
    )
}
fn manifest(
    slot: u64,
    parent_slot: u64,
    updates: u64,
    accounts: &BTreeMap<Pubkey, Arc<AccountUpdate>>,
) -> AccountCompletion {
    AccountCompletion::VersionedBankManifest {
        bank: bank(slot),
        parent: parent(parent_slot),
        blockhash: format!("hash-{slot}"),
        expected_account_updates: updates,
        final_versions: accounts
            .iter()
            .map(|(k, v)| (*k, ExpectedAccountVersion::of(v)))
            .collect(),
    }
}
fn headers(a: &mut StreamAssembler, seq: u64, slot: u64, parent_slot: u64) {
    send(
        a,
        seq,
        YellowstoneEvent::Metadata(BlockMetadata {
            bank: bank(slot),
            parent: parent(parent_slot),
            blockhash: format!("hash-{slot}"),
            executed_transaction_count: 0,
            entry_count: 0,
        }),
    )
    .unwrap();
    send(
        a,
        seq + 1,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(slot),
            parent_slot: Some(parent_slot),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
}
fn skip(target: u64) -> SkippedRangeProof {
    SkippedRangeProof {
        source: source(),
        parent: anchor(10),
        target: anchor(target),
        skipped_slots: (11..target).collect(),
    }
}
#[test]
fn two_banks_inherit_only_after_the_exact_barrier_and_preserve_original_write_bank() {
    let original = rows();
    let mut a = StreamAssembler::new(limits()).unwrap();
    a.anchor_accounts(
        snapshot(original.clone()),
        snapshot_manifest(&original),
        1,
        scope(),
        1,
    )
    .unwrap();
    let old_usage = a.usage();
    assert!(old_usage.bytes > 0);
    assert!(!a.ready());
    let changed = row(1, 11, 30);
    let mut expected = original.clone();
    expected.insert([1; 32], changed.clone());
    send(&mut a, 1, YellowstoneEvent::Account((*changed).clone())).unwrap();
    headers(&mut a, 2, 11, 10);
    assert!(a.commit_next().unwrap().is_none());
    assert!(!a.ready());
    a.seal_accounts(&source(), manifest(11, 10, 1, &expected))
        .unwrap();
    let first = a.commit_next().unwrap().unwrap();
    assert!(a.ready());
    assert_eq!(first.account_versions.len(), 1);
    assert!(Arc::ptr_eq(&first.accounts[&[2; 32]], &original[&[2; 32]]));
    assert_eq!(first.accounts[&[2; 32]].bank, bank(8));
    let parent_bytes = a.usage().bytes;
    assert!(parent_bytes < old_usage.bytes + 10_000);
    headers(&mut a, 4, 12, 11);
    assert!(a.commit_next().unwrap().is_none());
    a.seal_accounts(&source(), manifest(12, 11, 0, &expected))
        .unwrap();
    let second = a.commit_next().unwrap().unwrap();
    assert!(second.account_versions.is_empty());
    assert!(Arc::ptr_eq(
        &second.accounts[&[1; 32]],
        &first.accounts[&[1; 32]]
    ));
    assert!(Arc::ptr_eq(&second.accounts[&[2; 32]], &original[&[2; 32]]));
    assert_eq!(second.accounts[&[1; 32]].bank, bank(11));
    assert_eq!(a.usage().slots, 0);
    assert_eq!(a.usage().updates, 2);
    assert!(a.usage().bytes <= parent_bytes + 128);
    a.invalidate();
    assert_eq!(a.usage(), StreamUsage::default());
    assert!(!a.ready());
}
#[test]
fn a_missing_frame_or_changed_manifest_cannot_fall_back_to_parent_accounts() {
    let mut expected = rows();
    expected.insert([1; 32], row(1, 11, 30));
    let mut a = fresh();
    headers(&mut a, 1, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 1, &expected))
        .unwrap();
    assert!(a.commit_next().unwrap().is_none());
    assert!(a.finish_window().is_err());
    assert!(a.needs_reanchor());
    let mut a = fresh();
    headers(&mut a, 1, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 0, &expected))
        .unwrap();
    assert!(a.commit_next().is_err());
    assert!(a.needs_reanchor());
    let mut a = fresh();
    a.seal_accounts(&source(), manifest(11, 10, 0, &rows()))
        .unwrap();
    assert!(a
        .seal_accounts(&source(), manifest(11, 10, 0, &expected))
        .is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn snapshot_scope_identity_content_and_write_bank_must_all_match_independent_manifest() {
    for fault in 0..8 {
        let r = rows();
        let mut snap = snapshot(r.clone());
        let mut proof = snapshot_manifest(&r);
        let mut required = scope();
        match fault {
            0 => proof.source.session_id = "old".into(),
            1 => proof.source.source_id = "other".into(),
            2 => proof.anchor.blockhash = "wrong".into(),
            3 => {
                proof
                    .final_versions
                    .get_mut(&[1; 32])
                    .unwrap()
                    .account_sha256[0] ^= 1;
            }
            4 => {
                snap.accounts.remove(&[2; 32]);
            }
            5 => {
                required.insert([3; 32]);
            }
            6 => {
                snap.accounts.insert([1; 32], row(1, 11, 30));
                proof = snapshot_manifest(&snap.accounts);
            }
            7 => {
                let mut wrong = (*row(1, 10, 10)).clone();
                wrong.bank.bank_id = 999;
                snap.accounts.insert([1; 32], Arc::new(wrong));
                proof = snapshot_manifest(&snap.accounts);
            }
            _ => unreachable!(),
        }
        let mut a = fresh();
        assert!(
            a.anchor_versioned(snap, proof, 1, required, 1).is_err(),
            "fault {fault}"
        );
        assert!(a.needs_reanchor());
        assert_eq!(a.usage(), StreamUsage::default());
    }
}
#[test]
fn source_change_new_scope_and_nonincreasing_write_versions_release_trusted_parent() {
    for fault in 0..4 {
        let mut a = fresh();
        let mut event = ProjectedEvent {
            source: source(),
            sequence: 1,
            event: YellowstoneEvent::Account((*row(1, 11, 30)).clone()),
        };
        match fault {
            0 => event.source.session_id = "new".into(),
            1 => event.source.source_id = "other".into(),
            2 => event.event = YellowstoneEvent::Account((*row(3, 11, 30)).clone()),
            3 => event.event = YellowstoneEvent::Account((*row(1, 11, 10)).clone()),
            _ => unreachable!(),
        }
        assert!(a
            .admit(event, Admission::Live { wal_generation: 1 })
            .is_err());
        assert!(a.needs_reanchor());
        assert_eq!(a.usage(), StreamUsage::default());
    }
    let mut a = fresh();
    assert!(send(&mut a, 2, YellowstoneEvent::Heartbeat).is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn snapshot_payload_capacity_and_both_sets_of_manifest_keys_are_budgeted() {
    let mut r = rows();
    let mut large = (*r[&[1; 32]]).clone();
    large.data = Vec::with_capacity(100_000);
    large.data.push(1);
    r.insert([1; 32], Arc::new(large));
    let mut a = StreamAssembler::new(StreamLimits {
        max_bytes: 20_000,
        ..limits()
    })
    .unwrap();
    assert!(a
        .anchor_versioned(snapshot(r.clone()), snapshot_manifest(&r), 1, scope(), 1)
        .is_err());
    assert_eq!(a.usage(), StreamUsage::default());
    let r = rows();
    let mut proof = snapshot_manifest(&r);
    proof.anchor.blockhash = String::with_capacity(100_000);
    proof.anchor.blockhash.push_str("hash-10");
    assert!(a
        .anchor_versioned(snapshot(r), proof, 1, scope(), 1)
        .is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn verified_skips_allow_the_next_actual_bank_and_inherit_without_fake_sequences() {
    let original = rows();
    let mut a = StreamAssembler::new(limits()).unwrap();
    a.anchor_versioned(
        snapshot(original.clone()),
        snapshot_manifest(&original),
        1,
        scope(),
        1,
    )
    .unwrap();
    a.prove_skipped(skip(13)).unwrap();
    let after_proof = a.usage();
    assert_eq!(after_proof.slots, 2);
    a.prove_skipped(skip(13)).unwrap();
    assert_eq!(a.usage(), after_proof);
    let changed = row(1, 13, 30);
    let mut expected = original.clone();
    expected.insert([1; 32], changed.clone());
    send(&mut a, 1, YellowstoneEvent::Account((*changed).clone())).unwrap();
    headers(&mut a, 2, 13, 10);
    a.seal_accounts(&source(), manifest(13, 10, 1, &expected))
        .unwrap();
    let block = a.commit_next().unwrap().unwrap();
    assert_eq!(block.parent, anchor(10));
    assert_eq!(block.bank, bank(13));
    assert!(Arc::ptr_eq(&block.accounts[&[2; 32]], &original[&[2; 32]]));
    assert_eq!(a.usage().slots, 0);
    assert!(a.usage().bytes < after_proof.bytes + 1024);
    headers(&mut a, 4, 14, 13);
    a.seal_accounts(&source(), manifest(14, 13, 0, &expected))
        .unwrap();
    assert!(a.commit_next().unwrap().is_some());
    assert!(a.ready());
}
#[test]
fn absent_skip_proof_never_turns_a_numeric_gap_into_a_commit() {
    let mut a = fresh();
    let staged = send(
        &mut a,
        1,
        YellowstoneEvent::Metadata(BlockMetadata {
            bank: bank(13),
            parent: parent(10),
            blockhash: "hash-13".into(),
            executed_transaction_count: 0,
            entry_count: 0,
        }),
    );
    if staged.is_ok() {
        send(
            &mut a,
            2,
            YellowstoneEvent::Slot(SlotUpdate {
                bank: bank(13),
                parent_slot: Some(10),
                status: SlotStatus::Finalized,
            }),
        )
        .unwrap();
        a.seal_accounts(&source(), manifest(13, 10, 0, &rows()))
            .unwrap();
        assert!(a.commit_next().is_err() || !a.ready());
    }
    assert!(!a.ready());
}
#[test]
fn invalid_skipped_claims_reanchor_on_holes_duplicates_identity_or_anchor_mismatch() {
    for fault in 0..9 {
        let mut proof = skip(14);
        match fault {
            0 => proof.source.session_id = "old".into(),
            1 => proof.source.source_id = "other".into(),
            2 => proof.parent.blockhash = "wrong".into(),
            3 => proof.parent.bank.bank_id = 999,
            4 => {
                proof.skipped_slots.remove(1);
            }
            5 => proof.skipped_slots = vec![11, 11, 13],
            6 => proof.skipped_slots = vec![11, 13, 12],
            7 => proof.skipped_slots = vec![12, 13, 14],
            8 => proof.target.bank.slot = 15,
            _ => unreachable!(),
        }
        let mut a = fresh();
        assert!(a.prove_skipped(proof).is_err(), "fault {fault}");
        assert!(a.needs_reanchor());
        assert_eq!(a.usage(), StreamUsage::default());
    }
}
#[test]
fn skipped_and_real_banks_are_mutually_exclusive_in_either_arrival_order() {
    let mut a = fresh();
    send(
        &mut a,
        1,
        YellowstoneEvent::Account((*row(1, 11, 30)).clone()),
    )
    .unwrap();
    assert!(a.prove_skipped(skip(13)).is_err());
    assert!(a.needs_reanchor());
    let mut a = fresh();
    a.prove_skipped(skip(13)).unwrap();
    assert!(send(
        &mut a,
        1,
        YellowstoneEvent::Account((*row(1, 12, 30)).clone())
    )
    .is_err());
    assert!(a.needs_reanchor());
    let mut a = fresh();
    a.prove_skipped(skip(13)).unwrap();
    let mut other = (*row(1, 13, 30)).clone();
    other.bank.bank_id = 999;
    assert!(send(&mut a, 1, YellowstoneEvent::Account(other)).is_err());
    assert!(a.needs_reanchor());
    let mut a = fresh();
    a.prove_skipped(skip(13)).unwrap();
    let mut bad = skip(13);
    bad.target.blockhash = "other-target".into();
    assert!(a.prove_skipped(bad).is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn skipped_capacity_and_complete_range_are_bounded_and_released() {
    let mut a = StreamAssembler::new(StreamLimits {
        max_slots: 2,
        ..limits()
    })
    .unwrap();
    let r = rows();
    a.anchor_versioned(snapshot(r.clone()), snapshot_manifest(&r), 1, scope(), 1)
        .unwrap();
    assert!(a.prove_skipped(skip(14)).is_err());
    assert_eq!(a.usage(), StreamUsage::default());
    let mut a = fresh();
    let mut claim = skip(13);
    let mut values = Vec::with_capacity(100_000);
    values.extend([11, 12]);
    claim.skipped_slots = values;
    assert!(a.prove_skipped(claim).is_err());
    assert_eq!(a.usage(), StreamUsage::default());
    let mut a = fresh();
    a.prove_skipped(skip(13)).unwrap();
    assert!(a.finish_window().is_err());
    assert_eq!(a.usage(), StreamUsage::default());
}
#[test]
fn metadata_and_account_frames_may_precede_the_independent_skip_proof() {
    let mut a = fresh();
    headers(&mut a, 1, 13, 10);
    a.seal_accounts(&source(), manifest(13, 10, 0, &rows()))
        .unwrap();
    a.prove_skipped(skip(13)).unwrap();
    let b = a.commit_next().unwrap().unwrap();
    assert_eq!(b.bank, bank(13));
    assert!(a.ready());
}

#[test]
fn old_anchor_stays_strict_and_invalidating_releases_snapshot_arcs() {
    let mut a = fresh();
    a.anchor(anchor(10), source(), 1, scope(), 1).unwrap();
    headers(&mut a, 1, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 0, &rows()))
        .unwrap();
    assert!(a.commit_next().is_err());
    assert!(a.needs_reanchor());
    let r = rows();
    let weak = Arc::downgrade(&r[&[1; 32]]);
    let proof = snapshot_manifest(&r);
    a.anchor_versioned(snapshot(r), proof, 1, scope(), 1)
        .unwrap();
    assert!(weak.upgrade().is_some());
    a.invalidate();
    assert!(weak.upgrade().is_none());
    assert_eq!(a.usage(), StreamUsage::default());
}
#[test]
fn staged_next_bank_versions_must_remain_comparable_to_the_new_parent() {
    let mut a = fresh();
    let current = row(1, 11, 40);
    let future = row(1, 12, 30);
    let mut expected = rows();
    expected.insert([1; 32], current.clone());
    send(&mut a, 1, YellowstoneEvent::Account((*future).clone())).unwrap();
    send(&mut a, 2, YellowstoneEvent::Account((*current).clone())).unwrap();
    headers(&mut a, 3, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 1, &expected))
        .unwrap();
    assert!(a.commit_next().is_err());
    assert_eq!(a.tip(), Some(&bank(10)));
    assert!(a.needs_reanchor());
}
#[test]
fn skipped_proof_must_leave_capacity_for_its_target_bank() {
    let mut a = StreamAssembler::new(StreamLimits {
        max_slots: 2,
        ..limits()
    })
    .unwrap();
    let r = rows();
    a.anchor_versioned(snapshot(r.clone()), snapshot_manifest(&r), 1, scope(), 1)
        .unwrap();
    assert!(a.prove_skipped(skip(13)).is_err());
    assert_eq!(a.usage(), StreamUsage::default());
}

#[test]
fn a_staged_gap_must_not_skip_over_a_newly_committed_actual_parent() {
    let mut a = fresh();
    headers(&mut a, 1, 13, 10);
    headers(&mut a, 3, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 0, &rows()))
        .unwrap();
    assert!(a.commit_next().is_err());
    assert!(a.needs_reanchor());
    assert_eq!(a.tip(), Some(&bank(10)));
}
#[test]
fn inherited_accounts_and_verified_skips_do_not_promote_replay_to_live() {
    let mut a = fresh();
    a.prove_skipped(skip(13)).unwrap();
    for (sequence, event) in [
        (
            1,
            YellowstoneEvent::Metadata(BlockMetadata {
                bank: bank(13),
                parent: parent(10),
                blockhash: "hash-13".into(),
                executed_transaction_count: 0,
                entry_count: 0,
            }),
        ),
        (
            2,
            YellowstoneEvent::Slot(SlotUpdate {
                bank: bank(13),
                parent_slot: Some(10),
                status: SlotStatus::Finalized,
            }),
        ),
    ] {
        a.admit(
            ProjectedEvent {
                source: source(),
                sequence,
                event,
            },
            Admission::Replay,
        )
        .unwrap();
        assert!(!a.ready());
    }
    a.seal_accounts(&source(), manifest(13, 10, 0, &rows()))
        .unwrap();
    assert!(!a.ready());
    let b = a.commit_next().unwrap().unwrap();
    assert_eq!(b.accounts[&[2; 32]].bank, bank(8));
    assert!(!a.ready());
    assert!(send(&mut a, 3, YellowstoneEvent::Heartbeat).is_err());
    assert!(a.needs_reanchor());
    assert_eq!(a.usage(), StreamUsage::default());
}

#[test]
fn an_unproven_gap_revokes_ready_before_a_commit_is_attempted() {
    let mut a = fresh();
    headers(&mut a, 1, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 0, &rows()))
        .unwrap();
    a.commit_next().unwrap().unwrap();
    assert!(a.ready());
    send(
        &mut a,
        3,
        YellowstoneEvent::Metadata(BlockMetadata {
            bank: bank(14),
            parent: parent(11),
            blockhash: "hash-14".into(),
            executed_transaction_count: 0,
            entry_count: 0,
        }),
    )
    .unwrap();
    assert!(!a.ready());
    assert!(!a.needs_reanchor());
    a.prove_skipped(SkippedRangeProof {
        source: source(),
        parent: anchor(11),
        target: anchor(14),
        skipped_slots: vec![12, 13],
    })
    .unwrap();
    // A skip proof closes the empty interval, not the target bank's barriers.
    assert!(!a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
    assert!(a.commit_next().unwrap().is_none());
    a.seal_accounts(&source(), manifest(14, 11, 0, &rows()))
        .unwrap();
    assert!(!a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
    assert!(a.commit_next().unwrap().is_none());
    send(
        &mut a,
        4,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(14),
            parent_slot: Some(11),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(!a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
    assert!(a.commit_next().unwrap().is_some());
    assert_eq!(a.tip(), Some(&bank(14)));
    assert!(a.ready());
}

#[test]
fn higher_finalized_bank_blocks_ready_until_its_independent_barriers_commit() {
    let mut a = fresh();
    headers(&mut a, 1, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 0, &rows()))
        .unwrap();
    let old = a.commit_next().unwrap().unwrap();
    assert!(a.ready());
    // A finalized observation may precede both metadata and the independent manifest.
    send(
        &mut a,
        3,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(12),
            parent_slot: Some(11),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(!a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
    assert!(a.commit_next().unwrap().is_none());
    send(
        &mut a,
        4,
        YellowstoneEvent::Metadata(BlockMetadata {
            bank: bank(12),
            parent: parent(11),
            blockhash: "hash-12".into(),
            executed_transaction_count: 0,
            entry_count: 0,
        }),
    )
    .unwrap();
    assert!(!a.ready());
    assert!(a.commit_next().unwrap().is_none());
    assert_eq!(a.tip(), Some(&bank(11)));
    assert!(!a.needs_reanchor());
    a.seal_accounts(&source(), manifest(12, 11, 0, &old.accounts))
        .unwrap();
    // Complete input alone is insufficient: the actual ordered commit must happen.
    assert!(!a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
    let next = a.commit_next().unwrap().unwrap();
    assert!(a.ready());
    assert_eq!(a.tip(), Some(&bank(12)));
    assert!(Arc::ptr_eq(
        &old.accounts[&[2; 32]],
        &next.accounts[&[2; 32]]
    ));
}

#[test]
fn merely_processed_future_frames_preserve_the_tagged_immutable_old_tip() {
    let mut a = fresh();
    headers(&mut a, 1, 11, 10);
    a.seal_accounts(&source(), manifest(11, 10, 0, &rows()))
        .unwrap();
    let old = a.commit_next().unwrap().unwrap();
    assert!(a.ready());
    let changed = row(1, 12, 30);
    send(&mut a, 3, YellowstoneEvent::Account((*changed).clone())).unwrap();
    send(
        &mut a,
        4,
        YellowstoneEvent::Metadata(BlockMetadata {
            bank: bank(12),
            parent: parent(11),
            blockhash: "hash-12".into(),
            executed_transaction_count: 0,
            entry_count: 0,
        }),
    )
    .unwrap();
    send(
        &mut a,
        5,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(12),
            parent_slot: Some(11),
            status: SlotStatus::Processed,
        }),
    )
    .unwrap();
    assert!(a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
    assert!(a.commit_next().unwrap().is_none());
    assert_eq!(old.bank, bank(11));
    assert_eq!(old.accounts[&[1; 32]].bank, bank(10));
    assert_eq!(old.accounts[&[1; 32]].write_version, 10);
    assert_eq!(old.accounts[&[1; 32]].data, vec![1, 10]);
    let mut expected = old.accounts.clone();
    expected.insert([1; 32], changed);
    a.seal_accounts(&source(), manifest(12, 11, 1, &expected))
        .unwrap();
    assert!(a.ready());
    assert!(a.commit_next().unwrap().is_none());
    send(
        &mut a,
        6,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(12),
            parent_slot: Some(11),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(!a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
    let next = a.commit_next().unwrap().unwrap();
    assert!(a.ready());
    assert_eq!(a.tip(), Some(&bank(12)));
    assert_eq!(next.accounts[&[1; 32]].write_version, 30);
    assert_eq!(old.accounts[&[1; 32]].write_version, 10);
    assert_eq!(old.accounts[&[1; 32]].data, vec![1, 10]);
}
