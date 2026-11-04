use xone_domain::Amount;
use xone_protocol_math::singleton::infinity_bin::{quote_exact_in, InfinityBinState};
#[test]
fn original_infinity_bin_swap_matches_independent_single_bin_math() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-advanced-bin-quotes.json"
    ))
    .unwrap();
    assert!(fixture["cases"].as_array().unwrap().len() >= 180);
    for c in fixture["cases"].as_array().unwrap() {
        let s: InfinityBinState = serde_json::from_value(c["state"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let q = quote_exact_in(&s, &input, c["zero_for_one"].as_bool().unwrap());
        let e = &c["expected"];
        if e["reverted"].as_bool() == Some(true) {
            assert!(q.is_err(), "{}", c["name"]);
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{}: {}", c["name"], e.0));
        assert_eq!(
            q.amount_out.to_string(),
            e["amount_out"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(
            q.protocol_amount.to_string(),
            e["protocol_amount"].as_str().unwrap()
        );
        assert_eq!(q.swap_fee, e["swap_fee"].as_u64().unwrap() as u32);
        assert_eq!(q.price_x128.to_string(), e["price_x128"].as_str().unwrap());
        assert_eq!(
            q.next_state.reserve0.to_string(),
            e["reserve0"].as_str().unwrap()
        );
        assert_eq!(
            q.next_state.reserve1.to_string(),
            e["reserve1"].as_str().unwrap()
        );
    }
}
