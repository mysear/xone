use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
#[test]
fn unchanged_raydium_program_matches_amounts_raw_ledgers_and_net_poststate() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/raydium-quotes.json")).unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1604);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let result = quote_exact_in(&p, &input, c["zero_for_one"].as_bool().unwrap());
        let e = &c["expected"];
        if e["reverted"].as_bool() == Some(true) {
            assert!(result.is_err(), "{name}: should reject");
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("{name}: {}", e.0));
        assert_eq!(
            q.amount_out.to_string(),
            e["amount_out"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        assert_eq!(
            q.next_pool.reserve0.to_string(),
            e["reserve0"].as_str().unwrap(),
            "{name} reserve0"
        );
        assert_eq!(
            q.next_pool.reserve1.to_string(),
            e["reserve1"].as_str().unwrap(),
            "{name} reserve1"
        );
        assert_eq!(
            serde_json::to_value(q.next_pool.raydium_cpmm.unwrap()).unwrap(),
            e["raydium_cpmm"],
            "{name} raw/ledger/state"
        );
    }
}
