#[path = "../src/balancer_log_exp.rs"]
mod balancer_log_exp;
#[path = "../src/balancer_managed_fees.rs"]
mod balancer_managed_fees;
#[path = "../src/balancer_multi.rs"]
#[allow(dead_code)]
mod balancer_multi;
use balancer_managed_fees::{quote_exact_in, ManagedFeeState};
use num_bigint::BigUint;
use num_traits::{One, Zero};
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
fn state(p: u64, m: u64, same: bool) -> ManagedFeeState {
    ManagedFeeState {
        total_supply: n("2000000000000000000000000"),
        protocol_swap_fee: p.into(),
        management_swap_fee: m.into(),
        owner_balance: n("1999999999999999999000000"),
        protocol_collector_balance: if same {
            n("1999999999999999999000000")
        } else {
            BigUint::zero()
        },
        same_recipient: same,
    }
}
#[test]
fn nonzero_fees_mint_and_same_recipient_coalesces() {
    let b = vec![n("1000000000000000000000000"); 2];
    let w = vec![n("500000000000000000"); 2];
    let scales = vec![n("1000000000000000000"); 2];
    let s = state(300000000000000000, 100000000000000000, false);
    let q = quote_exact_in(
        &b,
        &w,
        &scales,
        &n("10000000000000000"),
        &n("1000000000000000000000"),
        0,
        1,
        &s,
    )
    .unwrap();
    assert!(q.protocol_bpt > BigUint::zero());
    assert!(q.management_bpt > BigUint::zero());
    assert_eq!(
        q.next_state.total_supply,
        &s.total_supply + &q.protocol_bpt + &q.management_bpt
    );
    let same = state(300000000000000000, 100000000000000000, true);
    let coalesced = quote_exact_in(
        &b,
        &w,
        &scales,
        &n("10000000000000000"),
        &n("1000000000000000000000"),
        0,
        1,
        &same,
    )
    .unwrap();
    assert_eq!(
        coalesced.next_state.owner_balance,
        &same.owner_balance + &q.protocol_bpt + &q.management_bpt
    );
    assert_eq!(
        coalesced.next_state.owner_balance,
        coalesced.next_state.protocol_collector_balance
    );
}
#[test]
fn bad_state_and_overflow_leave_inputs_untouched() {
    let b = vec![n("1000000000000000000000000"); 2];
    let w = vec![n("500000000000000000"); 2];
    let scales = vec![n("1000000000000000000"); 2];
    for s in [
        ManagedFeeState {
            owner_balance: BigUint::one(),
            ..state(1, 1, true)
        },
        ManagedFeeState {
            total_supply: (BigUint::one() << 256usize) - BigUint::one(),
            ..state(500000000000000000, 0, false)
        },
    ] {
        let saved = s.clone();
        assert!(quote_exact_in(
            &b,
            &w,
            &scales,
            &n("10000000000000000"),
            &n("1000000000000000000000"),
            0,
            1,
            &s
        )
        .is_err());
        assert_eq!(s, saved);
    }
}

fn amount(v: &serde_json::Value) -> BigUint {
    v.as_str().unwrap().parse().unwrap()
}
fn amounts(v: &serde_json::Value) -> Vec<BigUint> {
    v.as_array().unwrap().iter().map(amount).collect()
}
fn fee_state(v: &serde_json::Value) -> ManagedFeeState {
    ManagedFeeState {
        total_supply: amount(&v["total_supply"]),
        protocol_swap_fee: amount(&v["protocol_swap_fee"]),
        management_swap_fee: amount(&v["management_swap_fee"]),
        owner_balance: amount(&v["owner_balance"]),
        protocol_collector_balance: amount(&v["protocol_collector_balance"]),
        same_recipient: v["same_recipient"].as_bool().unwrap(),
    }
}
#[test]
fn original_initialized_managed_pool_transactions_match_math_and_mint_state() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/balancer-managed-fees.json");
    let data: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(path).expect("generate original ManagedPool fee fixtures"),
    )
    .unwrap();
    assert_eq!(
        data["reference"]["revision"],
        "e281ff40583c39bf1d46c322334b2b13936e0e33"
    );
    let (mut good, mut bad, mut minted, mut same, mut rounding) = (0, 0, 0, 0, 0);
    let mut counts = std::collections::BTreeSet::new();
    for (id, c) in data["cases"].as_array().unwrap().iter().enumerate() {
        let b = amounts(&c["balances"]);
        counts.insert(b.len());
        let s = fee_state(&c["state"]);
        let saved = s.clone();
        let q = quote_exact_in(
            &b,
            &amounts(&c["weights"]),
            &amounts(&c["scales"]),
            &amount(&c["fee"]),
            &amount(&c["input"]),
            c["index_in"].as_u64().unwrap() as usize,
            c["index_out"].as_u64().unwrap() as usize,
            &s,
        );
        assert_eq!(s, saved, "case {id} mutated inputs");
        let e = &c["expected"];
        if e["reverted"] == true {
            assert!(q.is_err(), "case {id}: {c}");
            bad += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("case {id}: {e}: {c}"));
        assert_eq!(q.amount_out, amount(&e["amount_out"]), "case {id} output");
        assert_eq!(
            q.next_balances,
            amounts(&e["next_balances"]),
            "case {id} raw Vault cash"
        );
        assert_eq!(
            q.next_state,
            fee_state(&e["next_state"]),
            "case {id} minted state"
        );
        assert_eq!(
            q.protocol_bpt,
            amount(&e["protocol_bpt"]),
            "case {id} protocol mint"
        );
        assert_eq!(
            q.management_bpt,
            amount(&e["management_bpt"]),
            "case {id} management mint"
        );
        if !q.protocol_bpt.is_zero() || !q.management_bpt.is_zero() {
            minted += 1;
        }
        if s.same_recipient {
            same += 1;
        }
        if c["internal"]["reconstructed_gross_differs"] == true
            || c["internal"]["pre_downscale_differs"] == true
        {
            rounding += 1;
        }
        assert!(e["transaction_hash"].as_str().unwrap().starts_with("0x"));
        good += 1;
    }
    assert_eq!(counts, [2, 3, 8, 20, 38].into_iter().collect());
    assert!(
        good >= 100 && bad >= 30 && minted >= 50 && same >= 10 && rounding >= 10,
        "{good}/{bad}/{minted}/{same}/{rounding}"
    );
    for seq in data["sequences"].as_array().unwrap() {
        let mut b = amounts(&seq["balances"]);
        let mut s = fee_state(&seq["state"]);
        for step in seq["steps"].as_array().unwrap() {
            let q = quote_exact_in(
                &b,
                &amounts(&seq["weights"]),
                &amounts(&seq["scales"]),
                &amount(&seq["fee"]),
                &amount(&step["input"]),
                step["index_in"].as_u64().unwrap() as usize,
                step["index_out"].as_u64().unwrap() as usize,
                &s,
            )
            .unwrap();
            assert_eq!(q.next_state, fee_state(&step["expected"]["next_state"]));
            assert_eq!(q.next_balances, amounts(&step["expected"]["next_balances"]));
            b = q.next_balances;
            s = q.next_state;
        }
    }
    assert!(data["guards"]
        .as_array()
        .unwrap()
        .iter()
        .any(|g| g["kind"] == "fee_mint_overflow_rollback"));
}

