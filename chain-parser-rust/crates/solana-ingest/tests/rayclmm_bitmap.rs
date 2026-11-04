// Reuse only this crate's existing synthetic raw-account fixture and mock RPC.
// Including it here keeps private helpers available without modifying old tests.
include!("rayclmm_decode.rs");

fn extension_address(f: &Fixture) -> String {
    pda(
        &[b"pool_tick_array_bitmap_extension", &bytes(&f.pool.id)],
        PROGRAM,
    )
    .0
}

fn bitmap_fixture() -> Fixture {
    let mut f = Fixture::new();
    attach_bitmap_extension(&mut f);
    f
}

fn attach_bitmap_extension(f: &mut Fixture) {
    f.pool.protocol = "raydium-clmm-bitmap".into();
    let mut data = vec![0; 1832];
    data[..8].copy_from_slice(&disc("TickArrayBitmapExtension"));
    put_key(&mut data, 8, &f.pool.id);
    f.accounts
        .insert(extension_address(f), account(PROGRAM, data, false));
}

#[test]
fn bitmap_required_accounts_adds_derived_proof_even_for_base_arrays() {
    let f = bitmap_fixture();
    let ids = required_accounts(&f.pool).unwrap();
    assert_eq!(f.pool.accounts.len(), 7);
    assert_eq!(ids.len(), 15);
    assert_eq!(
        ids.iter()
            .filter(|id| **id == extension_address(&f))
            .count(),
        1
    );
    assert!(ids.iter().all(|id| f.accounts.contains_key(id)));
}

// Raw account bits use explicit source group origins. The negative group [-1024,
// -513] runs from bit 0 to bit 511; an exact -1024 belongs to the preceding group.
fn set_raw_array_bit(f: &mut Fixture, index: i32, active: bool) {
    let (id, offset, bit) = if (-512..=511).contains(&index) {
        let bit = (index + 512) as usize;
        (f.pool.id.clone(), 904 + bit / 64 * 8, bit % 64)
    } else {
        let (base, group, bit) = if index >= 512 {
            (40, index / 512 - 1, index % 512)
        } else {
            let group = (-index - 1) / 512 - 1;
            (936, group, index - (-(group + 2) * 512))
        };
        (
            extension_address(f),
            base + group as usize * 64 + bit as usize / 64 * 8,
            bit as usize % 64,
        )
    };
    let d = &mut f.accounts.get_mut(&id).unwrap().data;
    let mut word = u64::from_le_bytes(d[offset..offset + 8].try_into().unwrap());
    if active {
        word |= 1u64 << bit;
    } else {
        word &= !(1u64 << bit);
    }
    put64(d, offset, word);
}

fn bitmap_window(indices: &[i32]) -> Fixture {
    let mut f = bitmap_fixture();
    for id in f.arrays.drain(..) {
        f.accounts.remove(&id);
    }
    f.state()[904..1032].fill(0);
    let starts = indices.iter().map(|i| i * 60).collect::<Vec<_>>();
    f.pool.raydium_clmm.as_mut().unwrap().tick_array_starts = starts.clone();
    let current = starts[starts.len() / 2];
    f.state()[269..273].copy_from_slice(&current.to_le_bytes());
    let price = xone_domain::rayclmm_math::sqrt_price_from_tick(current).unwrap();
    put128(f.state(), 253, price.to_string().parse().unwrap());
    for (&index, start) in indices.iter().zip(starts) {
        let id = pda(
            &[b"tick_array", &bytes(&f.pool.id), &start.to_be_bytes()],
            PROGRAM,
        )
        .0;
        let mut d = vec![0; 10240];
        d[..8].copy_from_slice(&disc("TickArrayState"));
        put_key(&mut d, 8, &f.pool.id);
        d[40..44].copy_from_slice(&start.to_le_bytes());
        // The first source array straddles MIN_TICK; only its in-domain slots can initialize.
        let tick = start.max(-443636);
        let offset = 44 + (tick - start) as usize * 168;
        d[offset..offset + 4].copy_from_slice(&tick.to_le_bytes());
        put128(&mut d, offset + 4, 1000);
        put128(&mut d, offset + 20, 1000);
        d[10124] = 1;
        put64(&mut d, 10125, 7);
        f.accounts.insert(id.clone(), account(PROGRAM, d, false));
        f.arrays.push(id);
        set_raw_array_bit(&mut f, index, true);
    }
    f
}

