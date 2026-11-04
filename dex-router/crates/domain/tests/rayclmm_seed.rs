use xone_domain::{
    raydium_clmm::{RaydiumCLMMBitmapExtension, RaydiumCLMMState, PROGRAM_ID},
    Pool,
};
fn pda(seeds: &[&[u8]]) -> (String, u32) {
    use sha2::{Digest, Sha256};
    let program = bs58::decode(PROGRAM_ID).into_vec().unwrap();
    for bump in (0..=255u8).rev() {
        let mut hash = Sha256::new();
        for seed in seeds {
            hash.update(seed)
        }
        hash.update([bump]);
        hash.update(&program);
        hash.update(b"ProgramDerivedAddress");
        let raw: [u8; 32] = hash.finalize().into();
        if curve25519_dalek::edwards::CompressedEdwardsY(raw)
            .decompress()
            .is_none()
        {
            return (bs58::encode(raw).into_string(), u32::from(bump));
        }
    }
    panic!("PDA")
}
fn bytes(s: &str) -> Vec<u8> {
    bs58::decode(s).into_vec().unwrap()
}
fn seeded(seed: u32) -> Pool {
    let mut p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    p.protocol = "raydium-clmm-custom-seed".into();
    p.adapter_version = "raydium-clmm/6d7a0f82-spl-custom-seed-1".into();
    let s = p.raydium_clmm.as_mut().unwrap();
    s.seed_index = seed;
    let (key, bump) = pda(&[
        b"pool",
        &bytes(&s.amm_config),
        &bytes(&p.token0),
        &bytes(&p.token1),
        &(seed as u16).to_le_bytes(),
    ]);
    p.pool_id = key;
    s.pool_bump = bump;
    let pool = bytes(&p.pool_id);
    s.vault0 = pda(&[b"pool_vault", &pool, &bytes(&p.token0)]).0;
    s.vault1 = pda(&[b"pool_vault", &pool, &bytes(&p.token1)]).0;
    s.observation = pda(&[b"observation", &pool]).0;
    s.bitmap_extension = Some(RaydiumCLMMBitmapExtension {
        address: pda(&[b"pool_tick_array_bitmap_extension", &pool]).0,
        positive: vec![0; 112],
        negative: vec![0; 112],
    });
    for a in &mut s.tick_arrays {
        a.address = pda(&[b"tick_array", &pool, &a.start_tick_index.to_be_bytes()]).0;
    }
    p
}
#[test]
fn custom_seed_is_bound_to_pool_and_protobuf_width() {
    for seed in [1, 255, 256, 65535] {
        let mut p = seeded(seed);
        p.validate().unwrap();
        let s = p.raydium_clmm.as_ref().unwrap();
        assert_eq!(
            &RaydiumCLMMState::from_proto(s.to_proto().unwrap()).unwrap(),
            s
        );
        p.raydium_clmm.as_mut().unwrap().seed_index ^= 1;
        assert!(p.validate().is_err());
    }
}
#[test]
fn custom_seed_rejects_zero_width_and_old_profile() {
    for seed in [0, 65536, u32::MAX] {
        assert!(seeded(seed).validate().is_err());
    }
    let mut p = seeded(1);
    p.protocol = "raydium-clmm-bitmap".into();
    p.adapter_version = "raydium-clmm/6d7a0f82-spl-bitmap-1".into();
    assert!(p.validate().is_err());
    p = seeded(1);
    p.raydium_clmm.as_mut().unwrap().bitmap_extension = None;
    assert!(p.validate().is_err());
}
#[test]
fn legacy_seed_zero_stays_omitted() {
    let p: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap();
    let value = serde_json::to_value(p.raydium_clmm.unwrap()).unwrap();
    assert!(value.get("seed_index").is_none());
}

#[test]
fn original_custom_seed_identity_vectors_bind_all_dependencies() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/rayclmm-seed-identities.json"
    ))
    .unwrap();
    let vectors = f["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 9);
    for v in vectors {
        let seed = v["seed_index"].as_u64().unwrap() as u16;
        let config = bytes(v["amm_config"].as_str().unwrap());
        let m0 = bytes(v["token0"].as_str().unwrap());
        let m1 = bytes(v["token1"].as_str().unwrap());
        let le = seed.to_le_bytes();
        let mut seeds = vec![
            b"pool".as_slice(),
            config.as_slice(),
            m0.as_slice(),
            m1.as_slice(),
        ];
        if seed > 0 {
            seeds.push(&le);
        }
        let (id, bump) = pda(&seeds);
        assert_eq!(id, v["pool_id"]);
        assert_eq!(u64::from(bump), v["pool_bump"].as_u64().unwrap());
        if seed > 0 {
            let be = seed.to_be_bytes();
            seeds[4] = &be;
            let (wrong, bump) = pda(&seeds);
            assert_eq!(wrong, v["wrong_be_pool_id"]);
            assert_eq!(u64::from(bump), v["wrong_be_pool_bump"].as_u64().unwrap());
            assert_eq!(wrong != id, v["be_differs"].as_bool().unwrap());
        }
        let pool = bytes(&id);
        for (key, seeds) in [
            (
                "vault0",
                vec![b"pool_vault".as_slice(), pool.as_slice(), m0.as_slice()],
            ),
            (
                "vault1",
                vec![b"pool_vault".as_slice(), pool.as_slice(), m1.as_slice()],
            ),
            (
                "observation",
                vec![b"observation".as_slice(), pool.as_slice()],
            ),
            (
                "bitmap_extension",
                vec![
                    b"pool_tick_array_bitmap_extension".as_slice(),
                    pool.as_slice(),
                ],
            ),
        ] {
            let (id, bump) = pda(&seeds);
            assert_eq!(id, v["dependencies"][key]["address"]);
            assert_eq!(
                u64::from(bump),
                v["dependencies"][key]["bump"].as_u64().unwrap()
            );
        }
        for a in v["dependencies"]["tick_arrays"].as_array().unwrap() {
            let start = i32::try_from(a["start_tick_index"].as_i64().unwrap()).unwrap();
            let (id, bump) = pda(&[b"tick_array", &pool, &start.to_be_bytes()]);
            assert_eq!(id, a["address"]);
            assert_eq!(u64::from(bump), a["bump"].as_u64().unwrap());
        }
    }
}
