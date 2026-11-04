mod orca_support;
use orca_support::*;
use xone_solana_ingest::orca;
#[test]
fn decodes_both_real_layouts_without_virtual_reserves() {
    for dynamic in [false, true] {
        let f = Fixture::new(dynamic);
        let p = f.decode().unwrap();
        assert_eq!(p.reserve0.to_string(), "0");
        assert_eq!(p.reserve1.to_string(), "0");
        assert_eq!(p.fee_numerator, 3000);
        assert_eq!(p.fee_denominator, 1000000);
        assert_eq!(p.adapter_version, "orca-whirlpools/cc683ec9-spl-static-1");
        let s = p.orca_whirlpool.as_ref().unwrap();
        s.validate(&p).unwrap();
        assert_eq!(s.raw_vault0, 10000000);
        assert_eq!(s.bank_epoch, 7);
        assert_eq!(s.tick_arrays.len(), 3);
        assert_eq!(
            s.tick_arrays[0].layout,
            if dynamic { "dynamic" } else { "fixed" }
        );
        assert_eq!(s.reward_last_updated_timestamp, 900);
    }
}
#[test]
fn plans_atomic_accounts_and_checks_programdata() {
    let mut f = Fixture::new(false);
    let ids = orca::required_accounts(&f.pool).unwrap();
    assert_eq!(ids.len(), 14);
    assert!(ids.contains(&PD.into()));
    assert!(ids.contains(&f.oracle));
    for a in &f.arrays {
        assert!(ids.contains(a));
    }
    f.pool.orca_whirlpool.as_mut().unwrap().expected_programdata = key(99);
    assert!(orca::required_accounts(&f.pool).is_err());
}
#[test]
fn only_declared_oracle_null_can_become_virtual_system_empty() {
    let f = Fixture::new(false);
    let a = orca::decode_rpc_account(&f.oracle, &f.oracle, &serde_json::Value::Null).unwrap();
    assert_eq!(a.owner, SYSTEM);
    assert!(a.data.is_empty());
    assert_eq!(a.lamports, 0);
    assert!(orca::decode_rpc_account(&f.pool.id, &f.oracle, &serde_json::Value::Null).is_err());
}

