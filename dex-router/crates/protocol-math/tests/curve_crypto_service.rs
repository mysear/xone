use xone_domain::{Amount, Pool};
use xone_protocol_math::multi_asset::quote_exact_in_tokens;
#[test]
fn complete_crypto_normal_dispatch_matches_original_exchange_and_lp_poststate() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/curve-crypto-service.json"
    ))
    .unwrap();
    let mut ok = 0;
    let mut reverted = 0;
    for c in f["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let pool: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        pool.validate().unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let i = c["index_in"].as_u64().unwrap() as usize;
        let j = c["index_out"].as_u64().unwrap() as usize;
        let s = pool.multi_asset.as_ref().unwrap();
        let q = quote_exact_in_tokens(&pool, &input, &s.assets[i].token, &s.assets[j].token);
        let e = &c["expected"];
        if e["reverted"] == true {
            assert!(q.is_err(), "expected revert {name}");
            reverted += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(
            q.amount_out.to_string(),
            e["amount_out"].as_str().unwrap(),
            "out {name}"
        );
        assert_eq!(q.remaining_in, Amount::zero());
        assert_eq!(q.consumed_in, input);
        let next = q.next_pool.multi_asset.as_ref().unwrap();
        let state = next.curve_crypto.as_ref().unwrap();
        let expected = &e["state"];
        for k in 0..2 {
            assert_eq!(
                next.assets[k].balance.to_string(),
                e["balances"][k].as_str().unwrap(),
                "balance {name}"
            );
            assert_eq!(
                state.raw_balances[k].to_string(),
                e["raw_balances"][k].as_str().unwrap(),
                "raw {name}"
            )
        }
        for (actual, key) in [
            (&state.invariant, "D"),
            (&state.price_scale, "price_scale"),
            (&state.price_oracle, "_price_oracle"),
            (&state.last_prices, "last_prices"),
            (&state.xcp_profit, "xcp_profit"),
            (&state.xcp_profit_a, "xcp_profit_a"),
            (&state.virtual_price, "virtual_price"),
        ] {
            assert_eq!(
                actual.to_string(),
                expected[key].as_str().unwrap(),
                "{key} {name}"
            )
        }
        assert_eq!(state.not_adjusted, expected["not_adjusted"] == "1");
        assert_eq!(
            state.last_prices_timestamp.to_string(),
            expected["last_prices_timestamp"]
        );
        assert_eq!(
            state.future_a_gamma_time.to_string(),
            expected["future_A_gamma_time"]
        );
        assert_eq!(state.total_supply.to_string(), e["total_supply"]);
        assert_eq!(
            state.admin_receiver_balance.to_string(),
            e["receiver_balance"]
        );
        assert_eq!(q.next_pool.reserve0, next.assets[0].balance);
        assert_eq!(q.next_pool.reserve1, next.assets[1].balance);
        assert_ne!(q.next_pool, pool);
        q.next_pool.validate().unwrap();
        ok += 1;
    }
    assert!(ok >= 18 && reverted >= 8, "{ok}/{reverted}");
}
