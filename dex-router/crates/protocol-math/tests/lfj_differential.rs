use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
#[test]
fn lfj_complete_original_swap_matches_outputs_parameters_fees_bins_and_oracle() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/lfj-v21-state.json")).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 96);
    let mut reverted = 0;
    for case in cases {
        let pool: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
        let input: Amount = case["input"].as_str().unwrap().parse().unwrap();
        let result = quote_exact_in(&pool, &input, case["zero_for_one"].as_bool().unwrap());
        if case["expected"]["reverted"] == true {
            assert!(result.is_err(), "case {}", case["index"]);
            reverted += 1;
        } else {
            let q = result.unwrap_or_else(|e| panic!("case {}: {e}", case["index"]));
            assert_eq!(
                q.amount_out.to_string(),
                case["expected"]["amount_out"].as_str().unwrap()
            );
            assert_eq!(q.consumed_in, input);
            assert_eq!(q.remaining_in, Amount::zero());
            let after = serde_json::to_value(q.next_pool.lfj.unwrap()).unwrap();
            for key in [
                "parameters",
                "raw_reserve0",
                "raw_reserve1",
                "protocol_fees0",
                "protocol_fees1",
                "bins",
                "oracle_samples",
            ] {
                assert_eq!(
                    after[key], case["expected"]["after"][key],
                    "case {} {key}",
                    case["index"]
                );
            }
        }
    }
    assert_eq!(reverted, 48);
}
