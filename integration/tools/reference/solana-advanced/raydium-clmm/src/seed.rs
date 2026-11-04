//! Customizable-v2 u16 LE identity adapters; all prices and swaps remain upstream.
use super::*;

pub const SEED_PROFILE: &str = "raydium-clmm-custom-seed";
pub const SEED_ADAPTER: &str = "raydium-clmm/6d7a0f82-spl-custom-seed-1";
pub const SEED_INDICES: [u32; 4] = [1, 255, 256, 65535];

fn pool_identity(p: &Pool, seed: u16) -> (String, u8) {
    let config = pk(&p.raydium_clmm.amm_config);
    let mint0 = pk(&p.token0);
    let mint1 = pk(&p.token1);
    let le = seed.to_le_bytes();
    let (address, bump) = pda(
        &[
            POOL_SEED.as_bytes(),
            config.as_ref(),
            mint0.as_ref(),
            mint1.as_ref(),
            if seed == 0 { &[] } else { &le },
        ],
        &pk(PROGRAM),
    );
    // Verify the actual original setter, signer seeds and key, independently of find.
    let mut native = PoolState::default();
    native.amm_config = config;
    native.token_mint_0 = mint0;
    native.token_mint_1 = mint1;
    native.bump = [bump];
    native.set_seed_index(seed);
    assert_eq!(native.seed_index, le);
    assert_eq!(
        native.seeds()[4],
        if seed == 0 { &[] } else { le.as_slice() }
    );
    assert_eq!(native.key().to_string(), address);
    (address, bump)
}

fn bind_seed(mut p: Pool, seed: u32) -> Pool {
    let old_extension = TickArrayBitmapExtension::key(pk(&p.pool_id)).to_string();
    let (address, bump) = pool_identity(&p, u16::try_from(seed).expect("u16 seed"));
    p.protocol = SEED_PROFILE.into();
    p.adapter_version = SEED_ADAPTER.into();
    p.pool_id = address;
    let pool = pk(&p.pool_id);
    let s = &mut p.raydium_clmm;
    s.seed_index = seed;
    s.pool_bump = bump as u32;
    s.vault0 = pda(
        &[
            POOL_VAULT_SEED.as_bytes(),
            pool.as_ref(),
            pk(&p.token0).as_ref(),
        ],
        &pk(PROGRAM),
    )
    .0;
    s.vault1 = pda(
        &[
            POOL_VAULT_SEED.as_bytes(),
            pool.as_ref(),
            pk(&p.token1).as_ref(),
        ],
        &pk(PROGRAM),
    )
    .0;
    s.observation = pda(&[OBSERVATION_SEED.as_bytes(), pool.as_ref()], &pk(PROGRAM)).0;
    for a in &mut s.tick_arrays {
        a.address = pda(
            &[
                TICK_ARRAY_SEED.as_bytes(),
                pool.as_ref(),
                &a.start_tick_index.to_be_bytes(),
            ],
            &pk(PROGRAM),
        )
        .0;
    }
    if let Some(e) = s.bitmap_extension.as_mut() {
        // Retain deliberate wrong-PDA probes while rebinding valid dependencies.
        if e.address == old_extension {
            e.address = TickArrayBitmapExtension::key(pool).to_string();
        }
    }
    p
}

pub fn seed_pool(center_index: i32, mode: u32, seed: u32) -> Pool {
    assert!((1..=65535).contains(&seed));
    bind_seed(bitmap_pool(center_index, mode), seed)
}

