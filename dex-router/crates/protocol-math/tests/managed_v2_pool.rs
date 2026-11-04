#![recursion_limit = "256"]
use serde_json::json;
use xone_domain::{Amount, Pool};
use xone_protocol_math::multi_asset::quote_exact_in_tokens;
fn a(s: &str) -> Amount {
    s.parse().unwrap()
}
fn fixture() -> Pool {
    let address = |i: u8| format!("0x{i:040x}");
    let code = "a".repeat(64);
    let template = "b".repeat(64);
    let roles = [
        "circuit-breaker-lib",
        "managed-pool-add-remove-token-lib",
        "managed-pool-amm-lib",
        "external-weighted-math",
        "recovery-mode-helper",
    ];
    let deps=roles.iter().enumerate().map(|(i,role)|json!({"role":role,"address":address(100+i as u8),"code_sha256":code,"template_sha256":template})).collect::<Vec<_>>();
    // Synthetic pricing state with original observed AUM monetary values; not
    // a claim that these synthetic addresses/templates were actually deployed.
    serde_json::from_value(json!({"chain_key":"evm:31337","pool_id":address(70),"protocol":"balancer-v2-managed-aum","adapter_version":"balancer-v2/20230411-managed-aum-3","token0":address(1),"token1":address(2),"reserve0":"10000000000000000000000","reserve1":"20000000000000000000000","fee_numerator":10000000000000000u64,"fee_denominator":1000000000000000000u64,"token_behavior":"standard","multi_asset":{"code_sha256":code,"vault":address(71),"vault_code_sha256":code,"pool_id":format!("{}0001{}",address(70),"0".repeat(20)),"assets":[{"token":address(1),"balance":"10000000000000000000000","scaling":"1000000000000000000","code_sha256":code},{"token":address(2),"balance":"20000000000000000000000","scaling":"1000000000000000000","code_sha256":code}],"weights":["800000000000000000","200000000000000000"],"amplification":"0","admin_fee":0,"managed_v2":{"timestamp":1769817600,"block_number":100,"block_hash":format!("0x{}","a".repeat(64)),"factory":address(80),"factory_code_sha256":code,"pool_runtime_template_sha256":template,"fee_provider":address(81),"fee_provider_code_sha256":code,"protocol_collector":address(82),"protocol_collector_code_sha256":code,"dependencies":deps,"total_supply":"2596148429267413814265248164610048","virtual_supply":"22973967099940240610944","actual_supply":"22993040547096687469068","pending_aum":"19073447156446858124","vault_bpt_cash":"2596148429244439847165307923999104","vault_bpt_managed":"0","vault_bpt_actual_balance":"2596148429244439847165307923999104","bpt_index":0,"manager":address(83),"manager_bpt_balance":"0","protocol_collector_bpt_balance":"0","management_aum_fee":"10000000000000000","last_collection_timestamp":1767225600,"provider_fee_id":"3","live_provider_aum_percentage":"700000000000000000","cached_protocol_aum_percentage":"300000000000000000","swaps_enabled":true,"paused":false,"vault_paused":false,"vault_pause_window_end":0,"vault_buffer_period_end":0,"recovery_mode":false,"allowlist_enabled":false,"pause_window_end":0,"buffer_period_end":0,"gradual_weight_end":0,"gradual_fee_end":0,"token_info":[{"cash":"10000000000000000000000","managed":"0","vault_actual_balance":"10000000000000000000000","asset_manager":address(0),"last_change_block":99,"circuit_breaker_enabled":false},{"cash":"20000000000000000000000","managed":"0","vault_actual_balance":"20000000000000000000000","asset_manager":address(0),"last_change_block":99,"circuit_breaker_enabled":false}]}}})).unwrap()
}
#[test]
fn normal_dispatch_updates_cash_but_does_not_mint_bpt_or_collect_aum() {
    let p = fixture();
    p.validate().unwrap();
    let before = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .clone();
    let q = quote_exact_in_tokens(&p, &a("100000000000000000000"), &p.token0, &p.token1).unwrap();
    assert_eq!(q.amount_out, a("772779500379197940000"));
    let m = q
        .next_pool
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert_eq!(m.total_supply, before.total_supply);
    assert_eq!(m.virtual_supply, before.virtual_supply);
    assert_eq!(
        m.last_collection_timestamp,
        before.last_collection_timestamp
    );
    assert_eq!(m.pending_aum, before.pending_aum);
    assert_eq!(m.manager_bpt_balance, before.manager_bpt_balance);
    assert_eq!(
        m.protocol_collector_bpt_balance,
        before.protocol_collector_bpt_balance
    );
    assert_eq!(m.token_info[0].cash, a("10100000000000000000000"));
    assert_eq!(m.token_info[1].cash, a("19227220499620802060000"));
    assert_eq!(m.token_info[0].last_change_block, 100);
}
#[test]
fn ordinary_swap_rejects_inconsistent_aum_view_and_live_flags_without_touching_source() {
    for bad in 0..4 {
        let mut p = fixture();
        let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
        match bad {
            0 => {
                m.pending_aum = a("1");
                m.actual_supply = Amount(&m.virtual_supply.0 + &m.pending_aum.0);
            }
            1 => m.swaps_enabled = false,
            2 => {
                m.paused = true;
                m.buffer_period_end = m.timestamp + 1;
            }
            _ => {
                m.token_info[1].cash = a("1");
                m.token_info[1].asset_manager = format!("0x{:040x}", 84u8);
                m.token_info[1].managed = Amount(&p.reserve1.0 - &m.token_info[1].cash.0);
            }
        }
        let saved = p.clone();
        assert!(
            quote_exact_in_tokens(&p, &a("100000000000000000000"), &p.token0, &p.token1).is_err(),
            "bad={bad}"
        );
        assert_eq!(p, saved);
    }
}