#[test]
fn bitmap_modes_decode_base_arrays_and_return_extension_only_for_new_profile() {
    for mode in 0..=2 {
        let mut f = bitmap_fixture();
        f.state()[390] = mode;
        let p = f.decode().unwrap();
        assert_eq!(p.adapter_version, "raydium-clmm/6d7a0f82-spl-bitmap-1");
        let s = p.raydium_clmm.unwrap();
        assert_eq!(s.collect_fee_on, u32::from(mode));
        let e = s.bitmap_extension.unwrap();
        assert_eq!(e.address, extension_address(&f));
        assert_eq!(e.positive, vec![0; 112]);
        assert_eq!(e.negative, vec![0; 112]);
    }
    let mut f = bitmap_fixture();
    f.state()[390] = 3;
    assert!(f.decode().is_err());
}

#[test]
fn bitmap_layout_preserves_all_224_little_endian_words_and_outside_window_bits() {
    let mut f = bitmap_fixture();
    let id = extension_address(&f);
    let words = (0..224u64)
        .map(|i| 0xdead_beef_8000_0001u64.wrapping_add(i << 32))
        .collect::<Vec<_>>();
    for (i, &w) in words.iter().enumerate() {
        put64(&mut f.accounts.get_mut(&id).unwrap().data, 40 + i * 8, w);
    }
    let s = f.decode().unwrap().raydium_clmm.unwrap();
    let e = s.bitmap_extension.unwrap();
    assert_eq!(e.positive, words[..112]);
    assert_eq!(e.negative, words[112..]);
}

#[test]
fn bitmap_actual_arrays_cross_base_edges_and_negative_whole_groups() {
    for center in [
        511, 512, 1023, 1024, 7167, 7168, -512, -513, -1024, -1025, -1536, -7168, -7169, -7393,
        7392,
    ]
    .into_iter()
    .chain((2..=14).map(|i| -i * 512))
    .chain((1..=14).map(|i| i * 512))
    {
        let indices = [center - 1, center, center + 1];
        let f = bitmap_window(&indices);
        let s = f
            .decode()
            .unwrap_or_else(|e| panic!("center {center}: {e:?}"))
            .raydium_clmm
            .unwrap();
        for &index in &indices {
            assert!(s.array_initialized(index).unwrap(), "index {index}");
        }
        assert_eq!(s.tick_arrays.len(), 3);
    }
    // Pin the exact negative boundary coordinates independently of the fixture formula.
    for (index, word, bit) in [
        (-513, 7, 63),
        (-1024, 0, 0),
        (-1025, 15, 63),
        (-1536, 8, 0),
        (-7168, 96, 0),
        (-7169, 111, 63),
    ] {
        let f = bitmap_window(&[index - 1, index, index + 1]);
        let e = &f.accounts[&extension_address(&f)].data;
        let raw = u64::from_le_bytes(e[936 + word * 8..944 + word * 8].try_into().unwrap());
        assert_ne!(raw & (1u64 << bit), 0, "index {index}");
    }
}

