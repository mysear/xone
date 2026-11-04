use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
#[test]
fn executed_primary_contracts_match_amounts_and_post_balances() {
    let file: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/stable-quotes.json")).unwrap();
    let mut count = 0;
    for c in file["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let result = quote_exact_in(&p, &input, c["zero_for_one"].as_bool().unwrap());
        let expected = &c["expected"];
        if expected["reverted"].as_bool() == Some(true) {
            assert!(
                result.is_err(),
                "{} case {} should revert",
                p.protocol,
                count
            );
        } else {
            let amount = expected["amount_out"].as_str().unwrap();
            if amount == "0" {
                assert!(
                    result.is_err(),
                    "zero output must not be returned as a route"
                );
            } else {
                let q = result.unwrap_or_else(|e| {
                    panic!("{} case {} input {}: {}", p.protocol, count, input, e.0)
                });
                assert_eq!(
                    q.amount_out.to_string(),
                    amount,
                    "{} case {} output",
                    p.protocol,
                    count
                );
                assert_eq!(
                    q.next_pool.reserve0.to_string(),
                    expected["next_balances"][0].as_str().unwrap(),
                    "case {} raw balance0",
                    count
                );
                assert_eq!(
                    q.next_pool.reserve1.to_string(),
                    expected["next_balances"][1].as_str().unwrap(),
                    "case {} raw balance1",
                    count
                );
                assert_eq!(q.consumed_in, input);
                assert_eq!(q.remaining_in, Amount::zero());
            }
        }
        count += 1;
    }
    assert_eq!(count, 180);
}
