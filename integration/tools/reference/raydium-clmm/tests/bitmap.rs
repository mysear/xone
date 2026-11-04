use base64::{engine::general_purpose::STANDARD, Engine};
use raydium_clmm_reference::{
    base_pool, bitmap_fixture, bitmap_pool, quote, BITMAP_ADAPTER, BITMAP_PROFILE,
};

#[test]
fn bitmap_profile_requires_extension_and_old_profiles_reject_it() {
    let original = base_pool();
    assert!(serde_json::to_value(&original).unwrap()["raydium_clmm"]
        .get("bitmap_extension")
        .is_none());
    let p = bitmap_pool(512, 0);
    assert_eq!(p.protocol, BITMAP_PROFILE);
    assert_eq!(p.adapter_version, BITMAP_ADAPTER);
    let ext = p.raydium_clmm.bitmap_extension.clone().unwrap();
    for mode in 0..=2 {
        let mut p = bitmap_pool(512, mode);
        for d in [false, true] {
            for amount in ["1000000", "2000000000"] {
                let q = quote(&p, amount, d).unwrap();
                assert!(q.amount_out > 0);
                assert_eq!(
                    q.pool.raydium_clmm.bitmap_extension,
                    p.raydium_clmm.bitmap_extension
                );
            }
        }
        p.raydium_clmm.bitmap_extension = None;
        assert!(quote(&p, "1000000", true).is_err());
    }
    let mut old = original;
    old.raydium_clmm.bitmap_extension = Some(ext);
    assert!(quote(&old, "1000000", true).is_err());
}

#[test]
fn bitmap_fixture_contains_native_1832_byte_pool_linked_account() {
    let f = bitmap_fixture();
    let ext = &f["pool"]["raydium_clmm"]["bitmap_extension"];
    let address = ext["address"].as_str().unwrap();
    let raw = STANDARD
        .decode(f["rpc_accounts"][address]["data_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(raw.len(), 1832);
    assert_eq!(ext["positive"].as_array().unwrap().len(), 112);
    assert_eq!(ext["negative"].as_array().unwrap().len(), 112);
    assert!(f["rpc_request_accounts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == address));
    assert!(f["metadata"]["alternate_negative_base"]["rpc_accounts"].is_object());
    assert_eq!(
        raw,
        STANDARD
            .decode(
                f["rpc_response"]["result"]["value"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap()["data_base64"]
                    .as_str()
                    .unwrap()
            )
            .unwrap()
    );
}

#[test]
fn source_flip_vectors_cover_all_groups_and_negative_integral_boundaries() {
    use raydium_clmm::states::TickArrayBitmapExtension;
    use raydium_clmm_reference::bitmap_vectors;
    let v = bitmap_vectors();
    let vectors = v["vectors"].as_array().unwrap();
    assert!((60..=100).contains(&vectors.len()));
    for index in [-7394, -1025, -1024, -513, 512, 1023, 1024, 7393] {
        assert!(vectors.iter().any(|v| v["index"] == index));
    }
    for side in ["positive", "negative"] {
        for group in 0..14 {
            assert!(vectors
                .iter()
                .any(|v| v[side].as_array().unwrap()[group * 8..group * 8 + 8]
                    .iter()
                    .any(|w| w.as_u64().unwrap() != 0)));
        }
    }
    for v in vectors {
        let mut e = TickArrayBitmapExtension::default();
        let start = v["start_tick_index"].as_i64().unwrap() as i32;
        e.flip_tick_array_bit(start, 1).unwrap();
        assert!(e.check_tick_array_is_initialized(start, 1).unwrap().0);
        let positive = { e.positive_tick_array_bitmap };
        let negative = { e.negative_tick_array_bitmap };
        assert_eq!(
            serde_json::to_value(positive.into_iter().flatten().collect::<Vec<_>>()).unwrap(),
            v["positive"]
        );
        assert_eq!(
            serde_json::to_value(negative.into_iter().flatten().collect::<Vec<_>>()).unwrap(),
            v["negative"]
        );
        e.flip_tick_array_bit(start, 1).unwrap();
        assert!(!e.check_tick_array_is_initialized(start, 1).unwrap().0);
    }
    assert!(v["invalid_source_starts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["rejected"] == true));
}

#[test]
fn next_bank_fixture_changes_real_extension_and_matching_array_bytes() {
    let f = bitmap_fixture();
    let next = &f["metadata"]["next_bank_fixture"];
    assert_eq!(f["pool_config"], next["pool_config"]);
    assert_eq!(f["rpc_request_accounts"], next["rpc_request_accounts"]);
    let a: raydium_clmm_reference::Pool = serde_json::from_value(f["pool"].clone()).unwrap();
    let b: raydium_clmm_reference::Pool = serde_json::from_value(next["pool"].clone()).unwrap();
    assert_eq!(b.raydium_clmm.collect_fee_on, 1);
    assert_ne!(
        a.raydium_clmm.bitmap_extension,
        b.raydium_clmm.bitmap_extension
    );
    assert!(b.raydium_clmm.tick_arrays.last().unwrap().ticks.is_empty());
    let address = &a.raydium_clmm.bitmap_extension.as_ref().unwrap().address;
    assert_ne!(
        f["rpc_accounts"][address]["data_base64"],
        next["rpc_accounts"][address]["data_base64"]
    );
    let post = quote(&a, "1000000", true).unwrap().pool;
    assert_eq!(
        serde_json::to_value(post).unwrap(),
        f["metadata"]["alternate_fixture"]["pool"]
    );
    for vector in f["metadata"]["next_bank_vectors"].as_array().unwrap() {
        let q = quote(
            &b,
            vector["input"].as_str().unwrap(),
            vector["zero_for_one"].as_bool().unwrap(),
        )
        .unwrap();
        assert_eq!(
            q.pool.raydium_clmm.bitmap_extension,
            b.raydium_clmm.bitmap_extension
        );
        assert_eq!(q.amount_out.to_string(), vector["expected"]["amount_out"]);
    }
}

#[test]
fn scope_probe_needs_the_cleared_extension_array_and_fee_alone_still_succeeds() {
    let f = bitmap_fixture();
    let probe = &f["metadata"]["next_bank_scope_probe"];
    assert_eq!(probe["input"], "30000000000");
    assert_eq!(probe["zero_for_one"], false);
    assert_eq!(probe["before_expected"]["amount_out"], "1377715832");
    assert!(probe["before_expected"]["next_pool"].is_object());
    assert!(probe["mode_only_expected"]["next_pool"].is_object());
    assert_eq!(
        probe["after_expected"],
        serde_json::json!({"reverted":true})
    );
}

#[test]
fn bitmap_budget_probes_require_multiple_native_steps() {
    let corpus = raydium_clmm_reference::generate_bitmap();
    assert_eq!(corpus["reference"]["business_math_imported"], false);
    for mode in 0..=2 {
        let name = format!("mode{mode}-dirfalse-conservative-max-steps");
        let c = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        assert_eq!(c["input"], "30000000000");
        let mut p: raydium_clmm_reference::Pool =
            serde_json::from_value(c["pool"].clone()).unwrap();
        assert!(quote(&p, "30000000000", false).is_err());
        p.raydium_clmm.max_steps = 8192;
        let q = quote(&p, "30000000000", false).unwrap();
        // A movement through >=120 ticks crosses multiple initialized ticks/arrays.
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
    }
}