#[test]
fn bitmap_rejects_missing_wrong_pool_pda_owner_executable_discriminator_or_length() {
    assert!(bitmap_fixture().decode().is_ok());
    let mut f = bitmap_fixture();
    f.accounts.remove(&extension_address(&f));
    assert!(f.decode().is_err());
    let mut f = bitmap_fixture();
    let a = f.accounts.remove(&extension_address(&f)).unwrap();
    f.accounts.insert(key(80), a);
    assert!(f.decode().is_err());
    for mutation in 0..4 {
        let mut f = bitmap_fixture();
        let id = extension_address(&f);
        let a = f.accounts.get_mut(&id).unwrap();
        match mutation {
            0 => put_key(&mut a.data, 8, &key(81)),
            1 => a.owner = TOKEN_PROGRAM.into(),
            2 => a.executable = true,
            _ => a.data[0] ^= 1,
        }
        assert!(f.decode().is_err(), "mutation {mutation}");
    }
    for len in [0, 7, 8, 39, 40, 935, 936, 1831, 1833] {
        let mut f = bitmap_fixture();
        let id = extension_address(&f);
        f.accounts.get_mut(&id).unwrap().data.resize(len, 0);
        assert!(f.decode().is_err(), "extension len {len}");
    }
}

#[test]
fn bitmap_rejects_bit_count_mismatches_in_base_positive_and_negative_arrays() {
    for indices in [
        [510, 511, 512],
        [-513, -512, -511],
        [1023, 1024, 1025],
        [-1025, -1024, -1023],
    ] {
        for i in 0..3 {
            let mut f = bitmap_window(&indices);
            assert!(f.decode().is_ok());
            set_raw_array_bit(&mut f, indices[i], false);
            assert_eq!(f.decode().unwrap_err().0, "RAYCLMM_BITMAP_PROOF");
            let mut f = bitmap_window(&indices);
            f.array(i)[44..10124].fill(0);
            f.array(i)[10124] = 0;
            assert_eq!(f.decode().unwrap_err().0, "RAYCLMM_BITMAP_PROOF");
            set_raw_array_bit(&mut f, indices[i], false);
            assert!(f.decode().is_ok());
            let mut f = bitmap_window(&indices);
            f.array(i)[10124] = 2;
            assert_eq!(f.decode().unwrap_err().0, "RAYCLMM_BITMAP_PROOF");
        }
    }
    // A naive abs(index) % 512 maps -1024 to the next group's bit zero.
    let mut f = bitmap_window(&[-1025, -1024, -1023]);
    set_raw_array_bit(&mut f, -1024, false);
    set_raw_array_bit(&mut f, -1536, true);
    assert_eq!(f.decode().unwrap_err().0, "RAYCLMM_BITMAP_PROOF");
}

#[test]
fn bitmap_old_profiles_stay_base_only_with_no_returned_extension() {
    for (profile, mode) in [
        ("raydium-clmm-static", 0),
        ("raydium-clmm-token-fee", 1),
        ("raydium-clmm-token-fee", 2),
    ] {
        let mut f = bitmap_fixture();
        f.pool.protocol = profile.into();
        f.state()[390] = mode;
        assert_eq!(required_accounts(&f.pool).unwrap().len(), 14);
        assert!(f
            .decode()
            .unwrap()
            .raydium_clmm
            .unwrap()
            .bitmap_extension
            .is_none());
        for indices in [[510, 511, 512], [-513, -512, -511]] {
            let mut f = bitmap_window(&indices);
            f.pool.protocol = profile.into();
            f.state()[390] = mode;
            assert!(required_accounts(&f.pool).is_err());
            assert!(f.decode().is_err());
        }
    }
}

#[test]
fn bitmap_selection_bounds_indices_and_actual_source_tick_domain() {
    let f = bitmap_fixture();
    for indices in [
        [-7681, -7680, -7679],
        [7678, 7679, 7680],
        [-7680, -7679, -7678],
        [7677, 7678, 7679],
    ] {
        let mut p = f.pool.clone();
        p.raydium_clmm.as_mut().unwrap().tick_array_starts =
            indices.iter().map(|i| i * 60).collect();
        assert!(required_accounts(&p).is_err(), "indices {indices:?}");
    }
    for indices in [[-7394, -7393, -7392], [7391, 7392, 7393]] {
        let mut p = f.pool.clone();
        p.raydium_clmm.as_mut().unwrap().tick_array_starts =
            indices.iter().map(|i| i * 60).collect();
        assert!(required_accounts(&p).is_ok(), "indices {indices:?}");
    }
}

