use xone_domain::Pool;
fn pool() -> Pool {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/weighted-state.json")).unwrap();
    serde_json::from_value(data["pools"][0].clone()).unwrap()
}

#[test]
fn weighted_is_distinct_and_rejects_other_parameters() {
    let p = pool();
    p.validate().unwrap();
    for i in 0..15 {
        let mut q = p.clone();
        match i {
            0 => q.weighted = None,
            1 => q.protocol = "balancer-v2-managed".into(),
            2 => q.protocol = "raydium-clmm-static".into(),
            3 => q.weighted.as_mut().unwrap().weight0 = "600000000000000000".parse().unwrap(),
            4 => q.weighted.as_mut().unwrap().scaling0 = "1000000000000000001".parse().unwrap(),
            5 => q.weighted.as_mut().unwrap().pool_id = format!("{}{}", q.pool_id, "0".repeat(24)),
            6 => q.weighted.as_mut().unwrap().token0_code_sha256.clear(),
            7 => q.token_behavior = "tax".into(),
            8 => q.fee_numerator = 999999999999,
            9 => q.fee_numerator = 100000000000000001,
            10 => q.adapter_version = "balancer-v2-stable2/stable-1".into(),
            11 => std::mem::swap(&mut q.token0, &mut q.token1),
            12 => q.reserve0 = "5192296858534827628530496329220096".parse().unwrap(),
            13 => q.chain_key = "solana:localnet".into(),
            14 => q.fee_denominator = 1000000,
            _ => unreachable!(),
        };
        assert!(q.validate().is_err(), "mutation {i}");
    }
    let s = p.weighted.unwrap();
    let w = s.to_proto().unwrap();
    let back = xone_domain::weighted::WeightedState::from_proto(w.clone()).unwrap();
    assert_eq!(back, s);
    let mut short = w.clone();
    short.pool_id.pop();
    assert!(xone_domain::weighted::WeightedState::from_proto(short).is_err());
    let mut short = w;
    short.weight0.as_mut().unwrap().uint256_be.pop();
    assert!(xone_domain::weighted::WeightedState::from_proto(short).is_err());
}