#[test]
fn full_pool_collection_is_separate_continuous_and_preserves_source_and_cash() {
    use xone_protocol_math::managed_v2_pool::collect_at;
    let original = fixture();
    let m = original
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    let block = m.block_number;
    let time = m.timestamp;
    let collected = collect_at(&original, block, time).unwrap();
    let after = collected.next_pool.multi_asset.as_ref().unwrap();
    let post = after.managed_v2.as_ref().unwrap();
    assert_eq!(post.total_supply, a("2596148429267432887712404611468172"));
    assert_eq!(post.virtual_supply, m.actual_supply);
    assert_eq!(post.actual_supply, m.actual_supply);
    assert_eq!(post.pending_aum, Amount::zero());
    assert_eq!(
        post.protocol_collector_bpt_balance,
        a("5722034146934057438")
    );
    assert_eq!(post.manager_bpt_balance, a("13351413009512800686"));
    assert_eq!(post.last_collection_timestamp, time as u32);
    assert_eq!(post.timestamp, m.timestamp);
    assert_eq!(post.block_hash, m.block_hash);
    assert_eq!(post.block_number, m.block_number);
    assert_eq!(after.assets, original.multi_asset.as_ref().unwrap().assets);
    assert_eq!(post.token_info, m.token_info);
    let repeated = collect_at(&collected.next_pool, block, time).unwrap();
    assert_eq!(repeated.accrual.total_bpt, Amount::zero());
    assert_eq!(repeated.next_pool, collected.next_pool);
}
#[test]
fn ordinary_swap_buffer_end_equality_stays_paused_and_source_views_do_not_use_live_fee() {
    use xone_protocol_math::managed_v2_pool::quote_at;
    let mut p = fixture();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    let time = m.timestamp;
    let block = m.block_number;
    m.paused = true;
    m.buffer_period_end = time;
    assert!(quote_at(
        &p,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        block,
        time
    )
    .is_err());
    assert!(quote_at(
        &p,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        block + 1,
        time + 1
    )
    .is_ok());
    let mut p = fixture();
    let before = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .clone();
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .live_provider_aum_percentage = a("1000000000000000000");
    let q = quote_exact_in_tokens(&p, &a("100000000000000000000"), &p.token0, &p.token1).unwrap();
    assert_eq!(
        q.next_pool
            .multi_asset
            .unwrap()
            .managed_v2
            .unwrap()
            .pending_aum,
        before.pending_aum
    );
}

#[test]
fn vault_last_change_block_matches_original_uint32_packing() {
    use xone_protocol_math::managed_v2_pool::quote_at;
    let p = fixture();
    let time = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .timestamp;
    for block in [u64::from(u32::MAX), 1u64 << 32, (1u64 << 32) + 1] {
        let q = quote_at(
            &p,
            &a("100000000000000000000"),
            &p.token0,
            &p.token1,
            block,
            time,
        )
        .unwrap();
        let m = q
            .next_pool
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        // Original BalanceAllocation._pack shifts block.number left by 224 in
        // a uint256, so getPoolTokenInfo exposes only the low 32 bits.
        for info in &m.token_info {
            assert_eq!(info.last_change_block, block & u64::from(u32::MAX));
        }
        assert_eq!(m.block_number, 100);
        assert_eq!(m.timestamp, time);
    }
}

#[test]
fn full_state_operations_reject_unrepresentable_vault_history() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    let mut p = fixture();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    let time = m.timestamp;
    m.token_info[0].last_change_block = 1u64 << 32;
    let saved = p.clone();
    // Even when the selected execution block is later, this value cannot have
    // come from the original Vault's packed getPoolTokenInfo getter.
    assert!(quote_at(
        &p,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        (1u64 << 32) + 1,
        time
    )
    .is_err());
    assert!(collect_at(&p, (1u64 << 32) + 1, time).is_err());
    assert_eq!(p, saved);
}

#[test]
fn derived_swap_cannot_rewind_vault_history_to_source_block() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    let p = fixture();
    let time = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .timestamp;
    let post = quote_at(
        &p,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        101,
        time,
    )
    .unwrap()
    .next_pool;
    let saved = post.clone();
    assert!(quote_at(
        &post,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        100,
        time
    )
    .is_err());
    assert!(collect_at(&post, 100, time).is_err());
    assert!(quote_at(
        &post,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        101,
        time
    )
    .is_ok());
    assert_eq!(post, saved);
}

