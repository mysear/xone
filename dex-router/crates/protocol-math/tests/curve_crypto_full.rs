#[allow(dead_code)]
#[path = "../src/curve_crypto.rs"]
mod curve_crypto;
#[allow(dead_code)]
#[path = "../src/curve_multi.rs"]
mod curve_multi;
use curve_crypto::*;
use num_bigint::BigUint;
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
fn fixture() -> (
    Vec<BigUint>,
    Vec<BigUint>,
    CryptoParams,
    CryptoState,
    CryptoFullLedger,
) {
    let b = vec![n("1000000000000000000000000"); 2];
    let p = CryptoParams {
        amp: n("400000"),
        gamma: n("145000000000000"),
        mid_fee: n("3000000"),
        out_fee: n("30000000"),
        fee_gamma: n("230000000000000"),
        allowed_extra_profit: n("1000000000000000000"),
        adjustment_step: n("146000000000000"),
        admin_fee: n("5000000000"),
        ma_half_time: n("600"),
    };
    let s = CryptoState {
        invariant: n("2000000000000000000000000"),
        price_scale: n("1000000000000000000"),
        price_oracle: n("1000000000000000000"),
        last_prices: n("1000000000000000000"),
        last_prices_timestamp: 100,
        xcp_profit: n("1000000000000000000"),
        xcp_profit_a: n("1000000000000000000"),
        virtual_price: n("1000000000000000000"),
        not_adjusted: true,
        future_a_gamma_time: 0,
    };
    let l = CryptoFullLedger {
        total_supply: b[0].clone(),
        receiver_balance: n("0"),
        raw_balances: b.clone(),
        admin_receiver_enabled: true,
        ramp: None,
    };
    (b, vec![n("1"); 2], p, s, l)
}
#[test]
fn full_claim_replaces_legacy_boundary_and_gulps_donation() {
    let (b, r, p, s, mut l) = fixture();
    l.raw_balances[0] += n("1000000000000000000");
    let dx = n("100000000000000000000");
    assert!(quote_exact_in(&b, &r, &p, &s, &l.total_supply, 100, &dx, 0, 1).is_err());
    let q = quote_full_exact_in(&b, &r, &p, &s, &l, 100, &dx, 0, 1).unwrap();
    assert_eq!(q.next_balances, q.next_raw_balances);
    assert_eq!(q.next_balances[0], &l.raw_balances[0] + &dx);
    assert!(!q.next_pricing_state.not_adjusted);
    assert!(q.next_total_supply > l.total_supply);
    assert_eq!(q.next_receiver_balance, q.minted_admin);
}
#[test]
fn full_no_adjust_matches_legacy_and_leaves_donations_ungulped() {
    let (b, r, p, mut s, mut l) = fixture();
    s.not_adjusted = false;
    l.raw_balances[0] += n("123");
    let dx = n("100000000000000000000");
    let old = quote_exact_in(&b, &r, &p, &s, &l.total_supply, 161, &dx, 0, 1).unwrap();
    let q = quote_full_exact_in(&b, &r, &p, &s, &l, 161, &dx, 0, 1).unwrap();
    assert_eq!(q.amount_out, old.amount_out);
    assert_eq!(q.next_pricing_state, old.next_state);
    assert_eq!(&q.next_raw_balances[0] - &q.next_balances[0], n("123"));
    assert_eq!(q.minted_admin, n("0"));
}
fn state_from(v: &serde_json::Value) -> CryptoState {
    let s = |k: &str| n(v[k].as_str().unwrap());
    let t = |k: &str| v[k].as_str().unwrap().parse().unwrap();
    CryptoState {
        invariant: s("D"),
        price_scale: s("price_scale"),
        price_oracle: s("_price_oracle"),
        last_prices: s("last_prices"),
        last_prices_timestamp: t("last_prices_timestamp"),
        xcp_profit: s("xcp_profit"),
        xcp_profit_a: s("xcp_profit_a"),
        virtual_price: s("virtual_price"),
        not_adjusted: v["not_adjusted"] == "1",
        future_a_gamma_time: t("future_A_gamma_time"),
    }
}
fn vector(v: &serde_json::Value) -> Vec<BigUint> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| n(v.as_str().unwrap()))
        .collect()
}
#[test]
fn original_exchange_and_original_lp_full_poststates() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/contracts/curve-crypto-full.json");
    let data: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert!(cases.len() >= 40);
    let mut success = 0;
    let mut reverted = 0;
    let mut adjusted = 0;
    let mut minted = 0;
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let pv = |k: &str| n(c["params"][k].as_str().unwrap());
        let p = CryptoParams {
            amp: pv("amp"),
            gamma: pv("gamma"),
            mid_fee: pv("mid_fee"),
            out_fee: pv("out_fee"),
            fee_gamma: pv("fee_gamma"),
            allowed_extra_profit: pv("allowed_extra_profit"),
            adjustment_step: pv("adjustment_step"),
            admin_fee: pv("admin_fee"),
            ma_half_time: pv("ma_half_time"),
        };
        let s = state_from(&c["state"]);
        let r = vector(&c["precisions"]);
        let b = vector(&c["balances"]);
        let l = CryptoFullLedger {
            total_supply: n(c["total_supply"].as_str().unwrap()),
            receiver_balance: n(c["receiver_balance"].as_str().unwrap()),
            raw_balances: vector(&c["raw_balances"]),
            admin_receiver_enabled: c["admin_receiver_enabled"].as_bool().unwrap(),
            ramp: if c["ramp"].is_null() {
                None
            } else {
                Some(CryptoRamp {
                    initial_amp: n(c["ramp"]["initial_amp"].as_str().unwrap()),
                    initial_gamma: n(c["ramp"]["initial_gamma"].as_str().unwrap()),
                    initial_time: c["ramp"]["initial_time"].as_u64().unwrap(),
                })
            },
        };
        let before = (b.clone(), s.clone(), l.clone());
        let time = c["timestamp"].as_u64().unwrap();
        let result = if c["operation"] == "claim_admin_fees" {
            quote_full_claim_admin(&r, &p, &s, &l, time)
        } else {
            quote_full_exact_in(
                &b,
                &r,
                &p,
                &s,
                &l,
                time,
                &n(c["input"].as_str().unwrap()),
                c["index_in"].as_u64().unwrap() as usize,
                c["index_out"].as_u64().unwrap() as usize,
            )
        };
        assert_eq!((b, s.clone(), l.clone()), before, "inputs mutated {name}");
        let e = &c["expected"];
        if e["reverted"] == true {
            assert!(result.is_err(), "must revert {name}: {result:?}");
            reverted += 1;
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(
            q.amount_out,
            n(e["amount_out"].as_str().unwrap()),
            "amount {name}"
        );
        assert_eq!(
            q.next_pricing_state,
            state_from(&e["state"]),
            "state {name}"
        );
        assert_eq!(q.next_balances, vector(&e["balances"]), "balances {name}");
        assert_eq!(
            q.next_raw_balances,
            vector(&e["raw_balances"]),
            "raw {name}"
        );
        assert_eq!(
            q.next_total_supply,
            n(e["total_supply"].as_str().unwrap()),
            "supply {name}"
        );
        assert_eq!(
            q.next_receiver_balance,
            n(e["receiver_balance"].as_str().unwrap()),
            "receiver {name}"
        );
        assert_eq!(
            q.minted_admin,
            n(e["minted_admin"].as_str().unwrap()),
            "mint {name}"
        );
        if q.next_pricing_state.price_scale != s.price_scale {
            adjusted += 1;
        }
        if q.minted_admin > n("0") {
            minted += 1;
        }
        success += 1;
    }
    assert!(success >= 25);
    assert!(reverted >= 6);
    assert!(adjusted >= 2);
    assert!(minted >= 6);
}
#[test]
fn full_ledger_rejections_leave_inputs_unchanged() {
    let (b, r, p, s, mut l) = fixture();
    l.raw_balances[1] = n("0");
    let copy = l.clone();
    assert!(
        quote_full_exact_in(&b, &r, &p, &s, &l, 100, &n("100000000000000000000"), 0, 1).is_err()
    );
    assert_eq!(l, copy);
    l.raw_balances = b.clone();
    l.receiver_balance = l.total_supply.clone() + n("1");
    assert!(
        quote_full_exact_in(&b, &r, &p, &s, &l, 100, &n("100000000000000000000"), 0, 1).is_err()
    );
}
#[test]
fn ramp_requires_origin_and_checks_timestamp_underflow() {
    let (b, r, p, mut s, mut l) = fixture();
    s.future_a_gamma_time = 200;
    assert!(
        quote_full_exact_in(&b, &r, &p, &s, &l, 150, &n("100000000000000000000"), 0, 1).is_err()
    );
    l.ramp = Some(CryptoRamp {
        initial_amp: p.amp.clone(),
        initial_gamma: p.gamma.clone(),
        initial_time: 160,
    });
    assert!(
        quote_full_exact_in(&b, &r, &p, &s, &l, 150, &n("100000000000000000000"), 0, 1).is_err()
    );
}
