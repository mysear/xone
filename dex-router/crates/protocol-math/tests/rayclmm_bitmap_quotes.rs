//! Public-dispatch bitmap quotes and preservation of the existing fee modes.
use num_bigint::BigUint;
use sha2::{Digest, Sha256};
use xone_domain::raydium_clmm::{RaydiumCLMMBitmapExtension, PROGRAM_ID};
use xone_domain::{rayclmm_math::sqrt_price_from_tick, Amount, Pool};
use xone_protocol_math::quote_exact_in;

fn base_pool() -> Pool {
    serde_json::from_str(include_str!("../../../test/contracts/rayclmm-state.json")).unwrap()
}
fn amount(n: u64) -> Amount {
    Amount(BigUint::from(n))
}
fn pda(seeds: &[&[u8]]) -> String {
    let program = bs58::decode(PROGRAM_ID).into_vec().unwrap();
    for bump in (0..=255u8).rev() {
        let mut hash = Sha256::new();
        for seed in seeds {
            hash.update(seed);
        }
        hash.update([bump]);
        hash.update(&program);
        hash.update(b"ProgramDerivedAddress");
        let raw: [u8; 32] = hash.finalize().into();
        if curve25519_dalek::edwards::CompressedEdwardsY(raw)
            .decompress()
            .is_none()
        {
            return bs58::encode(raw).into_string();
        }
    }
    panic!("fixture PDA derivation exhausted bumps");
}
fn with_extension(mut pool: Pool) -> Pool {
    let key = bs58::decode(&pool.pool_id).into_vec().unwrap();
    pool.protocol = "raydium-clmm-bitmap".into();
    pool.adapter_version = "raydium-clmm/6d7a0f82-spl-bitmap-1".into();
    pool.raydium_clmm.as_mut().unwrap().bitmap_extension = Some(RaydiumCLMMBitmapExtension {
        address: pda(&[b"pool_tick_array_bitmap_extension", &key]),
        positive: vec![0; 112],
        negative: vec![0; 112],
    });
    pool
}
fn boundary_pool(index: i32, mode: u32) -> Pool {
    let mut pool = with_extension(base_pool());
    let key = bs58::decode(&pool.pool_id).into_vec().unwrap();
    let state = pool.raydium_clmm.as_mut().unwrap();
    state.collect_fee_on = mode;
    state.tick_spacing = 1;
    state.tick_current = index * 60 + 30;
    state.sqrt_price_x64 = Amount(sqrt_price_from_tick(state.tick_current).unwrap());
    state.bitmap.fill(0);
    let array_template = state.tick_arrays[0].clone();
    let tick_template = array_template.ticks[0].clone();
    state.tick_arrays.clear();
    for index in index - 1..=index + 1 {
        let start = index * 60;
        let mut array = array_template.clone();
        array.start_tick_index = start;
        array.address = pda(&[b"tick_array", &key, &start.to_be_bytes()]);
        array.ticks = [start, start + 59]
            .into_iter()
            .map(|tick| {
                let mut initialized = tick_template.clone();
                initialized.tick = tick;
                initialized.liquidity_net = "0".into();
                initialized
            })
            .collect();
        state.tick_arrays.push(array);
        if (-512..=511).contains(&index) {
            let bit = (index + 512) as usize;
            state.bitmap[bit / 64] |= 1 << (bit % 64);
        } else {
            let extension = state.bitmap_extension.as_mut().unwrap();
            let (words, bit) = if index >= 512 {
                (&mut extension.positive, (index - 512) as usize)
            } else {
                let group = (-index - 1) / 512 - 1;
                let bit = index.rem_euclid(512);
                (&mut extension.negative, (group * 512 + bit) as usize)
            };
            words[bit / 64] |= 1 << (bit % 64);
        }
    }
    pool.validate().unwrap();
    pool
}

#[test]
fn bitmap_base_quotes_preserve_complete_legacy_fee_post_pools() {
    for mode in 0..=2 {
        for left in [true, false] {
            let mut legacy = base_pool();
            if mode != 0 {
                legacy.protocol = "raydium-clmm-token-fee".into();
                legacy.adapter_version = "raydium-clmm/6d7a0f82-spl-token-fee-1".into();
            }
            legacy.raydium_clmm.as_mut().unwrap().collect_fee_on = mode;
            let bitmap = with_extension(legacy.clone());
            let before = bitmap.clone();
            let old = quote_exact_in(&legacy, &amount(100_000), left).unwrap();
            let new = quote_exact_in(&bitmap, &amount(100_000), left).unwrap();
            assert_eq!(bitmap, before);
            assert_eq!(new.amount_out, old.amount_out);
            assert_eq!(new.consumed_in, old.consumed_in);
            assert_eq!(new.remaining_in, old.remaining_in);
            assert_eq!(new.next_pool, with_extension(old.next_pool));
        }
    }
}