#[test]
fn derived_collection_cannot_rewind_fee_clock_to_source_time() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    let p = fixture();
    let time = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .timestamp;
    let post = collect_at(&p, 101, time + 1).unwrap().next_pool;
    let saved = post.clone();
    assert!(collect_at(&post, 101, time).is_err());
    assert!(quote_at(
        &post,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        101,
        time
    )
    .is_err());
    let repeated = collect_at(&post, 101, time + 1).unwrap();
    assert_eq!(repeated.accrual.total_bpt, Amount::zero());
    assert_eq!(repeated.next_pool, post);
    assert!(quote_at(
        &post,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        101,
        time + 1
    )
    .is_ok());
    assert_eq!(post, saved);
}

#[test]
fn vault_recipient_alias_collection_preserves_registered_cash_managed_and_same_clock_supply() {
    use xone_protocol_math::managed_v2_pool::collect_at;
    let mut p = fixture();
    let s = p.multi_asset.as_mut().unwrap();
    let m = s.managed_v2.as_mut().unwrap();
    m.manager = s.vault.clone();
    m.protocol_collector = s.vault.clone();
    // A global ERC20 wallet can include BPT registered to other pools. Its
    // balance must stay separate from this pool's own cash + managed BPT.
    let registered_bpt = Amount(&m.vault_bpt_cash.0 + &m.vault_bpt_managed.0);
    m.manager_bpt_balance = Amount(&registered_bpt.0 + 5u8);
    m.protocol_collector_bpt_balance = m.manager_bpt_balance.clone();
    m.vault_bpt_actual_balance = m.manager_bpt_balance.clone();
    // The actual published composable registration fixes BPT managed to zero.
    m.vault_bpt_managed = Amount::zero();
    m.vault_bpt_cash = registered_bpt;
    for info in &mut m.token_info {
        info.managed = a("1000000000000000000000");
        info.asset_manager = format!("0x{:040x}", 84u8);
        info.cash = Amount(&info.cash.0 - &info.managed.0);
    }
    let before = m.clone();
    let saved = p.clone();
    let q = collect_at(&p, before.block_number, before.timestamp).unwrap();
    let next = q.next_pool.multi_asset.as_ref().unwrap();
    let after = next.managed_v2.as_ref().unwrap();
    assert_eq!(
        after.manager_bpt_balance,
        Amount(&before.manager_bpt_balance.0 + &q.accrual.total_bpt.0)
    );
    assert_eq!(
        after.manager_bpt_balance,
        after.protocol_collector_bpt_balance
    );
    assert_eq!(after.vault_bpt_actual_balance, after.manager_bpt_balance);
    assert_eq!(after.vault_bpt_cash, before.vault_bpt_cash);
    assert_eq!(after.vault_bpt_managed, before.vault_bpt_managed);
    assert_eq!(after.virtual_supply, before.actual_supply);
    assert_eq!(after.actual_supply, before.actual_supply);
    assert_eq!(after.pending_aum, Amount::zero());
    assert_eq!(after.token_info, before.token_info);
    assert_eq!(next.assets, p.multi_asset.as_ref().unwrap().assets);
    let swap = quote_exact_in_tokens(
        &q.next_pool,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
    )
    .unwrap();
    let swapped = swap
        .next_pool
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert_eq!(swapped.actual_supply, before.actual_supply);
    assert_eq!(swapped.manager_bpt_balance, after.manager_bpt_balance);
    assert_eq!(swapped.vault_bpt_cash, before.vault_bpt_cash);
    assert_eq!(swapped.vault_bpt_managed, before.vault_bpt_managed);
    for (got, expected) in swapped.token_info.iter().zip(&before.token_info) {
        assert_eq!(got.managed, expected.managed);
    }
    let repeated = collect_at(&swap.next_pool, before.block_number, before.timestamp).unwrap();
    assert_eq!(repeated.accrual.total_bpt, Amount::zero());
    assert_eq!(repeated.next_pool, swap.next_pool);
    assert_eq!(p, saved);
}

#[test]
fn future_swap_can_continue_at_same_clock_without_changing_source_h() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    let p = fixture();
    let source = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
    let time = source.timestamp + 1;
    let first = quote_at(
        &p,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        101,
        time,
    )
    .unwrap()
    .next_pool;
    collect_at(&first, 101, time).expect("future swap -> same clock collection");
    let second = quote_at(
        &first,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        101,
        time,
    )
    .expect("future swap -> same clock swap")
    .next_pool;
    let collected = collect_at(&second, 101, time)
        .expect("future swap -> same clock collection")
        .next_pool;
    assert!(
        quote_exact_in_tokens(&first, &a("100000000000000000000"), &p.token0, &p.token1).is_ok()
    );
    assert!(collect_at(&collected, 101, time).is_ok());
    for post in [&first, &second, &collected] {
        let m = post
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(
            (m.block_number, m.timestamp, &m.block_hash),
            (source.block_number, source.timestamp, &source.block_hash)
        );
        assert_eq!(
            serde_json::to_value(m).unwrap()["execution_context"],
            json!({"block_number":101,"timestamp":time})
        );
        assert_eq!(
            serde_json::from_slice::<Pool>(&serde_json::to_vec(post).unwrap()).unwrap(),
            *post
        );
        assert!(quote_at(
            post,
            &a("100000000000000000000"),
            &p.token0,
            &p.token1,
            101,
            source.timestamp
        )
        .is_err());
    }
    assert!(serde_json::to_value(source)
        .unwrap()
        .get("execution_context")
        .is_none());
}
#[test]
fn wrapped_vault_history_keeps_full_execution_height_for_swap_and_collection() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    let p = fixture();
    let time = p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .timestamp;
    let block = (1u64 << 32) + 1;
    let post = quote_at(
        &p,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        block,
        time,
    )
    .unwrap()
    .next_pool;
    let m = post
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert_eq!(m.token_info[0].last_change_block, 1);
    assert_eq!(m.block_number, 100);
    assert!(
        quote_at(
            &post,
            &a("100000000000000000000"),
            &p.token0,
            &p.token1,
            100,
            time
        )
        .is_err(),
        "rewound full height to source H"
    );
    assert!(collect_at(&post, 100, time).is_err());
    assert!(quote_at(
        &post,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        block,
        time
    )
    .is_ok());
    let collected = collect_at(&p, block, time).unwrap().next_pool;
    assert!(
        collect_at(&collected, 100, time).is_err(),
        "collection also retains height"
    );
    assert!(quote_at(
        &collected,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        100,
        time
    )
    .is_err());
    assert!(collect_at(&collected, block, time).is_ok());
}

