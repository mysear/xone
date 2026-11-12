use serde_json::Value;
use xone_domain::{Amount, Pool};
#[test]
fn pump_original_classic_sell_and_exact_sol_input_preserve_curve_vault_and_fee_accounts() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-bonding.json")).unwrap();
    let (mut full, mut rejected) = (0, 0);
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] != "buy")
    {
        let mut previous: Option<Pool> = None;
        for s in c["steps"].as_array().unwrap() {
            let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
            p.validate()
                .unwrap_or_else(|e| panic!("case {} validate {e}", c["index"]));
            if let Some(mut prior) = previous.take() {
                prior.pump.as_mut().unwrap().derived = false;
                assert!(prior == p, "case {} continuity", c["index"]);
            }
            let input: Amount = s["amount"].as_str().unwrap().parse().unwrap();
            let q = xone_protocol_math::quote_exact_in(&p, &input, c["kind"] == "sell");
            if s["passed"] == false {
                assert!(q.is_err(), "case {} accepted", c["index"]);
                rejected += 1;
                continue;
            }
            let q = q.unwrap_or_else(|e| panic!("case {} kernel {e}", c["index"]));
            let expected_output = if c["kind"] == "sell" {
                s["user_sol_delta"].as_i64().unwrap()
            } else {
                s["user_token_delta"].as_i64().unwrap()
            };
            assert_eq!(
                q.amount_out.to_string(),
                expected_output.to_string(),
                "case {} output",
                c["index"]
            );
            assert_eq!(q.consumed_in, input);
            let mut after: Pool = serde_json::from_value(s["after"].clone()).unwrap();
            after.pump.as_mut().unwrap().derived = true;
            for (a, b) in q
                .next_pool
                .pump
                .as_ref()
                .unwrap()
                .accounts
                .iter()
                .zip(&after.pump.as_ref().unwrap().accounts)
            {
                assert!(
                    a == b,
                    "case {} account {} actual-lamports={} expected={} data-equal={}",
                    c["index"],
                    a.address,
                    a.lamports,
                    b.lamports,
                    a.data == b.data
                );
            }
            assert!(q.next_pool == after, "case {} full state", c["index"]);
            previous = Some(q.next_pool);
            full += 1;
        }
    }
    assert!(full > 5);
    assert!(rejected > 10);
}

#[test]
fn pump_independent_exact_token_buy_matches_original_instruction_full_ledger() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-bonding.json")).unwrap();
    let mut full = 0;
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "buy")
    {
        for s in c["steps"].as_array().unwrap() {
            let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
            let target: Amount = s["amount"].as_str().unwrap().parse().unwrap();
            let q = xone_protocol_math::exact_out::quote_exact_out(&p, &target, false);
            if s["passed"] == false {
                assert!(q.is_err(), "case {} accepted", c["index"]);
                continue;
            }
            let q = q.unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
            assert_eq!(q.amount_out, target);
            assert_eq!(
                q.amount_in.to_string(),
                (-s["user_sol_delta"].as_i64().unwrap()).to_string()
            );
            let mut after: Pool = serde_json::from_value(s["after"].clone()).unwrap();
            after.pump.as_mut().unwrap().derived = true;
            assert_eq!(q.next_pool, after, "case {} full ledger", c["index"]);
            full += 1;
        }
    }
    assert_eq!(full, 11);
}

#[test]
fn pump_exact_out_graduation_and_hard_input_boundaries_match_original() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-exact-out.json")).unwrap();
    let mut full = 0;
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::exact_out::quote_exact_out(&p, &target, false);
        if c["passed"] == false {
            assert!(q.is_err());
            continue;
        }
        let q = q.unwrap();
        assert_eq!(
            q.amount_in.to_string(),
            c["required_input"].as_str().unwrap()
        );
        assert_eq!(q.amount_out, target);
        let mut after: Pool = serde_json::from_value(c["after"].clone()).unwrap();
        after.pump.as_mut().unwrap().derived = true;
        assert_eq!(q.next_pool, after);
        if q.next_pool.reserve0 == Amount::zero() {
            assert_eq!(
                q.next_pool
                    .pump
                    .as_ref()
                    .unwrap()
                    .data(&q.next_pool.pool_id, 166)
                    .unwrap()[48],
                1
            );
        }
        full += 1;
    }
    assert_eq!(full, 7);
}

#[test]
fn pump_sell_target_inverts_both_ceil_fees_and_reports_actual_native_surplus() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../test/contracts/pump-sell-exact-out.json"
    ))
    .unwrap();
    let (mut full, mut surplus) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::exact_out::quote_exact_out(&p, &target, true);
        if c["passed"] == false {
            assert!(q.is_err());
            continue;
        }
        let q = q.unwrap();
        assert_eq!(
            q.amount_in.to_string(),
            c["required_input"].as_str().unwrap()
        );
        assert_eq!(
            q.amount_out.to_string(),
            c["actual_output"].as_str().unwrap()
        );
        assert!(q.amount_out.0 >= target.0);
        surplus += usize::from(q.amount_out.0 > target.0);
        let mut after: Pool = serde_json::from_value(c["after"].clone()).unwrap();
        after.pump.as_mut().unwrap().derived = true;
        assert_eq!(q.next_pool, after);
        full += 1;
    }
    assert_eq!((full, surplus), (18, 9));
}
