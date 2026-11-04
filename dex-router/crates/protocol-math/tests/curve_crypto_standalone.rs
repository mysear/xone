use num_bigint::BigUint;
use xone_protocol_math::curve_crypto;
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
#[test]
fn gamma_newton_invariant_and_y() {
    let xp = vec![n("1000000000000000000000000"); 2];
    let a = n("400000");
    let g = n("145000000000000");
    let d = curve_crypto::newton_d(&a, &g, &xp).unwrap();
    assert_eq!(d, n("2000000000000000000000000"));
    let y = curve_crypto::newton_y(&a, &g, &xp, &d, 1).unwrap();
    let distance = if y > xp[1] { &y - &xp[1] } else { &xp[1] - &y };
    assert!(distance <= n("10000000000"));
}
#[test]
fn original_halfpow_integer_steps() {
    assert_eq!(
        curve_crypto::halfpow(&n("0")).unwrap(),
        n("1000000000000000000")
    );
    assert_eq!(
        curve_crypto::halfpow(&n("1000000000000000000")).unwrap(),
        n("500000000000000000")
    );
}
fn parameters() -> curve_crypto::CryptoParams {
    curve_crypto::CryptoParams {
        amp: n("400000"),
        gamma: n("145000000000000"),
        mid_fee: n("3000000"),
        out_fee: n("30000000"),
        fee_gamma: n("230000000000000"),
        allowed_extra_profit: n("1000000000000000000"),
        adjustment_step: n("146000000000000"),
        admin_fee: n("5000000000"),
        ma_half_time: n("600"),
    }
}
fn state() -> curve_crypto::CryptoState {
    curve_crypto::CryptoState {
        invariant: n("2000000000000000000000000"),
        price_scale: n("1000000000000000000"),
        price_oracle: n("1000000000000000000"),
        last_prices: n("1000000000000000000"),
        last_prices_timestamp: 100,
        xcp_profit: n("1000000000000000000"),
        xcp_profit_a: n("1000000000000000000"),
        virtual_price: n("1000000000000000000"),
        not_adjusted: false,
        future_a_gamma_time: 0,
    }
}
#[test]
fn crypto_exchange_updates_invariant_oracle_profit() {
    let b = vec![n("1000000000000000000000000"); 2];
    let r = vec![n("1"); 2];
    let q = curve_crypto::quote_exact_in(
        &b,
        &r,
        &parameters(),
        &state(),
        &n("1000000000000000000000000"),
        161,
        &n("100000000000000000000"),
        0,
        1,
    )
    .unwrap();
    assert!(q.amount_out > n("99000000000000000000"));
    assert_eq!(q.next_state.last_prices_timestamp, 161);
    assert!(q.next_state.invariant > state().invariant);
}
#[test]
fn original_crypto_get_dy_exchange_oracle_and_profit_288_cases() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-quotes.json"
    ))
    .unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 288);
    let mut executed = 0;
    let mut views = 0;
    for (k, c) in cases.iter().enumerate() {
        let b: Vec<BigUint> = c["balances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        let r: Vec<BigUint> = c["precisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        let pval = |s: &str| n(c["params"][s].as_str().unwrap());
        let p = curve_crypto::CryptoParams {
            amp: pval("amp"),
            gamma: pval("gamma"),
            mid_fee: pval("mid_fee"),
            out_fee: pval("out_fee"),
            fee_gamma: pval("fee_gamma"),
            allowed_extra_profit: pval("allowed_extra_profit"),
            adjustment_step: pval("adjustment_step"),
            admin_fee: pval("admin_fee"),
            ma_half_time: pval("ma_half_time"),
        };
        let sval = |s: &str| n(c["state"][s].as_str().unwrap());
        let snum = |s: &str| c["state"][s].as_str().unwrap().parse::<u64>().unwrap();
        let s = curve_crypto::CryptoState {
            invariant: sval("D"),
            price_scale: sval("price_scale"),
            price_oracle: sval("_price_oracle"),
            last_prices: sval("last_prices"),
            last_prices_timestamp: snum("last_prices_timestamp"),
            xcp_profit: sval("xcp_profit"),
            xcp_profit_a: sval("xcp_profit_a"),
            virtual_price: sval("virtual_price"),
            not_adjusted: snum("not_adjusted") != 0,
            future_a_gamma_time: snum("future_A_gamma_time"),
        };
        let (i, j) = (
            c["index_in"].as_u64().unwrap() as usize,
            c["index_out"].as_u64().unwrap() as usize,
        );
        let dx = n(c["input"].as_str().unwrap());
        let view = curve_crypto::quote_get_dy(&b, &r, &p, &s, &dx, i, j);
        if c["get_dy_reverted"] == true {
            assert!(view.is_err(), "view revert {k}");
        } else {
            assert_eq!(
                view.unwrap_or_else(|e| panic!("view {k}: {e:?}"))
                    .amount_out,
                n(c["get_dy"].as_str().unwrap()),
                "view {k}"
            );
            views += 1;
        }
        let q = curve_crypto::quote_exact_in(
            &b,
            &r,
            &p,
            &s,
            &n(c["total_supply"].as_str().unwrap()),
            c["timestamp"].as_u64().unwrap(),
            &dx,
            i,
            j,
        );
        if c["expected"]["reverted"] == true {
            assert!(q.is_err(), "exchange revert {k}");
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("exchange {k}: {e:?}"));
        executed += 1;
        assert_eq!(
            q.amount_out,
            n(c["expected"]["amount_out"].as_str().unwrap()),
            "output {k}"
        );
        let want: Vec<BigUint> = c["expected"]["next_balances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| n(x.as_str().unwrap()))
            .collect();
        assert_eq!(q.next_balances, want, "balances {k}");
        let e = &c["expected"]["state"];
        let ev = |key: &str| n(e[key].as_str().unwrap());
        for (key, v) in [
            ("D", &q.next_state.invariant),
            ("price_scale", &q.next_state.price_scale),
            ("_price_oracle", &q.next_state.price_oracle),
            ("last_prices", &q.next_state.last_prices),
            ("xcp_profit", &q.next_state.xcp_profit),
            ("xcp_profit_a", &q.next_state.xcp_profit_a),
            ("virtual_price", &q.next_state.virtual_price),
        ] {
            assert_eq!(v, &ev(key), "poststate {key} #{k}");
        }
        assert_eq!(
            q.next_state.last_prices_timestamp,
            e["last_prices_timestamp"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            "time {k}"
        );
        assert_eq!(
            q.next_state.future_a_gamma_time,
            e["future_A_gamma_time"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            "ramp {k}"
        );
        assert_eq!(
            q.next_state.not_adjusted,
            e["not_adjusted"] != "0",
            "flag {k}"
        );
    }
    assert_eq!(executed, 162);
    assert_eq!(views, 162);
}
#[test]
fn crypto_adjustment_and_admin_claim_are_explicit_boundaries() {
    let b = vec![n("1000000000000000000000000"); 2];
    let r = vec![n("1"); 2];
    let mut s = state();
    s.not_adjusted = true;
    let q = curve_crypto::quote_exact_in(
        &b,
        &r,
        &parameters(),
        &s,
        &n("1000000000000000000000000"),
        100,
        &n("100000000000000000000"),
        0,
        1,
    );
    assert_eq!(
        q.unwrap_err(),
        xone_domain::err("CURVE_CRYPTO_BOUNDARY_ADMIN_FEE_CLAIM_AND_GULP")
    );
    s.price_oracle = n("1100000000000000000");
    let q = curve_crypto::quote_exact_in(
        &b,
        &r,
        &parameters(),
        &s,
        &n("1000000000000000000000000"),
        100,
        &n("100000000000000000000"),
        0,
        1,
    );
    assert_eq!(
        q.unwrap_err(),
        xone_domain::err("CURVE_CRYPTO_BOUNDARY_PRICE_SCALE_ADJUSTMENT")
    );
}
#[test]
fn crypto_rejects_unsafe_a_gamma_and_d() {
    let xp = vec![n("1000000000000000000000000"); 2];
    assert!(curve_crypto::newton_d(&n("3999"), &n("145000000000000"), &xp).is_err());
    assert!(curve_crypto::newton_d(&n("400000"), &n("9999999999"), &xp).is_err());
    assert!(curve_crypto::newton_y(&n("400000"), &n("145000000000000"), &xp, &n("1"), 1).is_err());
}