#[test]
fn source_equal_explicit_context_and_all_fixed_h_transitions_stay_derived() {
    use xone_protocol_math::managed_v2_pool::collect_at;
    let p = fixture();
    let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
    let mut derived = serde_json::to_value(&p).unwrap();
    derived["multi_asset"]["managed_v2"]["execution_context"] =
        json!({"block_number":m.block_number,"timestamp":m.timestamp});
    let derived: Pool = serde_json::from_value(derived).unwrap();
    for input in [&p, &derived] {
        let swap = quote_exact_in_tokens(input, &a("100000000000000000000"), &p.token0, &p.token1)
            .unwrap();
        let collection = collect_at(input, m.block_number, m.timestamp).unwrap();
        for post in [&swap.next_pool, &collection.next_pool] {
            let next = post
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap();
            assert!(next.execution_context.is_some());
        }
    }
}

#[test]
fn global_vault_wallet_swap_uses_gross_input_and_full_raw_output_deltas() {
    use xone_protocol_math::managed_v2_pool::quote_at;
    let mut p = fixture();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    for (index, info) in m.token_info.iter_mut().enumerate() {
        info.managed = a("1000000000000000000000");
        info.asset_manager = format!("0x{:040x}", 84u8);
        info.cash = Amount(&info.cash.0 - &info.managed.0);
        info.vault_actual_balance = Amount(&info.cash.0 + (index + 1) * 12345usize);
    }
    m.vault_bpt_actual_balance = Amount(&m.vault_bpt_actual_balance.0 + 5u8);
    let before = m.clone();
    p.validate().unwrap();
    let saved = p.clone();
    let gross = a("100000000000000000000");
    let mut current = p.clone();
    for (i, j) in [(0, 1), (1, 0)] {
        let s = current.multi_asset.as_ref().unwrap();
        let old = s.managed_v2.as_ref().unwrap().clone();
        let q = quote_at(
            &current,
            &gross,
            &s.assets[i].token,
            &s.assets[j].token,
            before.block_number + 1,
            before.timestamp + 1,
        )
        .unwrap();
        let post = q
            .next_pool
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(q.consumed_in, gross);
        assert_eq!(q.remaining_in, Amount::zero());
        assert!(q.amount_out.0 > num_bigint::BigUint::from(0u8));
        assert_eq!(
            post.token_info[i].vault_actual_balance,
            Amount(&old.token_info[i].vault_actual_balance.0 + &gross.0)
        );
        assert_eq!(
            post.token_info[j].vault_actual_balance,
            Amount(&old.token_info[j].vault_actual_balance.0 - &q.amount_out.0)
        );
        for k in 0..2 {
            assert_eq!(post.token_info[k].managed, old.token_info[k].managed);
            assert_eq!(
                &post.token_info[k].vault_actual_balance.0 - &post.token_info[k].cash.0,
                &old.token_info[k].vault_actual_balance.0 - &old.token_info[k].cash.0
            );
        }
        assert_eq!(post.total_supply, before.total_supply);
        assert_eq!(post.virtual_supply, before.virtual_supply);
        assert_eq!(post.vault_bpt_cash, before.vault_bpt_cash);
        assert_eq!(post.vault_bpt_managed, before.vault_bpt_managed);
        assert_eq!(
            post.vault_bpt_actual_balance,
            before.vault_bpt_actual_balance
        );
        assert_eq!(post.manager_bpt_balance, before.manager_bpt_balance);
        assert_eq!(
            post.protocol_collector_bpt_balance,
            before.protocol_collector_bpt_balance
        );
        assert_eq!(
            post.last_collection_timestamp,
            before.last_collection_timestamp
        );
        assert_eq!(
            (post.block_number, post.timestamp, &post.block_hash),
            (before.block_number, before.timestamp, &before.block_hash)
        );
        q.next_pool.validate().unwrap();
        current = q.next_pool;
    }
    assert_eq!(p, saved);
}