#[test]
fn extension_current_array_and_boundary_crossings_use_public_family_dispatch() {
    let mut crossed = [false; 2];
    for index in [511, 512, -512, -513, 1023, 1024, -1024, -1025] {
        for mode in 0..=2 {
            for left in [true, false] {
                let pool = boundary_pool(index, mode);
                let before = pool.clone();
                let quote = quote_exact_in(&pool, &amount(100_000), left)
                    .unwrap_or_else(|e| panic!("index={index}, mode={mode}, left={left}: {}", e.0));
                assert_eq!(pool, before);
                assert_eq!(quote.consumed_in, amount(100_000));
                assert_eq!(quote.remaining_in, Amount::zero());
                assert!(quote.amount_out > Amount::zero());
                let old = pool.raydium_clmm.as_ref().unwrap();
                let new = quote.next_pool.raydium_clmm.as_ref().unwrap();
                assert_eq!(new.bitmap, old.bitmap);
                assert_eq!(new.bitmap_extension, old.bitmap_extension);
                assert_eq!(new.recent_epoch, new.bank_epoch);
                assert_eq!(new.observation_recent_epoch, old.observation_recent_epoch);
                crossed[usize::from(left)] |= new.tick_current.div_euclid(60) != index;
                quote.next_pool.validate().unwrap();
            }
        }
    }
    assert!(
        crossed.into_iter().all(|value| value),
        "cross arrays in both directions"
    );
}

#[test]
fn malformed_or_missing_extension_quotes_reject_without_mutating_the_pool() {
    let valid = boundary_pool(512, 0);
    for failure in [
        "missing",
        "positive-width",
        "negative-width",
        "wrong-pda",
        "legacy-static",
        "legacy-token-fee",
    ] {
        let mut pool = valid.clone();
        let state = pool.raydium_clmm.as_mut().unwrap();
        match failure {
            "missing" => state.bitmap_extension = None,
            "positive-width" => {
                state.bitmap_extension.as_mut().unwrap().positive.pop();
            }
            "negative-width" => {
                state.bitmap_extension.as_mut().unwrap().negative.push(0);
            }
            "wrong-pda" => state.bitmap_extension.as_mut().unwrap().address = pool.pool_id.clone(),
            "legacy-static" => {
                pool.protocol = "raydium-clmm-static".into();
                pool.adapter_version = "raydium-clmm/6d7a0f82-spl-static-1".into();
            }
            "legacy-token-fee" => {
                pool.protocol = "raydium-clmm-token-fee".into();
                pool.adapter_version = "raydium-clmm/6d7a0f82-spl-token-fee-1".into();
                state.collect_fee_on = 1;
            }
            _ => unreachable!(),
        }
        let before = pool.clone();
        for left in [true, false] {
            assert!(
                quote_exact_in(&pool, &amount(100_000), left).is_err(),
                "{failure}"
            );
            assert_eq!(pool, before, "{failure}: caller Pool changed");
        }
    }
}

#[test]
fn bitmap_step_budget_counts_executed_steps_instead_of_all_proof_ticks() {
    for mode in 0..=2 {
        let mut partial = boundary_pool(512, mode);
        partial.raydium_clmm.as_mut().unwrap().max_steps = 1;
        let before = partial.clone();
        let quote = quote_exact_in(&partial, &amount(100_000), false).unwrap();
        assert_eq!(partial, before);
        assert_eq!(quote.consumed_in, amount(100_000));
        assert_eq!(quote.remaining_in, Amount::zero());
        assert_eq!(
            quote.next_pool.raydium_clmm.as_ref().unwrap().tick_current / 60,
            512
        );

        let mut crossing = boundary_pool(1024, mode);
        crossing.raydium_clmm.as_mut().unwrap().max_steps = 1;
        let before = crossing.clone();
        assert_eq!(
            quote_exact_in(&crossing, &amount(100_000), true)
                .unwrap_err()
                .0,
            "RAYCLMM_STEP_BUDGET"
        );
        assert_eq!(crossing, before);
    }
}
