use serde_json::Value;
use xone_domain::{Amount, Pool};
fn check(fixture: &str, expected: (usize, usize)) {
    let f: Value = serde_json::from_slice(&std::fs::read(fixture).unwrap()).unwrap();
    let (mut full, mut reject) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let mut previous: Option<Pool> = None;
        for s in c["steps"].as_array().unwrap() {
            let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
            p.validate()
                .unwrap_or_else(|e| panic!("case {} validate {e}", c["index"]));
            if let Some(mut prior) = previous.take() {
                prior.meteora.as_mut().unwrap().derived = false;
                assert_eq!(prior, p);
            }
            let amount: Amount = s["amount"].as_str().unwrap().parse().unwrap();
            let zero = c["zero_for_one"].as_bool().unwrap();
            let q = if c["exact_out"] == true {
                xone_protocol_math::exact_out::quote_exact_out(&p, &amount, zero)
                    .map(|q| (q.amount_in, q.amount_out, q.next_pool))
            } else {
                xone_protocol_math::quote_exact_in(&p, &amount, zero)
                    .map(|q| (q.consumed_in, q.amount_out, q.next_pool))
            };
            if s["passed"] == false {
                assert!(q.is_err(), "case {} accepted", c["index"]);
                reject += 1;
                continue;
            }
            let (input, output, next) = q.unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
            assert_eq!(input.to_string(), s["actual_input"].as_str().unwrap());
            assert_eq!(output.to_string(), s["actual_output"].as_str().unwrap());
            let mut after: Pool = serde_json::from_value(s["after"].clone()).unwrap();
            after.meteora.as_mut().unwrap().derived = true;
            assert_eq!(next, after, "case {} complete order state", c["index"]);
            previous = Some(next);
            full += 1;
        }
    }
    assert_eq!((full, reject), expected);
}

#[test]
fn limit_order_layers_fills_fee_split_rollover_and_full_state_match_original() {
    check(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-orders.json"
        ),
        (96, 32),
    );
}
#[test]
fn orders_exact_out_original_rounding_full_drains_and_complete_state() {
    check(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/meteora-orders-exact-out.json"
        ),
        (193, 31),
    );
}