#[test]
fn global_vault_input_wallet_uint256_exact_boundary_and_overflow() {
    let max = (num_bigint::BigUint::from(1u8) << 256usize) - 1u8;
    let gross = a("100000000000000000000");
    for i in 0..2 {
        let mut p = fixture();
        let s = p.multi_asset.as_mut().unwrap();
        s.managed_v2.as_mut().unwrap().token_info[i].vault_actual_balance =
            Amount(&max - &gross.0 + 1u8);
        let token_in = s.assets[i].token.clone();
        let token_out = s.assets[1 - i].token.clone();
        p.validate().unwrap();
        let saved = p.clone();
        let error = quote_exact_in_tokens(&p, &gross, &token_in, &token_out)
            .expect_err("global input wallet overflow must reject before returning a state");
        assert!(
            error.to_string().contains("MANAGED_V2_UINT256_OVERFLOW"),
            "{error}"
        );
        assert_eq!(p, saved);
        p.multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap()
            .token_info[i]
            .vault_actual_balance = Amount(&max - &gross.0);
        p.validate().unwrap();
        let q = quote_exact_in_tokens(&p, &gross, &token_in, &token_out).unwrap();
        assert_eq!(
            q.next_pool
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap()
                .token_info[i]
                .vault_actual_balance,
            Amount(max.clone())
        );
    }
}

#[test]
fn global_vault_output_wallet_retains_cash_boundary_and_cannot_replace_registered_cash() {
    let gross = a("100000000000000000000");
    for surplus in [0u8, 1, 17] {
        let mut p = fixture();
        let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
        m.token_info[1].vault_actual_balance = Amount(&m.token_info[1].cash.0 + surplus);
        let before = m.clone();
        let q = quote_exact_in_tokens(&p, &gross, &p.token0, &p.token1).unwrap();
        let post = q
            .next_pool
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(
            post.token_info[1].vault_actual_balance,
            Amount(&before.token_info[1].vault_actual_balance.0 - &q.amount_out.0)
        );
        assert_eq!(
            &post.token_info[1].vault_actual_balance.0 - &post.token_info[1].cash.0,
            num_bigint::BigUint::from(surplus)
        );
        q.next_pool.validate().unwrap();
    }
    let mut p = fixture();
    let quote = quote_exact_in_tokens(&p, &gross, &p.token0, &p.token1).unwrap();
    for surplus in [0u8, 17] {
        let mut boundary = p.clone();
        let m = boundary
            .multi_asset
            .as_mut()
            .unwrap()
            .managed_v2
            .as_mut()
            .unwrap();
        m.token_info[1].cash = quote.amount_out.clone();
        m.token_info[1].asset_manager = format!("0x{:040x}", 84u8);
        m.token_info[1].managed = Amount(&p.reserve1.0 - &quote.amount_out.0);
        m.token_info[1].vault_actual_balance = Amount(&quote.amount_out.0 + surplus);
        boundary.validate().unwrap();
        let paid = quote_exact_in_tokens(&boundary, &gross, &p.token0, &p.token1).unwrap();
        assert_eq!(paid.amount_out, quote.amount_out);
        let post = paid
            .next_pool
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(post.token_info[1].cash, Amount::zero());
        assert_eq!(
            post.token_info[1].vault_actual_balance,
            Amount(num_bigint::BigUint::from(surplus))
        );
        paid.next_pool.validate().unwrap();
    }
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.token_info[1].cash = Amount(&quote.amount_out.0 - 1u8);
    m.token_info[1].asset_manager = format!("0x{:040x}", 84u8);
    m.token_info[1].managed = Amount(&p.reserve1.0 - &m.token_info[1].cash.0);
    p.validate().unwrap();
    assert!(quote_exact_in_tokens(&p, &gross, &p.token0, &p.token1).is_err());
    let mut p = fixture();
    p.multi_asset
        .as_mut()
        .unwrap()
        .managed_v2
        .as_mut()
        .unwrap()
        .token_info[1]
        .vault_actual_balance = Amount(&quote.amount_out.0 - 1u8);
    let saved = p.clone();
    assert!(quote_exact_in_tokens(&p, &gross, &p.token0, &p.token1).is_err());
    assert_eq!(p, saved);
}

