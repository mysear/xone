use base64::{engine::general_purpose::STANDARD, Engine};
use raydium_clmm_reference::*;

#[test]
fn dynamic_native_updates_all_nine_fields_and_old_json_stays_unchanged() {
    assert!(serde_json::to_value(base_pool()).unwrap()["raydium_clmm"]
        .get("dynamic_fee")
        .is_none());
    for seed in [0, 1, 255, 256, 65535] {
        for mode in 0..=2 {
            let p = dynamic_pool(512, mode, seed, 1, DYNAMIC_TIME_ANCHOR);
            let q = quote(&p, "2000000000", false).unwrap();
            let fee = q.pool.raydium_clmm.dynamic_fee.as_ref().unwrap();
            assert_eq!(fee.last_update_timestamp, DYNAMIC_TIME_ANCHOR);
            assert!(fee.volatility_accumulator > 0);
            assert_eq!(
                serde_json::to_value(fee)
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .len(),
                9
            );
            assert_eq!(
                serde_json::from_value::<Pool>(serde_json::to_value(&q.pool).unwrap()).unwrap(),
                q.pool
            );
        }
    }
}

#[test]
fn dynamic_filter_decay_future_reference_uses_original_saturation() {
    let mut p = dynamic_pool(-512, 0, 0, 1, DYNAMIC_TIME_ANCHOR);
    for elapsed in [0, 9, 10, 11, 119, 120, 121] {
        let f = p.raydium_clmm.dynamic_fee.as_mut().unwrap();
        f.last_update_timestamp = DYNAMIC_TIME_ANCHOR - elapsed;
        f.volatility_reference = 12345;
        f.volatility_accumulator = 54321;
        let q = quote(&p, "1000000", true).unwrap();
        let actual = q.pool.raydium_clmm.dynamic_fee.unwrap();
        assert_eq!(
            actual.volatility_reference,
            if elapsed < 10 {
                12345
            } else if elapsed < 120 {
                27160
            } else {
                0
            }
        );
        assert_eq!(
            actual.last_update_timestamp,
            if elapsed < 10 {
                DYNAMIC_TIME_ANCHOR - elapsed
            } else {
                DYNAMIC_TIME_ANCHOR
            }
        );
    }
    p.raydium_clmm
        .dynamic_fee
        .as_mut()
        .unwrap()
        .last_update_timestamp = DYNAMIC_TIME_ANCHOR + 1;
    let q = quote(&p, "1000000", true).unwrap();
    assert_eq!(
        q.pool
            .raydium_clmm
            .dynamic_fee
            .unwrap()
            .last_update_timestamp,
        DYNAMIC_TIME_ANCHOR + 1
    );
}

#[test]
fn dynamic_spacing_steps_have_separate_admission_domain() {
    let p = dynamic_pool(512, 0, 1, 1, DYNAMIC_TIME_ANCHOR);
    assert!(quote(&p, "2000000000", false).is_ok());
    let mut low = p.clone();
    low.raydium_clmm.max_steps = 22;
    assert!(quote(&low, "2000000000", false)
        .err()
        .unwrap()
        .contains("dynamic spacing admission"));
    assert_eq!(p.raydium_clmm.max_steps, 8192);
}

#[test]
fn dynamic_profile_rejects_old_profile_contamination_missing_fields_and_invalid_params() {
    let p = dynamic_pool(512, 0, 1, 1, DYNAMIC_TIME_ANCHOR);
    let mut bad = p.clone();
    bad.raydium_clmm.dynamic_fee = None;
    assert!(quote(&bad, "1000000", true).is_err());
    bad = p.clone();
    bad.protocol = SEED_PROFILE.into();
    bad.adapter_version = SEED_ADAPTER.into();
    assert!(quote(&bad, "1000000", true).is_err());
    bad = p.clone();
    bad.raydium_clmm.bitmap_extension = None;
    assert!(quote(&bad, "1000000", true).is_err());
    for which in 0..8 {
        bad = p.clone();
        let f = bad.raydium_clmm.dynamic_fee.as_mut().unwrap();
        match which {
            0 => f.filter_period = 65536,
            1 => f.decay_period = f.filter_period,
            2 => f.reduction_factor = 10000,
            3 => f.dynamic_fee_control = 100000,
            4 => f.volatility_reference = f.max_volatility_accumulator + 1,
            5 => f.volatility_accumulator = f.max_volatility_accumulator + 1,
            6 => f.last_update_timestamp = u32::MAX as u64 + 1,
            _ => f.tick_spacing_index_reference = i32::MIN,
        }
        assert!(quote(&bad, "1000000", true).is_err(), "invalid {which}");
    }
}

