// Existing raw fixtures and mock RPC stay untouched; exercise the real decoder and RPC.
include!("rayclmm_seed.rs");

const DYNAMIC_PROFILE: &str = "raydium-clmm-dynamic";
const DYNAMIC_OFFSET: usize = 1096;

fn dynamic_fixture(seed: u16) -> Fixture {
    dynamic_from(bind_seed(bitmap_fixture(), seed))
}

fn dynamic_from(mut f: Fixture) -> Fixture {
    f.pool.protocol = DYNAMIC_PROFILE.into();
    let d = &mut f.state()[DYNAMIC_OFFSET..1176];
    d.fill(0);
    d[0..2].copy_from_slice(&0x1234u16.to_le_bytes());
    d[2..4].copy_from_slice(&0x4567u16.to_le_bytes());
    d[4..6].copy_from_slice(&6789u16.to_le_bytes());
    d[6..10].copy_from_slice(&0x10203u32.to_le_bytes());
    d[10..14].copy_from_slice(&0x01020304u32.to_le_bytes());
    d[14..18].copy_from_slice(&(-257i32).to_le_bytes());
    d[18..22].copy_from_slice(&0x100203u32.to_le_bytes());
    d[22..26].copy_from_slice(&0x20304u32.to_le_bytes());
    // Future source timestamps are valid: update_reference uses saturating_sub.
    d[26..34].copy_from_slice(&0x1020304u64.to_le_bytes());
    f
}

fn dynamic_json() -> serde_json::Value {
    json!({"filter_period":0x1234,"decay_period":0x4567,"reduction_factor":6789,
        "dynamic_fee_control":0x10203,"max_volatility_accumulator":0x01020304,
        "tick_spacing_index_reference":-257,"volatility_reference":0x100203,
        "volatility_accumulator":0x20304,"last_update_timestamp":0x1020304})
}

#[test]
fn dynamic_actual_packed_offsets_signed_index_le_seed_and_all_fee_modes() {
    for seed in [0, 1, 255, 256, 65535] {
        for mode in 0..=2 {
            let mut f = dynamic_fixture(seed);
            f.state()[390] = mode;
            let p = f.decode().unwrap();
            assert_eq!(p.adapter_version, "raydium-clmm/6d7a0f82-spl-dynamic-1");
            let s = p.raydium_clmm.as_ref().unwrap();
            assert_eq!(s.seed_index, u32::from(seed));
            assert_eq!(s.collect_fee_on, u32::from(mode));
            assert!(s.dynamic_fee.is_some());
            assert_eq!(
                serde_json::to_value(s).unwrap()["dynamic_fee"],
                dynamic_json()
            );
            assert_eq!(
                s.bitmap_extension.as_ref().unwrap().address,
                extension_address(&f)
            );
            assert_eq!(p.pool_id, seed_pool_address(&f, seed).0);
            p.validate().unwrap();
        }
    }
}

#[test]
fn dynamic_padding_default_and_old_profiles_fail_closed() {
    for offset in 1130..1544 {
        let mut f = dynamic_fixture(256);
        f.state()[offset] = 1;
        assert!(f.decode().is_err(), "reserved byte {offset}");
    }
    let mut f = dynamic_fixture(0);
    f.state()[1096..1176].fill(0);
    assert!(f.decode().is_err(), "new profile requires active params");
    for profile in [
        "raydium-clmm-static",
        "raydium-clmm-token-fee",
        "raydium-clmm-bitmap",
        SEED_PROFILE,
    ] {
        let seed = if profile == SEED_PROFILE { 256 } else { 0 };
        let mut f = dynamic_fixture(seed);
        f.pool.protocol = profile.into();
        if profile == "raydium-clmm-token-fee" {
            f.state()[390] = 1;
        }
        assert!(
            f.decode().is_err(),
            "old profile borrowed dynamic state {profile}"
        );
        f.state()[1096..1176].fill(0);
        let s = f.decode().unwrap().raydium_clmm.unwrap();
        assert!(s.dynamic_fee.is_none());
        assert!(serde_json::to_value(s)
            .unwrap()
            .get("dynamic_fee")
            .is_none());
        // Every one of the old 80 bytes must still be default, including padding.
        for offset in 1096..1176 {
            f.state()[offset] = 1;
            assert!(f.decode().is_err(), "{profile} byte {offset}");
            f.state()[offset] = 0;
        }
    }
}