#[test]
fn rejects_static_profile_identity_clock_reward_and_token_violations() {
    let mutations: Vec<(&str, usize, Vec<u8>)> = vec![
        (PROGRAM, 0, vec![0]),
        (PD, 4, 101u64.to_le_bytes().to_vec()),
        (PD, 45, vec![255]),
        (CLOCK, 0, 99u64.to_le_bytes().to_vec()),
        (CLOCK, 32, (-1i64).to_le_bytes().to_vec()),
        ("pool", 40, vec![0]),
        ("pool", 43, 65u16.to_le_bytes().to_vec()),
        ("pool", 261, 1001u64.to_le_bytes().to_vec()),
        ("pool", 269, vec![1]),
        ("pool", 301, vec![1]),
        ("pool", 365, vec![1]),
        ("pool", 381, vec![1]),
        ("pool", 461, vec![1]),
        ("pool", 589, vec![1]),
        ("config", 106, vec![1]),
        ("config", 0, vec![1]),
        ("vault", 108, vec![2]),
        ("vault", 72, 1u32.to_le_bytes().to_vec()),
        ("vault", 121, 1u64.to_le_bytes().to_vec()),
        ("vault", 129, 1u32.to_le_bytes().to_vec()),
        ("vault", 109, 1u32.to_le_bytes().to_vec()),
        ("mint", 45, vec![0]),
        ("mint", 0, 2u32.to_le_bytes().to_vec()),
        ("mint", 46, 2u32.to_le_bytes().to_vec()),
    ];
    for (id, off, data) in mutations {
        let mut f = Fixture::new(false);
        let id = match id {
            "pool" => f.pool.id.clone(),
            "config" => f.pool.accounts[3].clone(),
            "vault" => f.pool.accounts[1].clone(),
            "mint" => f.pool.accounts[4].clone(),
            _ => id.into(),
        };
        f.accounts.get_mut(&id).unwrap().data[off..off + data.len()].copy_from_slice(&data);
        assert!(f.decode().is_err(), "accepted mutation {id}:{off}");
    }
}
#[test]
fn missing_wrong_owner_or_executable_business_accounts_are_rejected() {
    for dynamic in [false, true] {
        for id in Fixture::new(dynamic).accounts.keys() {
            let mut f = Fixture::new(dynamic);
            f.accounts.remove(id);
            assert!(f.decode().is_err(), "missing {id}");
            if id != PROGRAM && id != xone_solana_ingest::rpc::TOKEN_PROGRAM {
                let mut f = Fixture::new(dynamic);
                f.accounts.get_mut(id).unwrap().executable = true;
                assert!(f.decode().is_err(), "executable {id}");
            }
            let mut f = Fixture::new(dynamic);
            f.accounts.get_mut(id).unwrap().owner = key(88);
            assert!(f.decode().is_err(), "owner {id}");
        }
    }
}
#[test]
fn initialized_oracle_and_unfinalized_or_wrong_family_are_rejected() {
    let mut f = Fixture::new(false);
    f.accounts.get_mut(&f.oracle).unwrap().data = vec![0; 254];
    assert!(f.decode().is_err());
    let mut f = Fixture::new(false);
    f.chain.confirmation_tag = "confirmed".into();
    assert!(f.decode().is_err());
    let mut f = Fixture::new(false);
    f.chain.key = "eip155:1".into();
    assert!(f.decode().is_err());
}
#[test]
fn explicit_array_window_must_be_contiguous_aligned_and_cover_current_tick() {
    for starts in [
        vec![],
        vec![0, 5632],
        vec![-5632, 5632, 11264],
        vec![-5631, 1, 5633],
        vec![5632, 11264, 16896],
        vec![0; 8],
    ] {
        let mut f = Fixture::new(false);
        f.pool.orca_whirlpool.as_mut().unwrap().tick_array_starts = starts;
        assert!(f.decode().is_err());
    }
    for steps in [0, 8193] {
        let mut f = Fixture::new(false);
        f.pool.orca_whirlpool.as_mut().unwrap().max_steps = steps;
        assert!(f.decode().is_err());
    }
}
#[test]
fn arrays_reject_bad_lengths_discriminator_pool_start_flags_bitmap_and_reward_growth() {
    for dynamic in [false, true] {
        for len in [0, 7, 147, 149, 9987, 9989, 10005] {
            let mut f = Fixture::new(dynamic);
            f.accounts
                .get_mut(&f.arrays[0])
                .unwrap()
                .data
                .resize(len, 0);
            assert!(f.decode().is_err(), "layout {dynamic} len {len}");
        }
        for (off, value) in [
            (0, 1),
            (8, 1),
            (if dynamic { 12 } else { 9956 }, 1),
            (if dynamic { 60 } else { 12 }, 2),
            (if dynamic { 59 } else { 77 }, 1),
        ] {
            let mut f = Fixture::new(dynamic);
            f.accounts.get_mut(&f.arrays[0]).unwrap().data[off] = value;
            assert!(f.decode().is_err(), "array {dynamic} offset {off}");
        }
    }
}
#[test]
fn parses_initialized_ticks_fee_outside_and_rejects_inconsistent_gross_net() {
    for dynamic in [false, true] {
        let mut f = Fixture::new(dynamic);
        let a = &mut f.accounts.get_mut(&f.arrays[1]).unwrap().data;
        let off = if dynamic {
            a.splice(61..61, vec![0; 112]);
            put128(a, 44, 1);
            60
        } else {
            12
        };
        a[off] = 1;
        put128(a, off + 1, 5);
        put128(a, off + 17, 10);
        put128(a, off + 33, 123);
        put128(a, off + 49, 456);
        let p = f.decode().unwrap();
        let t = &p.orca_whirlpool.as_ref().unwrap().tick_arrays[1].ticks[0];
        assert_eq!(t.tick, 0);
        assert_eq!(t.liquidity_net, "5");
        assert_eq!(t.fee_growth_outside0.to_string(), "123");
        assert_eq!(t.fee_growth_outside1.to_string(), "456");
        let a = &mut f.accounts.get_mut(&f.arrays[1]).unwrap().data;
        put128(a, off + 1, 11);
        assert!(f.decode().is_err());
        let a = &mut f.accounts.get_mut(&f.arrays[1]).unwrap().data;
        put128(a, off + 1, 5);
        a[off + 65] = 1;
        assert!(f.decode().is_err());
    }
}
#[test]
fn malformed_extreme_tick_is_rejected_without_panicking() {
    for tick in [i32::MIN, i32::MAX, -443638, 443637] {
        let mut f = Fixture::new(false);
        f.accounts.get_mut(&f.pool.id).unwrap().data[81..85].copy_from_slice(&tick.to_le_bytes());
        assert!(f.decode().is_err());
    }
}
#[test]
fn native_mint_requires_classic_metadata_native_flag_and_lamport_backing() {
    for dynamic in [false, true] {
        let mut f = Fixture::with_native(dynamic, true);
        let p = f.decode().unwrap();
        assert_eq!(p.orca_whirlpool.unwrap().mint1_decimals, 9);
        let v = f.accounts.get_mut(&f.pool.accounts[2]).unwrap();
        v.lamports = 10_000_999;
        assert!(f.decode().is_err());
        let v = f.accounts.get_mut(&f.pool.accounts[2]).unwrap();
        v.lamports = 10_001_000;
        assert!(f.decode().is_ok());
        let v = f.accounts.get_mut(&f.pool.accounts[2]).unwrap();
        v.data[109] = 0;
        assert!(f.decode().is_err());
        let v = f.accounts.get_mut(&f.pool.accounts[2]).unwrap();
        v.data[109] = 1;
        f.accounts.get_mut(&f.pool.accounts[5]).unwrap().data[44] = 6;
        assert!(f.decode().is_err());
    }
}
#[test]
fn code_proof_options_fee_ranges_and_stale_none_payloads() {
    let mut f = Fixture::new(false);
    for off in [76, 113, 133] {
        let v = f.accounts.get_mut(&f.pool.accounts[1]).unwrap();
        v.data[off] = 7;
    }
    for off in [4, 50] {
        f.accounts.get_mut(&f.pool.accounts[4]).unwrap().data[off] = 7;
    }
    assert!(
        f.decode().is_ok(),
        "None payloads may be stale after SPL pack"
    );
    for (off, value) in [(45, 60001u16), (47, 2501u16)] {
        let mut f = Fixture::new(false);
        f.accounts.get_mut(&f.pool.id).unwrap().data[off..off + 2]
            .copy_from_slice(&value.to_le_bytes());
        assert!(f.decode().is_err());
    }
    let mut f = Fixture::new(false);
    f.accounts.get_mut(PD).unwrap().data[12] = 2;
    assert!(f.decode().is_err());
    let mut f = Fixture::new(false);
    f.accounts.get_mut(PD).unwrap().data.truncate(45);
    assert!(f.decode().is_err());
    let mut f = Fixture::new(false);
    let cfg = f.pool.orca_whirlpool.as_mut().unwrap();
    cfg.expected_program_code_sha256 = cfg.expected_program_code_sha256.to_uppercase();
    assert!(f.decode().is_err());
}
#[test]
fn all_tick_positions_and_dynamic_full_length_are_decoded() {
    for dynamic in [false, true] {
        let mut f = Fixture::new(dynamic);
        let a = f.accounts.get_mut(&f.arrays[1]).unwrap();
        if dynamic {
            a.data.resize(10004, 0);
            put128(&mut a.data, 44, (1u128 << 88) - 1);
        }
        for i in 0..88 {
            let off = if dynamic { 60 } else { 12 } + i * 113;
            a.data[off] = 1;
            put128(&mut a.data, off + 17, 2);
            put128(&mut a.data, off + 1, 0);
        }
        let p = f.decode().unwrap();
        let s = p.orca_whirlpool.unwrap();
        assert_eq!(s.tick_arrays[1].ticks.len(), 88);
        assert_eq!(s.tick_arrays[1].ticks[87].tick, 5568);
    }
}
#[test]
fn boundary_arrays_seven_arrays_and_left_cross_tick_price_relationship() {
    for dynamic in [false, true] {
        for (tick, starts) in [
            (-443636i32, vec![-444928, -439296, -433664]),
            (443636, vec![428032, 433664, 439296]),
            (-1, vec![-5632, 0, 5632]),
            (0, vec![-16896, -11264, -5632, 0, 5632, 11264, 16896]),
        ] {
            let mut f = Fixture::new(dynamic);
            for a in &f.arrays {
                f.accounts.remove(a);
            }
            f.pool.orca_whirlpool.as_mut().unwrap().tick_array_starts = starts.clone();
            for start in starts {
                let id = pda(&[
                    b"tick_array",
                    &bytes(&f.pool.id),
                    start.to_string().as_bytes(),
                ])
                .0;
                let mut a = vec![0; if dynamic { 148 } else { 9988 }];
                a[..8].copy_from_slice(&disc(if dynamic {
                    "DynamicTickArray"
                } else {
                    "TickArray"
                }));
                a[8..12].copy_from_slice(&start.to_le_bytes());
                put_key(&mut a, if dynamic { 12 } else { 9956 }, &f.pool.id);
                f.accounts.insert(id, account(PROGRAM, a, false));
            }
            let d = &mut f.accounts.get_mut(&f.pool.id).unwrap().data;
            d[81..85].copy_from_slice(&tick.to_le_bytes());
            let price = if tick == -1 {
                1u128 << 64
            } else {
                xone_domain::orca_math::sqrt_price_from_tick(tick)
                    .unwrap()
                    .to_string()
                    .parse()
                    .unwrap()
            };
            put128(d, 65, price);
            let p = f.decode().unwrap();
            assert_eq!(p.orca_whirlpool.unwrap().tick_current_index, tick);
        }
    }
}
#[test]
fn oracle_json_must_be_exact_system_empty_and_other_nulls_never_virtualize() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;
    let f = Fixture::new(false);
    let valid = json!({"owner":SYSTEM,"data":["","base64"],"executable":false,"lamports":0});
    assert!(orca::decode_rpc_account(&f.oracle, &f.oracle, &valid).is_ok());
    for bad in [
        json!({"owner":PROGRAM,"data":["","base64"],"executable":false,"lamports":0}),
        json!({"owner":SYSTEM,"data":[STANDARD.encode([0]),"base64"],"executable":false,"lamports":0}),
        json!({"owner":SYSTEM,"data":["","base64"],"executable":true,"lamports":0}),
        json!({"owner":SYSTEM,"data":["","base58"],"executable":false,"lamports":0}),
        json!({"owner":SYSTEM,"data":["","base64"],"executable":false}),
        json!({"owner":SYSTEM,"data":["","base64"],"lamports":0}),
    ] {
        assert!(orca::decode_rpc_account(&f.oracle, &f.oracle, &bad).is_err());
    }
    for id in f.accounts.keys().filter(|id| **id != f.oracle) {
        assert!(orca::decode_rpc_account(id, &f.oracle, &serde_json::Value::Null).is_err());
    }
}
