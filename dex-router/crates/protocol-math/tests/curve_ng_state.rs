#[allow(dead_code)]
#[path = "../src/curve_multi.rs"]
mod curve_multi;
#[allow(dead_code)]
#[path = "../src/curve_ng.rs"]
mod curve_ng;
use curve_ng::{quote_full_exact_in, NgOracleState};
use num_bigint::{BigInt, BigUint};
use serde_json::Value;
fn n(s: &str) -> BigUint {
    s.parse().unwrap()
}
fn amount(v: &Value) -> BigUint {
    n(v.as_str().unwrap())
}
fn amounts(v: &Value) -> Vec<BigUint> {
    v.as_array().unwrap().iter().map(amount).collect()
}
fn state(v: &Value) -> NgOracleState {
    NgOracleState {
        stored_balances: amounts(&v["stored_balances"]),
        admin_balances: amounts(&v["admin_balances"]),
        last_prices_packed: amounts(&v["last_prices_packed"]),
        last_d_packed: amount(&v["last_d_packed"]),
        ma_last_time: amount(&v["ma_last_time"]),
        ma_exp_time: v["ma_exp_time"].as_str().unwrap().parse().unwrap(),
        d_ma_time: v["d_ma_time"].as_str().unwrap().parse().unwrap(),
        timestamp: v["timestamp"].as_u64().unwrap(),
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/curve-ng-state.json")).unwrap()
}
#[test]
fn original_full_exchange_every_step_including_ema_raw_and_admin() {
    let data = fixture();
    let mut successful = 0;
    let mut reverts = 0;
    let mut caps = 0;
    let mut advanced = 0;
    let mut zero_spot_preserved = 0;
    for scenario in data["scenarios"].as_array().unwrap() {
        let mut previous: Option<NgOracleState> = None;
        for (step, row) in scenario["steps"].as_array().unwrap().iter().enumerate() {
            let s = state(&row["state"]);
            if let Some(mut p) = previous.take() {
                p.timestamp = s.timestamp;
                assert_eq!(
                    p, s,
                    "sequential fixture continuity {} step {}",
                    scenario["name"], step
                );
            }
            let result = quote_full_exact_in(
                &amounts(&row["balances"]),
                &amount(&row["amp"]),
                &amounts(&row["rates"]),
                &amount(&row["fee"]),
                &amount(&row["admin_fee"]),
                &amount(&row["offpeg_fee_multiplier"]),
                &amount(&row["input"]),
                row["index_in"].as_u64().unwrap() as usize,
                row["index_out"].as_u64().unwrap() as usize,
                &s,
            );
            if row["expected"]["reverted"] == true {
                assert!(
                    result.is_err(),
                    "{} step {} expected revert",
                    scenario["name"],
                    step
                );
                reverts += 1;
                previous = Some(s);
                continue;
            }
            let q = result.unwrap_or_else(|e| panic!("{} step {}: {}", scenario["name"], step, e));
            assert_eq!(
                q.amount_out,
                amount(&row["expected"]["amount_out"]),
                "{} step {} output",
                scenario["name"],
                step
            );
            assert_eq!(
                q.next_balances,
                amounts(&row["expected"]["next_balances"]),
                "{} step {} balances",
                scenario["name"],
                step
            );
            assert_eq!(
                q.next_state,
                state(&row["expected"]["next_state"]),
                "{} step {} full state",
                scenario["name"],
                step
            );
            assert_eq!(
                q.invariant,
                amount(&row["original_invariant"]),
                "original internal get_D"
            );
            let j = row["index_out"].as_u64().unwrap() as usize;
            assert_eq!(
                q.admin_fee_amount,
                &q.next_state.admin_balances[j] - &s.admin_balances[j]
            );
            if scenario["name"] == "zero-spot" {
                assert_eq!(
                    q.next_state.last_prices_packed, s.last_prices_packed,
                    "zero spot must preserve both packed halves, even with advanced time"
                );
                zero_spot_preserved += 1;
            }
            let mask = (BigUint::from(1u8) << 128usize) - BigUint::from(1u8);
            caps += q
                .next_state
                .last_prices_packed
                .iter()
                .filter(|p| (*p & &mask) == n("2000000000000000000"))
                .count();
            advanced += usize::from(q.next_state.ma_last_time != s.ma_last_time);
            successful += 1;
            previous = Some(q.next_state);
        }
    }
    assert_eq!(successful, 144);
    assert_eq!(zero_spot_preserved, 8);
    assert_eq!(reverts, 3);
    assert!(caps > 0);
    assert!(advanced > 0);
}
#[test]
fn original_signed_exp_boundaries_and_wrapping_spot_intermediates() {
    let data = fixture();
    for row in data["exp_cases"].as_array().unwrap() {
        let x: BigInt = row["x"].as_str().unwrap().parse().unwrap();
        let got = curve_ng::wad_exp(&x);
        if row["reverted"] == true {
            assert!(got.is_err(), "exp {}", x);
        } else {
            assert_eq!(got.unwrap(), amount(&row["expected"]), "exp {}", x);
        }
    }
    for row in data["invariant_cases"].as_array().unwrap() {
        let got = curve_ng::invariant(&amounts(&row["xp"]), &amount(&row["amp"]));
        if row["reverted"] == true {
            assert!(got.is_err(), "get_D {}", row);
        } else {
            assert_eq!(got.unwrap(), amount(&row["expected"]), "get_D {}", row);
        }
    }
    for row in data["spot_cases"].as_array().unwrap() {
        let got = curve_ng::get_p(
            &amounts(&row["xp"]),
            &amount(&row["amp"]),
            &amount(&row["d"]),
        );
        if row["reverted"] == true {
            assert!(got.is_err(), "spot {}", row);
        } else {
            assert_eq!(got.unwrap(), amounts(&row["expected"]), "spot {}", row);
        }
    }
}
#[test]
fn full_state_rejects_inconsistent_shape_and_effective_raw_balance() {
    let data = fixture();
    let row = &data["scenarios"][0]["steps"][0];
    let call = |s: &NgOracleState| {
        quote_full_exact_in(
            &amounts(&row["balances"]),
            &amount(&row["amp"]),
            &amounts(&row["rates"]),
            &amount(&row["fee"]),
            &amount(&row["admin_fee"]),
            &amount(&row["offpeg_fee_multiplier"]),
            &amount(&row["input"]),
            0,
            1,
            s,
        )
    };
    let original = state(&row["state"]);
    let mut s = original.clone();
    s.last_prices_packed.clear();
    assert!(call(&s).is_err());
    s = original.clone();
    s.stored_balances[0] += n("1");
    assert!(call(&s).is_err());
    s = original.clone();
    s.admin_balances[0] = s.stored_balances[0].clone() + n("1");
    assert!(call(&s).is_err());
    s = original.clone();
    s.last_d_packed = BigUint::from(1u8) << 256usize;
    assert!(call(&s).is_err());
    assert_eq!(state(&row["state"]), original);
}