pub(super) fn validate_seed_identity(p: &Pool) -> std::result::Result<(), String> {
    let s = &p.raydium_clmm;
    if !(if p.protocol == DYNAMIC_PROFILE {
        0..=65535
    } else {
        1..=65535
    })
    .contains(&s.seed_index)
    {
        return Err("seed_index must be 1..=65535 (u16 LE)".into());
    }
    for address in [
        &p.pool_id,
        &p.token0,
        &p.token1,
        &s.amm_config,
        &s.program_id,
    ] {
        Pubkey::from_str(address).map_err(|_| "seed identity pubkey")?;
    }
    if s.program_id != PROGRAM || pk(&p.token0) >= pk(&p.token1) {
        return Err("seed program / mint order".into());
    }
    let (address, bump) = pool_identity(p, s.seed_index as u16);
    if p.pool_id != address || s.pool_bump != bump as u32 {
        return Err("seed pool PDA / bump".into());
    }
    let pool = pk(&p.pool_id);
    for (actual, seeds) in [
        (
            &s.vault0,
            vec![
                POOL_VAULT_SEED.as_bytes(),
                pool.as_ref(),
                pk(&p.token0).as_ref(),
            ],
        ),
        (
            &s.vault1,
            vec![
                POOL_VAULT_SEED.as_bytes(),
                pool.as_ref(),
                pk(&p.token1).as_ref(),
            ],
        ),
        (
            &s.observation,
            vec![OBSERVATION_SEED.as_bytes(), pool.as_ref()],
        ),
    ] {
        if actual != &pda(&seeds, &pk(PROGRAM)).0 {
            return Err("seed dependency PDA".into());
        }
    }
    Ok(())
}

fn dependency(seeds: &[&[u8]]) -> Value {
    let (address, bump) = pda(seeds, &pk(PROGRAM));
    json!({"address":address,"bump":bump})
}

pub fn seed_identities() -> Value {
    let base = base_pool();
    let config = pk(&base.raydium_clmm.amm_config);
    let mint0 = pk(&base.token0);
    let mint1 = pk(&base.token1);
    let mut vectors = vec![];
    for seed in [0u16, 1, 2, 255, 256, 257, 32768, 65534, 65535] {
        let (pool_id, pool_bump) = pool_identity(&base, seed);
        let pool = pk(&pool_id);
        let (wrong_be_pool_id, wrong_be_pool_bump) = pda(
            &[
                POOL_SEED.as_bytes(),
                config.as_ref(),
                mint0.as_ref(),
                mint1.as_ref(),
                &seed.to_be_bytes(),
            ],
            &pk(PROGRAM),
        );
        let vault0 = dependency(&[POOL_VAULT_SEED.as_bytes(), pool.as_ref(), mint0.as_ref()]);
        let vault1 = dependency(&[POOL_VAULT_SEED.as_bytes(), pool.as_ref(), mint1.as_ref()]);
        let observation = dependency(&[OBSERVATION_SEED.as_bytes(), pool.as_ref()]);
        let extension = dependency(&[POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(), pool.as_ref()]);
        assert_eq!(
            extension["address"],
            TickArrayBitmapExtension::key(pool).to_string()
        );
        let arrays: Vec<_> = [
            -443640i32, -61440, -30780, 0, 30480, 30540, 30600, 30660, 30720, 30780, 30840, 61440,
            430080, 443580,
        ]
        .into_iter()
        .map(|start| {
            let mut d = dependency(&[
                TICK_ARRAY_SEED.as_bytes(),
                pool.as_ref(),
                &start.to_be_bytes(),
            ]);
            d["start_tick_index"] = json!(start);
            d
        })
        .collect();
        vectors.push(json!({"seed_index":seed,"seed_le":seed.to_le_bytes(),"seed_be":seed.to_be_bytes(),
            "pool_id":pool_id,"pool_bump":pool_bump,"program_id":PROGRAM,"amm_config":config.to_string(),"token0":base.token0,"token1":base.token1,
            "wrong_be_pool_id":wrong_be_pool_id,"wrong_be_pool_bump":wrong_be_pool_bump,
            "be_differs":seed != 0 && seed.to_le_bytes() != seed.to_be_bytes(),
            "vault0":vault0["address"],"vault1":vault1["address"],"observation":observation["address"],"bitmap_extension":extension["address"],
            "dependencies":{"vault0":vault0,"vault1":vault1,"observation":observation,"bitmap_extension":extension,"tick_arrays":arrays}}));
    }
    json!({"schema_version":1,"revision":REVISION,"program_id":PROGRAM,"profile":SEED_PROFILE,"adapter_version":SEED_ADAPTER,
        "amm_config":base.raydium_clmm.amm_config,"config_index":0,"config_index_encoding":"u16 BE", "token0":base.token0,"token1":base.token1,
        "pool_seed_encoding":"u16 LE; seed0 legacy omits nonempty seed", "array_start_encoding":"i32 BE",
        "source_functions":["Pubkey::find_program_address","Pubkey::create_program_address","PoolState::set_seed_index","PoolState::seeds","PoolState::key","TickArrayBitmapExtension::key"],
        "endian_note":"65535 (ff ff) and 257 (01 01) are endian-symmetric controls; seeds 1/255/256 must differ from BE. Legacy seed0 uses an empty seed, not two zero bytes.",
        "vectors":vectors})
}

