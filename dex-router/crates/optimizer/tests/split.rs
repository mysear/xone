use xone_domain::*;
use xone_optimizer::*;
use xone_routing::*;
#[test]
fn split_recomputes_remainders_and_shared_pools() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let input: Amount = "10003".parse().unwrap();
    let paths = search(
        &Graph::build(&s.pools).unwrap(),
        &s.pools,
        "A",
        "C",
        &input,
        Budget {
            split_parts: 20,
            max_hops: 3,
            max_candidates: 8,
            max_expansions: 1000,
        },
    )
    .unwrap();
    let best = optimize(&s.pools, &paths.candidates, &input, 20, 2, &Amount::zero()).unwrap();
    let sum = best
        .routes
        .iter()
        .map(|r| r.amount_in.0.clone())
        .sum::<num_bigint::BigUint>();
    assert_eq!(sum, input.0);
    assert_eq!(best.refund, Amount::zero());
    let replay = evaluate_allocation(&s.pools, &best.routes, &Amount::zero()).unwrap();
    assert_eq!(best.gross_buy_amount, replay.gross_buy_amount);
    assert!(best.gross_buy_amount >= paths.candidates[0].amount_out);
}
#[test]
fn final_path_amount_is_recomputed_for_allocated_input() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let amount = "10003".parse().unwrap();
    let paths = search(
        &Graph::build(&s.pools).unwrap(),
        &s.pools,
        "A",
        "C",
        &amount,
        Budget {
            split_parts: 20,
            max_hops: 3,
            max_candidates: 8,
            max_expansions: 1000,
        },
    )
    .unwrap();
    let result = optimize(&s.pools, &paths.candidates, &amount, 20, 2, &Amount::zero()).unwrap();
    assert!(result.routes.len() > 1);
    for route in result.routes {
        assert_eq!(route.path.amount_out, route.amount_out);
    }
}

