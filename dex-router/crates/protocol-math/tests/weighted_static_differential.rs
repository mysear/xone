use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
#[test]
fn original_arbitrary_weight_pool_callback_and_vault_cash_match() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/weighted-static-quotes.json"
    ))
    .unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5355);
    let mut positives = 0;
    let mut rejections = 0;
    for (i, c) in cases.iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let got = quote_exact_in(&p, &input, c["zero_for_one"].as_bool().unwrap());
        let expected = &c["expected"];
        if expected["reverted"] == true || expected["amount_out"] == "0" {
            assert!(got.is_err(), "case {i} expected rejected");
            rejections += 1;
        } else {
            let q = got.unwrap_or_else(|e| panic!("case {i}: {e}"));
            assert_eq!(
                q.amount_out.to_string(),
                expected["amount_out"].as_str().unwrap(),
                "case {i} output"
            );
            assert_eq!(
                q.next_pool.reserve0.to_string(),
                expected["next_balances"][0].as_str().unwrap(),
                "case {i} balance0"
            );
            assert_eq!(
                q.next_pool.reserve1.to_string(),
                expected["next_balances"][1].as_str().unwrap(),
                "case {i} balance1"
            );
            assert_eq!(q.consumed_in, input);
            assert_eq!(q.remaining_in, Amount::zero());
            assert_eq!(q.next_pool.weighted, p.weighted);
            positives += 1;
        }
    }
    assert!(positives > 200 && rejections > 200);
    for sequence in data["sequences"].as_array().unwrap() {
        let mut p: Pool = serde_json::from_value(sequence["pool"].clone()).unwrap();
        for step in sequence["steps"].as_array().unwrap() {
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let q = quote_exact_in(&p, &input, step["zero_for_one"].as_bool().unwrap()).unwrap();
            let e = &step["expected"];
            assert_eq!(q.amount_out.to_string(), e["amount_out"].as_str().unwrap());
            assert_eq!(
                q.next_pool.reserve0.to_string(),
                e["next_balances"][0].as_str().unwrap()
            );
            assert_eq!(
                q.next_pool.reserve1.to_string(),
                e["next_balances"][1].as_str().unwrap()
            );
            q.next_pool.validate().unwrap();
            p = q.next_pool;
        }
    }
    assert_eq!(data["sequences"].as_array().unwrap().len(), 315);
}