#[test]
fn dynamic_parameter_and_time_boundaries() {
    for (offset, value) in [
        (0, 0u16),
        (2, 0x1234),
        (2, 0x1233),
        (4, 0),
        (4, 10000),
        (4, u16::MAX),
    ] {
        let mut f = dynamic_fixture(0);
        f.state()[1096 + offset..1098 + offset].copy_from_slice(&value.to_le_bytes());
        assert!(f.decode().is_err(), "u16 {offset}={value}");
    }
    for value in [0u32, 100000, u32::MAX] {
        let mut f = dynamic_fixture(0);
        f.state()[1102..1106].copy_from_slice(&value.to_le_bytes());
        assert!(f.decode().is_err(), "control {value}");
    }
    for offset in [1114, 1118] {
        let mut f = dynamic_fixture(0);
        f.state()[offset..offset + 4].copy_from_slice(&(0x01020304u32 + 1).to_le_bytes());
        assert!(f.decode().is_err(), "volatility {offset}");
        f.state()[offset..offset + 4].copy_from_slice(&0x01020304u32.to_le_bytes());
        f.decode().unwrap();
    }
    for tick in [-443637i32, 443637, i32::MIN, i32::MAX] {
        let mut f = dynamic_fixture(0);
        f.state()[1110..1114].copy_from_slice(&tick.to_le_bytes());
        assert!(f.decode().is_err(), "reference index {tick}");
    }
    for tick in [-443636i32, -1, 0, 443636] {
        let mut f = dynamic_fixture(0);
        f.state()[1110..1114].copy_from_slice(&tick.to_le_bytes());
        f.decode().unwrap();
    }
    for timestamp in [0u64, 999, 1000, 1001, u32::MAX as u64] {
        let mut f = dynamic_fixture(0);
        put64(f.state(), 1122, timestamp);
        f.decode().unwrap();
    }
    for timestamp in [u32::MAX as u64 + 1, u64::MAX] {
        let mut f = dynamic_fixture(0);
        put64(f.state(), 1122, timestamp);
        assert!(f.decode().is_err(), "timestamp {timestamp}");
    }
    // Maximum valid packed periods, with strict decay/filter ordering.
    let mut f = dynamic_fixture(0);
    f.state()[1096..1098].copy_from_slice(&65534u16.to_le_bytes());
    f.state()[1098..1100].copy_from_slice(&65535u16.to_le_bytes());
    f.state()[1100..1102].copy_from_slice(&9999u16.to_le_bytes());
    f.state()[1102..1106].copy_from_slice(&99999u32.to_le_bytes());
    f.decode().unwrap();
}

#[test]
fn dynamic_mandatory_extension_negative_bitmap_proofs_and_index_limits() {
    for indices in [
        vec![511, 512, 513],
        vec![-513, -512, -511],
        vec![-1025, -1024, -1023],
    ] {
        for seed in [0, 256] {
            let f = dynamic_from(bind_seed(bitmap_window(&indices), seed));
            let s = f.decode().unwrap().raydium_clmm.unwrap();
            for &index in &indices {
                assert!(s.array_initialized(index).unwrap());
            }
            let mut f = dynamic_from(bind_seed(bitmap_window(&indices), seed));
            set_raw_array_bit(&mut f, indices[1], false);
            assert!(
                f.decode().is_err(),
                "missing negative/positive proof {indices:?}"
            );
        }
    }
    let f = dynamic_fixture(0);
    let extension = extension_address(&f);
    for id in required_accounts(&f.pool).unwrap() {
        let mut f = dynamic_fixture(0);
        f.accounts.remove(&id);
        assert!(f.decode().is_err(), "missing {id}");
    }
    let mut f = dynamic_fixture(0);
    f.accounts.get_mut(&extension).unwrap().data[8] ^= 1;
    assert!(f.decode().is_err(), "wrong pool extension");
    for starts in [
        vec![-7681 * 60, -7680 * 60, -7679 * 60],
        vec![7678 * 60, 7679 * 60, 7680 * 60],
    ] {
        let mut f = dynamic_fixture(0);
        f.pool.raydium_clmm.as_mut().unwrap().tick_array_starts = starts;
        assert!(required_accounts(&f.pool).is_err());
    }
}

