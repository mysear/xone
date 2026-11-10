//! Synthetic source-pool projections. These do not attest captured native parity.
use num_bigint::BigUint;
use serde_json::Value;
use xone_domain::{
    managed_v2_schedule::{ManagedV2Schedule, ManagedV2ScheduledToken, ADAPTER_VERSION, PROTOCOL},
    managed_v2_state::BalancerManagedV2State,
    Amount, Pool,
};
use xone_protocol_math::{
    managed_v2_pool::{collect_at, quote_at, quote_exact_in_tokens, reclock},
    managed_v2_swap::{quote_regular, WeightedInputs},
};
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
                state_word: Amount(BigUint::from(start) | (BigUint::from(end) << 64usize)),
            }
        })
        .collect();
    let raw = ManagedV2Schedule {
        pool_state_word: Amount(
            BigUint::from(t + 10)
                | (BigUint::from(t + 110) << 32usize)
                | (BigUint::from(t + 10) << 64usize)
                | (BigUint::from(t + 110) << 96usize)
                | (BigUint::from(ONE / 100) << 128usize)
                | (BigUint::from(3 * ONE / 100) << 190usize)
                | (BigUint::from(9u8) << 252usize),
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
#[test]
fn scheduled_dispatch_uses_raw_selected_weights_without_sum_normalization() {
    let p = fixture();
    let s = p.multi_asset.as_ref().unwrap();
    assert_eq!(
        s.weights.iter().map(|w| w.0.clone()).sum::<BigUint>(),
        BigUint::from(ONE + 1)
    );
    let q = xone_protocol_math::multi_asset::quote_exact_in_tokens(
        &p,
        &a("1000000000000000000"),
        &p.token0,
        &p.token1,
    )
    .unwrap();
    assert!(q.amount_out > Amount::zero());
    let cash = m(&p)
        .token_info
        .iter()
        .map(|i| i.cash.0.clone())
        .collect::<Vec<_>>();
    let managed = m(&p)
        .token_info
        .iter()
        .map(|i| i.managed.0.clone())
        .collect::<Vec<_>>();
    let weights = s.weights.iter().map(|w| w.0.clone()).collect::<Vec<_>>();
    let scales = s
        .assets
        .iter()
        .map(|a| a.scaling.0.clone())
        .collect::<Vec<_>>();
    assert!(quote_regular(
        WeightedInputs {
            cash: &cash,
            managed: &managed,
            weights: &weights,
            scales: &scales,
            fee: &BigUint::from(p.fee_numerator)
        },
        &BigUint::from(1u8),
        0,
        1
    )
    .is_err());
}
#[test]
fn future_reclock_projects_views_and_preserves_source_and_ledgers() {
    let p = fixture();
    let t = m(&p).timestamp;
    let b = m(&p).block_number;
    for time in [t + 9, t + 10, t + 11, t + 60, t + 109, t + 110, t + 111] {
        let r = reclock(&p, b + 1, time).unwrap();
        let v = m(&p).schedule.as_ref().unwrap().at(time).unwrap();
        assert_eq!(r.multi_asset.as_ref().unwrap().weights, v.weights);
        assert_eq!(r.fee_numerator.to_string(), v.swap_fee.to_string());
        assert_eq!(m(&r).schedule, m(&p).schedule);
        assert_eq!(m(&r).block_hash, m(&p).block_hash);
        assert_eq!(m(&r).timestamp, t);
        assert_eq!(m(&r).block_number, b);
        assert_eq!(m(&r).token_info, m(&p).token_info);
        assert_eq!(m(&r).total_supply, m(&p).total_supply);
        assert_eq!(reclock(&r, b + 1, time).unwrap(), r);
        for (ti, to) in [(&p.token0, &p.token1), (&p.token1, &p.token0)] {
            let q = quote_at(&p, &a("1000000000000000000"), ti, to, b + 1, time).unwrap();
            assert_eq!(
                quote_exact_in_tokens(&r, &a("1000000000000000000"), ti, to)
                    .unwrap()
                    .next_pool,
                q.next_pool
            );
            let repeated =
                quote_exact_in_tokens(&q.next_pool, &a("1000000000000000000"), to, ti).unwrap();
            let c = collect_at(&repeated.next_pool, b + 1, time).unwrap();
            assert_eq!(
                collect_at(&c.next_pool, b + 1, time).unwrap().next_pool,
                c.next_pool
            );
            assert_eq!(m(&q.next_pool).total_supply, m(&p).total_supply);
        }
    }
}
#[test]
fn bad_source_getters_missing_raw_and_old_promotion_rejected() {
    for case in 0..6 {
        let mut p = fixture();
        match case {
            0 => p.fee_numerator += 1,
            1 => p.multi_asset.as_mut().unwrap().weights[0].0 += BigUint::from(1u8),
            2 => p.multi_asset.as_mut().unwrap().assets[0].scaling.0 += BigUint::from(1u8),
            3 => mm(&mut p).schedule = None,
            4 => {
                p.protocol = xone_domain::managed_v2_state::PROTOCOL.into();
                p.adapter_version = xone_domain::managed_v2_state::ADAPTER_VERSION.into();
            }
            _ => {
                mm(&mut p).pending_aum.0 += BigUint::from(1u8);
                mm(&mut p).actual_supply.0 += BigUint::from(1u8);
            }
        };
        let before = p.clone();
        let t = m(&p).timestamp;
        let b = m(&p).block_number;
        assert!(reclock(&p, b + 1, t + 111).is_err(), "case {case}");
        assert_eq!(p, before);
    }
}
#[test]
fn explicit_clock_cannot_rewind_source_context_or_collection() {
    let p = fixture();
    let t = m(&p).timestamp;
    let b = m(&p).block_number;
    assert!(reclock(&p, b - 1, t).is_err());
    assert!(reclock(&p, b, t - 1).is_err());
    let r = reclock(&p, b + 2, t + 111).unwrap();
    assert!(reclock(&r, b + 1, t + 111).is_err());
    assert!(reclock(&r, b + 2, t + 110).is_err());
    assert!(m(&r).execution_context.is_some());
    assert!(r.multi_asset.as_ref().unwrap().to_proto().is_err());
    let bank: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/managed-v2-aum-wire.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut snap: xone_domain::Snapshot = serde_json::from_value(bank["snapshot"].clone()).unwrap();
    snap.pools = vec![r];
    snap.reference.scope_hash = xone_domain::scope_hash(&snap.pools).unwrap();
    snap.reference.adapter_set_hash =
        xone_domain::hash(&vec![&snap.pools[0].adapter_version]).unwrap();
    assert!(snap
        .validate(
            &snap.reference.owner_service,
            snap.reference.owner_assignment_version
        )
        .is_err());
    assert!(xone_domain::wire_conversion::snapshot_to_proto(&snap).is_err());
}

#[test]
fn three_tokens_current_sum_plus_minus_two_keeps_exact_selected_ratio() {
    for offset in [-2i64, -1, 0, 1, 2] {
        let mut p = fixture();
        let mut asset = p.multi_asset.as_ref().unwrap().assets[1].clone();
        asset.token = format!("0x{}", "f".repeat(40));
        let info = m(&p).token_info[1].clone();
        p.multi_asset.as_mut().unwrap().assets.push(asset.clone());
        mm(&mut p).token_info.push(info);
        let starts = [
            (3 * ONE / 10) as i64,
            (3 * ONE / 10) as i64,
            (4 * ONE / 10) as i64 + offset,
        ];
        let ends = [2 * ONE / 10, 4 * ONE / 10, 4 * ONE / 10];
        let tokens = p
            .multi_asset
            .as_ref()
            .unwrap()
            .assets
            .iter()
            .enumerate()
            .map(|(i, a)| ManagedV2ScheduledToken {
                token: a.token.clone(),
                state_word: Amount(
                    BigUint::from(starts[i] as u64) | (BigUint::from(ends[i]) << 64usize),
                ),
            })
            .collect();
        mm(&mut p).schedule.as_mut().unwrap().tokens = tokens;
        project(&mut p);
        p.validate().unwrap();
        let b = m(&p).block_number;
        let t = m(&p).timestamp;
        for time in [t, t + 11, t + 60, t + 109, t + 110] {
            let r = reclock(&p, b, time).unwrap();
            let s = r.multi_asset.as_ref().unwrap();
            let cash = m(&r)
                .token_info
                .iter()
                .map(|i| i.cash.0.clone())
                .collect::<Vec<_>>();
            let managed = m(&r)
                .token_info
                .iter()
                .map(|i| i.managed.0.clone())
                .collect::<Vec<_>>();
            let mut weights = s.weights.iter().map(|a| a.0.clone()).collect::<Vec<_>>();
            weights[2] = BigUint::from(ONE) - &weights[0] - &weights[1];
            let scales = s
                .assets
                .iter()
                .map(|a| a.scaling.0.clone())
                .collect::<Vec<_>>();
            for (i, j) in [(0, 1), (1, 0)] {
                let expected = quote_regular(
                    WeightedInputs {
                        cash: &cash,
                        managed: &managed,
                        weights: &weights,
                        scales: &scales,
                        fee: &BigUint::from(r.fee_numerator),
                    },
                    &a("1000000000000000000").0,
                    i,
                    j,
                )
                .unwrap();
                let q = quote_exact_in_tokens(
                    &r,
                    &a("1000000000000000000"),
                    &s.assets[i].token,
                    &s.assets[j].token,
                )
                .unwrap();
                assert_eq!(
                    q.amount_out.0, expected.amount_out,
                    "offset={offset} t={time} direction={i}"
                );
            }
        }
    }
}
#[test]
fn scheduled_minimum_raw_unit_retains_upscale_then_fee_order() {
    let mut p = fixture();
    let raw = mm(&mut p).schedule.as_mut().unwrap();
    for (i, row) in raw.tokens.iter_mut().enumerate() {
        row.state_word = Amount(
            BigUint::from(ONE / 2)
                | (BigUint::from(ONE / 2) << 64usize)
                | (BigUint::from(if i == 0 { 12u8 } else { 0u8 }) << 128usize),
        );
    }
    for (i, balance) in [a("1000000"), a("2000000000000000000")]
        .into_iter()
        .enumerate()
    {
        p.multi_asset.as_mut().unwrap().assets[i].balance = balance.clone();
        mm(&mut p).token_info[i].cash = balance.clone();
        mm(&mut p).token_info[i].managed = Amount::zero();
        mm(&mut p).token_info[i].vault_actual_balance = balance;
    }
    p.reserve0 = a("1000000");
    p.reserve1 = a("2000000000000000000");
    project(&mut p);
    p.validate().unwrap();
    let q = quote_exact_in_tokens(&p, &a("1"), &p.token0, &p.token1).unwrap();
    assert_eq!(q.amount_out, a("1979998039800"));
    assert_eq!(m(&q.next_pool).token_info[0].cash, a("1000001"));
    assert_eq!(
        m(&q.next_pool).token_info[0].vault_actual_balance,
        a("1000001")
    );
}

#[test]
fn scheduled_future_collection_keeps_bpt_alias_wallet_and_registered_ledgers_separate() {
    for alias in 0..4 {
        let mut p = fixture();
        let vault = p.multi_asset.as_ref().unwrap().vault.clone();
        let base = mm(&mut p);
        base.vault_bpt_actual_balance = Amount(&base.vault_bpt_cash.0 + 5u8);
        if alias == 1 || alias == 3 {
            base.manager = vault.clone();
            base.manager_bpt_balance = base.vault_bpt_actual_balance.clone();
        }
        if alias == 2 || alias == 3 {
            base.protocol_collector = vault.clone();
            base.protocol_collector_bpt_balance = base.vault_bpt_actual_balance.clone();
        }
        p.validate().unwrap();
        let before = m(&p).clone();
        let block = before.block_number + 1;
        let time = before.timestamp + 60;
        let c = collect_at(&p, block, time).unwrap();
        let after = m(&c.next_pool);
        let minted_wallet = if alias == 1 {
            c.accrual.manager_bpt.clone()
        } else if alias == 2 {
            c.accrual.protocol_bpt.clone()
        } else if alias == 3 {
            c.accrual.total_bpt.clone()
        } else {
            Amount::zero()
        };
        assert_eq!(
            after.vault_bpt_actual_balance,
            Amount(&before.vault_bpt_actual_balance.0 + minted_wallet.0)
        );
        assert_eq!(after.vault_bpt_cash, before.vault_bpt_cash);
        assert_eq!(after.token_info, before.token_info);
        assert_eq!(after.schedule, before.schedule);
        assert_eq!(after.block_hash, before.block_hash);
        assert_eq!(c.next_pool.fee_numerator, 2 * ONE / 100);
        assert_eq!(after.pending_aum, Amount::zero());
        assert_eq!(
            collect_at(&c.next_pool, block, time).unwrap().next_pool,
            c.next_pool
        );
        quote_exact_in_tokens(
            &c.next_pool,
            &a("1000000000000000000"),
            &p.token0,
            &p.token1,
        )
        .unwrap();
    }
}
#[test]
fn scheduled_eth_clock_packed_u32_and_original_guards_remain_exact() {
    let mut p = fixture();
    p.chain_key = "evm:1".into();
    let time = m(&p).timestamp + 60;
    let block = (1u64 << 32) + 17;
    let q = quote_at(
        &p,
        &a("1000000000000000000"),
        &p.token0,
        &p.token1,
        block,
        time,
    )
    .unwrap();
    assert_eq!(m(&q.next_pool).token_info[0].last_change_block, 17);
    assert_eq!(
        m(&q.next_pool)
            .execution_context
            .as_ref()
            .unwrap()
            .timestamp,
        time
    );
    assert_eq!(m(&q.next_pool).block_number, m(&p).block_number);
    let mut invalid = p.clone();
    mm(&mut invalid).token_info[0].last_change_block = 1u64 << 32;
    assert!(reclock(&invalid, block, time).is_err());
    assert!(quote_at(
        &p,
        &a("4000000000000000000000"),
        &p.token0,
        &p.token1,
        block,
        time
    )
    .is_err());
    let mut wallet = p.clone();
    mm(&mut wallet).token_info[0].vault_actual_balance =
        Amount((BigUint::from(1u8) << 256usize) - BigUint::from(1u8));
    assert_eq!(
        quote_at(
            &wallet,
            &a("1000000000000000000"),
            &p.token0,
            &p.token1,
            block,
            time
        )
        .unwrap_err()
        .0,
        "MANAGED_V2_UINT256_OVERFLOW"
    );
}
