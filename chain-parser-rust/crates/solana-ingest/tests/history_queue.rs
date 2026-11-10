use std::sync::{Arc, Mutex};
use xone_solana_ingest::{history_queue::*, store::Store};
static NEXT_HISTORY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn fixture(limit: HistoryLimits) -> (std::path::PathBuf, Arc<Mutex<Store>>, HistoryQueue) {
    let path = std::env::temp_dir().join(format!(
        "xone-history-{}-{}-{}",
        std::process::id(),
        NEXT_HISTORY.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = Arc::new(Mutex::new(
        Store::open(&path, "chain-parser-rust", 1).unwrap(),
    ));
    let registration = HistoryRegistration::new(
        "solana:localnet",
        "localnet",
        "http://127.0.0.1:1",
        true,
        limit,
    )
    .unwrap();
    let queue = HistoryQueue::open(db.clone(), registration).unwrap();
    (path, db, queue)
}
fn baseline(
    db: &Mutex<Store>,
    generation: u64,
    epoch: u64,
    scope: &str,
    head: u64,
    gaps: Vec<xone_domain::Gap>,
) {
    let mut s: xone_domain::Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/solana/baseline.json")).unwrap();
    s.reference.online_generation = generation;
    s.reference.journal_seq = generation;
    s.reference.canonical_epoch = epoch;
    s.reference.block.number_or_slot = head;
    s.reference.block.timestamp_ms = 1000;
    if scope == "scope-b" {
        s.pools.truncate(1);
    }
    s.reference.scope_hash = xone_domain::scope_hash(&s.pools).unwrap();
    s.history_gaps = gaps;
    db.lock().unwrap().publish_baseline(s).unwrap();
}
#[test]
fn huge_interval_is_one_durable_job_and_old_scope_claim_is_fenced() {
    let (path, db, q) = fixture(HistoryLimits::default());
    baseline(
        &db,
        1,
        1,
        "scope-a",
        1_000_000_000,
        vec![xone_domain::Gap {
            from: 1,
            to: 999_999_999,
            reason: "HEAD_FIRST".into(),
        }],
    );
    let status = q.status().unwrap();
    assert_eq!(status.jobs.len(), 1);
    assert_eq!(status.jobs[0].next, Some(1));
    let claim = q.claim(1000).unwrap().unwrap();
    baseline(
        &db,
        2,
        1,
        "scope-b",
        1_000_000_001,
        vec![xone_domain::Gap {
            from: 1,
            to: 999_999_999,
            reason: "PARTIAL_SCOPE".into(),
        }],
    );
    assert!(q.retry(&claim, 1001, "late-old-dataset").is_err());
    assert_ne!(q.status().unwrap().epoch, claim.epoch);
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn history_count_budget_and_pause_cannot_block_online_or_grant_ready() {
    let limits = HistoryLimits {
        max_jobs: 1,
        ..HistoryLimits::default()
    };
    let (path, db, q) = fixture(limits);
    baseline(
        &db,
        1,
        1,
        "scope",
        100,
        vec![
            xone_domain::Gap {
                from: 1,
                to: 10,
                reason: "first".into(),
            },
            xone_domain::Gap {
                from: 20,
                to: 30,
                reason: "second".into(),
            },
        ],
    );
    assert!(db.lock().unwrap().current().is_some());
    assert_eq!(q.status().unwrap().last_error, "HISTORY_QUEUE_BUDGET");
    q.set_paused(true).unwrap();
    assert!(q.claim(1000).unwrap().is_none());
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

fn empty_page(c: &HistoryClaim) -> HistoryPage {
    use serde_json::json;
    let response =
        |method: &str, params: serde_json::Value, result: serde_json::Value| RpcReceipt {
            method: method.into(),
            params,
            response: serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":result})).unwrap(),
        };
    let head = json!({"blockhash":c.head.hash,"previousBlockhash":c.head.parent_hash,"parentSlot":0,"blockTime":1});
    let hp = json!([c.head.slot,{"encoding":"json","transactionDetails":"none","commitment":"finalized"}]);
    HistoryPage {
        dataset: c.dataset.clone(),
        canonical_revision: c.canonical_revision,
        history_epoch: c.epoch.clone(),
        verification: "PUBLIC_RPC_FINALIZED_TRANSACTIONS_META_RAW_V1".into(),
        historical_accounts_complete: false,
        skipped: vec![xone_domain::Gap {
            from: c.from,
            to: c.to,
            reason: "GETBLOCKS_FINALIZED_AT_BOUND_HEAD".into(),
        }],
        genesis_hash: c.dataset.genesis_hash.clone(),
        source_fingerprint: c.dataset.source_fingerprint.clone(),
        from: c.from,
        to: c.to,
        head: c.head.clone(),
        produced: vec![],
        raw: vec![
            response("getGenesisHash", json!([]), json!(c.dataset.genesis_hash)),
            response(
                "getSlot",
                json!([{"commitment":"finalized"}]),
                json!(c.head.slot),
            ),
            response("getBlock", hp.clone(), head.clone()),
            response(
                "getBlocks",
                json!([c.from,c.to,{"commitment":"finalized"}]),
                json!([]),
            ),
            response("getBlock", hp, head),
            response(
                "getSlot",
                json!([{"commitment":"finalized"}]),
                json!(c.head.slot),
            ),
            response("getGenesisHash", json!([]), json!(c.dataset.genesis_hash)),
        ],
    }
}
#[test]
fn restart_cursor_lease_cancel_and_renew_are_persistent_and_fenced() {
    let (path, db, q) = fixture(HistoryLimits {
        page_slots: 2,
        ..HistoryLimits::default()
    });
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 6,
            reason: "restart".into(),
        }],
    );
    let now = xone_observability::now_ms();
    let first = q.claim(now).unwrap().unwrap();
    let renewed = q.renew(&first, now + 1).unwrap();
    assert!(q.cancel(&first, now + 1).is_err());
    q.complete(&renewed, empty_page(&renewed), now + 2).unwrap();
    assert_eq!(q.status().unwrap().jobs[0].next, Some(3));
    let in_flight = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    let registration = HistoryRegistration::new(
        "solana:localnet",
        "localnet",
        "http://127.0.0.1:1",
        true,
        HistoryLimits {
            page_slots: 2,
            ..HistoryLimits::default()
        },
    )
    .unwrap();
    drop(q);
    drop(db);
    let db = Arc::new(Mutex::new(
        Store::open(&path, "chain-parser-rust", 1).unwrap(),
    ));
    let q = HistoryQueue::open(db.clone(), registration).unwrap();
    assert!(q
        .complete(
            &in_flight,
            empty_page(&in_flight),
            xone_observability::now_ms()
        )
        .is_err());
    let next = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    assert_eq!(next.from, 3);
    assert_ne!(next.epoch, in_flight.epoch);
    q.cancel(&next, xone_observability::now_ms()).unwrap();
    assert!(q
        .complete(&next, empty_page(&next), xone_observability::now_ms())
        .is_err());
    let next = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    assert!(next.attempt > in_flight.attempt);
    let live = db.lock().unwrap().persisted_snapshot().unwrap().unwrap();
    q.complete(&next, empty_page(&next), xone_observability::now_ms())
        .unwrap();
    assert_eq!(
        db.lock().unwrap().persisted_snapshot().unwrap().unwrap(),
        live
    );
    assert_eq!(q.pages(2).unwrap().len(), 2);
    assert!(
        db.lock().unwrap().current().is_none(),
        "history replay cannot grant online readiness after reopen"
    );
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn byte_quota_future_raw_identity_and_canonical_revision_reject_results() {
    let (path, db, q) = fixture(HistoryLimits {
        max_history_bytes: 1,
        ..HistoryLimits::default()
    });
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "gap".into(),
        }],
    );
    let now = xone_observability::now_ms();
    let claim = q.claim(now).unwrap().unwrap();
    let mut wrong = empty_page(&claim);
    wrong.genesis_hash = "different".into();
    assert!(q.complete(&claim, wrong, now).is_err());
    let mut future = empty_page(&claim);
    future.to = 11;
    assert!(q.complete(&claim, future, now).is_err());
    let mut missing = empty_page(&claim);
    missing.raw.retain(|r| r.method != "getBlocks");
    assert!(q.complete(&claim, missing, now).is_err());
    assert_eq!(
        q.complete(&claim, empty_page(&claim), now).unwrap_err().0,
        "HISTORY_FACT_BUDGET"
    );
    assert_eq!(q.status().unwrap().records, 0);
    baseline(
        &db,
        2,
        2,
        "scope",
        11,
        vec![xone_domain::Gap {
            from: 1,
            to: 10,
            reason: "canonical-reorg".into(),
        }],
    );
    assert!(q
        .retry(&claim, xone_observability::now_ms(), "old epoch")
        .is_err());
    assert!(q.status().unwrap().canonical_revision > claim.canonical_revision);
    assert!(db.lock().unwrap().current().is_some());
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn expired_attempt_and_invalid_limits_never_authorize_results() {
    let (path, db, q) = fixture(HistoryLimits::default());
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "gap".into(),
        }],
    );
    let now = xone_observability::now_ms();
    let c = q.claim(now).unwrap().unwrap();
    assert!(q.complete(&c, empty_page(&c), c.lease_until_ms).is_err());
    let reclaimed = q.claim(c.lease_until_ms).unwrap().unwrap();
    assert!(reclaimed.attempt > c.attempt);
    assert!(q.retry(&c, now, "old attempt").is_err());
    assert!(HistoryLimits {
        page_slots: 129,
        ..HistoryLimits::default()
    }
    .validate()
    .is_err());
    assert!(HistoryLimits {
        max_response_bytes: 0,
        ..HistoryLimits::default()
    }
    .validate()
    .is_err());
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn superseded_source_capability_and_false_account_completeness_are_rejected() {
    let (path, db, q) = fixture(HistoryLimits::default());
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "gap".into(),
        }],
    );
    let claim = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    let mut page = empty_page(&claim);
    page.historical_accounts_complete = true;
    assert!(q
        .complete(&claim, page, xone_observability::now_ms())
        .is_err());
    let replacement = HistoryQueue::open(
        db.clone(),
        HistoryRegistration::new(
            "solana:localnet",
            "localnet",
            "http://127.0.0.1:2",
            true,
            HistoryLimits::default(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(q
        .complete(&claim, empty_page(&claim), xone_observability::now_ms())
        .is_err());
    assert!(replacement.claim(xone_observability::now_ms()).is_err());
    baseline(
        &db,
        2,
        1,
        "scope",
        11,
        vec![xone_domain::Gap {
            from: 1,
            to: 10,
            reason: "new-public-source".into(),
        }],
    );
    let next = replacement
        .claim(xone_observability::now_ms())
        .unwrap()
        .unwrap();
    assert_ne!(
        next.dataset.source_fingerprint,
        claim.dataset.source_fingerprint
    );
    assert_ne!(next.epoch, claim.epoch);
    assert_eq!(replacement.status().unwrap().records, 0);
    drop(q);
    drop(replacement);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn full_count_queue_defers_tail_then_retires_terminal_jobs_without_losing_watermarks() {
    let (path, db, q) = fixture(HistoryLimits {
        max_jobs: 1,
        ..HistoryLimits::default()
    });
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "old".into(),
        }],
    );
    let first = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    baseline(
        &db,
        2,
        1,
        "scope",
        20,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "old".into(),
        }],
    );
    assert!(q.status().unwrap().gap_sync_pending);
    assert_eq!(q.status().unwrap().jobs.len(), 1);
    q.complete(&first, empty_page(&first), xone_observability::now_ms())
        .unwrap();
    let next = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    assert_eq!(next.from, 10);
    assert_eq!(next.to, 19);
    assert_eq!(q.status().unwrap().jobs.len(), 1);
    q.complete(&next, empty_page(&next), xone_observability::now_ms())
        .unwrap();
    assert_eq!(q.status().unwrap().covered_ranges[0].from, 1);
    assert_eq!(q.status().unwrap().covered_ranges[0].to, 19);
    assert_eq!(q.status().unwrap().records, 2);
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn head_first_baseline_fences_old_attempt_but_keeps_canonical_history_revision() {
    let (path, db, q) = fixture(HistoryLimits::default());
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "HEAD_FIRST_INITIAL".into(),
        }],
    );
    let old = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    baseline(
        &db,
        2,
        1,
        "scope",
        20,
        vec![xone_domain::Gap {
            from: 1,
            to: 19,
            reason: "HEAD_FIRST_NEW_BASELINE".into(),
        }],
    );
    assert_ne!(q.status().unwrap().epoch, old.epoch);
    assert_eq!(
        q.status().unwrap().canonical_revision,
        old.canonical_revision
    );
    assert!(q
        .complete(&old, empty_page(&old), xone_observability::now_ms())
        .is_err());
    let current = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    assert!(current.attempt > old.attempt);
    assert_eq!(current.head, old.head);
    q.complete(&current, empty_page(&current), xone_observability::now_ms())
        .unwrap();
    assert!(db.lock().unwrap().current().is_some());
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn trailing_absence_without_a_successor_header_is_unknown_not_skipped() {
    let (path, db, q) = fixture(HistoryLimits {
        page_slots: 2,
        ..HistoryLimits::default()
    });
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "gap".into(),
        }],
    );
    let c = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    let mut page = empty_page(&c);
    for receipt in &mut page.raw {
        if receipt.method == "getBlock" {
            let mut v: serde_json::Value = serde_json::from_slice(&receipt.response).unwrap();
            v["result"]["parentSlot"] = serde_json::json!(9);
            receipt.response = serde_json::to_vec(&v).unwrap();
        }
    }
    assert!(
        q.complete(&c, page, xone_observability::now_ms()).is_err(),
        "a pruned/unknown old window cannot become skipped merely because getBlocks returned empty"
    );
    assert_eq!(q.status().unwrap().records, 0);
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn successor_header_can_certify_a_real_empty_finalized_window() {
    use serde_json::json;
    let (path, db, q) = fixture(HistoryLimits {
        page_slots: 2,
        ..HistoryLimits::default()
    });
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 9,
            reason: "gap".into(),
        }],
    );
    let c = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    let mut page = empty_page(&c);
    for r in &mut page.raw {
        if r.method == "getBlock" {
            let mut v: serde_json::Value = serde_json::from_slice(&r.response).unwrap();
            v["result"]["parentSlot"] = json!(9);
            r.response = serde_json::to_vec(&v).unwrap();
        }
    }
    page.raw.push(RpcReceipt {
        method: "getBlocksWithLimit".into(),
        params: json!([3,1,{"commitment":"finalized"}]),
        response: serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":[3]})).unwrap(),
    });
    page.raw.push(RpcReceipt{method:"getBlock".into(),params:json!([3,{"encoding":"json","transactionDetails":"none","commitment":"finalized"}]),response:serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"result":{"blockhash":bs58::encode([3u8;32]).into_string(),"previousBlockhash":c.head.parent_hash,"parentSlot":0,"blockTime":1}})).unwrap()});
    q.complete(&c, page, xone_observability::now_ms()).unwrap();
    assert_eq!(q.status().unwrap().jobs[0].next, Some(3));
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn a_claim_cannot_expand_its_admitted_interval_within_the_same_head() {
    let (path, db, q) = fixture(HistoryLimits::default());
    baseline(
        &db,
        1,
        1,
        "scope",
        10,
        vec![xone_domain::Gap {
            from: 1,
            to: 2,
            reason: "bounded-request".into(),
        }],
    );
    let mut forged = q.claim(xone_observability::now_ms()).unwrap().unwrap();
    forged.to = 5;
    assert!(
        q.complete(&forged, empty_page(&forged), xone_observability::now_ms())
            .is_err(),
        "same-head facts outside the leased interval cannot advance its cursor"
    );
    assert_eq!(q.status().unwrap().records, 0);
    drop(q);
    drop(db);
    std::fs::remove_dir_all(path).unwrap();
}