pub fn seed_error_probes() -> Value {
    let base = seed_pool(512, 0, 1);
    let mut probes = vec![];
    let mut add = |name: &str, p: Pool, reason: &str| {
        let error = quote(&p, "1000000", false).err().expect(name);
        assert!(error.contains(reason), "{name}: {error}");
        probes.push(json!({"name":name,"pool":p,"input":"1000000","zero_for_one":false,"expected":{"reverted":true},"reason_contains":reason,"native_reason":error}));
    };
    for seed in [0, 65536] {
        let mut p = base.clone();
        p.raydium_clmm.seed_index = seed;
        add(&format!("seed-width-{seed}"), p, "seed_index");
    }
    for (profile, adapter) in [
        (PROFILE, ADAPTER),
        (TOKEN_FEE_PROFILE, TOKEN_FEE_ADAPTER),
        (BITMAP_PROFILE, BITMAP_ADAPTER),
    ] {
        let mut p = base.clone();
        p.protocol = profile.into();
        p.adapter_version = adapter.into();
        if profile == TOKEN_FEE_PROFILE {
            p.raydium_clmm.collect_fee_on = 1;
        }
        if profile != BITMAP_PROFILE {
            p.raydium_clmm.bitmap_extension = None;
        }
        add(
            &format!("nonzero-seed-old-profile-{profile}"),
            p,
            "bounded profile",
        );
    }
    let mut p = bitmap_pool(512, 0);
    p.protocol = SEED_PROFILE.into();
    p.adapter_version = SEED_ADAPTER.into();
    p.raydium_clmm.seed_index = 1;
    add("new-profile-old-pool-PDA", p, "seed pool PDA");
    for seed in [1u16, 255, 256] {
        let mut p = seed_pool(512, 0, seed as u32);
        let (wrong, bump) = pda(
            &[
                POOL_SEED.as_bytes(),
                pk(&p.raydium_clmm.amm_config).as_ref(),
                pk(&p.token0).as_ref(),
                pk(&p.token1).as_ref(),
                &seed.to_be_bytes(),
            ],
            &pk(PROGRAM),
        );
        p.pool_id = wrong;
        p.raydium_clmm.pool_bump = bump as u32;
        add(&format!("wrong-BE-seed{seed}"), p, "seed pool PDA");
    }
    let other = seed_pool(512, 0, 256);
    for field in [
        "pool_bump",
        "vault0",
        "vault1",
        "observation",
        "array",
        "extension",
        "missing-extension",
        "adapter",
        "program",
    ] {
        let mut p = base.clone();
        let s = &mut p.raydium_clmm;
        let reason = match field {
            "pool_bump" => {
                s.pool_bump = (s.pool_bump + 1) % 256;
                "seed pool PDA"
            }
            "vault0" => {
                s.vault0 = other.raydium_clmm.vault0.clone();
                "seed dependency PDA"
            }
            "vault1" => {
                s.vault1 = other.raydium_clmm.vault1.clone();
                "seed dependency PDA"
            }
            "observation" => {
                s.observation = other.raydium_clmm.observation.clone();
                "seed dependency PDA"
            }
            "array" => {
                s.tick_arrays[0].address = other.raydium_clmm.tick_arrays[0].address.clone();
                "array PDA"
            }
            "extension" => {
                s.bitmap_extension.as_mut().unwrap().address = other
                    .raydium_clmm
                    .bitmap_extension
                    .as_ref()
                    .unwrap()
                    .address
                    .clone();
                "extension width / PDA"
            }
            "missing-extension" => {
                s.bitmap_extension = None;
                "extension missing"
            }
            "adapter" => {
                p.adapter_version = BITMAP_ADAPTER.into();
                "bounded profile"
            }
            _ => {
                s.program_id = TOKEN.into();
                "seed program"
            }
        };
        add(&format!("mixed-identity-{field}"), p, reason);
    }
    json!(probes)
}

