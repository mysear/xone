use serde_json::Value;
use xone_domain::{Amount, Pool};
#[test]
fn original_pump_amm_v1_templates_preserve_all_vault_fee_and_native_backing_state() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/pump-amm.json")).unwrap();
    let (mut full, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        for s in c["steps"].as_array().unwrap() {
            let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
            p.validate()
                .unwrap_or_else(|e| panic!("case {} validate {e}", c["index"]));
            let amount: Amount = s["amount"].as_str().unwrap().parse().unwrap();
            let q = if c["kind"] == "buy" {
                xone_protocol_math::exact_out::quote_exact_out(&p, &amount, false)
                    .map(|q| (q.amount_in, q.amount_out, q.next_pool))
            } else {
                xone_protocol_math::quote_exact_in(&p, &amount, c["kind"] == "sell")
                    .map(|q| (q.consumed_in, q.amount_out, q.next_pool))
            };
            if s["passed"] == false {
                assert!(q.is_err(), "case {} accepted", c["index"]);
                rejected += 1;
                continue;
            }
            let (input, output, next) =
                q.unwrap_or_else(|e| panic!("case {} quote {e}", c["index"]));
            let zero = c["kind"] == "sell";
            assert_eq!(
                input.to_string(),
                (-s[if zero { "actual_base" } else { "actual_quote" }]
                    .as_i64()
                    .unwrap())
                .to_string()
            );
            assert_eq!(
                output.to_string(),
                s[if zero { "actual_quote" } else { "actual_base" }]
                    .as_i64()
                    .unwrap()
                    .to_string()
            );
            let mut after: Pool = serde_json::from_value(s["after"].clone()).unwrap();
            after.pump.as_mut().unwrap().derived = true;
            assert_eq!(next, after, "case {} complete state", c["index"]);
            full += 1;
        }
    }
    assert!(full >= 15);
    assert!(rejected > 0);
}
