use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_in;
#[test]
fn original_meteora_swap2_complete_account_bytes_and_consecutive_swaps() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/meteora-mm.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 70);
    let mut full = 0;
    let mut rejected = 0;
    for case in data["cases"].as_array().unwrap() {
        let mut previous: Option<Pool> = None;
        for (step_index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let pool: Pool = serde_json::from_value(step["pool"].clone()).unwrap();
            pool.validate()
                .unwrap_or_else(|e| panic!("case {} {e}", case["index"]));
            if let Some(mut next) = previous {
                next.meteora.as_mut().unwrap().derived = false;
                assert_eq!(next, pool);
            }
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let result = quote_exact_in(&pool, &input, step["zero_for_one"].as_bool().unwrap());
            if step["passed"] == false {
                assert!(result.is_err(), "case {} accepted", case["index"]);
                rejected += 1;
                previous = None;
            } else {
                let q = result
                    .unwrap_or_else(|e| panic!("case {} step {step_index} {e}", case["index"]));
                assert_eq!(
                    q.amount_out.to_string(),
                    step["output"].as_str().unwrap(),
                    "case {}",
                    case["index"]
                );
                assert_eq!(q.consumed_in, input);
                assert_eq!(q.remaining_in, Amount::zero());
                let mut after: Pool = serde_json::from_value(step["after"].clone()).unwrap();
                after.meteora.as_mut().unwrap().derived = true;
                assert_eq!(
                    q.next_pool.meteora.as_ref().unwrap().pair_data,
                    after.meteora.as_ref().unwrap().pair_data,
                    "case {} pair",
                    case["index"]
                );
                assert_eq!(
                    q.next_pool.meteora.as_ref().unwrap().oracle_data,
                    after.meteora.as_ref().unwrap().oracle_data,
                    "case {} oracle",
                    case["index"]
                );
                for (a, b) in q
                    .next_pool
                    .meteora
                    .as_ref()
                    .unwrap()
                    .bin_arrays
                    .iter()
                    .zip(&after.meteora.as_ref().unwrap().bin_arrays)
                {
                    let x = hex::decode(&a.data).unwrap();
                    let y = hex::decode(&b.data).unwrap();
                    let changes = x
                        .iter()
                        .zip(&y)
                        .enumerate()
                        .filter(|(_, (x, y))| x != y)
                        .take(16)
                        .map(|(i, (x, y))| (i, *x, *y))
                        .collect::<Vec<_>>();
                    assert!(
                        changes.is_empty(),
                        "case {} array {} changes {:?}",
                        case["index"],
                        a.index,
                        changes
                    );
                }
                assert_eq!(q.next_pool.reserve0, after.reserve0);
                assert_eq!(q.next_pool.reserve1, after.reserve1);
                previous = Some(q.next_pool);
                full += 1;
            }
        }
    }
    assert_eq!((full, rejected), (55, 35));
}

#[test]
fn original_oracle_lifetime_ring_growth_wrap_and_fee_boundaries() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/meteora-mm-edge.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 25);
    let (mut success, mut rejected) = (0, 0);
    for c in data["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let input: Amount = c["input"].as_str().unwrap().parse().unwrap();
        let q = quote_exact_in(&p, &input, c["zero_for_one"].as_bool().unwrap());
        if c["passed"] == false {
            assert!(q.is_err(), "{} unexpectedly accepted", c["name"]);
            rejected += 1;
            continue;
        }
        let q = q.unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        assert_eq!(
            q.amount_out.to_string(),
            c["output"].as_str().unwrap(),
            "{} amount",
            c["name"]
        );
        let mut expected: Pool = serde_json::from_value(c["after"].clone()).unwrap();
        expected.meteora.as_mut().unwrap().derived = true;
        let a = q.next_pool.meteora.as_ref().unwrap();
        let b = expected.meteora.as_ref().unwrap();
        assert!(a.pair_data == b.pair_data, "{} pair", c["name"]);
        assert!(a.oracle_data == b.oracle_data, "{} oracle", c["name"]);
        assert!(a.bin_arrays == b.bin_arrays, "{} arrays", c["name"]);
        assert_eq!(q.next_pool.reserve0, expected.reserve0);
        assert_eq!(q.next_pool.reserve1, expected.reserve1);
        success += 1;
    }
    assert_eq!((success, rejected), (23, 2));
}
