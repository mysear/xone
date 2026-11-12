use xone_domain::{Amount, Pool};
use xone_protocol_math::quote_exact_out;
#[test]
fn original_swap_exact_out2_matches_exact_credit_bin_rounding_and_complete_ledgers() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/meteora-exact-out.json"
    ))
    .unwrap();
    let (mut full, mut rejected, mut dust) = (0, 0, 0);
    for c in f["cases"].as_array().unwrap() {
        let mut previous: Option<Pool> = None;
        for s in c["steps"].as_array().unwrap() {
            let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
            p.validate()
                .unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
            if let Some(mut a) = previous.take() {
                a.meteora.as_mut().unwrap().derived = false;
                assert!(a == p, "{} sequential state", c["index"]);
            }
            let target: Amount = s["target"].as_str().unwrap().parse().unwrap();
            let q = quote_exact_out(&p, &target, s["zero_for_one"].as_bool().unwrap());
            if s["passed"] == false {
                assert!(q.is_err(), "case {} unexpectedly quoted", c["index"]);
                rejected += 1;
                continue;
            }
            let q = q.unwrap_or_else(|e| panic!("case {} {}: {e}", c["index"], c["name"]));
            assert_eq!(q.amount_out, target);
            assert_eq!(
                q.amount_in.to_string(),
                s["required_input"].as_str().unwrap(),
                "case {} input",
                c["index"]
            );
            let mut after: Pool = serde_json::from_value(s["after"].clone()).unwrap();
            after.meteora.as_mut().unwrap().derived = true;
            let a = q.next_pool.meteora.as_ref().unwrap();
            let b = after.meteora.as_ref().unwrap();
            assert!(a.pair_data == b.pair_data, "case {} pair", c["index"]);
            assert!(a.oracle_data == b.oracle_data, "case {} oracle", c["index"]);
            for (x, y) in a.bin_arrays.iter().zip(&b.bin_arrays) {
                let left = hex::decode(&x.data).unwrap();
                let right = hex::decode(&y.data).unwrap();
                let delta = left
                    .iter()
                    .zip(&right)
                    .enumerate()
                    .filter(|(_, (x, y))| x != y)
                    .take(12)
                    .map(|(i, (x, y))| (i, *x, *y))
                    .collect::<Vec<_>>();
                assert!(
                    delta.is_empty(),
                    "case {} array {} {:?}",
                    c["index"],
                    x.index,
                    delta
                );
            }
            assert_eq!(q.next_pool.reserve0, after.reserve0);
            assert_eq!(q.next_pool.reserve1, after.reserve1);
            let side = if s["zero_for_one"].as_bool().unwrap() {
                8
            } else {
                0
            };
            let sum = |p: &Pool| -> u128 {
                p.meteora
                    .as_ref()
                    .unwrap()
                    .bin_arrays
                    .iter()
                    .map(|a| {
                        let raw = hex::decode(&a.data).unwrap();
                        (0..70)
                            .map(|i| {
                                u128::from(xone_domain::meteora::u64_at(&raw, 56 + i * 144 + side))
                            })
                            .sum::<u128>()
                    })
                    .sum()
            };
            let used = sum(&p) - sum(&q.next_pool);
            if used > target.to_string().parse::<u128>().unwrap() {
                dust += 1;
            }
            previous = Some(q.next_pool);
            full += 1;
        }
    }
    assert_eq!(f["cases"].as_array().unwrap().len(), 164);
    assert_eq!((full, rejected), (162, 22));
    assert!(dust > 20);
}
