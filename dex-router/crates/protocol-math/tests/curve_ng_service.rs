use xone_domain::{Amount, Pool};
use xone_protocol_math::{multi_asset::quote_exact_in_tokens, quote_exact_in};
#[test]
fn normal_ng_profile_dispatch_matches_complete_original_exchange_postpool() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-ng-service-state.json"
    ))
    .unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert!(cases.len() >= 42);
    for c in cases {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let native = &c["original"];
        let input: Amount = native["input"].as_str().unwrap().parse().unwrap();
        let i = native["index_in"].as_u64().unwrap() as usize;
        let j = native["index_out"].as_u64().unwrap() as usize;
        let assets = &p.multi_asset.as_ref().unwrap().assets;
        let result = quote_exact_in_tokens(&p, &input, &assets[i].token, &assets[j].token);
        if native["expected"]["amount_out"] == "0" {
            assert!(result.is_err());
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("{} step{}: {}", c["name"], c["step_index"], e));
        let expected: Pool = serde_json::from_value(c["expected_pool"].clone()).unwrap();
        assert_eq!(q.next_pool, expected);
        assert_eq!(
            q.amount_out.to_string(),
            native["expected"]["amount_out"].as_str().unwrap()
        );
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        if i == 0 && j == 1 {
            assert_eq!(
                quote_exact_in(&p, &input, true).unwrap().next_pool,
                expected
            );
        }
    }
}
