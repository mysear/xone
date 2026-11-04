// Reuse this crate's synthetic raw accounts and mock RPC without editing old tests.
include!("rayclmm_bitmap.rs");

const SEED_PROFILE: &str = "raydium-clmm-custom-seed";

fn seed_pool_address(f: &Fixture, seed: u16) -> (String, u8) {
    let config = bytes(&f.pool.accounts[3]);
    let mint0 = bytes(&f.pool.token0);
    let mint1 = bytes(&f.pool.token1);
    let seed_bytes = seed.to_le_bytes();
    let seed_slice: &[u8] = if seed == 0 { &[] } else { &seed_bytes };
    pda(&[b"pool", &config, &mint0, &mint1, seed_slice], PROGRAM)
}

// Rebuild every account derived from the pool, using the source's LE u16 seed.
fn bind_seed(mut f: Fixture, seed: u16) -> Fixture {
    let old_pool = f.pool.id.clone();
    let old_extension = extension_address(&f);
    let (pool, bump) = seed_pool_address(&f, seed);
    let mut state = f.accounts.remove(&old_pool).unwrap();
    state.data[8] = bump;
    state.data[391..393].copy_from_slice(&seed.to_le_bytes());
    for i in 0..2 {
        let old_vault = f.pool.accounts[1 + i].clone();
        let vault = pda(
            &[
                b"pool_vault",
                &bytes(&pool),
                &bytes(&f.pool.accounts[4 + i]),
            ],
            PROGRAM,
        )
        .0;
        let mut account = f.accounts.remove(&old_vault).unwrap();
        put_key(&mut account.data, 32, &pool);
        put_key(&mut state.data, 137 + 32 * i, &vault);
        f.accounts.insert(vault.clone(), account);
        f.pool.accounts[1 + i] = vault;
    }
    let mut obs = f.accounts.remove(&f.pool.accounts[6]).unwrap();
    put_key(&mut obs.data, 19, &pool);
    let obs_id = pda(&[b"observation", &bytes(&pool)], PROGRAM).0;
    put_key(&mut state.data, 201, &obs_id);
    f.accounts.insert(obs_id.clone(), obs);
    f.pool.accounts[6] = obs_id;
    for id in &mut f.arrays {
        let mut a = f.accounts.remove(id).unwrap();
        put_key(&mut a.data, 8, &pool);
        let start = i32::from_le_bytes(a.data[40..44].try_into().unwrap());
        *id = pda(
            &[b"tick_array", &bytes(&pool), &start.to_be_bytes()],
            PROGRAM,
        )
        .0;
        f.accounts.insert(id.clone(), a);
    }
    f.pool.id = pool.clone();
    f.pool.accounts[0] = pool.clone();
    f.pool.protocol = SEED_PROFILE.into();
    f.accounts.insert(pool.clone(), state);
    let mut extension = f.accounts.remove(&old_extension).unwrap();
    put_key(&mut extension.data, 8, &pool);
    f.accounts.insert(extension_address(&f), extension);
    f
}

fn seed_fixture(seed: u16) -> Fixture {
    bind_seed(bitmap_fixture(), seed)
}

#[test]
fn seed_decodes_all_u16_boundaries_and_static_fee_modes() {
    let mut pools = std::collections::BTreeSet::new();
    for seed in [1u16, 255, 256, 65535] {
        for mode in 0..=2 {
            let mut f = seed_fixture(seed);
            f.state()[390] = mode;
            let p = f.decode().unwrap();
            p.validate().unwrap();
            assert_eq!(p.adapter_version, "raydium-clmm/6d7a0f82-spl-custom-seed-1");
            let s = p.raydium_clmm.as_ref().unwrap();
            assert_eq!(s.seed_index, u32::from(seed));
            assert_eq!(s.collect_fee_on, u32::from(mode));
            assert_eq!(
                s.bitmap_extension.as_ref().unwrap().address,
                extension_address(&f)
            );
            assert_eq!(
                serde_json::to_value(&p).unwrap()["raydium_clmm"]["seed_index"],
                seed
            );
            assert_eq!(p.pool_id, seed_pool_address(&f, seed).0);
            pools.insert(p.pool_id);
        }
    }
    assert_eq!(pools.len(), 4);
}

