use serde_json::Value;
use xone_domain::{Amount, Pool};
#[test]
fn bitmap_boundary_swaps_preserve_original_all_pair_array_extension_and_oracle_bytes() {
    let f: Value =
        serde_json::from_str(include_str!("../../../test/contracts/meteora-bitmap.json")).unwrap();
    let (mut full, mut rejected, mut crossing) = (0, 0, 0);
    for c in f["cases"].as_array().unwrap() {
        let s = &c["steps"][0];
        let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
        p.validate().unwrap();
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
            rejected += 1;
            continue;
        }
        let (input, output, next) = q.unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
        assert_eq!(input.to_string(), s["actual_input"].as_str().unwrap());
        assert_eq!(output.to_string(), s["actual_output"].as_str().unwrap());
        let mut after: Pool = serde_json::from_value(s["after"].clone()).unwrap();
        after.meteora.as_mut().unwrap().derived = true;
        crossing += usize::from(
            xone_domain::meteora::i32_at(&p.meteora.as_ref().unwrap().pair().unwrap(), 76)
                .div_euclid(70)
                != xone_domain::meteora::i32_at(
                    &next.meteora.as_ref().unwrap().pair().unwrap(),
                    76,
                )
                .div_euclid(70),
        );
        assert_eq!(next, after, "case {} complete state", c["index"]);

        full += 1;
    }
    assert_eq!((full, rejected), (34, 22));
    assert!(crossing >= 6);
}
