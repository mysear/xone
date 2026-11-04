use serde_json::Value;
use std::{collections::BTreeMap, str::FromStr};
use xone_domain::{
    managed_v2_state::{BalancerManagedV2State, ManagedV2ExecutionContext},
    Amount, Pool,
};
use xone_protocol_math::{managed_v2_aum::view_aum, managed_v2_pool};
use xone_routing::{evaluate_path, Edge};
type Map = BTreeMap<String, Pool>;
const BANK: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../test/contracts/managed-v2-shared-vault.json"
);
fn bank() -> Value {
    serde_json::from_slice(&std::fs::read(BANK).unwrap()).unwrap()
}
fn load(b: &Value, h: &str) -> Map {
    b["banks"][h]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let p: Pool = serde_json::from_value(r["pool"].clone()).unwrap();
            (p.pool_id.clone(), p)
        })
        .collect()
}
fn m(p: &Pool) -> &BalancerManagedV2State {
    p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap()
}
fn mm(p: &mut Pool) -> &mut BalancerManagedV2State {
    p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap()
}
fn value(map: &Map) -> Value {
    serde_json::to_value(map).unwrap()
}
fn number(v: &Value) -> u64 {
    u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
}
fn ctx(c: &Value) -> ManagedV2ExecutionContext {
    let h = &c["execution"]["execution_header"];
    ManagedV2ExecutionContext {
        block_number: number(&h["number"]),
        timestamp: number(&h["timestamp"]),
    }
}
fn reclock(map: &mut Map, c: &ManagedV2ExecutionContext) {
    for p in map.values_mut() {
        let s = mm(p);
        let fee = if s.recovery_mode {
            Amount::zero()
        } else {
            s.management_aum_fee.clone()
        };
        let v = view_aum(
            &s.virtual_supply,
            c.timestamp,
            s.last_collection_timestamp,
            &fee,
        )
        .unwrap();
        s.pending_aum = v.pending_bpt;
        s.actual_supply = v.actual_supply;
        s.paused &= c.timestamp <= s.buffer_period_end;
        s.vault_paused &= c.timestamp <= s.vault_buffer_period_end;
        s.execution_context = Some(c.clone());
        p.validate().unwrap();
    }
}
fn edge(map: &Map, c: &Value) -> Edge {
    let e = &c["expected_native"]["swap_event"];
    let p = map
        .values()
        .find(|p| p.multi_asset.as_ref().unwrap().pool_id == e["pool_id"].as_str().unwrap())
        .unwrap();
    Edge {
        pool_id: p.pool_id.clone(),
        protocol: p.protocol.clone(),
        from_token: Some(e["token_in"].as_str().unwrap().into()),
        to: e["token_out"].as_str().unwrap().into(),
        zero_for_one: e["token_in"].as_str().unwrap() == p.token0,
    }
}
fn input(c: &Value) -> Amount {
    Amount::from_str(c["expected_native"]["amount_in"].as_str().unwrap()).unwrap()
}
fn a(n: u64) -> Amount {
    Amount::from_str(&n.to_string()).unwrap()
}
fn compare(actual: &Map, expected: &Map, label: &str) {
    let av = value(actual);
    let ev = value(expected);
    let mut diffs = vec![];
    fn walk(a: &Value, b: &Value, path: String, d: &mut Vec<String>) {
        if a == b {
            return;
        }
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for k in x
                    .keys()
                    .chain(y.keys())
                    .collect::<std::collections::BTreeSet<_>>()
                {
                    walk(&a[k], &b[k], format!("{path}/{k}"), d)
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for i in 0..x.len() {
                    walk(&x[i], &y[i], format!("{path}/{i}"), d)
                }
            }
            _ => d.push(format!("{path}: actual={a} expected={b}")),
        }
    }
    walk(&av, &ev, String::new(), &mut diffs);
    assert!(
        diffs.is_empty(),
        "{label}: full poststate mismatch\n{}",
        diffs.join("\n")
    );
}
fn expected(b: &Value, c: &Value, before: &Map) -> Map {
    let mut out = load(b, c["expected_after_bank2pools"].as_str().unwrap());
    reclock(&mut out, &ctx(c));
    for (id, p) in &mut out {
        let source = m(&before[id]);
        let s = mm(p);
        s.block_number = source.block_number;
        s.timestamp = source.timestamp;
        s.block_hash = source.block_hash.clone();
    }
    out
}
fn prepared() -> (Value, Map, Edge, Amount) {
    let b = bank();
    let c = &b["cases"][0];
    let mut map = load(&b, "H38");
    reclock(&mut map, &ctx(c));
    let e = edge(&map, c);
    let i = input(c);
    (b, map, e, i)
}
#[test]
fn native_all_seven_banks_legal_and_collection_ledgers_distinct() {
    let b = bank();
    assert_eq!(b["banks"].as_object().unwrap().len(), 7);
    for (h, rows) in b["banks"].as_object().unwrap() {
        let map = load(&b, h);
        assert_eq!(map.len(), 2);
        for p in map.values() {
            p.validate().unwrap();
            let s = m(p);
            assert!(s.execution_context.is_none());
            s.to_proto().unwrap();
            let v = view_aum(
                &s.virtual_supply,
                s.timestamp,
                s.last_collection_timestamp,
                &s.management_aum_fee,
            )
            .unwrap();
            assert_eq!(v.pending_bpt, s.pending_aum);
            assert_eq!(v.actual_supply, s.actual_supply);
        }
        assert_eq!(rows["getter_count"], 152);
        let ps: Vec<_> = map.values().collect();
        for i in 0..2 {
            assert_eq!(
                m(ps[0]).token_info[i].vault_actual_balance,
                m(ps[1]).token_info[i].vault_actual_balance
            );
        }
        println!("source {h}: 2 full Pool/state39/tokeninfo snapshots validated");
    }
    let pre = load(&b, "H43");
    let post = load(&b, "H44");
    let selected = post
        .iter()
        .find(|(id, p)| m(p).manager_bpt_balance != m(&pre[*id]).manager_bpt_balance)
        .unwrap()
        .0;
    for (id, p) in &post {
        if id == selected {
            assert!(m(p).manager_bpt_balance > m(&pre[id]).manager_bpt_balance);
            assert!(
                m(p).protocol_collector_bpt_balance > m(&pre[id]).protocol_collector_bpt_balance
            );
        } else {
            assert_eq!(m(p).manager_bpt_balance, m(&pre[id]).manager_bpt_balance);
            assert_eq!(
                m(p).protocol_collector_bpt_balance,
                m(&pre[id]).protocol_collector_bpt_balance
            );
            assert!(m(p).pending_aum > Amount::zero());
        }
    }
    println!("H43/H44 collection reference only; no evaluate_path collection claim");
}
#[test]
fn native_four_swaps_full_poststate() {
    let b = bank();
    let mut mismatches = vec![];
    for c in b["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["operation"] == "swap")
    {
        let mut map = load(&b, c["source_bank2pools"].as_str().unwrap());
        reclock(&mut map, &ctx(c));
        let before = map.clone();
        let e = edge(&map, c);
        let out = evaluate_path(&mut map, std::slice::from_ref(&e), &input(c)).unwrap();
        assert_eq!(
            out.to_string(),
            c["expected_native"]["amount_out"].as_str().unwrap()
        );
        let exp = expected(&b, c, &before);
        if value(&map) != value(&exp) {
            let result =
                std::panic::catch_unwind(|| compare(&map, &exp, c["name"].as_str().unwrap()));
            mismatches.push(result.is_err());
        } else {
            println!(
                "{}: rawout={} ALL POOLS complete expected poststate equal",
                c["name"], out
            );
        }
        for (id, p) in &map {
            assert_eq!(m(p).block_hash, m(&before[id]).block_hash);
            if id != &e.pool_id {
                for i in 0..2 {
                    assert_eq!(m(p).token_info[i].cash, m(&before[id]).token_info[i].cash);
                    assert_eq!(
                        m(p).token_info[i].managed,
                        m(&before[id]).token_info[i].managed
                    );
                }
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} real swaps have complete poststate mismatches",
        mismatches.len()
    );
}
#[test]
fn carried_four_steps_full_poststate_and_two_paths() {
    let b = bank();
    let mut map = load(&b, "H38");
    for c in b["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["operation"] == "swap")
    {
        reclock(&mut map, &ctx(c));
        let before = map.clone();
        let e = edge(&map, c);
        let out = evaluate_path(&mut map, &[e], &input(c)).unwrap();
        assert_eq!(
            out.to_string(),
            c["expected_native"]["amount_out"].as_str().unwrap()
        );
        compare(&map, &expected(&b, c, &before), c["name"].as_str().unwrap());
        println!("carried {} full state equal", c["name"]);
    }
}
#[test]
fn fixed_h_marks_both_derived_and_rejects_canonical_wire() {
    let b = bank();
    let mut map = load(&b, "H38");
    let before = map.clone();
    let e = edge(&map, &b["cases"][0]);
    evaluate_path(&mut map, &[e], &input(&b["cases"][0])).unwrap();
    for (id, p) in &map {
        let s = m(p);
        assert_eq!(s.block_hash, m(&before[id]).block_hash);
        assert_eq!(
            s.execution_context,
            Some(ManagedV2ExecutionContext {
                block_number: s.block_number,
                timestamp: s.timestamp
            })
        );
        assert!(s.to_proto().is_err());
    }
}
fn rejected_unchanged(mut map: Map, e: Edge, i: Amount, needle: &str) {
    let before = value(&map);
    let error = evaluate_path(&mut map, &[e], &i)
        .expect_err("must reject")
        .to_string();
    assert!(error.contains(needle), "expected {needle}, got {error}");
    assert_eq!(value(&map), before, "failed candidate changed input");
    println!("rejected atomically: {error}");
}
#[test]
fn initial_wallet_disagreement() {
    let (_, mut map, e, i) = prepared();
    let other = map.keys().find(|id| *id != &e.pool_id).unwrap().clone();
    mm(map.get_mut(&other).unwrap()).token_info[0]
        .vault_actual_balance
        .0 += 1u8;
    rejected_unchanged(map, e, i, "SHARED_VAULT_WALLET");
}
#[test]
fn token_runtime_disagreement() {
    let (_, mut map, e, i) = prepared();
    let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
    p.multi_asset.as_mut().unwrap().assets[0].code_sha256 = "11".repeat(32);
    p.validate().unwrap();
    rejected_unchanged(map, e, i, "SHARED_TOKEN_IDENTITY");
}
#[test]
fn scaling_disagreement() {
    let (_, mut map, e, i) = prepared();
    let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
    p.multi_asset.as_mut().unwrap().assets[0].scaling.0 *= 10u8;
    p.validate().unwrap();
    rejected_unchanged(map, e, i, "SHARED_TOKEN_IDENTITY");
}
#[test]
fn source_h_disagreement() {
    let (_, mut map, e, i) = prepared();
    let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
    mm(p).block_hash = format!("0x{}", "11".repeat(32));
    p.validate().unwrap();
    rejected_unchanged(map, e, i, "SHARED_VAULT_SOURCE");
}
#[test]
fn execution_clock_disagreement() {
    let (_, mut map, e, i) = prepared();
    let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
    mm(p).execution_context.as_mut().unwrap().block_number += 1;
    rejected_unchanged(map, e, i, "SHARED_VAULT_CLOCK");
}
#[test]
fn cross_chain_rejected() {
    let (_, mut map, e, i) = prepared();
    map.iter_mut()
        .find(|(id, _)| *id != &e.pool_id)
        .unwrap()
        .1
        .chain_key = "evm:1".into();
    rejected_unchanged(map, e, i, "CROSS_CHAIN");
}
#[test]
fn different_vault_does_not_propagate() {
    let (_, mut map, e, i) = prepared();
    let other = map.keys().find(|id| *id != &e.pool_id).unwrap().clone();
    map.get_mut(&other)
        .unwrap()
        .multi_asset
        .as_mut()
        .unwrap()
        .vault = "0x1111111111111111111111111111111111111111".into();
    let before = serde_json::to_value(&map[&other]).unwrap();
    evaluate_path(&mut map, &[e], &i).unwrap();
    assert_eq!(serde_json::to_value(&map[&other]).unwrap(), before);
}
#[test]
fn wallet_input_overflow_atomic() {
    let (_, mut map, e, i) = prepared();
    let max = Amount::from_str(
        "115792089237316195423570985008687907853269984665640564039457584007913129639935",
    )
    .unwrap();
    for p in map.values_mut() {
        mm(p).token_info[0].vault_actual_balance = max.clone();
        p.validate().unwrap();
    }
    rejected_unchanged(map, e, i, "OVERFLOW");
}
#[test]
fn inconsistent_sum_of_sibling_cash_rejects_before_swap_atomically() {
    let (_, mut map, e, i) = prepared();
    let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
    let cash = m(p).token_info[1].vault_actual_balance.clone();
    mm(p).token_info[1].cash = cash.clone();
    p.multi_asset.as_mut().unwrap().assets[1].balance = cash.clone();
    p.reserve1 = cash;
    p.validate().unwrap();
    rejected_unchanged(map, e, i, "SHARED_VAULT_REGISTERED_CASH");
}
#[test]
fn invalid_initial_output_wallet_rejected_atomic() {
    let (_, mut map, e, i) = prepared();
    for p in map.values_mut() {
        mm(p).token_info[1].vault_actual_balance = a(1);
    }
    rejected_unchanged(map, e, i, "ACTUAL_TOKEN_BALANCE");
}
#[test]
fn unsupported_other_pool_bpt_as_underlying() {
    let (_, mut map, e, i) = prepared();
    let own_wallet = m(&map[&e.pool_id]).vault_bpt_actual_balance.clone();
    let own = map[&e.pool_id]
        .multi_asset
        .as_ref()
        .unwrap()
        .code_sha256
        .clone();
    let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
    p.token0 = e.pool_id.clone();
    let s = p.multi_asset.as_mut().unwrap();
    s.assets[0].token = e.pool_id.clone();
    s.assets[0].code_sha256 = own;
    let other_cash = s.managed_v2.as_ref().unwrap().token_info[0].cash.clone();
    let funded = Amount(&own_wallet.0 + &other_cash.0);
    s.managed_v2.as_mut().unwrap().token_info[0].vault_actual_balance = funded.clone();
    p.validate().unwrap();
    mm(map.get_mut(&e.pool_id).unwrap()).vault_bpt_actual_balance = funded;
    rejected_unchanged(map, e, i, "SHARED_VAULT_BPT_UNSUPPORTED");
}
#[test]
fn whole_path_failure_rolls_back_first_success() {
    let (_, mut map, e, i) = prepared();
    let before = value(&map);
    let mut bad = e.clone();
    bad.from_token = Some(e.to.clone());
    bad.to = e.from_token.clone().unwrap();
    bad.protocol = "bad-protocol".into();
    let err = evaluate_path(&mut map, &[e, bad], &i).unwrap_err();
    assert!(err.to_string().contains("PATH_PROTOCOL_MISMATCH"));
    assert_eq!(value(&map), before);
}
#[test]
fn same_physical_pool_repeated_swap_correct() {
    let (_, mut map, e, i) = prepared();
    let mut direct = map[&e.pool_id].clone();
    for _ in 0..3 {
        let q = managed_v2_pool::quote_exact_in_tokens(
            &direct,
            &i,
            e.from_token.as_deref().unwrap(),
            &e.to,
        )
        .unwrap();
        let out = evaluate_path(&mut map, std::slice::from_ref(&e), &i).unwrap();
        assert_eq!(out, q.amount_out);
        assert_eq!(
            serde_json::to_value(&map[&e.pool_id]).unwrap(),
            serde_json::to_value(&q.next_pool).unwrap()
        );
        direct = q.next_pool;
    }
}
#[test]
fn allocation_two_paths_shared_wallet_and_registered_ledgers() {
    let (_, mut map, e, i) = prepared();
    let other = map.keys().find(|id| *id != &e.pool_id).unwrap().clone();
    let mut second = e.clone();
    second.pool_id = other;
    let mut total = Amount::zero();
    for path in [e, second] {
        let before = map.clone();
        let q = managed_v2_pool::quote_exact_in_tokens(
            &before[&path.pool_id],
            &i,
            path.from_token.as_deref().unwrap(),
            &path.to,
        )
        .unwrap();
        let out = evaluate_path(&mut map, std::slice::from_ref(&path), &i).unwrap();
        assert_eq!(out, q.amount_out);
        total.0 += &out.0;
        for (id, p) in &map {
            if id != &path.pool_id {
                let mut expected = before[id].clone();
                mm(&mut expected).execution_context = m(&q.next_pool).execution_context.clone();
                for t in 0..2 {
                    mm(&mut expected).token_info[t].vault_actual_balance =
                        m(&q.next_pool).token_info[t].vault_actual_balance.clone();
                }
                assert_eq!(
                    serde_json::to_value(p).unwrap(),
                    serde_json::to_value(expected).unwrap(),
                    "all sibling fields except wallet/context must stay identical"
                );
            }
        }
    }
    println!("two separate allocation paths raw aggregate={total}");
}
#[test]
fn wallet_may_be_less_than_assets_but_not_cash() {
    let (_, mut map, e, i) = prepared();
    let other = map.keys().find(|id| *id != &e.pool_id).unwrap().clone();
    let p = map.get_mut(&other).unwrap();
    let managed = m(p).token_info[1].vault_actual_balance.clone();
    let manager = m(p).manager.clone();
    mm(p).token_info[1].managed = managed;
    mm(p).token_info[1].asset_manager = manager;
    let balance = Amount(&m(p).token_info[1].cash.0 + &m(p).token_info[1].managed.0);
    p.multi_asset.as_mut().unwrap().assets[1].balance = balance.clone();
    p.reserve1 = balance;
    p.validate().unwrap();
    let before = map.clone();
    evaluate_path(&mut map, &[e], &i).unwrap();
    let after = m(&map[&other]);
    assert!(after.token_info[1].vault_actual_balance.0 < map[&other].reserve1.0);
    assert!(after.token_info[1].vault_actual_balance.0 >= after.token_info[1].cash.0);
    assert_eq!(
        after.token_info[1].cash,
        m(&before[&other]).token_info[1].cash
    );
    assert_eq!(
        after.token_info[1].managed,
        m(&before[&other]).token_info[1].managed
    );
}
fn add_third(map: &mut Map, e: &Edge) -> String {
    let mut third = map
        .iter()
        .find(|(id, _)| *id != &e.pool_id)
        .unwrap()
        .1
        .clone();
    let id = "0xfffffffffffffffffffffffffffffffffffffff1".to_owned();
    third.pool_id = id.clone();
    let s = third.multi_asset.as_mut().unwrap();
    s.pool_id = format!("{}{}", id, &s.pool_id[42..]);
    third.validate().unwrap();
    // This synthetic registration adds real cash backing to the global
    // wallet. Every declared view observes that same additional funding.
    let funding = third
        .multi_asset
        .as_ref()
        .unwrap()
        .assets
        .iter()
        .zip(&m(&third).token_info)
        .map(|(asset, info)| (asset.token.clone(), info.cash.clone()))
        .collect::<BTreeMap<_, _>>();
    map.insert(id.clone(), third);
    for pool in map.values_mut() {
        let multi = pool.multi_asset.as_mut().unwrap();
        let state = multi.managed_v2.as_mut().unwrap();
        for (asset, info) in multi.assets.iter().zip(&mut state.token_info) {
            if let Some(cash) = funding.get(&asset.token) {
                info.vault_actual_balance.0 += &cash.0;
            }
        }
    }
    id
}
#[test]
fn late_sibling_cash_inconsistency_keeps_all_candidates_atomic() {
    let (_, mut map, e, i) = prepared();
    let id = add_third(&mut map, &e);
    let p = map.get_mut(&id).unwrap();
    let cash = m(p).token_info[1].vault_actual_balance.clone();
    mm(p).token_info[1].cash = cash.clone();
    p.multi_asset.as_mut().unwrap().assets[1].balance = cash.clone();
    p.reserve1 = cash;
    p.validate().unwrap();
    rejected_unchanged(map, e, i, "SHARED_VAULT_REGISTERED_CASH");
}
#[test]
fn extra_sibling_partial_token_overlap_updates_by_token() {
    let (_, mut map, e, i) = prepared();
    let id = add_third(&mut map, &e);
    let new_token = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned();
    let p = map.get_mut(&id).unwrap();
    p.token1 = new_token.clone();
    p.multi_asset.as_mut().unwrap().assets[1].token = new_token;
    p.validate().unwrap();
    let before = map[&id].clone();
    evaluate_path(&mut map, std::slice::from_ref(&e), &i).unwrap();
    let mut expected = before;
    mm(&mut expected).execution_context = m(&map[&e.pool_id]).execution_context.clone();
    mm(&mut expected).token_info[0].vault_actual_balance = m(&map[&e.pool_id]).token_info[0]
        .vault_actual_balance
        .clone();
    assert_eq!(
        serde_json::to_value(&map[&id]).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}
#[test]
fn vault_runtime_disagreement() {
    let (_, mut map, e, i) = prepared();
    let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
    p.multi_asset.as_mut().unwrap().vault_code_sha256 = "11".repeat(32);
    p.validate().unwrap();
    rejected_unchanged(map, e, i, "SHARED_VAULT_IDENTITY");
}
#[test]
fn source_height_and_timestamp_disagreement() {
    for time in [false, true] {
        let (_, mut map, e, i) = prepared();
        let p = map.iter_mut().find(|(id, _)| *id != &e.pool_id).unwrap().1;
        let s = mm(p);
        s.execution_context.as_mut().unwrap().block_number += 1;
        if time {
            s.timestamp += 1;
            s.execution_context.as_mut().unwrap().timestamp += 1;
        } else {
            s.block_number += 1;
        }
        p.validate().unwrap();
        rejected_unchanged(map, e, i, "SHARED_VAULT_SOURCE");
    }
}
#[test]
fn mixed_same_vault_adapter_explicitly_unsupported() {
    let (_, mut map, e, i) = prepared();
    let id = add_third(&mut map, &e);
    let p = map.get_mut(&id).unwrap();
    p.protocol = "balancer-v2-managed-snapshot".into();
    p.adapter_version = "balancer-v2-managed-snapshot/no-bpt-fees-1".into();
    p.multi_asset.as_mut().unwrap().managed_v2 = None;
    p.validate().unwrap();
    rejected_unchanged(map, e, i, "UNSUPPORTED");
}
fn snapshot(p: Pool) -> xone_domain::Snapshot {
    use xone_domain::{scope_hash, BlockRef, Snapshot, SnapshotRef};
    let s = m(&p);
    Snapshot {
        reference: SnapshotRef {
            block: BlockRef {
                chain_key: p.chain_key.clone(),
                number_or_slot: s.block_number,
                hash: s.block_hash.clone(),
                parent_hash: String::new(),
                timestamp_ms: s.timestamp * 1000,
            },
            schema_version: 1,
            owner_service: "overlay-test".into(),
            owner_assignment_version: 1,
            writer_epoch: 1,
            canonical_epoch: 1,
            online_generation: 1,
            journal_seq: 1,
            adapter_set_hash: "synthetic-test-envelope".into(),
            scope_hash: scope_hash(std::slice::from_ref(&p)).unwrap(),
        },
        pools: vec![p],
        source: String::new(),
        history_gaps: vec![],
    }
}
#[test]
fn canonical_snapshot_and_multiasset_wire_both_reject_every_derived_pool() {
    let b = bank();
    let mut map = load(&b, "H38");
    for p in map.values() {
        snapshot(p.clone()).validate("overlay-test", 1).unwrap();
        p.multi_asset.as_ref().unwrap().to_proto().unwrap();
    }
    let e = edge(&map, &b["cases"][0]);
    evaluate_path(&mut map, &[e], &input(&b["cases"][0])).unwrap();
    for p in map.values() {
        assert!(snapshot(p.clone())
            .validate("overlay-test", 1)
            .unwrap_err()
            .to_string()
            .contains("DERIVED_STATE_NOT_CANONICAL"));
        assert!(p
            .multi_asset
            .as_ref()
            .unwrap()
            .to_proto()
            .unwrap_err()
            .to_string()
            .contains("DERIVED_STATE_NOT_CANONICAL"));
    }
}
fn word(hex: &str, index: usize) -> Amount {
    let h = hex.strip_prefix("0x").unwrap();
    let bytes: Vec<u8> = h.as_bytes()[index * 64..(index + 1) * 64]
        .chunks_exact(2)
        .map(|x| u8::from_str_radix(std::str::from_utf8(x).unwrap(), 16).unwrap())
        .collect();
    Amount::from_be_bytes(&bytes).unwrap()
}
fn getter<'a>(row: &'a Value, to: &str, method: &str, args: &Value) -> &'a str {
    row["getters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["to"] == to && g["method"] == method && &g["args"] == args)
        .unwrap()["result"]
        .as_str()
        .unwrap()
}
#[test]
fn original_raw_wallets_registered_cash_managed_and_trader_deltas() {
    let b = bank();
    let mut wallets = 0;
    for (h, bank) in b["banks"].as_object().unwrap() {
        assert_eq!(bank["getter_count"], 152);
        let mut tokens = std::collections::BTreeSet::new();
        for row in bank["pools"].as_array().unwrap() {
            assert_eq!(row["getter_count"], 76);
            let p: Pool = serde_json::from_value(row["pool"].clone()).unwrap();
            let s = p.multi_asset.as_ref().unwrap();
            let state = m(&p);
            assert_eq!(state.block_number, number(&bank["header"]["number"]));
            assert_eq!(state.timestamp, number(&bank["header"]["timestamp"]));
            assert_eq!(state.block_hash, bank["header"]["hash"].as_str().unwrap());
            snapshot(p.clone()).validate("overlay-test", 1).unwrap();
            for (a, info) in s.assets.iter().zip(&state.token_info) {
                tokens.insert(a.token.clone());
                let raw = getter(row, &a.token, "balanceOf", &serde_json::json!([s.vault]));
                assert_eq!(word(raw, 0), info.vault_actual_balance);
                wallets += 1;
                let reg = getter(
                    row,
                    &s.vault,
                    "getPoolTokenInfo",
                    &serde_json::json!([s.pool_id, a.token]),
                );
                assert_eq!(word(reg, 0), info.cash);
                assert_eq!(word(reg, 1), info.managed);
                assert_eq!(word(reg, 2), a64(info.last_change_block));
                assert_eq!(
                    format!("0x{}", &reg[2 + 3 * 64 + 24..2 + 4 * 64]),
                    info.asset_manager
                );
            }
            tokens.insert(p.pool_id.clone());
            assert_eq!(
                word(
                    getter(row, &p.pool_id, "balanceOf", &serde_json::json!([s.vault])),
                    0
                ),
                state.vault_bpt_actual_balance
            );
            wallets += 1;
            let reg = getter(
                row,
                &s.vault,
                "getPoolTokenInfo",
                &serde_json::json!([s.pool_id, p.pool_id]),
            );
            assert_eq!(word(reg, 0), state.vault_bpt_cash);
            assert_eq!(word(reg, 1), state.vault_bpt_managed);
            for (owner, amount) in [
                (&state.manager, &state.manager_bpt_balance),
                (
                    &state.protocol_collector,
                    &state.protocol_collector_bpt_balance,
                ),
            ] {
                assert_eq!(
                    &word(
                        getter(row, &p.pool_id, "balanceOf", &serde_json::json!([owner])),
                        0
                    ),
                    amount
                );
            }
            for g in row["getters"].as_array().unwrap() {
                assert_eq!(g["block_tag"], bank["header"]["number"]);
                assert_eq!(g["block_hash"], bank["header"]["hash"]);
            }
        }
        assert_eq!(tokens.len(), 4, "{h}: 2 underlying + 2 distinct BPT");
        let mut candidate = load(&b, h);
        let before = value(&candidate);
        evaluate_path(&mut candidate, &[], &a(1)).unwrap();
        assert_eq!(value(&candidate), before);
    }
    assert_eq!(wallets, 42);
    for c in b["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["operation"] == "swap")
    {
        let scenario = c["scenario"].as_str().unwrap();
        let source = b["banks"][c["source_bank2pools"].as_str().unwrap()]["pools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["scenario"] == scenario)
            .unwrap();
        let after = b["banks"][c["expected_after_bank2pools"].as_str().unwrap()]["pools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["scenario"] == scenario)
            .unwrap();
        let trader = c["execution"]["transaction"]["from"].as_str().unwrap();
        let e = &c["expected_native"]["swap_event"];
        let prein = word(
            getter(
                source,
                e["token_in"].as_str().unwrap(),
                "balanceOf",
                &serde_json::json!([trader]),
            ),
            0,
        );
        let postin = word(
            getter(
                after,
                e["token_in"].as_str().unwrap(),
                "balanceOf",
                &serde_json::json!([trader]),
            ),
            0,
        );
        let preout = word(
            getter(
                source,
                e["token_out"].as_str().unwrap(),
                "balanceOf",
                &serde_json::json!([trader]),
            ),
            0,
        );
        let postout = word(
            getter(
                after,
                e["token_out"].as_str().unwrap(),
                "balanceOf",
                &serde_json::json!([trader]),
            ),
            0,
        );
        assert_eq!(Amount(prein.0 - postin.0), input(c));
        assert_eq!(
            Amount(postout.0 - preout.0).to_string(),
            c["expected_native"]["amount_out"].as_str().unwrap()
        );
    }
    let collection = &b["cases"][4];
    let pre = load(&b, "H43");
    let post = load(&b, "H44");
    let id = b["banks"]["H44"]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["scenario"] == collection["scenario"])
        .unwrap()["pool"]["pool_id"]
        .as_str()
        .unwrap();
    let old = m(&pre[id]);
    let new = m(&post[id]);
    for (actual, key) in [
        (
            Amount(&new.total_supply.0 - &old.total_supply.0),
            "minted_bpt",
        ),
        (
            Amount(&new.manager_bpt_balance.0 - &old.manager_bpt_balance.0),
            "manager_bpt_delta",
        ),
        (
            Amount(&new.protocol_collector_bpt_balance.0 - &old.protocol_collector_bpt_balance.0),
            "protocol_collector_bpt_delta",
        ),
    ] {
        assert_eq!(
            actual.to_string(),
            collection["expected_native"][key].as_str().unwrap()
        );
    }
    println!("7 banks/1064 original getters anchored; 42 raw Vault wallet observations + registrations/BPT owner ledgers; 4 actual raw trader deltas");
}
fn a64(n: u64) -> Amount {
    Amount::from_str(&n.to_string()).unwrap()
}

fn unrelated_cpmm() -> Pool {
    let source: xone_domain::Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    source.pools[0].clone()
}
fn add_unsupported_mixed_group(map: &mut Map, e: &Edge) -> String {
    let id = add_third(map, e);
    let p = map.get_mut(&id).unwrap();
    p.protocol = "balancer-v2-managed-snapshot".into();
    p.adapter_version = "balancer-v2-managed-snapshot/no-bpt-fees-1".into();
    p.multi_asset.as_mut().unwrap().managed_v2 = None;
    p.validate().unwrap();
    id
}
#[test]
fn mixed_vault_is_explicitly_excluded_without_blocking_unrelated_routes() {
    use xone_routing::Graph;
    let (_, mut map, e, i) = prepared();
    let mixed = add_unsupported_mixed_group(&mut map, &e);
    let unrelated = unrelated_cpmm();
    let id = unrelated.pool_id.clone();
    let sell = unrelated.token0.clone();
    let buy = unrelated.token1.clone();
    map.insert(id.clone(), unrelated);
    let rows = map.values().cloned().collect::<Vec<_>>();
    let graph = Graph::build(&rows).unwrap();
    assert_eq!(graph.excluded_pools.len(), 3);
    assert!(!graph.excluded_pools.contains_key(&id));
    assert!(graph
        .adjacency
        .values()
        .flatten()
        .all(|edge| !graph.excluded_pools.contains_key(&edge.pool_id)));
    let saved = map.clone();
    let ordinary = Edge {
        pool_id: id.clone(),
        protocol: map[&id].protocol.clone(),
        from_token: None,
        zero_for_one: true,
        to: buy,
    };
    assert!(evaluate_path(&mut map, &[ordinary], &a(10)).unwrap() > Amount::zero());
    for (pool, before) in &saved {
        if *pool != id {
            assert_eq!(&map[pool], before);
        }
    }
    let blocked = Edge {
        pool_id: mixed.clone(),
        protocol: map[&mixed].protocol.clone(),
        from_token: Some(sell),
        zero_for_one: true,
        to: e.to.clone(),
    };
    let before = value(&map);
    assert!(evaluate_path(&mut map, &[blocked], &i)
        .unwrap_err()
        .to_string()
        .contains("SHARED_VAULT_MIXED"));
    assert_eq!(value(&map), before);
    rejected_unchanged(map, e, i, "SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED");
}
#[test]
fn shared_vault_pause_contradictions_reject_atomically_at_all_route_entry_points() {
    use xone_routing::Graph;
    for field in 0..3 {
        let (_, mut map, e, i) = prepared();
        let other = map.keys().find(|id| *id != &e.pool_id).unwrap().clone();
        let p = map.get_mut(&other).unwrap();
        let state = mm(p);
        match field {
            0 => state.vault_paused = true,
            1 => state.vault_pause_window_end += 1,
            _ => state.vault_buffer_period_end += 1,
        }
        p.validate().unwrap();
        let rows = map.values().cloned().collect::<Vec<_>>();
        assert!(xone_domain::validate_pool_dependencies(rows.iter()).is_err());
        assert!(Graph::build(&rows).is_err());
        rejected_unchanged(map, e, i, "SHARED_VAULT_PAUSE");
    }
}
#[test]
fn own_pool_pause_and_provider_cache_are_not_shared_vault_facts() {
    use xone_routing::Graph;
    let (_, mut map, e, i) = prepared();
    let other = map.keys().find(|id| *id != &e.pool_id).unwrap().clone();
    let sibling = map.get_mut(&other).unwrap();
    let state = mm(sibling);
    state.paused = true;
    state.buffer_period_end = state.timestamp + 500;
    state.pause_window_end = state.timestamp + 100;
    state.cached_protocol_aum_percentage = "400000000000000000".parse().unwrap();
    sibling.validate().unwrap();
    Graph::build(&map.values().cloned().collect::<Vec<_>>()).unwrap();
    let saved = map[&other].clone();
    evaluate_path(&mut map, &[e], &i).unwrap();
    assert!(m(&map[&other]).paused);
    assert_eq!(
        m(&map[&other]).cached_protocol_aum_percentage,
        m(&saved).cached_protocol_aum_percentage
    );
}
#[test]
fn every_original_shared_source_bank_is_a_valid_canonical_dependency_view() {
    let b = bank();
    for rows in b["banks"].as_object().unwrap().values() {
        let pools = rows["pools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| serde_json::from_value::<Pool>(row["pool"].clone()).unwrap())
            .collect::<Vec<_>>();
        xone_domain::validate_pool_dependencies(pools.iter()).unwrap();
        assert!(xone_domain::unavailable_pool_reasons(pools.iter())
            .unwrap()
            .is_empty());
        xone_routing::Graph::build(&pools).unwrap();
    }
}

#[test]
fn managed_vault_capacity_failure_retries_smaller_discovery_probes() {
    use xone_routing::{search, Budget, Graph};
    let b = bank();
    let mut source = load(&b, "H38");
    let mut third = source.values().last().unwrap().clone();
    third.pool_id = "0xfffffffffffffffffffffffffffffffffffffffc".into();
    third.multi_asset.as_mut().unwrap().pool_id =
        format!("{}000100000000000000000002", third.pool_id);
    third.validate().unwrap();
    let funding = m(&third)
        .token_info
        .iter()
        .map(|info| info.cash.clone())
        .collect::<Vec<_>>();
    source.insert(third.pool_id.clone(), third);
    for pool in source.values_mut() {
        for (index, info) in mm(pool).token_info.iter_mut().enumerate() {
            info.vault_actual_balance.0 += &funding[index].0;
        }
    }
    let rows = source.values().cloned().collect::<Vec<_>>();
    let graph = Graph::build(&rows).unwrap();
    let p = &rows[0];
    let amount: Amount = "5000000000000000000000".parse().unwrap();
    let found = search(
        &graph,
        &rows,
        &p.token0,
        &p.token1,
        &amount,
        Budget {
            split_parts: 10,
            max_hops: 1,
            max_candidates: 16,
            max_expansions: 1000,
        },
    )
    .unwrap();
    assert_eq!(
        found.candidates.len(),
        3,
        "full-size Managed ratio failures must preserve all feasible smaller physical paths"
    );
    assert!(found
        .candidates
        .iter()
        .all(|path| path.amount_in < amount && path.amount_out > Amount::zero()));
}
