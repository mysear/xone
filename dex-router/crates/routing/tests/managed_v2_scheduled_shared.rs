//! Synthetic shared Vault clock/view tests; no captured native parity claim.
use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{
    managed_v2_schedule::{ManagedV2Schedule, ManagedV2ScheduledToken, ADAPTER_VERSION, PROTOCOL},
    managed_v2_state::BalancerManagedV2State,
    Amount, Pool,
};
use xone_protocol_math::{
    managed_v2_aum::view_aum,
    managed_v2_pool::{quote_at, reclock},
};
#[path = "../src/shared_vault.rs"]
mod shared_vault;

macro_rules! uint {
    ($n:expr) => {
        a(&$n.to_string()).0
    };
}
const ONE: u64 = 1_000_000_000_000_000_000;
fn m(p: &Pool) -> &BalancerManagedV2State {
    p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap()
}
fn mm(p: &mut Pool) -> &mut BalancerManagedV2State {
    p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap()
}
fn a(s: &str) -> Amount {
    s.parse().unwrap()
}
fn fixture() -> Pool {
    let bank: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/managed-v2-aum-wire.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut p: Pool = serde_json::from_value(bank["original_fields"].clone()).unwrap();
    let t = m(&p).timestamp;
    p.protocol = PROTOCOL.into();
    p.adapter_version = ADAPTER_VERSION.into();
    let tokens = p
        .multi_asset
        .as_ref()
        .unwrap()
        .assets
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (start, end) = if i == 0 {
                (8 * ONE / 10, 2 * ONE / 10)
            } else {
                (2 * ONE / 10 + 1, 8 * ONE / 10)
            };
            ManagedV2ScheduledToken {
                token: a.token.clone(),
                state_word: Amount(uint!(start) | (uint!(end) << 64usize)),
            }
        })
        .collect();
    let raw = ManagedV2Schedule {
        pool_state_word: Amount(
            uint!(t + 10)
                | (uint!(t + 110) << 32usize)
                | (uint!(t + 10) << 64usize)
                | (uint!(t + 110) << 96usize)
                | (uint!(ONE / 100) << 128usize)
                | (uint!(3 * ONE / 100) << 190usize)
                | (uint!(9u8) << 252usize),
        ),
        tokens,
    };
    mm(&mut p).schedule = Some(raw);
    project(&mut p);
    p.validate().unwrap();
    p
}
fn project(p: &mut Pool) {
    let t = m(p)
        .execution_context
        .as_ref()
        .map_or(m(p).timestamp, |c| c.timestamp);
    let v = m(p).schedule.as_ref().unwrap().at(t).unwrap();
    p.fee_numerator = v.swap_fee.to_string().parse().unwrap();
    let s = p.multi_asset.as_mut().unwrap();
    s.weights = v.weights;
    for (a, scale) in s.assets.iter_mut().zip(v.scaling) {
        a.scaling = scale;
    }
    let m = s.managed_v2.as_mut().unwrap();
    m.gradual_weight_end = v.weight_end;
    m.gradual_fee_end = v.fee_end;
    m.swaps_enabled = v.swaps_enabled;
    m.recovery_mode = v.recovery_mode;
    m.allowlist_enabled = v.must_allowlist_lps;
}

