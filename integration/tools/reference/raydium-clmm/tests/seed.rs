use anchor_lang::prelude::Pubkey;
use base64::{engine::general_purpose::STANDARD, Engine};
use raydium_clmm::states::{PoolState, POOL_SEED};
use raydium_clmm_reference::*;
use std::str::FromStr;

#[test]
fn original_seed_le_identity_and_dependencies() {
    let vectors = seed_identities();
    for seed in [1, 255, 256, 65535] {
        let p = seed_pool(512, 0, seed);
        let s = &p.raydium_clmm;
        let mut native = PoolState::default();
        native.amm_config = Pubkey::from_str(&s.amm_config).unwrap();
        native.token_mint_0 = Pubkey::from_str(&p.token0).unwrap();
        native.token_mint_1 = Pubkey::from_str(&p.token1).unwrap();
        native.set_seed_index(seed as u16);
        native.bump = [s.pool_bump as u8];
        assert_eq!(native.seed_index, (seed as u16).to_le_bytes());
        assert_eq!(native.seeds()[4], &(seed as u16).to_le_bytes());
        assert_eq!(native.key().to_string(), p.pool_id);
        let wrong = Pubkey::find_program_address(
            &[
                POOL_SEED.as_bytes(),
                native.amm_config.as_ref(),
                native.token_mint_0.as_ref(),
                native.token_mint_1.as_ref(),
                &(seed as u16).to_be_bytes(),
            ],
            &raydium_clmm::id(),
        )
        .0;
        assert_eq!(wrong.to_string() == p.pool_id, seed == 65535);
        assert!(vectors["vectors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["seed_index"] == seed));
        let old = bitmap_pool(512, 0);
        assert_eq!(p.token0, old.token0);
        assert_eq!(s.amm_config, old.raydium_clmm.amm_config);
        for (a, b) in s.tick_arrays.iter().zip(&old.raydium_clmm.tick_arrays) {
            assert_ne!(a.address, b.address);
            assert_eq!(a.ticks, b.ticks);
        }
        assert_ne!(s.vault0, old.raydium_clmm.vault0);
        assert_ne!(s.vault1, old.raydium_clmm.vault1);
        assert_ne!(s.observation, old.raydium_clmm.observation);
        assert_ne!(s.bitmap_extension, old.raydium_clmm.bitmap_extension);
        for d in [false, true] {
            for amount in ["1000000", "2000000000"] {
                assert_eq!(
                    quote(&p, amount, d).unwrap().amount_out,
                    quote(&old, amount, d).unwrap().amount_out
                );
            }
        }
    }
}

#[test]
fn reject_width_legacy_be_and_dependency_mixing_without_mutation() {
    let probes = seed_error_probes();
    assert!(probes.as_array().unwrap().len() >= 14);
    for c in probes.as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let before = p.clone();
        let reason = quote(
            &p,
            c["input"].as_str().unwrap(),
            c["zero_for_one"].as_bool().unwrap(),
        )
        .err()
        .expect(c["name"].as_str().unwrap());
        assert!(
            reason.contains(c["reason_contains"].as_str().unwrap()),
            "{}: {reason}",
            c["name"]
        );
        assert_eq!(p, before);
    }
    assert!(serde_json::to_value(base_pool()).unwrap()["raydium_clmm"]
        .get("seed_index")
        .is_none());
    for seed in [1, 255, 256, 65535] {
        let p = seed_pool(512, 0, seed);
        assert_eq!(
            serde_json::from_value::<Pool>(serde_json::to_value(&p).unwrap()).unwrap(),
            p
        );
    }
}

#[test]
fn packed_seed_offset_and_same_seed_next_bank() {
    let f = seed_fixture();
    let next = &f["metadata"]["next_bank_fixture"];
    assert_eq!(f["pool_config"], next["pool_config"]);
    assert_eq!(f["rpc_request_accounts"], next["rpc_request_accounts"]);
    for fixture in std::iter::once(&f)
        .chain(std::iter::once(next))
        .chain(f["metadata"]["seed_fixtures"].as_array().unwrap())
    {
        let p: Pool = serde_json::from_value(fixture["pool"].clone()).unwrap();
        let raw = STANDARD
            .decode(
                fixture["rpc_accounts"][&p.pool_id]["data_base64"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            &raw[391..393],
            &(p.raydium_clmm.seed_index as u16).to_le_bytes()
        );
        assert_eq!(&raw[393..397], &[0; 4]);
        assert_eq!(raw[390], p.raydium_clmm.collect_fee_on as u8);
        assert_eq!(
            fixture["rpc_request_accounts"].as_array().unwrap().len(),
            19
        );
    }
    assert_eq!(f["pool"]["raydium_clmm"]["tick_current"], 30720);
    assert_eq!(next["pool"]["raydium_clmm"]["seed_index"], 1);
    assert_eq!(next["pool"]["raydium_clmm"]["collect_fee_on"], 1);
    let probe = &f["metadata"]["next_bank_scope_probe"];
    assert_eq!(probe["before_expected"]["amount_out"], "1377715832");
    assert_eq!(
        probe["after_expected"],
        serde_json::json!({"reverted":true})
    );
    assert!(probe["mode_only_expected"]["next_pool"].is_object());
    for key in ["base_vectors", "next_bank_vectors"] {
        assert!(f["metadata"][key]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["expected"]["next_pool"].is_object()));
    }
}

#[test]
fn same_mints_distinct_seed_pools_are_independent() {
    let a = seed_pool(512, 0, 1);
    let b = seed_pool(512, 0, 256);
    let before = b.clone();
    let q = quote(&a, "2000000000", false).unwrap();
    assert_ne!(q.pool, a);
    assert_eq!(b, before);
    assert_ne!(a.pool_id, b.pool_id);
    let r = quote(&b, "2000000000", false).unwrap();
    assert_eq!(q.amount_out, r.amount_out);
    assert_ne!(q.pool.pool_id, r.pool.pool_id);
}

#[test]
fn corpus_full_poststate_sequences_and_real_multistep_budget() {
    let c = generate_seed();
    assert!(c["cases"].as_array().unwrap().len() >= 160);
    assert!(c["sequences"].as_array().unwrap().len() >= 8);
    let bank = seed_next_bank_pool();
    for direction in [false, true] {
        for amount in ["1000000", "2000000000"] {
            let exact: Vec<_> = c["cases"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|case| {
                    case["name"]
                        .as_str()
                        .unwrap()
                        .starts_with("seed1-next-bank-exact-fixture-")
                        && case["pool"] == serde_json::to_value(&bank).unwrap()
                        && case["input"] == amount
                        && case["zero_for_one"] == direction
                })
                .collect();
            assert_eq!(exact.len(), 1, "next-bank fixture {amount} / {direction}");
            let q = quote(&bank, amount, direction).unwrap();
            assert_eq!(
                exact[0]["expected"]["next_pool"],
                serde_json::to_value(q.pool).unwrap()
            );
            assert_eq!(exact[0]["expected"]["amount_out"], q.amount_out.to_string());
        }
    }
    for seq in c["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        for s in seq["steps"].as_array().unwrap() {
            let q = quote(
                &p,
                s["input"].as_str().unwrap(),
                s["zero_for_one"].as_bool().unwrap(),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(&q.pool).unwrap(),
                s["expected"]["next_pool"]
            );
            p = q.pool;
        }
        assert_eq!(serde_json::to_value(p).unwrap(), seq["expected_final_pool"]);
    }
    for case in c["cases"].as_array().unwrap() {
        if !case["name"]
            .as_str()
            .unwrap()
            .contains("conservative-max-steps")
        {
            continue;
        }
        let mut p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        p.raydium_clmm.max_steps = 8192;
        let q = quote(
            &p,
            case["input"].as_str().unwrap(),
            case["zero_for_one"].as_bool().unwrap(),
        )
        .unwrap();
        assert!(
            q.pool
                .raydium_clmm
                .tick_arrays
                .iter()
                .filter(|a| a.recent_epoch == p.raydium_clmm.bank_epoch)
                .count()
                >= 2,
            "{}",
            case["name"]
        );
    }
    for seed in [1, 255, 256, 65535] {
        let mut p = seed_pool(512, 0, seed);
        let q = quote(&p, "30000000000", false).unwrap();
        assert!(q.pool.raydium_clmm.tick_current >= 30840);
        assert!(
            q.pool
                .raydium_clmm
                .tick_arrays
                .iter()
                .filter(|a| a.recent_epoch == p.raydium_clmm.bank_epoch)
                .count()
                >= 3
        );
        p.raydium_clmm.max_steps = 1;
        assert!(quote(&p, "30000000000", false)
            .err()
            .unwrap()
            .contains("max_steps"));
    }
}