#[test]
fn seed_required_accounts_derive_from_configured_pool_without_guessing_seed() {
    for seed in [1, 255, 256, 65535] {
        for count in 3..=7 {
            let indices = (-1..count - 1).collect::<Vec<_>>();
            let f = bind_seed(bitmap_window(&indices), seed);
            let ids = required_accounts(&f.pool).unwrap();
            assert_eq!(f.pool.accounts.len(), 7);
            assert_eq!(ids.len(), 12 + count as usize);
            assert_eq!(ids.last().unwrap(), &extension_address(&f));
            assert_eq!(
                ids.iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>(),
                f.accounts.keys().cloned().collect()
            );
            f.decode().unwrap();
        }
    }
    // No seed exists in PoolConfig: the configured pool is sufficient for fetching.
    let mut f = seed_fixture(256);
    f.state()[391..393].fill(0);
    assert_eq!(required_accounts(&f.pool).unwrap().len(), 15);
    assert!(f.decode().is_err());
}

#[test]
fn seed_rejects_zero_wrong_endianness_padding_and_legacy_pool_identity() {
    assert!(seed_fixture(0).decode().is_err());
    for seed in [1u16, 255, 256, 65535] {
        let mut legacy = bitmap_fixture();
        legacy.pool.protocol = SEED_PROFILE.into();
        legacy.state()[391..393].copy_from_slice(&seed.to_le_bytes());
        assert!(legacy.decode().is_err(), "legacy pool PDA, seed {seed}");
        for offset in 393..397 {
            let mut f = seed_fixture(seed);
            f.state()[offset] = 1;
            assert!(f.decode().is_err(), "padding {offset}, seed {seed}");
        }
    }
    for seed in [1u16, 255, 256] {
        let mut f = seed_fixture(seed);
        f.state()[391..393].copy_from_slice(&seed.to_be_bytes());
        assert!(f.decode().is_err(), "BE seed {seed}");
    }
    // A two-byte zero suffix is not the source's legacy empty seed.
    let f = seed_fixture(1);
    let wrong = pda(
        &[
            b"pool",
            &bytes(&f.pool.accounts[3]),
            &bytes(&f.pool.token0),
            &bytes(&f.pool.token1),
            &[0, 0],
        ],
        PROGRAM,
    )
    .0;
    assert_ne!(wrong, seed_pool_address(&f, 0).0);
}

#[test]
fn seed_old_profiles_remain_zero_and_omit_zero_json_field() {
    for profile in [
        "raydium-clmm-static",
        "raydium-clmm-token-fee",
        "raydium-clmm-bitmap",
    ] {
        let mut old = bitmap_fixture();
        old.pool.protocol = profile.into();
        if profile == "raydium-clmm-token-fee" {
            old.state()[390] = 1;
        }
        let p = old.decode().unwrap();
        assert_eq!(p.raydium_clmm.as_ref().unwrap().seed_index, 0);
        assert!(serde_json::to_value(p).unwrap()["raydium_clmm"]
            .get("seed_index")
            .is_none());
        for seed in [1u16, 255, 256, 65535] {
            old.state()[391..393].copy_from_slice(&seed.to_le_bytes());
            assert!(old.decode().is_err(), "{profile} borrowed seed {seed}");
            let mut custom = seed_fixture(seed);
            custom.pool.protocol = profile.into();
            if profile == "raydium-clmm-token-fee" {
                custom.state()[390] = 1;
            }
            assert!(
                custom.decode().is_err(),
                "{profile} borrowed custom PDA {seed}"
            );
        }
    }
}

