use raydium_clmm_reference::{base_pool, quote};
#[test]
fn executes_original_swap_and_clock_syscall() {
    let p = base_pool();
    let q = quote(&p, "100000000", true).unwrap();
    assert!(q.amount_out > 0);
    assert_eq!(q.pool.raydium_clmm.recent_epoch, p.raydium_clmm.bank_epoch);
    assert_eq!(
        q.pool.raydium_clmm.raw_vault0,
        p.raydium_clmm.raw_vault0 + 100000000
    );
    assert_eq!(
        q.pool.raydium_clmm.observation_recent_epoch,
        p.raydium_clmm.observation_recent_epoch
    );
    assert!(q.pool.raydium_clmm.observation_initialized);
}
#[test]
fn rejects_future_open_and_discards_overflow_mutations() {
    let mut p = base_pool();
    p.raydium_clmm.open_time = p.raydium_clmm.bank_unix_timestamp;
    assert!(quote(&p, "100000000", true).is_err());
    p.raydium_clmm.open_time = 1;
    p.raydium_clmm.protocol_fees0 = u64::MAX;
    let old = p.clone();
    assert!(quote(&p, "100000000", true).is_err());
    assert_eq!(p, old);
}
#[test]
fn crossing_updates_original_array_epoch_and_wraps_observation() {
    let mut p = base_pool();
    let s = &mut p.raydium_clmm;
    s.observation_initialized = true;
    s.observation_index = 99;
    s.observations[99].block_timestamp = s.bank_unix_timestamp as u32 - 30;
    s.observations[99].tick_cumulative = i64::MAX - 5;
    let q = quote(&p, "3000000000", true).unwrap();
    assert_eq!(q.pool.raydium_clmm.observation_index, 0);
    assert_eq!(
        q.pool.raydium_clmm.observations[0].tick_cumulative,
        (i64::MAX - 5).wrapping_add(10 * 30)
    );
    assert!(
        q.pool
            .raydium_clmm
            .tick_arrays
            .iter()
            .filter(|a| a.recent_epoch == p.raydium_clmm.bank_epoch)
            .count()
            >= 2
    );
    assert_eq!(q.pool.raydium_clmm.observation_recent_epoch, 986);
}
#[test]
fn fixture_uses_original_packed_account_bytes_and_integer_json() {
    use base64::Engine;
    let f = raydium_clmm_reference::fixture();
    assert_eq!(f["metadata"]["pool_len"], 1544);
    assert_eq!(f["metadata"]["array_len"], 10240);
    let p = base_pool();
    let encoded = f["rpc_accounts"][&p.pool_id]["data"][0].as_str().unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    assert_eq!(bytes.len(), 1544);
    assert_eq!(
        u128::from_le_bytes(bytes[253..269].try_into().unwrap()).to_string(),
        p.raydium_clmm.sqrt_price_x64
    );
    let mut wide = p.clone();
    wide.raydium_clmm.raw_vault0 = u64::MAX;
    let v = serde_json::to_value(wide).unwrap();
    assert_eq!(v["raydium_clmm"]["raw_vault0"].as_u64(), Some(u64::MAX));
    assert!(v["raydium_clmm"]["liquidity"].is_string());
}