#[test]
fn dynamic_unknown_profile_seed_and_fee_fail_closed() {
    for profile in [
        "raydium-clmm-dynamic-unknown",
        "raydium-clmm-dynamic ",
        "unknown",
    ] {
        let mut f = dynamic_fixture(0);
        f.pool.protocol = profile.into();
        assert!(required_accounts(&f.pool).is_err());
    }
    for mode in [3, 255] {
        let mut f = dynamic_fixture(0);
        f.state()[390] = mode;
        assert!(f.decode().is_err());
    }
    for seed in [1u16, 255, 256] {
        let mut f = dynamic_fixture(seed);
        f.state()[391..393].copy_from_slice(&seed.to_be_bytes());
        assert!(f.decode().is_err(), "wrong seed endian {seed}");
    }
    let mut f = dynamic_fixture(256);
    f.state()[8] ^= 1;
    assert!(f.decode().is_err(), "wrong PDA bump");
    for offset in 393..397 {
        let mut f = dynamic_fixture(256);
        f.state()[offset] = 1;
        assert!(f.decode().is_err());
    }
}

#[tokio::test]
async fn dynamic_rpc_15_to_19_finalized_batch_preserves_state_and_post_anchor() {
    for seed in [0, 256] {
        for count in 3..=7 {
            let indices = (-1..count - 1).collect::<Vec<_>>();
            let f = dynamic_from(bind_seed(bitmap_window(&indices), seed));
            let ids = required_accounts(&f.pool)
                .unwrap()
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            let (result, calls) = run_rpc(f, false).await;
            let snapshot = result.unwrap();
            let s = snapshot.pools[0].raydium_clmm.as_ref().unwrap();
            assert_eq!(s.seed_index, u32::from(seed));
            assert_eq!(
                serde_json::to_value(s).unwrap()["dynamic_fee"],
                dynamic_json()
            );
            assert_eq!(calls.len(), 4);
            assert_eq!(calls[1]["method"], "getMultipleAccounts");
            let batch = calls[1]["params"][0].as_array().unwrap();
            assert_eq!(batch.len(), 12 + count as usize);
            assert_eq!(
                batch
                    .iter()
                    .map(|id| id.as_str().unwrap().to_owned())
                    .collect::<std::collections::BTreeSet<_>>(),
                ids
            );
            assert_eq!(calls[1]["params"][1]["commitment"], "finalized");
            for call in &calls[2..] {
                assert_eq!(call["method"], "getBlock");
                assert_eq!(call["params"][0], 100);
                assert_eq!(call["params"][1]["commitment"], "finalized");
            }
        }
    }
    let (result, calls) = run_rpc(dynamic_fixture(0), true).await;
    assert_eq!(result.unwrap_err().0, "ACCOUNT_BANK_ANCHOR_CHANGED");
    assert_eq!(calls.len(), 4);
    let mut f = dynamic_fixture(0);
    f.chain.confirmation_tag = "confirmed".into();
    let (result, calls) = run_rpc(f, false).await;
    assert_eq!(result.unwrap_err().0, "RAYCLMM_REQUIRES_FINALIZED");
    assert!(calls.is_empty());
}

// Keep spacing proofs valid while testing floor division and the u32 product bound.
fn dynamic_spacing_fixture(spacing: u16) -> Fixture {
    let mut f = dynamic_fixture(0);
    f.state()[235..237].copy_from_slice(&spacing.to_le_bytes());
    f.data(3)[51..53].copy_from_slice(&spacing.to_le_bytes());
    let starts = [-60 * i32::from(spacing), 0, 60 * i32::from(spacing)];
    f.pool.raydium_clmm.as_mut().unwrap().tick_array_starts = starts.to_vec();
    for (id, start) in f.arrays.iter_mut().zip(starts) {
        let mut a = f.accounts.remove(id).unwrap();
        a.data[40..44].copy_from_slice(&start.to_le_bytes());
        *id = pda(
            &[b"tick_array", &bytes(&f.pool.id), &start.to_be_bytes()],
            PROGRAM,
        )
        .0;
        f.accounts.insert(id.clone(), a);
    }
    f
}