#[tokio::test]
async fn bitmap_rpc_fetches_exact_identity_set_for_every_14_to_19_account_batch() {
    for profile in ["raydium-clmm-static", "raydium-clmm-bitmap"] {
        for count in 3..=7 {
            let indices = (-1..count - 1).collect::<Vec<_>>();
            let mut f = bitmap_window(&indices);
            f.pool.protocol = profile.into();
            let extension = extension_address(&f);
            let expected = f
                .accounts
                .keys()
                .filter(|id| profile == "raydium-clmm-bitmap" || **id != extension)
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            let expected_len = 11 + count as usize + usize::from(profile == "raydium-clmm-bitmap");
            let (r, calls) = run_rpc(f, false).await;
            let snapshot = r.unwrap();
            assert_eq!(snapshot.pools[0].protocol, profile);
            assert_eq!(calls.len(), 4);
            assert_eq!(calls[1]["method"], "getMultipleAccounts");
            let actual = calls[1]["params"][0]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                calls[1]["params"][0].as_array().unwrap().len(),
                expected_len
            );
            assert_eq!(actual, expected);
            assert_eq!(calls[1]["params"][1]["commitment"], "finalized");
            assert_eq!(calls[2]["method"], "getBlock");
            assert_eq!(calls[3]["method"], "getBlock");
        }
    }
}

#[tokio::test]
async fn bitmap_rpc_requires_finalized_before_provider_and_rechecks_bank() {
    let mut f = bitmap_fixture();
    f.chain.confirmation_tag = "confirmed".into();
    f.chain.pools = vec![f.pool];
    let rpc = xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    };
    assert_eq!(
        rpc.current_baseline(&f.chain).await.unwrap_err().0,
        "RAYCLMM_REQUIRES_FINALIZED"
    );
    let (r, _) = run_rpc(bitmap_fixture(), true).await;
    assert_eq!(r.unwrap_err().0, "ACCOUNT_BANK_ANCHOR_CHANGED");
}

#[tokio::test]
async fn bitmap_rpc_rejects_every_missing_atomic_dependency_and_clock_mismatch() {
    let f = bitmap_window(&[-513, -512, -511]);
    for id in required_accounts(&f.pool).unwrap() {
        let mut f = bitmap_window(&[-513, -512, -511]);
        f.accounts.remove(&id);
        let (r, _) = run_rpc(f, false).await;
        assert!(r.is_err(), "missing {id}");
    }
    let mut f = bitmap_fixture();
    put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 0, 101);
    let (r, _) = run_rpc(f, false).await;
    assert_eq!(r.unwrap_err().0, "CLOCK_CONTEXT_MISMATCH");
}

#[test]
fn bitmap_preserves_static_seed_dynamic_reward_order_padding_and_strict_open_guards() {
    for mode in 0..=2 {
        for offset in [
            273, 325, 389, 391, 392, 397, 454, 486, 550, 1032, 1096, 1175, 1288, 1543,
        ] {
            let mut f = bitmap_fixture();
            f.state()[390] = mode;
            f.state()[offset] = 255;
            assert!(f.decode().is_err(), "mode {mode}, pool offset {offset}");
        }
        for offset in [
            44 + 68,
            44 + 116,
            44 + 124,
            44 + 132,
            44 + 140,
            44 + 156,
            10133,
            10239,
        ] {
            let mut f = bitmap_window(&[-1025, -1024, -1023]);
            f.state()[390] = mode;
            f.array(1)[offset] = 7;
            assert!(f.decode().is_err(), "mode {mode}, array offset {offset}");
        }
        let mut f = bitmap_fixture();
        f.state()[390] = mode;
        put64(f.state(), 1080, 1000);
        assert_eq!(f.decode().unwrap_err().0, "POOL_NOT_OPEN");
        put64(f.state(), 1080, 999);
        assert!(f.decode().is_ok());
    }
}