#[test]
fn native_zero_raw_output_still_mints_bpt_before_downscale() {
    let b = vec![n("1000000000000000000000000"), n("1000000")];
    let w = vec![n("500000000000000000"); 2];
    let scales = vec![
        n("1000000000000000000"),
        n("1000000000000000000000000000000000000"),
    ];
    let s = state(300000000000000000, 100000000000000000, false);
    let q = quote_exact_in(
        &b,
        &w,
        &scales,
        &n("10000000000000000"),
        &n("1000000000000"),
        0,
        1,
        &s,
    )
    .unwrap();
    assert!(q.amount_out.is_zero());
    assert!(q.protocol_bpt + q.management_bpt > BigUint::zero());
    assert_eq!(q.next_balances[1], b[1]);
    let zero = quote_exact_in(
        &b,
        &w,
        &scales,
        &n("10000000000000000"),
        &BigUint::zero(),
        0,
        1,
        &s,
    )
    .unwrap();
    assert!(zero.amount_out.is_zero());
    assert_eq!(zero.next_state, s);
}

#[test]
fn native_fee_caps_and_all_standard_decimal_scales() {
    for d in 0..=18 {
        let b = vec![BigUint::from(1_000_000u64) * BigUint::from(10u8).pow(d); 2];
        let scales = vec![n("1000000000000000000") * BigUint::from(10u8).pow(18 - d); 2];
        let w = vec![n("500000000000000000"); 2];
        for fee in [n("1000000000000"), n("800000000000000000")] {
            let q = quote_exact_in(
                &b,
                &w,
                &scales,
                &fee,
                &BigUint::one(),
                0,
                1,
                &state(500000000000000000, 1000000000000000000, false),
            );
            assert!(q.is_ok(), "decimals {d}: {q:?}");
        }
        for (fee, p, m) in [
            (n("999999999999"), 0, 0),
            (n("800000000000000001"), 0, 0),
            (n("1000000000000"), 500000000000000001, 0),
            (n("1000000000000"), 0, 1000000000000000001),
        ] {
            assert!(quote_exact_in(
                &b,
                &w,
                &scales,
                &fee,
                &BigUint::one(),
                0,
                1,
                &state(p, m, false)
            )
            .is_err());
        }
    }
}
#[test]
fn distinct_recipient_balances_cannot_exceed_total_supply_together() {
    let b = vec![n("1000000000000000000000000"); 2];
    let w = vec![n("500000000000000000"); 2];
    let scales = vec![n("1000000000000000000"); 2];
    let mut s = state(300_000_000_000_000_000, 100_000_000_000_000_000, false);
    s.protocol_collector_balance = &s.total_supply - &s.owner_balance + BigUint::one();
    let before = s.clone();
    let error = quote_exact_in(
        &b,
        &w,
        &scales,
        &n("1000000000000000"),
        &n("1000000000000000000000"),
        0,
        1,
        &s,
    )
    .unwrap_err();
    assert_eq!(error.0, "BALANCER_MANAGED_FEE_STATE");
    assert_eq!(s, before);
}