#[test]
fn dynamic_spacing_product_limit_and_negative_reference_floor_are_exact() {
    for spacing in [2u16, 3, 60, 1000] {
        let max = u32::MAX / u32::from(spacing);
        for (reference, valid) in [
            ((-443636i32).div_euclid(i32::from(spacing)), true),
            ((-443636i32).div_euclid(i32::from(spacing)) - 1, false),
            (443636i32.div_euclid(i32::from(spacing)), true),
            (443636i32.div_euclid(i32::from(spacing)) + 1, false),
        ] {
            let mut f = dynamic_spacing_fixture(spacing);
            f.state()[1106..1110].copy_from_slice(&max.to_le_bytes());
            f.state()[1110..1114].copy_from_slice(&reference.to_le_bytes());
            f.state()[1114..1118].copy_from_slice(&max.to_le_bytes());
            f.state()[1118..1122].copy_from_slice(&max.to_le_bytes());
            assert_eq!(
                f.decode().is_ok(),
                valid,
                "spacing {spacing}, reference {reference}"
            );
        }
        let mut f = dynamic_spacing_fixture(spacing);
        f.state()[1110..1114].copy_from_slice(&0i32.to_le_bytes());
        f.state()[1106..1110].copy_from_slice(&(max + 1).to_le_bytes());
        assert!(f.decode().is_err(), "product overflow, spacing {spacing}");
    }
    // Source permits zero max volatility; no artificial nonzero constraint.
    let mut f = dynamic_fixture(0);
    f.state()[1106..1110].fill(0);
    f.state()[1114..1122].fill(0);
    f.decode().unwrap();
    // At spacing 1 the full u32 maximum is allowed, without signed narrowing.
    let mut f = dynamic_fixture(0);
    f.state()[1106..1110].copy_from_slice(&u32::MAX.to_le_bytes());
    f.state()[1114..1118].copy_from_slice(&u32::MAX.to_le_bytes());
    f.state()[1118..1122].copy_from_slice(&u32::MAX.to_le_bytes());
    f.decode().unwrap();
}

#[test]
fn dynamic_keeps_code_token_clock_epoch_strict_open_reward_and_order_guards() {
    // A valid baseline prevents malformed-profile admission from masking guard failures.
    dynamic_fixture(256).decode().unwrap();
    for id in required_accounts(&dynamic_fixture(256).pool).unwrap() {
        let mut f = dynamic_fixture(256);
        f.accounts.get_mut(&id).unwrap().owner = key(88);
        assert!(f.decode().is_err(), "owner {id}");
        let mut f = dynamic_fixture(256);
        f.accounts.get_mut(&id).unwrap().executable ^= true;
        assert!(f.decode().is_err(), "executable {id}");
    }
    for id in ["HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx", TOKEN_PROGRAM] {
        let mut f = dynamic_fixture(256);
        *f.accounts.get_mut(id).unwrap().data.last_mut().unwrap() ^= 1;
        assert!(f.decode().is_err(), "code proof {id}");
    }
    for timestamp in [u32::MAX as u64 + 1, u64::MAX] {
        let mut f = dynamic_fixture(256);
        put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 32, timestamp);
        assert_eq!(f.decode().unwrap_err().0, "CLOCK_TIMESTAMP_INVALID");
    }
    for offset in [1080, 1088] {
        let mut f = dynamic_fixture(256);
        put64(f.state(), offset, if offset == 1080 { 1000 } else { 8 });
        assert!(f.decode().is_err(), "strict open/pool epoch {offset}");
    }
    let mut f = dynamic_fixture(256);
    put64(f.data(6), 9, 8);
    assert!(f.decode().is_err(), "observation epoch");
    let mut f = dynamic_fixture(256);
    put64(f.array(1), 10125, 8);
    assert!(f.decode().is_err(), "array epoch");
    for offset in [397, 397 + 8, 397 + 153] {
        let mut f = dynamic_fixture(256);
        f.state()[offset] = 1;
        assert!(f.decode().is_err(), "reward {offset}");
    }
    let mut f = dynamic_fixture(256);
    f.array(1)[44 + 124] = 1;
    assert!(f.decode().is_err(), "limit order");
    for (account_index, offset) in [(1, 72), (1, 129), (4, 45)] {
        let mut f = dynamic_fixture(256);
        f.data(account_index)[offset] ^= 1;
        assert!(f.decode().is_err(), "SPL {account_index}:{offset}");
    }
    let mut f = dynamic_fixture(256);
    put_key(f.data(1), 32, &dynamic_fixture(0).pool.id);
    assert!(f.decode().is_err(), "wrong vault authority");
    for kind in 0..4 {
        let mut f = dynamic_fixture(256);
        let id = extension_address(&f);
        let e = f.accounts.get_mut(&id).unwrap();
        match kind {
            0 => e.owner = TOKEN_PROGRAM.into(),
            1 => e.executable = true,
            2 => e.data[0] ^= 1,
            3 => {
                e.data.pop();
            }
            _ => unreachable!(),
        }
        assert!(f.decode().is_err(), "extension {kind}");
    }
}

