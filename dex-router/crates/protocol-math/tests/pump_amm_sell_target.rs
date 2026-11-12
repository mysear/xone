use serde_json::Value;
use xone_domain::{Amount, Pool};
#[test]
fn sell_target_three_ceil_fees_minimum_input_and_all_native_post_state() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/pump-amm-sell-target.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let (mut full, mut rejected, mut boost) = (0, 0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let target: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let q = xone_protocol_math::quote_exact_out(&p, &target, true);
        if c["passed"] == false {
            assert!(q.is_err(), "case {} accepted", c["index"]);
            rejected += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
        assert_eq!(
            q.amount_in.to_string(),
            c["required_input"].as_str().unwrap()
        );
        assert_eq!(
            q.amount_out.to_string(),
            c["actual_output"].as_str().unwrap()
        );
        assert!(q.amount_out.0 >= target.0);
        let mut after: Pool = serde_json::from_value(c["after"].clone()).unwrap();
        after.pump.as_mut().unwrap().derived = true;
        assert!(q.next_pool == after, "case {} full post state", c["index"]);
        full += 1;
        boost += usize::from(c["boost"] == true);
    }
    assert_eq!((full, rejected, boost), (42, 6, 28));
}