#[test]
fn dynamic_raw_offset_next_bank_fixed_identity_actual_parameter_change() {
    let f = dynamic_fixture(DYNAMIC_TIME_ANCHOR);
    let next = &f["metadata"]["next_bank_fixture"];
    assert_eq!(f["pool_config"], next["pool_config"]);
    assert_eq!(f["rpc_request_accounts"], next["rpc_request_accounts"]);
    for fixture in [&f, next] {
        let pool = fixture["pool"]["pool_id"].as_str().unwrap();
        let data = STANDARD
            .decode(
                fixture["rpc_accounts"][pool]["data_base64"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(data.len(), 1544);
        assert_eq!(u16::from_le_bytes(data[1096..1098].try_into().unwrap()), 10);
        assert!(data[1130..1544].iter().all(|x| *x == 0));
        assert_eq!(
            fixture["rpc_request_accounts"].as_array().unwrap().len(),
            19
        );
    }
    let a = f["metadata"]["base_vectors"].as_array().unwrap();
    let b = f["metadata"]["next_bank_vectors"].as_array().unwrap();
    assert_eq!(a.len(), 4);
    assert_eq!(b.len(), 4);
    for (a, b) in a.iter().zip(b).filter(|(a, _)| a["input"] == "2000000000") {
        assert_ne!(a["expected"]["amount_out"], b["expected"]["amount_out"]);
    }
}

#[test]
fn dynamic_corpus_full_pool_sequences_and_budget_separation() {
    let c = generate_dynamic(DYNAMIC_TIME_ANCHOR);
    assert!(c["cases"].as_array().unwrap().len() >= 300);
    assert!(c["sequences"].as_array().unwrap().len() >= 8);
    assert!(c["admission_probes"].as_array().unwrap().len() >= 2);
    for s in c["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
        for step in s["steps"].as_array().unwrap() {
            p = quote(
                &p,
                step["input"].as_str().unwrap(),
                step["zero_for_one"].as_bool().unwrap(),
            )
            .unwrap()
            .pool;
            assert_eq!(
                serde_json::to_value(&p).unwrap(),
                step["expected"]["next_pool"]
            );
        }
        assert_eq!(serde_json::to_value(p).unwrap(), s["expected_final_pool"]);
    }
}

#[test]
fn both_raw_banks_amounts_invariant_after_decay_with_current_clock() {
    for probe in dynamic_time_probes(DYNAMIC_TIME_ANCHOR).as_array().unwrap() {
        assert_eq!(
            probe["expected"]["amount_out"],
            probe["later_expected"]["amount_out"]
        );
        let mut before = probe["expected"]["next_pool"].clone();
        let after = &probe["later_expected"]["next_pool"];
        assert_ne!(
            before["raydium_clmm"]["dynamic_fee"]["last_update_timestamp"],
            after["raydium_clmm"]["dynamic_fee"]["last_update_timestamp"]
        );
        for field in ["bank_unix_timestamp", "observations"] {
            before["raydium_clmm"][field] = after["raydium_clmm"][field].clone();
        }
        before["raydium_clmm"]["dynamic_fee"]["last_update_timestamp"] =
            after["raydium_clmm"]["dynamic_fee"]["last_update_timestamp"].clone();
        assert_eq!(&before, after);
    }
}
