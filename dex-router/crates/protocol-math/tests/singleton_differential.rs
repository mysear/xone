use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
#[test]
fn actual_pinned_pool_swaps_match_singleton_amount_and_core_post_state() {
    let file: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/singleton-quotes.json"
    ))
    .unwrap();
    let cases = file["cases"].as_array().unwrap();
    assert!(cases.len() >= 300);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let result = quote_exact_in(&p, &input, c["zero_for_one"].as_bool().unwrap());
        let e = &c["expected"];
        if e["reverted"].as_bool() == Some(true) {
            assert!(
                result.is_err(),
                "{name}: accepted incomplete, zero, or source revert"
            );
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("{name}: {}", e.0));
        assert_eq!(
            q.amount_out.to_string(),
            e["amount_out"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(q.consumed_in, input, "{name}");
        assert_eq!(q.remaining_in, Amount::zero(), "{name}");
        let state = &q.next_pool.singleton_cl.unwrap().core;
        assert_eq!(
            state.sqrt_price_x96.to_string(),
            e["sqrt_price_x96"].as_str().unwrap(),
            "{name} price"
        );
        assert_eq!(
            state.tick,
            i32::try_from(e["tick"].as_i64().unwrap()).unwrap(),
            "{name} tick"
        );
        assert_eq!(
            state.liquidity.to_string(),
            e["liquidity"].as_str().unwrap(),
            "{name} liquidity"
        );
    }
}
