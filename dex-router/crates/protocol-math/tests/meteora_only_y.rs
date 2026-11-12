use serde_json::Value;
use xone_domain::{Amount, Pool};
#[test]
fn only_y_original_sbf_exact_in_out_fee_token_and_all_state_bytes() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/meteora-only-y.json")).unwrap();
    let (mut full, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let mut previous: Option<Pool> = None;
        for s in c["steps"].as_array().unwrap() {
            let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
            p.validate()
                .unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
            if let Some(mut prev) = previous.take() {
                prev.meteora.as_mut().unwrap().derived = false;
                assert!(prev == p, "case {} sequence", c["index"]);
            }
            let amount: Amount = s["specified_amount"].as_str().unwrap().parse().unwrap();
            let zero = s["zero_for_one"].as_bool().unwrap();
            let exact = s["exact_out"].as_bool().unwrap();
            let q = if exact {
                xone_protocol_math::quote_exact_out(&p, &amount, zero)
                    .map(|q| (q.amount_in, q.amount_out, q.next_pool))
            } else {
                xone_protocol_math::quote_exact_in(&p, &amount, zero)
                    .map(|q| (q.consumed_in, q.amount_out, q.next_pool))
            };
            if s["passed"] == false {
                assert!(q.is_err(), "case {} accepted", c["index"]);
                rejected += 1;
                continue;
            }
            let (input, output, next) = q.unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
            assert_eq!(
                input.to_string(),
                s["actual_input"].as_str().unwrap(),
                "case {} input",
                c["index"]
            );
            assert_eq!(
                output.to_string(),
                s["actual_output"].as_str().unwrap(),
                "case {} output",
                c["index"]
            );
            let mut expected: Pool = serde_json::from_value(s["after"].clone()).unwrap();
            expected.meteora.as_mut().unwrap().derived = true;
            let a = next.meteora.as_ref().unwrap();
            let b = expected.meteora.as_ref().unwrap();
            assert!(a.pair_data == b.pair_data, "case {} pair", c["index"]);
            assert!(a.oracle_data == b.oracle_data, "case {} oracle", c["index"]);
            for (x, y) in a.bin_arrays.iter().zip(&b.bin_arrays) {
                let x = hex::decode(&x.data).unwrap();
                let y = hex::decode(&y.data).unwrap();
                let delta = x
                    .iter()
                    .zip(&y)
                    .enumerate()
                    .filter(|(_, (x, y))| x != y)
                    .take(12)
                    .map(|(i, (x, y))| (i, *x, *y))
                    .collect::<Vec<_>>();
                assert!(delta.is_empty(), "case {} array {:?}", c["index"], delta);
            }
            assert_eq!(next.reserve0, expected.reserve0);
            assert_eq!(next.reserve1, expected.reserve1);
            previous = Some(next);
            full += 1;
        }
    }
    assert_eq!(f["cases"].as_array().unwrap().len(), 236);
    assert_eq!((full, rejected), (237, 39));
}