#[test]
fn dynamic_wsol_backing_is_preserved_for_zero_and_nonzero_seed() {
    for seed in [0, 65535] {
        let mut f =
            Fixture::with_mints(key(1), "So11111111111111111111111111111111111111112".into());
        attach_bitmap_extension(&mut f);
        let mut f = dynamic_from(bind_seed(f, seed));
        f.state()[234] = 9;
        f.data(5)[44] = 9;
        f.data(2)[109..113].copy_from_slice(&1u32.to_le_bytes());
        put64(f.data(2), 113, 2039280);
        f.decode().unwrap();
        let id = f.pool.accounts[2].clone();
        f.accounts.get_mut(&id).unwrap().lamports = 2039280 + 19999;
        assert_eq!(f.decode().unwrap_err().0, "NATIVE_VAULT_BACKING");
        f.accounts.get_mut(&id).unwrap().lamports += 1;
        f.decode().unwrap();
        f.data(2)[109..113].fill(0);
        assert!(f.decode().is_err());
    }
}

#[tokio::test]
async fn dynamic_rpc_rejects_missing_dependencies_clock_and_preserves_fee_modes() {
    let ids = required_accounts(&dynamic_fixture(256).pool).unwrap();
    for id in ids {
        let mut f = dynamic_fixture(256);
        f.accounts.remove(&id);
        let (result, _) = run_rpc(f, false).await;
        assert!(result.is_err(), "missing RPC dependency {id}");
    }
    let mut f = dynamic_fixture(256);
    put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 0, 101);
    let (result, _) = run_rpc(f, false).await;
    assert_eq!(result.unwrap_err().0, "CLOCK_CONTEXT_MISMATCH");
    for mode in 0..=2 {
        let mut f = dynamic_fixture(256);
        f.state()[390] = mode;
        let (result, calls) = run_rpc(f, false).await;
        let snapshot = result.unwrap();
        assert_eq!(
            snapshot.pools[0]
                .raydium_clmm
                .as_ref()
                .unwrap()
                .collect_fee_on,
            u32::from(mode)
        );
        assert_eq!(calls.len(), 4);
    }
}

#[test]
fn original_dynamic_account_fixture_matches_full_pool_and_next_bank() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/solana/rayclmm-dynamic-base.json"
    ))
    .unwrap();
    let fixtures = [&fixture, &fixture["metadata"]["next_bank_fixture"]];
    for f in fixtures {
        let pool: PoolConfig = serde_json::from_value(f["pool_config"].clone()).unwrap();
        let expected: xone_domain::Pool = serde_json::from_value(f["pool"].clone()).unwrap();
        let slot = f["metadata"]["bank_slot"].as_u64().unwrap();
        let chain:Chain=serde_json::from_value(json!({"key":expected.chain_key,"owner_service":"chain-parser-rust","assignment_version":1,"genesis_hash":key(20),"rpc_urls":[],"confirmation_tag":"finalized","auto_reorg_window":32,"pools":[pool]})).unwrap();
        let accounts = f["rpc_accounts"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, a)| (id.clone(), RpcAccount::decode(a).unwrap()))
            .collect();
        let actual = decode_raydium_clmm(&chain, &pool, &accounts, slot).unwrap();
        assert_eq!(actual, expected);
    }
}