#[test]
fn seed_rejects_wrong_bump_and_legacy_or_other_custom_vault_authority() {
    let legacy = Fixture::new();
    let legacy_bump = legacy.accounts[&legacy.pool.id].data[8];
    let mut checked_legacy_bump = false;
    for seed in [1, 255, 256, 65535] {
        let mut f = seed_fixture(seed);
        f.state()[8] ^= 1;
        assert!(f.decode().is_err(), "wrong bump {seed}");
        let mut f = seed_fixture(seed);
        if f.state()[8] != legacy_bump {
            checked_legacy_bump = true;
            f.state()[8] = legacy_bump;
            assert!(f.decode().is_err(), "legacy bump {seed}");
        }
        for authority in [
            &legacy.pool.id,
            &seed_fixture(if seed == 1 { 256 } else { 1 }).pool.id,
        ] {
            for i in 1..=2 {
                let mut f = seed_fixture(seed);
                put_key(f.data(i), 32, authority);
                assert!(f.decode().is_err(), "vault {i} seed {seed}");
            }
        }
    }
    assert!(checked_legacy_bump);
}

#[test]
fn seed_rejects_every_missing_dependency_and_mixed_pool_bindings() {
    let f = seed_fixture(256);
    let other = seed_fixture(1);
    for id in required_accounts(&f.pool).unwrap() {
        let mut f = seed_fixture(256);
        f.accounts.remove(&id);
        assert!(f.decode().is_err(), "missing {id}");
    }
    for kind in ["extension", "observation", "array", "vault"] {
        let mut f = seed_fixture(256);
        let (id, other_id) = match kind {
            "extension" => (extension_address(&f), extension_address(&other)),
            "observation" => (f.pool.accounts[6].clone(), other.pool.accounts[6].clone()),
            "array" => (f.arrays[1].clone(), other.arrays[1].clone()),
            "vault" => (f.pool.accounts[1].clone(), other.pool.accounts[1].clone()),
            _ => unreachable!(),
        };
        f.accounts.insert(id, other.accounts[&other_id].clone());
        assert!(f.decode().is_err(), "mixed {kind}");
    }
    let mut f = seed_fixture(256);
    let extension = f.accounts.remove(&extension_address(&f)).unwrap();
    f.accounts.insert(extension_address(&other), extension);
    assert!(f.decode().is_err(), "wrong extension PDA");
}

#[test]
fn seed_preserves_bitmap_edge_proofs_and_all_224_words() {
    for indices in [
        vec![511, 512, 513],
        vec![-513, -512, -511],
        vec![-1025, -1024, -1023],
    ] {
        for seed in [1, 255, 256, 65535] {
            let mut f = bind_seed(bitmap_window(&indices), seed);
            let s = f.decode().unwrap().raydium_clmm.unwrap();
            assert_eq!(s.tick_arrays.len(), 3);
            for &index in &indices {
                assert!(s.array_initialized(index).unwrap());
            }
            set_raw_array_bit(&mut f, indices[0], false);
            assert!(
                f.decode().is_err(),
                "missing initialized array bit {seed} {indices:?}"
            );
        }
    }
    let mut f = seed_fixture(65535);
    let id = extension_address(&f);
    let words = (0..224u64)
        .map(|i| 0xa123_4567_0000_0001u64 + (i << 24))
        .collect::<Vec<_>>();
    for (i, word) in words.iter().enumerate() {
        put64(
            &mut f.accounts.get_mut(&id).unwrap().data,
            40 + i * 8,
            *word,
        );
    }
    let extension = f
        .decode()
        .unwrap()
        .raydium_clmm
        .unwrap()
        .bitmap_extension
        .unwrap();
    assert_eq!(extension.positive, words[..112]);
    assert_eq!(extension.negative, words[112..]);
}