#[test]
fn shared_v3_sequential_allocations_match_official_sdk_oracle_in_both_orders() {
    let all: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/v3-quotes.json")).unwrap();
    for case in all["allocation_cases"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(case["pools"].clone()).unwrap();
        let input: Amount = case["sell_amount"].as_str().unwrap().parse().unwrap();
        let candidates = search(
            &Graph::build(&pools).unwrap(),
            &pools,
            &pools[1].token0,
            &pools[0].token1,
            &input,
            Budget {
                split_parts: 20,
                max_hops: 3,
                max_candidates: 8,
                max_expansions: 100,
            },
        )
        .unwrap();
        let mut routes = Vec::new();
        for expected in case["routes"].as_array().unwrap() {
            let ids = expected["pool_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>();
            let path = candidates
                .candidates
                .iter()
                .find(|p| {
                    p.edges
                        .iter()
                        .map(|e| e.pool_id.as_str())
                        .collect::<Vec<_>>()
                        == ids
                })
                .unwrap()
                .clone();
            routes.push(AllocatedRoute {
                path,
                amount_in: expected["amount_in"].as_str().unwrap().parse().unwrap(),
                amount_out: Amount::zero(),
            });
        }
        let replay = evaluate_allocation(&pools, &routes, &Amount::zero()).unwrap();
        assert_eq!(replay.sell_amount, input);
        assert_eq!(replay.refund, Amount::zero());
        assert_eq!(
            replay.gross_buy_amount.to_string(),
            case["gross_buy_amount"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        for (actual, expected) in replay.routes.iter().zip(case["routes"].as_array().unwrap()) {
            assert_eq!(
                actual.amount_out.to_string(),
                expected["amount_out"].as_str().unwrap()
            );
            assert_eq!(actual.path.amount_out, actual.amount_out);
            assert_eq!(actual.path.amount_in, actual.amount_in);
        }
    }
}

#[test]
fn paths_unusable_for_whole_order_are_preserved_for_valid_split() {
    let a: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    let mut b = a.clone();
    b.pool_id = "0x0606060606060606060606060606060606060606".into();
    b.v3.as_mut().unwrap().factory = "0x0707070707070707070707070707070707070707".into();
    let pools = vec![a.clone(), b];
    let input: Amount = "60000000".parse().unwrap();
    let routes = search(
        &Graph::build(&pools).unwrap(),
        &pools,
        &a.token0,
        &a.token1,
        &input,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 100,
        },
    )
    .unwrap();
    assert_eq!(
        routes.candidates.len(),
        2,
        "both split-capable paths were lost"
    );
    let result = optimize(&pools, &routes.candidates, &input, 20, 2, &Amount::zero()).unwrap();
    assert_eq!(result.gross_buy_amount.to_string(), "59048234");
    assert_eq!(result.sell_amount, input);
    assert_eq!(result.refund, Amount::zero());
    assert_eq!(result.routes.len(), 2);
    for route in result.routes {
        assert_eq!(route.amount_in.to_string(), "30000000");
        assert_eq!(route.amount_out.to_string(), "29524117");
    }
}

#[test]
fn dust_probe_does_not_hide_larger_valid_small_order_splits() {
    let mut a: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    let v = a.v3.as_mut().unwrap();
    v.liquidity = Amount::checked(&v.liquidity.0 / 10000000u64).unwrap();
    for t in &mut v.ticks {
        t.liquidity_gross = Amount::checked(&t.liquidity_gross.0 / 10000000u64).unwrap();
        t.liquidity_net = (t.liquidity_net.parse::<i128>().unwrap() / 10000000).to_string();
    }
    let mut b = a.clone();
    b.pool_id = "0x0606060606060606060606060606060606060606".into();
    let pools = vec![a.clone(), b];
    let input: Amount = "8".parse().unwrap();
    let candidates = search(
        &Graph::build(&pools).unwrap(),
        &pools,
        &a.token0,
        &a.token1,
        &input,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 100,
        },
    )
    .unwrap();
    assert_eq!(
        candidates.candidates.len(),
        2,
        "dust probe stopped larger sizes"
    );
    let result = optimize(
        &pools,
        &candidates.candidates,
        &input,
        20,
        2,
        &Amount::zero(),
    )
    .unwrap();
    assert_eq!(result.sell_amount, input);
    assert_eq!(result.refund, Amount::zero());
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/v3-quotes.json")).unwrap();
    assert_eq!(
        result.gross_buy_amount.to_string(),
        reference["discovery_regressions"][0]["gross_buy_amount"]
            .as_str()
            .unwrap()
    );
}
#[test]
fn candidate_limit_retains_a_confirmed_full_order_fallback() {
    let a: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    let mut b = a.clone();
    b.pool_id = "0x0606060606060606060606060606060606060606".into();
    let mut v2: Pool =
        serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()
            .pools[0]
            .clone();
    v2.token0 = a.token0.clone();
    v2.token1 = a.token1.clone();
    v2.reserve0 = "1000000000".parse().unwrap();
    v2.reserve1 = v2.reserve0.clone();
    let pools = vec![a.clone(), b, v2];
    let input: Amount = "100000000".parse().unwrap();
    let candidates = search(
        &Graph::build(&pools).unwrap(),
        &pools,
        &a.token0,
        &a.token1,
        &input,
        Budget {
            split_parts: 20,
            max_hops: 1,
            max_candidates: 2,
            max_expansions: 100,
        },
    )
    .unwrap();
    assert!(
        candidates.candidates.iter().any(|p| p.amount_in == input),
        "truncation discarded a known full-input route"
    );
    let result = optimize(
        &pools,
        &candidates.candidates,
        &input,
        20,
        2,
        &Amount::zero(),
    )
    .unwrap();
    assert_eq!(result.sell_amount, input);
    assert!(result.gross_buy_amount >= "90661089".parse().unwrap());
}

#[test]
fn differently_cased_addresses_cannot_duplicate_physical_liquidity() {
    let mut p: Pool =
        serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()
            .pools[0]
            .clone();
    p.protocol = "pancake-v2".into();
    p.adapter_version = "pancake-v2/core-1".into();
    p.fee_numerator = 25;
    p.fee_denominator = 10000;
    p.v2 = Some(xone_domain::protocols::V2State {
        swap: None,
        factory: format!("0x{}", "44".repeat(20)),
        code_sha256: "aa".repeat(32),
    });
    p.pool_id = format!("0x{}", "ab".repeat(20));
    let mut alias = p.clone();
    alias.pool_id = format!("0x{}", "AB".repeat(20));
    assert!(
        Graph::build(&[p, alias]).is_err(),
        "alias addresses counted one EVM pool twice"
    );
}

#[test]
fn legacy_label_cannot_bypass_physical_address_alias_guard() {
    let mut named: Pool =
        serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()
            .pools[0]
            .clone();
    named.pool_id = format!("0x{}", "ab".repeat(20));
    named.protocol = "pancake-v2".into();
    named.adapter_version = "pancake-v2/core-1".into();
    named.fee_numerator = 25;
    named.fee_denominator = 10000;
    named.v2 = Some(xone_domain::protocols::V2State {
        swap: None,
        factory: format!("0x{}", "44".repeat(20)),
        code_sha256: "aa".repeat(32),
    });
    let mut legacy = named.clone();
    legacy.pool_id = format!("0x{}", "AB".repeat(20));
    legacy.protocol = "uniswap-v2".into();
    legacy.adapter_version = "uniswap-v2/1".into();
    legacy.v2 = None;
    assert!(
        Graph::build(&[named, legacy]).is_err(),
        "legacy label made same physical address a second pool"
    );
}