#[test]
fn aum_vault_recipient_aliases_mint_each_share_once_and_continue_at_same_clock() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    for alias in 0..5 {
        for share in ["0", "300000000000000000", "1000000000000000000"] {
            let mut p = fixture();
            let s = p.multi_asset.as_mut().unwrap();
            let vault = s.vault.clone();
            let m = s.managed_v2.as_mut().unwrap();
            let registered = Amount(&m.vault_bpt_cash.0 + &m.vault_bpt_managed.0);
            m.vault_bpt_managed = Amount::zero();
            m.vault_bpt_cash = registered.clone();
            m.vault_bpt_actual_balance = Amount(&registered.0 + 5u8);
            match alias {
                1 => m.manager = vault.clone(),
                2 => m.protocol_collector = vault.clone(),
                3 => {
                    m.manager = vault.clone();
                    m.protocol_collector = vault.clone();
                }
                4 => m.protocol_collector = m.manager.clone(),
                _ => {}
            }
            m.manager_bpt_balance = if m.manager == vault {
                m.vault_bpt_actual_balance.clone()
            } else {
                a("19")
            };
            m.protocol_collector_bpt_balance = if m.protocol_collector == vault {
                m.vault_bpt_actual_balance.clone()
            } else if m.protocol_collector == m.manager {
                a("19")
            } else {
                a("23")
            };
            m.cached_protocol_aum_percentage = a(share);
            let before = m.clone();
            p.validate().unwrap();
            let saved = p.clone();
            let block = before.block_number + 1;
            let time = before.timestamp + 1;
            let swap = quote_at(
                &p,
                &a("100000000000000000000"),
                &p.token0,
                &p.token1,
                block,
                time,
            )
            .unwrap()
            .next_pool;
            let pending = swap
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap()
                .actual_supply
                .clone();
            let result = collect_at(&swap, block, time).unwrap();
            let post = result
                .next_pool
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap();
            assert!(result.accrual.total_bpt.0 > num_bigint::BigUint::from(0u8));
            let mut vault_mint = Amount::zero();
            if before.manager == vault {
                vault_mint.0 += &result.accrual.manager_bpt.0;
            }
            if before.protocol_collector == vault {
                vault_mint.0 += &result.accrual.protocol_bpt.0;
            }
            assert_eq!(
                post.vault_bpt_actual_balance,
                Amount(&before.vault_bpt_actual_balance.0 + &vault_mint.0),
                "alias={alias}, share={share}"
            );
            let manager_mint = if before.manager == before.protocol_collector {
                &result.accrual.total_bpt
            } else {
                &result.accrual.manager_bpt
            };
            let collector_mint = if before.manager == before.protocol_collector {
                &result.accrual.total_bpt
            } else {
                &result.accrual.protocol_bpt
            };
            assert_eq!(
                post.manager_bpt_balance,
                Amount(&before.manager_bpt_balance.0 + &manager_mint.0)
            );
            assert_eq!(
                post.protocol_collector_bpt_balance,
                Amount(&before.protocol_collector_bpt_balance.0 + &collector_mint.0)
            );
            assert_eq!(post.vault_bpt_cash, before.vault_bpt_cash);
            assert_eq!(post.vault_bpt_managed, before.vault_bpt_managed);
            assert_eq!(
                post.total_supply,
                Amount(&before.total_supply.0 + &result.accrual.total_bpt.0)
            );
            assert_eq!(post.virtual_supply, pending);
            assert_eq!(post.actual_supply, pending);
            assert_eq!(post.pending_aum, Amount::zero());
            let repeated = collect_at(&result.next_pool, block, time).unwrap();
            assert_eq!(repeated.accrual.total_bpt, Amount::zero());
            assert_eq!(repeated.next_pool, result.next_pool);
            let continued = quote_exact_in_tokens(
                &result.next_pool,
                &a("100000000000000000000"),
                &p.token1,
                &p.token0,
            )
            .unwrap();
            let after_swap = continued
                .next_pool
                .multi_asset
                .as_ref()
                .unwrap()
                .managed_v2
                .as_ref()
                .unwrap();
            assert_eq!(
                after_swap.vault_bpt_actual_balance,
                post.vault_bpt_actual_balance
            );
            assert_eq!(after_swap.manager_bpt_balance, post.manager_bpt_balance);
            assert_eq!(
                after_swap.protocol_collector_bpt_balance,
                post.protocol_collector_bpt_balance
            );
            assert_eq!(after_swap.actual_supply, pending);
            assert_eq!(
                (
                    after_swap.block_number,
                    after_swap.timestamp,
                    &after_swap.block_hash
                ),
                (before.block_number, before.timestamp, &before.block_hash)
            );
            assert_eq!(
                serde_json::to_value(after_swap).unwrap()["execution_context"],
                json!({"block_number":block,"timestamp":time})
            );
            continued.next_pool.validate().unwrap();
            assert_eq!(p, saved);
        }
    }
}

