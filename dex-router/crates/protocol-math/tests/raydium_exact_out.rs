use serde_json::Value;
use xone_domain::{Amount, Pool};
use xone_protocol_math::exact_out::{quote_exact_out, quote_exact_out_with_limit};
#[test]
fn raydium_original_output_inverse_and_complete_fee_ledgers_match() {
    let f: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/raydium-exact-out.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let (mut good, mut rejected) = (0, 0);
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let amount: Amount = c["target"].as_str().unwrap().parse().unwrap();
        let zero = c["zero_for_one"].as_bool().unwrap();
        let q = quote_exact_out(&p, &amount, zero);
        let expected = &c["expected"];
        if expected["reverted"] == true {
            assert!(q.is_err(), "case {i}: expected reject");
            rejected += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("case {i}: {e}"));
        assert_eq!(q.amount_out, amount, "{i}");
        assert_eq!(
            q.amount_in.to_string(),
            expected["amount_in"].as_str().unwrap(),
            "{i}"
        );
        let after: Pool = serde_json::from_value(expected["pool"].clone()).unwrap();
        assert_eq!(q.next_pool, after, "{i}: full post state");
        assert!(
            quote_exact_out_with_limit(
                &p,
                &amount,
                &Amount::checked(&q.amount_in.0 - 1u8).unwrap(),
                zero
            )
            .is_err(),
            "{i}: max input minus one"
        );
        assert_eq!(
            quote_exact_out_with_limit(&p, &amount, &q.amount_in, zero)
                .unwrap()
                .next_pool,
            after
        );
        good += 1;
    }
    assert!(
        good > 200 && rejected > 200,
        "good={good} rejected={rejected}"
    );
}
