use xone_domain::{Amount, Pool};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../test/contracts/v2-exact-out.json")).unwrap()
}
#[test]
fn original_router_exact_outputs_match_complete_pair_transitions() {
    let f = data();
    let (mut transitions, mut exact_in_overproduces) = (0, 0);
    for case in f["cases"].as_array().unwrap() {
        let before: Vec<Pool> = serde_json::from_value(case["pools"].clone()).unwrap();
        let after: Vec<Pool> = serde_json::from_value(case["expected_pools"].clone()).unwrap();
        let path = case["path"].as_array().unwrap();
        let amounts = case["expected"]["amounts"].as_array().unwrap();
        for i in 0..path.len() - 1 {
            let from = path[i].as_str().unwrap();
            let to = path[i + 1].as_str().unwrap();
            let p = before
                .iter()
                .find(|p| {
                    (p.token0 == from && p.token1 == to) || (p.token1 == from && p.token0 == to)
                })
                .unwrap();
            let zero = p.token0 == from;
            let output: Amount = amounts[i + 1].as_str().unwrap().parse().unwrap();
            let q = xone_protocol_math::quote_exact_out(p, &output, zero)
                .unwrap_or_else(|e| panic!("case {} hop {i}: {e}", case["id"]));
            assert_eq!(q.amount_in.to_string(), amounts[i].as_str().unwrap());
            assert_eq!(q.amount_out, output);
            assert_eq!(
                q.next_pool,
                *after.iter().find(|x| x.pool_id == p.pool_id).unwrap(),
                "case {} hop {i}",
                case["id"]
            );
            let too_small = Amount::checked(&q.amount_in.0 - 1u8).unwrap();
            assert_eq!(
                xone_protocol_math::exact_out::quote_exact_out_with_limit(
                    p, &output, &too_small, zero
                )
                .unwrap_err()
                .0,
                "EXACT_OUT_MAX_INPUT"
            );
            let in_quote = xone_protocol_math::quote_exact_in(p, &q.amount_in, zero).unwrap();
            exact_in_overproduces += usize::from(in_quote.amount_out.0 > output.0);
            transitions += 1;
        }
    }
    assert_eq!(transitions, 162);
    assert!(
        exact_in_overproduces > 0,
        "must exercise output targets distinct from ExactIn maxima"
    );
}
#[test]
fn exact_out_rejects_zero_reserve_bound_and_unadmitted_protocols() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][0]["pools"][0].clone()).unwrap();
    for (output, error) in [
        (Amount::zero(), "EXACT_OUT_ZERO_OUTPUT"),
        (p.reserve1.clone(), "EXACT_OUT_LIQUIDITY_BOUND"),
    ] {
        assert_eq!(
            xone_protocol_math::quote_exact_out(&p, &output, true)
                .unwrap_err()
                .0,
            error
        );
    }
    let mut unknown = p.clone();
    unknown.protocol = "pancake-v2".into();
    assert_eq!(
        xone_protocol_math::quote_exact_out(&unknown, &"1".parse().unwrap(), true)
            .unwrap_err()
            .0,
        "EXACT_OUT_PROTOCOL_UNSUPPORTED"
    );
    let saved = p.clone();
    let max = Amount::zero();
    assert!(xone_protocol_math::exact_out::quote_exact_out_with_limit(
        &p,
        &"1".parse().unwrap(),
        &max,
        true
    )
    .is_err());
    assert_eq!(p, saved);
}
