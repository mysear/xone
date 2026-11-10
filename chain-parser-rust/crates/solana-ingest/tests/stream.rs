use std::collections::{BTreeMap, BTreeSet};
use xone_solana_ingest::stream::*;

fn source(session: &str) -> SourceSession {
    SourceSession {
        source_id: "provider-a".into(),
        session_id: session.into(),
    }
}
fn bank(slot: u64) -> BankId {
    BankId {
        slot,
        bank_id: slot,
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
        max_bytes: 100_000,
        max_slots: 3,
        max_updates: 20,
    }
}
fn account(slot: u64, version: u64, byte: u8) -> AccountUpdate {
    AccountUpdate {
        bank: bank(slot),
        pubkey: [1; 32],
        owner: [2; 32],
        lamports: 1,
        executable: false,
        rent_epoch: 0,
        write_version: version,
        is_startup: false,
        txn_signature: None,
        data: vec![byte],
    }
}
fn tx(slot: u64) -> TransactionUpdate {
    TransactionUpdate {
        bank: bank(slot),
        index: 0,
        is_vote: false,
        signature: [3; 64],
        transaction: vec![4],
        meta: vec![5],
    }
}
fn meta(slot: u64) -> BlockMetadata {
    BlockMetadata {
        bank: bank(slot),
        parent: parent(slot - 1),
        blockhash: format!("hash-{slot}"),
        executed_transaction_count: 1,
        entry_count: 0,
    }
}
fn seal(slot: u64, version: u64, count: u64) -> AccountCompletion {
    AccountCompletion::VersionedBankManifest {
        bank: bank(slot),
        parent: parent(slot - 1),
        blockhash: format!("hash-{slot}"),
        expected_account_updates: count,
        final_versions: BTreeMap::from([(
            [1; 32],
            ExpectedAccountVersion::of(&account(slot, version, version as u8)),
        )]),
    }
}
fn fresh() -> StreamAssembler {
    let mut a = StreamAssembler::new(limits()).unwrap();
    a.anchor(anchor(10), source("s1"), 1, BTreeSet::from([[1; 32]]), 1)
        .unwrap();
    a
}
fn send(
    a: &mut StreamAssembler,
    sequence: u64,
    event: YellowstoneEvent,
) -> xone_domain::Result<()> {
    a.admit(
        ProjectedEvent {
            source: source("s1"),
            sequence,
            event,
        },
        Admission::Live { wal_generation: 1 },
    )
}
fn finish(a: &mut StreamAssembler, start: u64, slot: u64, version: u64, count: u64) {
    send(a, start, YellowstoneEvent::Metadata(meta(slot))).unwrap();
    send(a, start + 1, YellowstoneEvent::Transaction(tx(slot))).unwrap();
    send(
        a,
        start + 2,
        YellowstoneEvent::AccountsComplete(seal(slot, version, count)),
    )
    .unwrap();
    send(
        a,
        start + 3,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(slot),
            parent_slot: Some(slot - 1),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
}
#[test]
fn out_of_order_versions_counts_and_explicit_barrier_commit() {
    let mut a = fresh();
    send(
        &mut a,
        1,
        YellowstoneEvent::AccountsComplete(seal(11, 9, 2)),
    )
    .unwrap();
    send(&mut a, 2, YellowstoneEvent::Account(account(11, 9, 9))).unwrap();
    send(&mut a, 3, YellowstoneEvent::Metadata(meta(11))).unwrap();
    send(
        &mut a,
        4,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    send(&mut a, 5, YellowstoneEvent::Transaction(tx(11))).unwrap();
    assert!(a.commit_next().unwrap().is_none());
    send(&mut a, 6, YellowstoneEvent::Account(account(11, 2, 2))).unwrap();
    let committed = a.commit_next().unwrap().unwrap();
    assert_eq!(committed.accounts[&[1; 32]].write_version, 9);
    assert_eq!(committed.account_versions.len(), 2);
    assert!(a.ready());
    assert_eq!(a.tip(), Some(&bank(11)));
}
#[test]
fn transactions_and_metadata_never_prove_accounts_complete() {
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    send(&mut a, 2, YellowstoneEvent::Metadata(meta(11))).unwrap();
    send(&mut a, 3, YellowstoneEvent::Transaction(tx(11))).unwrap();
    send(
        &mut a,
        4,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(a.commit_next().unwrap().is_none());
    assert!(!a.ready());
    assert_eq!(
        send(
            &mut a,
            5,
            YellowstoneEvent::AccountsComplete(AccountCompletion::UnknownYellowstoneMetadata {
                bank: bank(11)
            })
        )
        .unwrap_err()
        .0,
        "STREAM_ACCOUNT_COMPLETION_UNPROVEN"
    );
    assert!(a.needs_reanchor());
}
#[test]
fn inconsistent_duplicates_and_source_sequences_poison_until_new_anchor() {
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    send(&mut a, 2, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    assert_eq!(a.usage().updates, 1);
    assert!(send(&mut a, 3, YellowstoneEvent::Account(account(11, 1, 2))).is_err());
    assert!(a.needs_reanchor());
    assert_eq!(a.usage().updates, 0);
    let mut a = fresh();
    assert!(send(&mut a, 2, YellowstoneEvent::Metadata(meta(11))).is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn all_fork_session_and_parent_ambiguity_reanchors() {
    for event in [
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Dead,
        }),
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Skipped,
        }),
        YellowstoneEvent::Metadata(BlockMetadata {
            parent: parent(9),
            ..meta(11)
        }),
        YellowstoneEvent::Reconnect,
    ] {
        let mut a = fresh();
        assert!(send(&mut a, 1, event).is_err());
        assert!(a.needs_reanchor());
    }
    let mut a = fresh();
    let mut event = ProjectedEvent {
        source: source("s2"),
        sequence: 1,
        event: YellowstoneEvent::Account(account(11, 1, 1)),
    };
    assert!(a
        .admit(event.clone(), Admission::Live { wal_generation: 1 })
        .is_err());
    event.source.source_id = "provider-b".into();
    assert!(a
        .admit(event, Admission::Live { wal_generation: 1 })
        .is_err());
    assert_eq!(
        compare_write_versions(&source("s1"), 1, &source("s2"), 99),
        None
    );
    assert_eq!(
        compare_write_versions(&source("s1"), 1, &source("s1"), 99),
        Some(std::cmp::Ordering::Less)
    );
}
#[test]
fn bounded_memory_includes_versions_metadata_and_payload_bytes() {
    for small in [
        StreamLimits {
            max_bytes: 1,
            ..limits()
        },
        StreamLimits {
            max_updates: 1,
            ..limits()
        },
        StreamLimits {
            max_slots: 1,
            ..limits()
        },
    ] {
        let mut a = StreamAssembler::new(small).unwrap();
        if a.anchor(anchor(10), source("s1"), 1, BTreeSet::from([[1; 32]]), 1)
            .is_err()
        {
            assert!(a.needs_reanchor());
            continue;
        }
        let first = send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1)));
        let second = if first.is_ok() {
            send(&mut a, 2, YellowstoneEvent::Account(account(12, 2, 2)))
        } else {
            first
        };
        assert!(second.is_err());
        assert!(a.needs_reanchor());
        assert_eq!(a.usage().bytes, 0);
    }
}
#[test]
fn fork_versions_are_isolated_and_finalized_conflict_is_rejected() {
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    let mut other = account(11, 1, 2);
    other.bank.bank_id = 111;
    send(&mut a, 2, YellowstoneEvent::Account(other)).unwrap();
    assert_eq!(a.usage().slots, 2);
    assert_eq!(a.usage().updates, 2);
    finish(&mut a, 3, 11, 1, 1);
    assert!(a.commit_next().unwrap().is_some());
    assert_eq!(a.usage().slots, 0);
    let mut late = account(11, 2, 2);
    late.bank.bank_id = 111;
    assert!(send(&mut a, 7, YellowstoneEvent::Account(late)).is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn completed_counts_with_missing_scope_or_wrong_final_version_reject() {
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    finish(&mut a, 2, 11, 2, 1);
    assert!(a.commit_next().is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn replay_can_reconstruct_but_cannot_restore_online_readiness() {
    let mut a = fresh();
    for (index, event) in [
        YellowstoneEvent::Account(account(11, 1, 1)),
        YellowstoneEvent::Metadata(meta(11)),
        YellowstoneEvent::Transaction(tx(11)),
        YellowstoneEvent::AccountsComplete(seal(11, 1, 1)),
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    ]
    .into_iter()
    .enumerate()
    {
        a.admit(
            ProjectedEvent {
                source: source("s1"),
                sequence: index as u64 + 1,
                event,
            },
            Admission::Replay,
        )
        .unwrap();
    }
    assert!(a.commit_next().unwrap().is_some());
    assert!(!a.ready());
    assert!(send(&mut a, 6, YellowstoneEvent::Account(account(12, 2, 2))).is_err());
}
#[test]
fn reconnect_gap_plans_use_window_and_preserve_head_first_history_boundary() {
    assert_eq!(
        plan_gap(10, 13, Some(11), 5).unwrap(),
        GapPlan::ReplayWindow {
            from_slot: 11,
            through_slot: 13
        }
    );
    assert_eq!(
        plan_gap(10, 30, Some(11), 5).unwrap(),
        GapPlan::HeadFirst {
            history: HistoricalGap {
                from_slot: 11,
                through_slot: 30
            }
        }
    );
    assert!(matches!(
        plan_gap(10, 13, Some(12), 5).unwrap(),
        GapPlan::HeadFirst { .. }
    ));
    assert_eq!(plan_gap(10, 10, None, 1).unwrap(), GapPlan::NoGap);
}

#[test]
fn spare_vec_capacity_and_anchor_scope_are_in_the_total_byte_budget() {
    let mut a = StreamAssembler::new(StreamLimits {
        max_bytes: 5000,
        ..limits()
    })
    .unwrap();
    a.anchor(anchor(10), source("s1"), 1, BTreeSet::from([[1; 32]]), 1)
        .unwrap();
    let mut update = account(11, 1, 1);
    update.data = Vec::with_capacity(20_000);
    update.data.push(1);
    assert!(send(&mut a, 1, YellowstoneEvent::Account(update)).is_err());
    assert!(a.needs_reanchor());
    let mut a = StreamAssembler::new(StreamLimits {
        max_bytes: 1,
        ..limits()
    })
    .unwrap();
    assert!(a
        .anchor(anchor(10), source("s1"), 1, BTreeSet::from([[1; 32]]), 1)
        .is_err());
}
#[test]
fn dangling_losing_fork_metadata_parent_invalidates_before_tip_moves() {
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    let mut fork = account(11, 2, 2);
    fork.bank.bank_id = 111;
    send(&mut a, 2, YellowstoneEvent::Account(fork.clone())).unwrap();
    send(
        &mut a,
        3,
        YellowstoneEvent::Metadata(BlockMetadata {
            bank: fork.bank,
            blockhash: "losing".into(),
            ..meta(11)
        }),
    )
    .unwrap();
    send(
        &mut a,
        4,
        YellowstoneEvent::Metadata(BlockMetadata {
            parent: ParentBank {
                slot: 11,
                blockhash: "losing".into(),
            },
            ..meta(12)
        }),
    )
    .unwrap();
    finish(&mut a, 5, 11, 1, 1);
    assert!(a.commit_next().is_err());
    assert!(a.needs_reanchor());
    assert_eq!(a.tip(), Some(&bank(10)));
}

#[test]
fn sealed_zero_counts_do_not_accept_an_unknown_or_missing_account_scope() {
    let mut a = fresh();
    send(
        &mut a,
        1,
        YellowstoneEvent::AccountsComplete(seal(11, 1, 0)),
    )
    .unwrap();
    send(
        &mut a,
        2,
        YellowstoneEvent::Metadata(BlockMetadata {
            executed_transaction_count: 0,
            ..meta(11)
        }),
    )
    .unwrap();
    send(
        &mut a,
        3,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(a.commit_next().is_err());
    assert!(a.needs_reanchor());
}
#[test]
fn an_explicit_window_end_with_missing_updates_always_reanchors() {
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    finish(&mut a, 2, 11, 2, 2);
    assert!(a.commit_next().unwrap().is_none());
    assert!(a.finish_window().is_err());
    assert!(a.needs_reanchor());
    assert!(!a.ready());
}

#[test]
fn native_full_block_is_an_atomic_provider_event_set_without_a_bank_scope_proof() {
    let mut a = fresh();
    send(
        &mut a,
        1,
        YellowstoneEvent::ProviderBlock(ProviderBlock {
            metadata: meta(11),
            entries: vec![],
            updated_account_count: 1,
            accounts: vec![account(11, 1, 1)],
            transactions: vec![tx(11)],
        }),
    )
    .unwrap();
    send(
        &mut a,
        2,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(a.commit_next().unwrap().is_none());
    assert!(!a.ready());
    send(
        &mut a,
        3,
        YellowstoneEvent::AccountsComplete(seal(11, 1, 1)),
    )
    .unwrap();
    assert_eq!(a.commit_next().unwrap().unwrap().bank.bank_id, 11);
}
#[test]
fn a_truncated_or_duplicate_full_block_never_advances_the_bank() {
    for block in [
        ProviderBlock {
            metadata: meta(11),
            entries: vec![],
            updated_account_count: 2,
            accounts: vec![account(11, 1, 1)],
            transactions: vec![tx(11)],
        },
        ProviderBlock {
            metadata: meta(11),
            entries: vec![],
            updated_account_count: 2,
            accounts: vec![account(11, 1, 1), account(11, 1, 1)],
            transactions: vec![tx(11)],
        },
        ProviderBlock {
            metadata: meta(11),
            entries: vec![],
            updated_account_count: 1,
            accounts: vec![account(11, 1, 1)],
            transactions: vec![],
        },
    ] {
        let mut a = fresh();
        assert!(send(&mut a, 1, YellowstoneEvent::ProviderBlock(block)).is_err());
        assert!(a.needs_reanchor());
        assert_eq!(a.tip(), Some(&bank(10)));
    }
}
#[test]
fn full_block_can_contain_other_accounts_but_scope_requires_its_own_manifest() {
    let mut other = account(11, 2, 2);
    other.pubkey = [9; 32];
    let mut a = fresh();
    send(
        &mut a,
        1,
        YellowstoneEvent::ProviderBlock(ProviderBlock {
            metadata: meta(11),
            entries: vec![],
            updated_account_count: 2,
            accounts: vec![account(11, 1, 1), other],
            transactions: vec![tx(11)],
        }),
    )
    .unwrap();
    send(
        &mut a,
        2,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    send(
        &mut a,
        3,
        YellowstoneEvent::AccountsComplete(seal(11, 1, 1)),
    )
    .unwrap();
    assert_eq!(a.commit_next().unwrap().unwrap().accounts.len(), 1);
}
#[test]
fn parent_blockhash_and_bank_id_are_both_checked() {
    let mut a = fresh();
    let mut m = meta(11);
    m.parent.blockhash = "wrong".into();
    assert!(send(&mut a, 1, YellowstoneEvent::Metadata(m)).is_err());
    assert!(a.needs_reanchor());
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    let mut m = meta(11);
    m.bank.bank_id = 999;
    send(&mut a, 2, YellowstoneEvent::Metadata(m)).unwrap();
    send(&mut a, 3, YellowstoneEvent::Transaction(tx(11))).unwrap();
    send(
        &mut a,
        4,
        YellowstoneEvent::AccountsComplete(seal(11, 1, 1)),
    )
    .unwrap();
    send(
        &mut a,
        5,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(a.commit_next().unwrap().is_none());
    assert!(a.finish_window().is_err());
}

#[test]
fn manifest_binds_exact_account_state_not_only_a_write_version_number() {
    let mut a = fresh();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 9))).unwrap();
    finish(&mut a, 2, 11, 1, 1);
    assert!(a.commit_next().is_err());
    assert!(a.needs_reanchor());
}

#[test]
fn external_account_manifest_does_not_consume_a_fictional_geyser_frame_sequence() {
    let mut a = fresh();
    a.seal_accounts(&source("s1"), seal(11, 1, 1)).unwrap();
    send(&mut a, 1, YellowstoneEvent::Account(account(11, 1, 1))).unwrap();
    send(&mut a, 2, YellowstoneEvent::Transaction(tx(11))).unwrap();
    send(&mut a, 3, YellowstoneEvent::Metadata(meta(11))).unwrap();
    send(
        &mut a,
        4,
        YellowstoneEvent::Slot(SlotUpdate {
            bank: bank(11),
            parent_slot: Some(10),
            status: SlotStatus::Finalized,
        }),
    )
    .unwrap();
    assert!(a.commit_next().unwrap().is_some());
    assert!(a.ready());
}