pub fn seed_next_bank_pool() -> Pool {
    bind_seed(bitmap_next_bank_pool(), 1)
}
fn vectors_for(p: &Pool) -> Value {
    json!([false,true].into_iter().flat_map(|d|[1_000_000,2_000_000_000u64].map(move |amount| json!({"input":amount.to_string(),"zero_for_one":d,"expected":expected(p,amount,d)}))).collect::<Vec<_>>())
}
pub fn seed_fixture() -> Value {
    let p = seed_pool(512, 0, 1);
    let mut f = fixture_from_bitmap(&p);
    f["metadata"]["seed_index"] = json!(1);
    f["metadata"]["seed_offset"] = json!([391, 393]);
    f["metadata"]["seed_encoding"] = json!(
        "original PoolState::set_seed_index(u16) little-endian; padding393..397 remains zero"
    );
    f["metadata"]["create_instruction"]=json!("create_customizable_pool_v2; canonical [pool,config,mint0,mint1,u16LE] PDA; source requires seed_index > 0");
    f["metadata"]["base_vectors"] = vectors_for(&p);
    f["metadata"]["alternate_negative_base"] = fixture_from_bitmap(&seed_pool(-512, 0, 1));
    let post = quote(&p, "1000000", true).unwrap().pool;
    f["metadata"]["alternate_fixture"] = fixture_from_bitmap(&post);
    f["metadata"]["alternate_fixture_source"] =
        json!({"input":"1000000","zero_for_one":true,"native_next_pool":true});
    let bank = seed_next_bank_pool();
    f["metadata"]["next_bank_fixture"] = fixture_from_bitmap(&bank);
    f["metadata"]["next_bank_vectors"] = vectors_for(&bank);
    f["metadata"]["next_bank_change"] = json!({"seed_index":[1,1],"collect_fee_on":[0,1],"cleared_array_start":bank.raydium_clmm.tick_arrays.last().unwrap().start_tick_index,"extension_bit_cleared":true,"initialized_tick_count_after":0,"same_account_window":true});
    f["metadata"]["next_bank_collect_fee_on"] = json!(1);
    f["metadata"]["next_bank_scope_probe"] = json!({"input":"30000000000","zero_for_one":false,"before_expected":expected(&p,30_000_000_000,false),"after_expected":expected(&bank,30_000_000_000,false),"mode_only_expected":expected(&seed_pool(512,1,1),30_000_000_000,false)});
    f["metadata"]["seed_fixtures"] =
        json!([255, 256, 65535].map(|seed| fixture_from_bitmap(&seed_pool(512, 0, seed))));
    f["metadata"]["seed_fixtures_scope"]=json!("Separate physical pools sharing config/mints. Each fixture carries its own pool_config and rpc account identities; never mutate the seed of the baseline account.");
    f
}