#[test]
fn aum_vault_wallet_uses_source_total_supply_cap_and_never_registered_cash_as_wallet() {
    use xone_protocol_math::{managed_v2_aum::view_aum, managed_v2_pool::collect_at};
    let mut p = fixture();
    let s = p.multi_asset.as_mut().unwrap();
    let m = s.managed_v2.as_mut().unwrap();
    // Global wallets are uint256, while only this pool's registered BPT is uint112.
    m.total_supply = Amount(num_bigint::BigUint::from(1u8) << 128usize);
    let registered = &m.vault_bpt_cash.0 + &m.vault_bpt_managed.0;
    m.virtual_supply = Amount(&m.total_supply.0 - registered);
    let view = view_aum(
        &m.virtual_supply,
        m.timestamp,
        m.last_collection_timestamp,
        &m.management_aum_fee,
    )
    .unwrap();
    m.pending_aum = view.pending_bpt;
    m.actual_supply = view.actual_supply;
    m.manager = s.vault.clone();
    m.protocol_collector = s.vault.clone();
    m.vault_bpt_actual_balance = m.total_supply.clone();
    m.manager_bpt_balance = m.vault_bpt_actual_balance.clone();
    m.protocol_collector_bpt_balance = m.vault_bpt_actual_balance.clone();
    let before = m.clone();
    p.validate().unwrap();
    let collected = collect_at(&p, before.block_number, before.timestamp).unwrap();
    let post = collected
        .next_pool
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert!(post.vault_bpt_actual_balance.0 > before.total_supply.0);
    assert_eq!(post.vault_bpt_actual_balance, post.total_supply);
    assert_eq!(post.manager_bpt_balance, post.vault_bpt_actual_balance);
    assert_eq!(
        post.protocol_collector_bpt_balance,
        post.vault_bpt_actual_balance
    );
    assert_eq!(post.vault_bpt_cash, before.vault_bpt_cash);
    assert_eq!(post.vault_bpt_managed, before.vault_bpt_managed);
    assert_eq!(post.actual_supply, before.actual_supply);
    assert_eq!(post.virtual_supply, before.actual_supply);
    collected.next_pool.validate().unwrap();
    // Equality of aliases is insufficient if their actual wallet exceeds source supply.
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    m.vault_bpt_actual_balance.0 += 1u8;
    m.manager_bpt_balance = m.vault_bpt_actual_balance.clone();
    m.protocol_collector_bpt_balance = m.vault_bpt_actual_balance.clone();
    let saved = p.clone();
    assert!(collect_at(&p, before.block_number, before.timestamp).is_err());
    assert!(quote_exact_in_tokens(&p, &a("100000000000000000000"), &p.token0, &p.token1).is_err());
    assert_eq!(p, saved);
}

#[test]
fn fixed_source_clock_transitions_never_become_canonical_observations() {
    use xone_domain::{hash as state_hash, scope_hash, BlockRef, Snapshot, SnapshotRef};
    use xone_protocol_math::managed_v2_pool::collect_at;
    fn observed(p: Pool) -> Snapshot {
        let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
        Snapshot {
            reference: SnapshotRef {
                block: BlockRef {
                    chain_key: p.chain_key.clone(),
                    number_or_slot: m.block_number,
                    hash: m.block_hash.clone(),
                    parent_hash: format!("0x{}", "b".repeat(64)),
                    timestamp_ms: m.timestamp * 1000,
                },
                canonical_epoch: 1,
                journal_seq: 1,
                schema_version: 1,
                adapter_set_hash: state_hash(&vec![p.adapter_version.clone()]).unwrap(),
                online_generation: 1,
                scope_hash: scope_hash(std::slice::from_ref(&p)).unwrap(),
                writer_epoch: 1,
                owner_service: "test".into(),
                owner_assignment_version: 1,
            },
            pools: vec![p],
            source: "SYNTHETIC_CANONICAL_BOUNDARY_TEST".into(),
            history_gaps: vec![],
        }
    }
    let p = fixture();
    observed(p.clone()).validate("test", 1).unwrap();
    let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
    let swap = quote_exact_in_tokens(&p, &a("100000000000000000000"), &p.token0, &p.token1)
        .unwrap()
        .next_pool;
    let collection = collect_at(&p, m.block_number, m.timestamp)
        .unwrap()
        .next_pool;
    for derived in [swap, collection] {
        let state = derived
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert!(
            state.execution_context.is_some(),
            "transition was relabeled as source observation"
        );
        assert_eq!(
            (state.block_number, state.timestamp, &state.block_hash),
            (m.block_number, m.timestamp, &m.block_hash)
        );
        assert!(
            derived.multi_asset.as_ref().unwrap().to_proto().is_err(),
            "derived fixed-clock wire accepted"
        );
        assert!(
            observed(derived).validate("test", 1).is_err(),
            "derived fixed-clock snapshot accepted"
        );
    }
    assert!(p
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap()
        .execution_context
        .is_none());
}

#[test]
fn t34_collection_ignores_vault_pause() {
    use xone_protocol_math::managed_v2_pool::collect_at;
    let mut p = fixture();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    let (block, time) = (m.block_number, m.timestamp);
    // Separate observed effective Pool and Vault getter flags.
    let (pool_paused, vault_paused) = (false, true);
    m.paused = pool_paused;
    m.vault_paused = vault_paused;
    m.buffer_period_end = time + 300;
    m.vault_buffer_period_end = time + 120;
    let q = collect_at(&p, block + 1, time + 30)
        .expect("Vault pause must allow public AUM collection when the pool is unpaused");
    assert!(q.accrual.total_bpt.0 > num_bigint::BigUint::from(0u8));
}

#[test]
fn t34_swap_uses_vault_buffer_independently() {
    use xone_protocol_math::managed_v2_pool::quote_at;
    let mut p = fixture();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    let (block, time) = (m.block_number, m.timestamp);
    let (pool_paused, vault_paused) = (false, true);
    m.paused = pool_paused;
    m.vault_paused = vault_paused;
    m.buffer_period_end = time + 300;
    let vault_buffer_end = time + 120;
    m.vault_buffer_period_end = vault_buffer_end;
    let q = quote_at(
        &p,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        block + 1,
        vault_buffer_end + 1,
    )
    .expect("Swap must resume after the Vault buffer without waiting for the pool buffer");
    q.next_pool.validate().unwrap();
}

