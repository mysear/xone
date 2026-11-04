use xone_domain::{Amount, Pool};
use xone_protocol_math::multi_asset::quote_exact_in_tokens;
#[test]
fn meta_profile_matches_original_outputs_and_complete_physical_poststate() {
    let d: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-meta-service.json"
    ))
    .unwrap();
    assert_eq!(d["cases"].as_array().unwrap().len(), 32);
    for c in d["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let native = &c["original"];
        let input: Amount = native["input"].as_str().unwrap().parse().unwrap();
        let i = native["index_in"].as_u64().unwrap() as usize;
        let j = native["index_out"].as_u64().unwrap() as usize;
        let a = &p.multi_asset.as_ref().unwrap().assets;
        let result = quote_exact_in_tokens(&p, &input, &a[i].token, &a[j].token);
        if native["expected"]["amount_out"] == "0" {
            assert_eq!(result.unwrap_err().0, "INSUFFICIENT_OUTPUT");
            continue;
        }
        let q = result.unwrap();
        let expected: Pool = serde_json::from_value(c["expected_pool"].clone()).unwrap();
        assert_eq!(
            q.next_pool, expected,
            "{} phase{}",
            c["name"], c["step_index"]
        );
        assert_eq!(
            q.amount_out.to_string(),
            native["expected"]["amount_out"].as_str().unwrap()
        );
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
    }
}