pub fn generate_seed() -> Value {
    // Reuse only the independent native bitmap test shapes; recompute every quote
    // after binding native customizable-v2 identity and every dependency to seed.
    let old = generate_bitmap();
    let mut cases = vec![];
    for (i, c) in old["cases"].as_array().unwrap().iter().enumerate() {
        let seed = SEED_INDICES[i % SEED_INDICES.len()];
        let p = bind_seed(serde_json::from_value(c["pool"].clone()).unwrap(), seed);
        let direction = c["zero_for_one"].as_bool().unwrap();
        // At the negative boundary, 2b finishes in one original step. 5b
        // succeeds through two arrays for all fee modes, so max_steps=1 is
        // an actual multi-step failure rather than just stricter admission.
        let amount = if direction
            && c["name"]
                .as_str()
                .unwrap()
                .contains("conservative-max-steps")
        {
            5_000_000_000
        } else {
            c["input"].as_str().unwrap().parse().unwrap()
        };
        add_case(
            &mut cases,
            &format!("seed{seed}-{}", c["name"].as_str().unwrap()),
            p,
            amount,
            direction,
        );
    }
    // Keep the four exact raw next-bank state/input pairs in the quote
    // corpus so full business poststate differential tests cover them too.
    for direction in [false, true] {
        for amount in [1_000_000, 2_000_000_000u64] {
            add_case(
                &mut cases,
                &format!("seed1-next-bank-exact-fixture-dir{direction}-input{amount}"),
                seed_next_bank_pool(),
                amount,
                direction,
            );
        }
    }
    let mut independent = vec![];
    for seed in SEED_INDICES {
        for mode in 0..=2 {
            for side in [512, -512] {
                for d in [false, true] {
                    for amount in [1_000_000, 2_000_000_000u64] {
                        let p = seed_pool(side, mode, seed);
                        let name = format!("seed{seed}-mode{mode}-index{side}-dir{d}-base{amount}");
                        let result = expected(&p, amount, d);
                        assert!(result.get("reverted").is_none());
                        independent.push(json!({"name":name,"seed_index":seed,"collect_fee_on":mode,"center_index":side,"input":amount.to_string(),"zero_for_one":d,"expected":result}));
                        add_case(&mut cases, &name, p, amount, d);
                    }
                }
            }
        }
    }
    let mut sequences = vec![];
    for seed in SEED_INDICES {
        for mode in 0..=2 {
            for directions in [[true, true], [false, false], [true, false], [false, true]] {
                let p = seed_pool(if directions[0] { 512 } else { -512 }, mode, seed);
                let mut current = p.clone();
                let mut steps = vec![];
                for (d, amount) in directions.into_iter().zip([1_000_000, 2_000_000_000u64]) {
                    let q = quote(&current, &amount.to_string(), d).expect("seed sequence");
                    steps.push(json!({"input":amount.to_string(),"zero_for_one":d,"expected":{"amount_out":q.amount_out.to_string(),"next_pool":q.pool}}));
                    current = q.pool;
                }
                sequences.push(json!({"name":format!("seed{seed}-mode{mode}-sequence-{}-{}",directions[0],directions[1]),"pool":p,"steps":steps,"expected_final_pool":current}));
            }
        }
    }
    let mut independence = vec![];
    for other in [255, 256, 65535] {
        let pools = [seed_pool(512, 0, 1), seed_pool(512, 0, other)];
        let mut current = pools.clone();
        let mut steps = vec![];
        for (index, d, amount) in [
            (0, false, 1_000_000u64),
            (1, true, 2_000_000_000),
            (0, true, 2_000_000_000),
            (1, false, 1_000_000),
        ] {
            let q = quote(&current[index], &amount.to_string(), d).unwrap();
            steps.push(json!({"pool_id":current[index].pool_id,"input":amount.to_string(),"zero_for_one":d,"expected":{"amount_out":q.amount_out.to_string(),"next_pool":q.pool}}));
            current[index] = q.pool;
        }
        independence.push(json!({"name":format!("same-config-mints-seed1-and-{other}"),"pools":pools,"steps":steps,"expected_final_pools":current}));
    }
    json!({"schema_version":1,"reference":{"revision":REVISION,"profile":SEED_PROFILE,"adapter_version":SEED_ADAPTER,
        "source":"unchanged native PoolState::set_seed_index/seeds/key and swap_internal; actual extension AccountInfo and RefMut/Clock", "sbf_transaction_executed":false,"business_math_imported":false,
        "scope":"u16 LE seed1..65535; mandatory full bitmap extension; static fee0/1/2; classic SPL; default dynamic fee; no orders/rewards", "max_steps":"conservative preflight initialized ticks + 1, not a runtime meter"},
        "base_pool":seed_pool(512,0,1),"negative_base_pool":seed_pool(-512,0,1),"seed_pools":SEED_INDICES.map(|seed|seed_pool(512,0,seed)),
        "next_bank_pool":seed_next_bank_pool(),"independent_vectors":independent,"cases":cases,"sequences":sequences,"independence_cases":independence,"error_probes":seed_error_probes()})
}