fn pools() -> BTreeMap<String, Pool> {
    let mut p = fixture();
    let mut b = p.clone();
    let mut c = p.clone();
    let wallet = Amount(&m(&p).token_info[0].cash.0 * 2u8);
    mm(&mut p).token_info[0].vault_actual_balance = wallet.clone();
    mm(&mut b).token_info[0].vault_actual_balance = wallet;
    for (i, q) in [(201u64, &mut b), (202u64, &mut c)] {
        q.pool_id = format!("0x{i:040x}");
        q.multi_asset.as_mut().unwrap().pool_id = format!("{}0001{}", q.pool_id, "0".repeat(20));
    }
    let token = format!("0x{}", "f".repeat(40));
    b.token1 = token.clone();
    b.multi_asset.as_mut().unwrap().assets[1].token = token.clone();
    mm(&mut b).schedule.as_mut().unwrap().tokens[1].token = token;
    let t = m(&b).timestamp;
    // B has already reached its own terminal weights and fee at source H.
    mm(&mut b).schedule.as_mut().unwrap().pool_state_word = Amount(
        uint!(t - 101)
            | (uint!(t - 1) << 32usize)
            | (uint!(t - 101) << 64usize)
            | (uint!(t - 1) << 96usize)
            | (uint!(ONE / 100) << 128usize)
            | (uint!(3 * ONE / 100) << 190usize)
            | (uint!(9u8) << 252usize),
    );
    project(&mut b);
    c.protocol = xone_domain::managed_v2_state::PROTOCOL.into();
    c.adapter_version = xone_domain::managed_v2_state::ADAPTER_VERSION.into();
    mm(&mut c).schedule = None;
    mm(&mut c).gradual_weight_end = 0;
    mm(&mut c).gradual_fee_end = 0;
    c.multi_asset.as_mut().unwrap().weights =
        vec![a("800000000000000000"), a("200000000000000000")];
    for (idx, n) in [102u64, 103u64].iter().enumerate() {
        c.multi_asset.as_mut().unwrap().assets[idx].token = format!("0x{n:040x}");
    }
    c.token0 = c.multi_asset.as_ref().unwrap().assets[0].token.clone();
    c.token1 = c.multi_asset.as_ref().unwrap().assets[1].token.clone();
    for q in [&mut p, &mut b, &mut c] {
        let t = m(q).timestamp;
        let m = mm(q);
        m.vault_paused = true;
        m.vault_pause_window_end = t;
        m.vault_buffer_period_end = t + 20;
    }
    mm(&mut b).paused = true;
    mm(&mut b).pause_window_end = t;
    mm(&mut b).buffer_period_end = t + 30;
    let map = BTreeMap::from([
        (p.pool_id.clone(), p),
        (b.pool_id.clone(), b),
        (c.pool_id.clone(), c),
    ]);
    xone_domain::validate_pool_dependencies(map.values()).unwrap();
    map
}
#[test]
fn future_swap_reclocks_all_three_peers_with_own_getters_and_ledgers() {
    let mut map = pools();
    let before = map.clone();
    let p = map
        .values()
        .find(|p| {
            p.pool_id != format!("0x{:040x}", 201u64) && p.pool_id != format!("0x{:040x}", 202u64)
        })
        .unwrap()
        .clone();
    let t = m(&p).timestamp + 60;
    let block = m(&p).block_number + 1;
    let next = quote_at(
        &p,
        &a("1000000000000000000"),
        &p.token0,
        &p.token1,
        block,
        t,
    )
    .unwrap()
    .next_pool;
    shared_vault::propagate(&mut map, &next).unwrap();
    map.insert(next.pool_id.clone(), next.clone());
    xone_domain::validate_pool_dependencies(map.values()).unwrap();
    for (id, r) in &map {
        let old = &before[id];
        let view = reclock(old, block, t).unwrap();
        assert_eq!(
            r.multi_asset.as_ref().unwrap().weights,
            view.multi_asset.as_ref().unwrap().weights
        );
        assert_eq!(r.fee_numerator, view.fee_numerator);
        assert_eq!(m(r).execution_context, m(&next).execution_context);
        assert_eq!(m(r).block_hash, m(old).block_hash);
        assert_eq!(m(r).timestamp, m(old).timestamp);
        assert!(!m(r).paused && !m(r).vault_paused);
        assert_eq!(m(r).schedule, m(old).schedule);
        assert_eq!(m(r).total_supply, m(old).total_supply);
        assert_eq!(m(r).virtual_supply, m(old).virtual_supply);
        assert_eq!(m(r).manager_bpt_balance, m(old).manager_bpt_balance);
        assert_eq!(
            m(r).pending_aum,
            view_aum(
                &m(r).virtual_supply,
                t,
                m(r).last_collection_timestamp,
                &m(r).management_aum_fee
            )
            .unwrap()
            .pending_bpt
        );
        if id != &p.pool_id {
            assert_eq!(r.reserve0, old.reserve0);
            assert_eq!(r.reserve1, old.reserve1);
            for ((asset, info), oi) in r
                .multi_asset
                .as_ref()
                .unwrap()
                .assets
                .iter()
                .zip(&m(r).token_info)
                .zip(&m(old).token_info)
            {
                assert_eq!(info.cash, oi.cash);
                assert_eq!(info.managed, oi.managed);
                assert_eq!(info.last_change_block, oi.last_change_block);
                if asset.token == p.token0 {
                    assert_eq!(
                        info.vault_actual_balance,
                        m(&next).token_info[0].vault_actual_balance
                    );
                } else {
                    assert_eq!(info.vault_actual_balance, oi.vault_actual_balance);
                }
            }
        }
        assert!(r.multi_asset.as_ref().unwrap().to_proto().is_err());
    }
}
#[test]
fn invalid_disjoint_peer_makes_propagation_atomic() {
    let mut map = pools();
    let p = map
        .values()
        .find(|p| {
            p.pool_id != format!("0x{:040x}", 201u64) && p.pool_id != format!("0x{:040x}", 202u64)
        })
        .unwrap()
        .clone();
    let next = quote_at(
        &p,
        &a("1000000000000000000"),
        &p.token0,
        &p.token1,
        m(&p).block_number + 1,
        m(&p).timestamp + 60,
    )
    .unwrap()
    .next_pool;
    let disjoint = map.get_mut(&format!("0x{:040x}", 202u64)).unwrap();
    mm(disjoint).pending_aum.0 += a("1").0;
    mm(disjoint).actual_supply.0 += a("1").0;
    let before = map.clone();
    assert_eq!(
        shared_vault::propagate(&mut map, &next).unwrap_err().0,
        "MANAGED_V2_AUM_VIEW_MISMATCH"
    );
    assert_eq!(map, before);
}
#[test]
fn conflicting_input_group_rejected_before_path_mutation() {
    let mut map = pools();
    let id = map.keys().next().unwrap().clone();
    let p = map.get_mut(&id).unwrap();
    mm(p).token_info[0].vault_actual_balance.0 += a("1").0;
    let before = map.clone();
    assert!(xone_routing::evaluate_path(&mut map, &[], &a("1")).is_err());
    assert_eq!(map, before);
}

