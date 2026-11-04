use orca_whirlpools_reference::{base_pool, quote};
use serde_json::json;

#[test]
fn original_swap_preserves_schema_and_updates_full_state() {
    let p = base_pool();
    for direction in [true, false] {
        let q = quote(&p, "1000000000", direction).unwrap();
        assert!(q.amount_out > 0);
        assert_eq!(q.next_pool["reserve0"], "0");
        assert_eq!(q.next_pool["reserve1"], "0");
        assert_eq!(
            q.next_pool.as_object().unwrap().keys().collect::<Vec<_>>(),
            p.as_object().unwrap().keys().collect::<Vec<_>>()
        );
        assert_eq!(
            q.next_pool["orca_whirlpool"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            p["orca_whirlpool"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
        assert_ne!(
            q.next_pool["orca_whirlpool"]["sqrt_price_x64"],
            p["orca_whirlpool"]["sqrt_price_x64"]
        );
        assert_eq!(
            q.next_pool["orca_whirlpool"]["reward_last_updated_timestamp"],
            p["orca_whirlpool"]["bank_unix_timestamp"]
        );
        assert_eq!(q.audit["amount_consumed"], "1000000000");
    }
}

#[test]
fn invalid_fees_and_u64_inputs_revert() {
    let mut p = base_pool();
    assert!(quote(&p, "0", true).is_err());
    assert!(quote(&p, "18446744073709551616", true).is_err());
    p["fee_numerator"] = json!(65535);
    p["orca_whirlpool"]["fee_rate"] = json!(65535);
    assert!(quote(&p, "1000000", true).is_err());
    p["fee_numerator"] = json!(3000);
    p["orca_whirlpool"]["fee_rate"] = json!(3000);
    p["orca_whirlpool"]["protocol_fee_rate"] = json!(2501);
    assert!(quote(&p, "1000000", true).is_err());
}

#[test]
fn raw_token_account_overflow_is_atomic() {
    let mut p = base_pool();
    p["orca_whirlpool"]["raw_vault0"] = json!(u64::MAX);
    let saved = p.clone();
    assert!(quote(&p, "1000000", true)
        .unwrap_err()
        .contains("raw vault input overflow"));
    assert_eq!(p, saved);
}

#[test]
fn fixed_and_dynamic_accounts_use_original_loaders_with_equal_numeric_results() {
    let p = base_pool();
    let mut dynamic = p.clone();
    for a in dynamic["orca_whirlpool"]["tick_arrays"]
        .as_array_mut()
        .unwrap()
    {
        a["layout"] = json!("dynamic");
    }
    for direction in [true, false] {
        let fixed = quote(&p, "1000000000", direction).unwrap();
        let mut d = quote(&dynamic, "1000000000", direction).unwrap();
        for a in d.next_pool["orca_whirlpool"]["tick_arrays"]
            .as_array_mut()
            .unwrap()
        {
            a["layout"] = json!("fixed");
        }
        assert_eq!(fixed.next_pool, d.next_pool);
        assert_eq!(fixed.amount_out, d.amount_out);
    }
}

#[test]
fn original_owed_fee_update_wraps_at_u64() {
    let mut p = base_pool();
    p["orca_whirlpool"]["protocol_fee_rate"] = json!(2500);
    p["orca_whirlpool"]["protocol_fee_owed0"] = json!(u64::MAX - 1);
    let q = quote(&p, "1000000000", true).unwrap();
    assert!(
        q.next_pool["orca_whirlpool"]["protocol_fee_owed0"]
            .as_u64()
            .unwrap()
            < 1_000_000
    );
    assert!(q.audit["next_protocol_fee"].as_u64().unwrap() > 1);
}

#[test]
fn official_sdk_tick_array_pda_vector_uses_decimal_signed_start_seed() {
    use anchor_lang::prelude::Pubkey;
    use orca_whirlpools_reference::tick_array_address;
    use std::str::FromStr;
    // Exact vector from the unchanged official rust-sdk/client/src/pda/tick_array.rs.
    let pool = Pubkey::from_str("2kJmUjxWBwL2NGPBV2PiA5hWtmLCqcKY6reQgkrPtaeS").unwrap();
    assert_eq!(
        tick_array_address(&pool, 0).0.to_string(),
        "8PhPzk7n4wU98Z6XCbVtPai2LtXSxYnfjkmgWuoAU8Zy"
    );
}

#[test]
fn original_sequence_selection_is_visible_and_bounded_to_three() {
    let p = base_pool();
    let left = quote(&p, "1000000000", true).unwrap();
    let right = quote(&p, "1000000000", false).unwrap();
    assert_eq!(left.audit["selected_tick_array_starts"], json!([0, -5632]));
    assert_eq!(right.audit["selected_tick_array_starts"], json!([0, 5632]));
}

fn boundary_return_pool(layout: &str) -> serde_json::Value {
    use anchor_lang::prelude::Pubkey;
    use orca_whirlpools_reference::tick_array_address;
    use std::str::FromStr;
    let mut pool = base_pool();
    let pool_key = Pubkey::from_str(pool["pool_id"].as_str().unwrap()).unwrap();
    let s = &mut pool["orca_whirlpool"];
    s["tick_current_index"] = json!(1);
    s["sqrt_price_x64"] = json!(whirlpool::math::sqrt_price_from_tick_index(1).to_string());
    let mut arrays = s["tick_arrays"].as_array().unwrap()[1..].to_vec();
    arrays.push(json!({"address":tick_array_address(&pool_key,11264).0.to_string(),"start_tick_index":11264,"layout":layout,"initialized_bitmap":"0","ticks":[]}));
    for a in &mut arrays {
        a["layout"] = json!(layout);
    }
    s["tick_arrays"] = json!(arrays);
    pool
}

#[test]
fn original_boundary_return_accepts_shifted_window_without_unused_left_array() {
    use anchor_lang::prelude::Pubkey;
    use orca_whirlpools_reference::tick_array_address;
    use std::str::FromStr;
    for layout in ["fixed", "dynamic"] {
        let p = boundary_return_pool(layout);
        let first = quote(&p, "50146692", true).unwrap();
        assert_eq!(first.amount_out, 49998750);
        assert_eq!(first.next_pool["orca_whirlpool"]["tick_current_index"], -1);
        assert_eq!(
            first.next_pool["orca_whirlpool"]["sqrt_price_x64"],
            "18446744073709551616"
        );
        assert_eq!(first.audit["selected_tick_array_starts"], json!([0]));
        let returned = quote(&first.next_pool, "10000", false).unwrap_or_else(|e| {
            panic!("{layout} boundary return must be selected by original builder: {e}")
        });
        assert_eq!(returned.amount_out, 9969);
        assert_eq!(returned.audit["amount_consumed"], "10000");
        assert_eq!(
            returned.audit["selected_tick_array_starts"],
            json!([0, 5632, 11264])
        );
        let mut padded = first.next_pool.clone();
        let pool_key = Pubkey::from_str(padded["pool_id"].as_str().unwrap()).unwrap();
        padded["orca_whirlpool"]["tick_arrays"].as_array_mut().unwrap().insert(0,
            json!({"address":tick_array_address(&pool_key,-5632).0.to_string(),"start_tick_index":-5632,"layout":layout,"initialized_bitmap":"0","ticks":[]}));
        let mut padded_returned = quote(&padded, "10000", false).unwrap();
        assert_eq!(padded_returned.amount_out, returned.amount_out);
        assert_eq!(padded_returned.audit, returned.audit);
        padded_returned.next_pool["orca_whirlpool"]["tick_arrays"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        assert_eq!(
            padded_returned.next_pool, returned.next_pool,
            "unused proof must not change full poststate"
        );
    }
}

#[test]
fn boundary_return_keeps_window_proofs_and_directional_requirements() {
    let p = boundary_return_pool("fixed");
    let first = quote(&p, "50146692", true).unwrap();
    let bad_direction = quote(&first.next_pool, "10000", true).unwrap_err();
    assert!(
        bad_direction.starts_with("original builder:"),
        "directional proof rejection must come from original builder: {bad_direction}"
    );
    let mut missing = first.next_pool.clone();
    missing["orca_whirlpool"]["tick_arrays"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(quote(&missing, "10000", false)
        .unwrap_err()
        .contains("3..7 complete arrays"));
    let mut unordered = first.next_pool.clone();
    unordered["orca_whirlpool"]["tick_arrays"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(quote(&unordered, "10000", false)
        .unwrap_err()
        .contains("ordered and contiguous"));
    let mut bad_bitmap = first.next_pool.clone();
    bad_bitmap["orca_whirlpool"]["tick_arrays"][0]["initialized_bitmap"] = json!("0");
    assert!(quote(&bad_bitmap, "10000", false)
        .unwrap_err()
        .contains("bitmap and initialized ticks disagree"));
}