fn pause_state(p: &Pool) -> &xone_domain::managed_v2_state::BalancerManagedV2State {
    p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap()
}
#[test]
fn vault_only_pause_allows_collection_but_swap_uses_its_own_buffer() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    let mut p = fixture();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    let (block, time) = (m.block_number, m.timestamp);
    let end = time + 30 * 86400;
    m.vault_paused = true;
    m.vault_pause_window_end = end - 1;
    m.vault_buffer_period_end = end;
    // Vault expires while Pool's own buffer is still open.
    m.pause_window_end = end + 100;
    m.buffer_period_end = end + 200;
    let saved = p.clone();
    let collected = collect_at(&p, block, end).unwrap();
    assert!(pause_state(&collected.next_pool).vault_paused);
    assert!(!pause_state(&collected.next_pool).paused);
    let baseline = collect_at(&fixture(), block, end).unwrap();
    assert_eq!(collected.accrual, baseline.accrual);
    assert_eq!(
        collect_at(&collected.next_pool, block, end)
            .unwrap()
            .accrual
            .total_bpt,
        Amount::zero()
    );
    for t in [time, end] {
        assert!(
            quote_at(
                &p,
                &a("100000000000000000000"),
                &p.token0,
                &p.token1,
                block,
                t
            )
            .is_err(),
            "vault pause at {t}"
        );
    }
    let wide_block = (1u64 << 32) + 101;
    let q = quote_at(
        &collected.next_pool,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
        wide_block,
        end + 1,
    )
    .unwrap();
    let after = pause_state(&q.next_pool);
    assert!(!after.paused && !after.vault_paused);
    assert_eq!(after.buffer_period_end, end + 200);
    assert_eq!(after.vault_buffer_period_end, end);
    assert_eq!(
        after.execution_context.as_ref().unwrap().block_number,
        wide_block
    );
    assert_eq!(after.execution_context.as_ref().unwrap().timestamp, end + 1);
    assert_eq!(after.token_info[0].last_change_block, 101);
    assert_eq!(
        (after.block_number, after.timestamp, &after.block_hash),
        (block, time, &pause_state(&p).block_hash)
    );
    let repeated = collect_at(&q.next_pool, wide_block, end + 1).unwrap();
    assert_eq!(
        collect_at(&repeated.next_pool, wide_block, end + 1)
            .unwrap()
            .next_pool,
        repeated.next_pool
    );
    quote_exact_in_tokens(
        &repeated.next_pool,
        &a("100000000000000000000"),
        &p.token0,
        &p.token1,
    )
    .unwrap();
    assert_eq!(p, saved);
}
#[test]
fn pool_pause_blocks_collection_and_future_collection_clears_only_expired_flags() {
    use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
    for vault_expires_first in [false, true] {
        let mut p = fixture();
        let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
        let (block, time) = (m.block_number, m.timestamp);
        m.paused = true;
        m.vault_paused = true;
        m.buffer_period_end = time + if vault_expires_first { 20 } else { 10 };
        m.vault_buffer_period_end = time + if vault_expires_first { 10 } else { 20 };
        let pool_end = m.buffer_period_end;
        let saved = p.clone();
        assert!(collect_at(&p, block, pool_end).is_err());
        assert!(quote_at(
            &p,
            &a("100000000000000000000"),
            &p.token0,
            &p.token1,
            block,
            time + 11
        )
        .is_err());
        let collected = collect_at(&p, block, pool_end + 1).unwrap();
        let after = pause_state(&collected.next_pool);
        assert!(!after.paused, "expired Pool getter must be false");
        assert_eq!(after.vault_paused, !vault_expires_first);
        assert_eq!(
            after.execution_context.as_ref().unwrap().timestamp,
            pool_end + 1
        );
        assert_eq!(
            collect_at(&collected.next_pool, block, pool_end + 1)
                .unwrap()
                .next_pool,
            collected.next_pool
        );
        let expired = collect_at(&collected.next_pool, block, time + 21).unwrap();
        assert!(
            !pause_state(&expired.next_pool).paused
                && !pause_state(&expired.next_pool).vault_paused
        );
        quote_exact_in_tokens(
            &expired.next_pool,
            &a("100000000000000000000"),
            &p.token0,
            &p.token1,
        )
        .unwrap();
        assert_eq!(p, saved);
    }
}

#[test]
fn pool_only_pause_blocks_collection_through_its_buffer_equality() {
    use xone_protocol_math::managed_v2_pool::collect_at;
    let mut p = fixture();
    let m = p.multi_asset.as_mut().unwrap().managed_v2.as_mut().unwrap();
    let (block, time) = (m.block_number, m.timestamp);
    m.paused = true;
    m.pause_window_end = time;
    m.buffer_period_end = time + 10;
    assert!(!m.vault_paused);
    let saved = p.clone();
    for t in [time, time + 10] {
        assert!(collect_at(&p, block, t).is_err(), "Pool-only pause at {t}");
    }
    let collected = collect_at(&p, block, time + 11).unwrap();
    let after = pause_state(&collected.next_pool);
    assert!(!after.paused && !after.vault_paused);
    assert_eq!(
        after.execution_context.as_ref().unwrap().timestamp,
        time + 11
    );
    assert_eq!(p, saved);
}