#[test]
fn same_clock_mixed_profiles_route_twice_and_late_failure_is_atomic() {
    let mut map = pools();
    // Expired pause getters are projected for every peer before route preflight.
    for p in map.values_mut() {
        *p = reclock(p, m(p).block_number, m(p).timestamp + 111).unwrap();
    }
    let selected = map
        .values()
        .find(|p| {
            p.pool_id != format!("0x{:040x}", 201u64) && p.pool_id != format!("0x{:040x}", 202u64)
        })
        .unwrap()
        .clone();
    let edge = xone_routing::Edge {
        pool_id: selected.pool_id.clone(),
        protocol: selected.protocol.clone(),
        from_token: Some(selected.token0.clone()),
        to: selected.token1.clone(),
        zero_for_one: true,
    };
    let before = map.clone();
    let mut invalid = edge.clone();
    invalid.from_token = Some(selected.token1.clone());
    invalid.to = format!("0x{:040x}", 999u64);
    assert!(xone_routing::evaluate_path(
        &mut map,
        &[edge.clone(), invalid],
        &a("1000000000000000000")
    )
    .is_err());
    assert_eq!(map, before);
    let out = xone_routing::evaluate_path(&mut map, &[edge], &a("1000000000000000000")).unwrap();
    assert!(out > Amount::zero());
    xone_domain::validate_pool_dependencies(map.values()).unwrap();
    let back = xone_routing::Edge {
        pool_id: selected.pool_id.clone(),
        protocol: selected.protocol,
        from_token: Some(selected.token1),
        to: selected.token0,
        zero_for_one: false,
    };
    assert!(xone_routing::evaluate_path(&mut map, &[back], &out).unwrap() > Amount::zero());
}

#[test]
fn fixed_source_clock_none_and_some_are_consistent_and_disjoint_peer_is_unchanged() {
    let mut map = pools();
    for p in map.values_mut() {
        mm(p).paused = false;
        mm(p).vault_paused = false;
    }
    let id = format!("0x{:040x}", 201u64);
    let peer = &map[&id];
    let peer = reclock(peer, m(peer).block_number, m(peer).timestamp).unwrap();
    map.insert(id, peer);
    xone_domain::validate_pool_dependencies(map.values()).unwrap();
    let disjoint_id = format!("0x{:040x}", 202u64);
    let disjoint = map[&disjoint_id].clone();
    let selected = map
        .values()
        .find(|p| p.pool_id != format!("0x{:040x}", 201u64) && p.pool_id != disjoint_id)
        .unwrap()
        .clone();
    let edge = xone_routing::Edge {
        pool_id: selected.pool_id,
        protocol: selected.protocol,
        from_token: Some(selected.token0),
        to: selected.token1,
        zero_for_one: true,
    };
    xone_routing::evaluate_path(&mut map, &[edge], &a("1000000000000000000")).unwrap();
    assert_eq!(map[&disjoint_id], disjoint);
    xone_domain::validate_pool_dependencies(map.values()).unwrap();
}
#[test]
fn future_collection_with_no_underlying_wallet_delta_reclocks_all_peers() {
    let mut map = pools();
    let before = map.clone();
    let p = map
        .values()
        .find(|p| {
            p.pool_id != format!("0x{:040x}", 201u64) && p.pool_id != format!("0x{:040x}", 202u64)
        })
        .unwrap()
        .clone();
    let block = m(&p).block_number + 1;
    let time = m(&p).timestamp + 60;
    let next = xone_protocol_math::managed_v2_pool::collect_at(&p, block, time)
        .unwrap()
        .next_pool;
    shared_vault::propagate(&mut map, &next).unwrap();
    map.insert(next.pool_id.clone(), next);
    xone_domain::validate_pool_dependencies(map.values()).unwrap();
    for (id, r) in &map {
        assert_eq!(m(r).token_info, m(&before[id]).token_info);
        assert_eq!(m(r).execution_context.as_ref().unwrap().timestamp, time);
        if id != &p.pool_id {
            assert_eq!(*r, reclock(&before[id], block, time).unwrap());
        }
    }
}