#[test]
fn seed_keeps_code_clock_spl_open_time_and_unsupported_feature_guards() {
    for offset in [
        273, 325, 389, 390, 393, 396, 397, 454, 550, 1032, 1096, 1176, 1543,
    ] {
        let mut f = seed_fixture(256);
        f.state()[offset] = 255;
        assert!(f.decode().is_err(), "unsupported pool offset {offset}");
    }
    let mut f = seed_fixture(256);
    put64(f.state(), 1080, 1000);
    assert_eq!(f.decode().unwrap_err().0, "POOL_NOT_OPEN");
    for id in [
        PROGRAM.to_owned(),
        seed_fixture(256)
            .pool
            .raydium_clmm
            .unwrap()
            .expected_programdata,
        TOKEN_PROGRAM.to_owned(),
        CLOCK.to_owned(),
    ] {
        let mut f = seed_fixture(256);
        f.accounts.get_mut(&id).unwrap().data[0] ^= 1;
        assert!(f.decode().is_err(), "bad proof {id}");
    }
    for index in [1, 2, 4, 5] {
        let mut f = seed_fixture(256);
        f.accounts.get_mut(&f.pool.accounts[index]).unwrap().owner = PROGRAM.into();
        assert!(f.decode().is_err(), "wrong SPL owner {index}");
    }
    let mut f = seed_fixture(256);
    f.array(1)[44 + 116] = 1;
    assert!(f.decode().is_err(), "limit orders");
    for kind in 0..4 {
        let mut f = seed_fixture(256);
        let id = extension_address(&f);
        let extension = f.accounts.get_mut(&id).unwrap();
        match kind {
            0 => extension.owner = TOKEN_PROGRAM.into(),
            1 => extension.executable = true,
            2 => extension.data[0] ^= 1,
            3 => {
                extension.data.pop();
            }
            _ => unreachable!(),
        }
        assert!(f.decode().is_err(), "bad extension {kind}");
    }
}

#[tokio::test]
async fn seed_rpc_fetches_complete_finalized_15_to_19_account_batches() {
    for count in 3..=7 {
        let indices = (-1..count - 1).collect::<Vec<_>>();
        let f = bind_seed(bitmap_window(&indices), 256);
        let ids = required_accounts(&f.pool)
            .unwrap()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let (result, calls) = run_rpc(f, false).await;
        let snapshot = result.unwrap();
        assert_eq!(
            snapshot.pools[0].raydium_clmm.as_ref().unwrap().seed_index,
            256
        );
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[1]["method"], "getMultipleAccounts");
        assert_eq!(
            calls[1]["params"][0].as_array().unwrap().len(),
            12 + count as usize
        );
        assert_eq!(
            calls[1]["params"][0]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>(),
            ids
        );
        assert_eq!(calls[1]["params"][1]["commitment"], "finalized");
        assert_eq!(calls[2]["method"], "getBlock");
        assert_eq!(calls[3]["method"], "getBlock");
        for call in &calls[2..] {
            assert_eq!(call["params"][0], 100);
            assert_eq!(call["params"][1]["commitment"], "finalized");
        }
    }
}

#[tokio::test]
async fn seed_rpc_rejects_nonfinalized_missing_dependencies_clock_and_postblock_changes() {
    let mut f = seed_fixture(256);
    f.chain.confirmation_tag = "confirmed".into();
    let (result, calls) = run_rpc(f, false).await;
    assert_eq!(result.unwrap_err().0, "RAYCLMM_REQUIRES_FINALIZED");
    assert!(calls.is_empty());
    let (result, calls) = run_rpc(seed_fixture(256), true).await;
    assert_eq!(result.unwrap_err().0, "ACCOUNT_BANK_ANCHOR_CHANGED");
    assert_eq!(calls.len(), 4);
    for id in required_accounts(&seed_fixture(256).pool).unwrap() {
        let mut f = seed_fixture(256);
        f.accounts.remove(&id);
        let (result, _) = run_rpc(f, false).await;
        assert!(result.is_err(), "missing atomic dependency {id}");
    }
    let mut f = seed_fixture(256);
    put64(&mut f.accounts.get_mut(CLOCK).unwrap().data, 0, 101);
    let (result, _) = run_rpc(f, false).await;
    assert_eq!(result.unwrap_err().0, "CLOCK_CONTEXT_MISMATCH");
}

