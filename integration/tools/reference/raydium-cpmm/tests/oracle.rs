use raydium_cpmm_reference::{base_pool, core_boundaries, generate, quote};
use serde_json::Value;

#[test]
fn retains_lp_fee_and_all_ledger_columns() {
    let p = base_pool();
    let q = quote(&p, "100000", true).unwrap();
    let old = &p.raydium_cpmm;
    let next = &q.pool.raydium_cpmm;
    assert_eq!(next.raw_vault0, old.raw_vault0 + 100000);
    assert_eq!(next.raw_vault1, old.raw_vault1 - q.amount_out);
    assert_eq!(
        next.protocol_fees0,
        old.protocol_fees0 + q.fees.protocol_fee
    );
    assert_eq!(next.fund_fees0, old.fund_fees0 + q.fees.fund_fee);
    assert_eq!(next.creator_fees0, old.creator_fees0 + q.fees.creator_fee);
    assert_eq!(next.recent_epoch, old.bank_epoch);
    assert!(
        q.pool.reserve0.parse::<u128>().unwrap() > q.fees.curve_new_input.parse::<u128>().unwrap()
    );
    assert_eq!(q.pool.reserve1, q.fees.curve_new_output);
}

#[test]
fn disabled_creator_preserves_old_ledger_and_share_is_collect_only() {
    let mut p = base_pool();
    p.raydium_cpmm.creator_fee_enabled = false;
    let q = quote(&p, "100000", false).unwrap();
    assert_eq!(q.fees.creator_fee, 0);
    assert_eq!(
        q.pool.raydium_cpmm.creator_fees0,
        p.raydium_cpmm.creator_fees0
    );
    assert_eq!(
        q.pool.raydium_cpmm.creator_fees1,
        p.raydium_cpmm.creator_fees1
    );
    let a = quote(&p, "100000", true).unwrap();
    p.raydium_cpmm.creator_fee_share_rate = 1000000;
    let b = quote(&p, "100000", true).unwrap();
    assert_eq!(a.amount_out, b.amount_out);
    assert_eq!(a.fees, b.fees);
}

#[test]
fn rejects_dust_u64_transfer_overflow_and_zero_combined_rate() {
    let mut p = base_pool();
    assert!(quote(&p, "0", true).is_err());
    assert!(quote(&p, "1", true).is_err());
    assert!(quote(&p, "18446744073709551616", true).is_err());
    assert!(quote(&p, "18446744073709551615", true).is_err());
    p.fee_numerator = 0;
    p.raydium_cpmm.creator_fee_rate = 0;
    assert!(quote(&p, "100000", true).is_err());
    p.raydium_cpmm.creator_fee_on = 1;
    assert!(quote(&p, "100000", false).is_ok());
}

#[test]
fn all_modes_directions_have_original_rounding_and_positive_invariants() {
    for mode in 0..3 {
        for enabled in [true, false] {
            for dir in [true, false] {
                let mut p = base_pool();
                p.raydium_cpmm.creator_fee_on = mode;
                p.raydium_cpmm.creator_fee_enabled = enabled;
                let q = quote(&p, "1234567", dir).unwrap();
                assert_eq!(q.fees.creator_fee_on_input, mode == 0 || (mode == 1) == dir);
                let next = &q.pool.raydium_cpmm;
                assert_eq!(
                    next.raw_vault0 - next.protocol_fees0 - next.fund_fees0 - next.creator_fees0,
                    q.pool.reserve0.parse::<u64>().unwrap()
                );
                assert_eq!(
                    next.raw_vault1 - next.protocol_fees1 - next.fund_fees1 - next.creator_fees1,
                    q.pool.reserve1.parse::<u64>().unwrap()
                );
                assert!(
                    q.pool.reserve0.parse::<u128>().unwrap()
                        * q.pool.reserve1.parse::<u128>().unwrap()
                        >= p.reserve0.parse::<u128>().unwrap()
                            * p.reserve1.parse::<u128>().unwrap()
                );
            }
        }
    }
}

#[test]
fn generated_corpus_has_full_state_and_chained_sequences() {
    let v = generate();
    let cases = v["cases"].as_array().unwrap();
    assert!(cases.len() >= 200);
    assert!(cases.iter().any(|c| c["expected"]["reverted"] == true));
    assert!(cases
        .iter()
        .any(|c| c["expected"]["raydium_cpmm"]["raw_vault0"]
            .as_u64()
            .is_some_and(|n| n > (1 << 53))));
    for seq in v["sequences"].as_array().unwrap() {
        let mut p = seq["pool"].clone();
        for s in seq["steps"].as_array().unwrap() {
            assert_eq!(s["pool"], p);
            p["reserve0"] = s["expected"]["reserve0"].clone();
            p["reserve1"] = s["expected"]["reserve1"].clone();
            p["raydium_cpmm"] = s["expected"]["raydium_cpmm"].clone();
        }
    }
    for c in cases.iter().filter(|c| {
        c["name"]
            .as_str()
            .unwrap()
            .starts_with("zero-fees-output-branch-input10")
    }) {
        assert_eq!(c["expected"]["amount_out"], "9");
    }
    assert!(v["sequences"].as_array().unwrap().len() >= 8);
    assert!(core_boundaries()
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["expected"]["reverted"] == Value::Bool(true)));
}

#[test]
fn exact_pda_and_rounding_regressions() {
    let p = base_pool();
    assert_eq!(
        p.raydium_cpmm.authority,
        "GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL"
    );
    assert_eq!(p.raydium_cpmm.authority_bump, 253);
    let a = quote(&p, "100000", true).unwrap();
    assert_eq!(a.amount_out, 169489);
    assert_eq!(
        (
            a.fees.trade_fee,
            a.fees.creator_fee,
            a.fees.protocol_fee,
            a.fees.fund_fee
        ),
        (250, 50, 30, 10)
    );
    assert_eq!(a.pool.reserve0, "1000000099910");
    let mut output = p.clone();
    output.raydium_cpmm.creator_fee_on = 1;
    let b = quote(&output, "100000", false).unwrap();
    assert_eq!(b.amount_out, 58646);
    assert_eq!(b.fees.creator_fee, 30);
    assert_eq!(b.pool.reserve0, b.fees.curve_new_output);
    let before = serde_json::to_value(&p.raydium_cpmm).unwrap();
    let after = serde_json::to_value(&a.pool.raydium_cpmm).unwrap();
    for (key, value) in before.as_object().unwrap() {
        if ![
            "raw_vault0",
            "raw_vault1",
            "protocol_fees0",
            "fund_fees0",
            "creator_fees0",
            "recent_epoch",
        ]
        .contains(&key.as_str())
        {
            assert_eq!(&after[key], value, "unrelated poststate field {key}");
        }
    }
}

#[test]
fn broader_arithmetic_probe_above_public_config_bound_is_branch_specific() {
    let mut p = base_pool();
    p.fee_numerator = 600000;
    p.raydium_cpmm.creator_fee_rate = 600000;
    // Public create_amm_config (upstream src/lib.rs:106) and updates reject this
    // configured sum. These assertions probe only the broader helper arithmetic;
    // success below, including with creator disabled, does not imply admission.
    assert!(p.fee_numerator + p.raydium_cpmm.creator_fee_rate >= 1_000_000);
    assert!(quote(&p, "100000", true).is_err());
    p.raydium_cpmm.creator_fee_on = 1;
    assert!(quote(&p, "100000", false).is_ok());
    p.raydium_cpmm.creator_fee_enabled = false;
    assert!(quote(&p, "100000", true).is_ok());
}