#[test]
fn bitmap_preserves_account_code_clock_pda_spl_and_wsol_proofs() {
    let f = bitmap_fixture();
    for id in required_accounts(&f.pool).unwrap() {
        let mut f = bitmap_fixture();
        f.accounts.get_mut(&id).unwrap().owner = key(50);
        assert!(f.decode().is_err(), "owner {id}");
    }
    for id in [
        PROGRAM.to_string(),
        TOKEN_PROGRAM.to_string(),
        f.pool
            .raydium_clmm
            .as_ref()
            .unwrap()
            .expected_programdata
            .clone(),
    ] {
        let mut f = bitmap_fixture();
        f.accounts.get_mut(&id).unwrap().data.push(1);
        assert!(f.decode().is_err(), "code {id}");
    }
    for (account, offsets) in [
        (1, vec![0, 32, 72, 108, 109, 121, 129]),
        (4, vec![0, 45, 46]),
        (3, vec![0, 8, 9, 57, 93]),
    ] {
        for offset in offsets {
            let mut f = bitmap_fixture();
            f.data(account)[offset] ^= 255;
            assert!(f.decode().is_err(), "account {account} offset {offset}");
        }
    }
    let mut f = bitmap_fixture();
    f.state()[8] ^= 255;
    assert!(f.decode().is_err());
    let mut f = bitmap_fixture();
    put64(f.state(), 1088, 8);
    assert!(f.decode().is_err());
    for timestamp in [u64::MAX, u32::MAX as u64 + 1] {
        let mut f = bitmap_fixture();
        put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 32, timestamp);
        assert!(f.decode().is_err());
    }
    let mut f = Fixture::with_mints(key(1), "So11111111111111111111111111111111111111112".into());
    attach_bitmap_extension(&mut f);
    f.state()[234] = 9;
    f.data(5)[44] = 9;
    f.data(2)[109..113].copy_from_slice(&1u32.to_le_bytes());
    put64(f.data(2), 113, 2039280);
    assert!(f.decode().is_ok());
    let id = f.pool.accounts[2].clone();
    f.accounts.get_mut(&id).unwrap().lamports = 2039280 + 19999;
    assert_eq!(f.decode().unwrap_err().0, "NATIVE_VAULT_BACKING");
    f.accounts.get_mut(&id).unwrap().lamports = 2039280 + 20000;
    assert!(f.decode().is_ok());
    f.data(2)[109..113].fill(0);
    assert!(f.decode().is_err());
}

#[tokio::test]
async fn bitmap_rpc_preserves_each_fee_mode_across_positive_and_negative_edges() {
    for mode in 0..=2 {
        for indices in [[510, 511, 512], [-513, -512, -511]] {
            let mut f = bitmap_window(&indices);
            f.state()[390] = mode;
            let (r, calls) = run_rpc(f, false).await;
            let p = &r.as_ref().unwrap().pools[0];
            assert_eq!(p.adapter_version, "raydium-clmm/6d7a0f82-spl-bitmap-1");
            assert_eq!(
                p.raydium_clmm.as_ref().unwrap().collect_fee_on,
                u32::from(mode)
            );
            assert!(p.raydium_clmm.as_ref().unwrap().bitmap_extension.is_some());
            assert_eq!(calls[1]["params"][0].as_array().unwrap().len(), 15);
        }
    }
}

#[test]
fn original_bitmap_account_fixture_matches_all_words_and_full_pool() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/solana/rayclmm-bitmap-base.json"
    ))
    .unwrap();
    for f in [
        &fixture,
        &fixture["metadata"]["next_bank_fixture"],
        &fixture["metadata"]["alternate_negative_base"],
    ] {
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