#[test]
fn seed_preserves_custom_pool_wsol_backing_and_native_flags() {
    let mut f = Fixture::with_mints(key(1), "So11111111111111111111111111111111111111112".into());
    attach_bitmap_extension(&mut f);
    let mut f = bind_seed(f, 65535);
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

// Exercise the actual RPC with multiple configured pools in one atomic scope.
async fn run_seed_scope_rpc(
    mut fixtures: Vec<Fixture>,
) -> (Result<xone_domain::Snapshot>, Vec<serde_json::Value>) {
    use base64::Engine;
    let mut f = fixtures.remove(0);
    f.chain.pools = vec![f.pool];
    for next in fixtures {
        f.chain.pools.push(next.pool);
        f.accounts.extend(next.accounts);
    }
    let accounts = f.accounts.iter().map(|(id, a)| (id.clone(), json!({
        "owner":a.owner, "data":[base64::engine::general_purpose::STANDARD.encode(&a.data), "base64"],
        "executable":a.executable, "lamports":a.lamports,
    }))).collect();
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let state = MockRpc {
        accounts: std::sync::Arc::new(accounts),
        calls: calls.clone(),
        change_anchor: false,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new()
                .route("/", axum::routing::post(serve_rpc))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    f.chain.rpc_urls = vec![format!("http://{addr}/")];
    let result = xone_solana_ingest::rpc::Rpc {
        http: reqwest::Client::new(),
        max_bytes: 1_000_000,
    }
    .current_baseline(&f.chain)
    .await;
    task.abort();
    let requests = calls.lock().unwrap().clone();
    (result, requests)
}

#[tokio::test]
async fn seed_rpc_keeps_same_mint_custom_pools_distinct_in_one_bank_and_scope() {
    let a = seed_fixture(1);
    let b = seed_fixture(256);
    let expected = vec![a.decode().unwrap(), b.decode().unwrap()];
    let expected_ids = required_accounts(&a.pool)
        .unwrap()
        .into_iter()
        .chain(required_accounts(&b.pool).unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(a.pool.token0, b.pool.token0);
    assert_eq!(a.pool.token1, b.pool.token1);
    assert_ne!(a.pool.id, b.pool.id);
    assert_ne!(
        xone_domain::scope_hash(&expected[..1]).unwrap(),
        xone_domain::scope_hash(&expected[1..]).unwrap()
    );
    let (result, calls) = run_seed_scope_rpc(vec![a, b]).await;
    let snapshot = result.unwrap();
    assert_eq!(snapshot.pools, expected);
    assert_eq!(
        snapshot.reference.scope_hash,
        xone_domain::scope_hash(&expected).unwrap()
    );
    let ids = calls[1]["params"][0].as_array().unwrap();
    assert_eq!(ids.len(), expected_ids.len());
    assert_eq!(
        ids.iter()
            .map(|id| id.as_str().unwrap().to_owned())
            .collect::<std::collections::BTreeSet<_>>(),
        expected_ids
    );
    assert_eq!(calls.len(), 4);
    let mut b = seed_fixture(256);
    put_key(b.data(1), 32, &seed_fixture(1).pool.id);
    let (result, _) = run_seed_scope_rpc(vec![seed_fixture(1), b]).await;
    assert_eq!(result.unwrap_err().0, "POOL_ACCOUNT_IDENTITY");
}

#[tokio::test]
async fn seed_rpc_decodes_all_custom_pools_across_same_context_shards() {
    let fixtures = (1..=13).map(seed_fixture).collect::<Vec<_>>();
    let expected = fixtures
        .iter()
        .map(|f| f.decode().unwrap())
        .collect::<Vec<_>>();
    let (result, calls) = run_seed_scope_rpc(fixtures).await;
    let snapshot = result.unwrap();
    assert_eq!(snapshot.pools, expected);
    assert_eq!(snapshot.source, "RPC_SAME_FINALIZED_CONTEXT_SHARDS");
    let batches = calls
        .iter()
        .filter(|q| q["method"] == "getMultipleAccounts")
        .collect::<Vec<_>>();
    assert!(batches.len() > 1);
    for q in batches {
        let ids = q["params"][0].as_array().unwrap();
        assert!(ids.len() <= 100);
        assert!(ids.contains(&json!(CLOCK)));
        assert!(q["params"][1].get("minContextSlot").is_none());
    }
    assert_eq!(
        calls.iter().filter(|q| q["method"] == "getBlock").count(),
        2
    );
}

#[test]
fn original_seed_account_fixture_matches_full_custom_pool() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/solana/rayclmm-seed-base.json"
    ))
    .unwrap();
    let mut fixtures = vec![
        &fixture,
        &fixture["metadata"]["next_bank_fixture"],
        &fixture["metadata"]["alternate_negative_base"],
    ];
    fixtures.extend(fixture["metadata"]["seed_fixtures"].as_array().unwrap());
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
