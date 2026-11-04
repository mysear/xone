use xone_domain::*;
use xone_routing::*;
#[test]
fn amount_aware_graph_finds_multiple_physical_pools() {
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let g = Graph::build(&s.pools).unwrap();
    let r = search(
        &g,
        &s.pools,
        "A",
        "C",
        &"10000".parse().unwrap(),
        Budget {
            split_parts: 20,
            max_hops: 3,
            max_candidates: 32,
            max_expansions: 1000,
        },
    )
    .unwrap();
    assert!(r.candidates.len() >= 3);
    assert_eq!(r.candidates[0].tokens.last().unwrap(), "C");
    let limited = search(
        &g,
        &s.pools,
        "A",
        "C",
        &"10000".parse().unwrap(),
        Budget {
            split_parts: 20,
            max_hops: 3,
            max_candidates: 32,
            max_expansions: 1,
        },
    )
    .unwrap();
    assert!(limited.budget_exhausted);
}
#[test]
fn cancelled_search_stops_without_partial_amount() {
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;
    let s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    let cancelled = AtomicBool::new(true);
    let result = search_bounded(
        &Graph::build(&s.pools).unwrap(),
        &s.pools,
        "A",
        "C",
        &"1000".parse().unwrap(),
        Budget {
            split_parts: 20,
            max_hops: 3,
            max_candidates: 32,
            max_expansions: 1000,
        },
        Some(SearchControl {
            cancel: &cancelled,
            deadline: Instant::now(),
        }),
    );
    assert_eq!(result.err().unwrap().0, "BUDGET_EXHAUSTED");
}

#[test]
fn mixed_v2_v3_path_uses_v3_post_state_and_exact_output() {
    let v3: Pool =
        serde_json::from_str(include_str!("../../../test/contracts/v3-state.json")).unwrap();
    let mut v2: Pool =
        serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()
            .pools[0]
            .clone();
    v2.token0 = v3.token1.clone();
    v2.token1 = "C".into();
    let pools = vec![v3.clone(), v2];
    let graph = Graph::build(&pools).unwrap();
    let routes = search(
        &graph,
        &pools,
        &v3.token0,
        "C",
        &"10003".parse().unwrap(),
        Budget {
            split_parts: 20,
            max_hops: 3,
            max_candidates: 8,
            max_expansions: 100,
        },
    )
    .unwrap();
    assert_eq!(routes.candidates.len(), 1);
    assert_eq!(routes.candidates[0].edges.len(), 2);
    let mut state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    let actual = evaluate_path(
        &mut state,
        &routes.candidates[0].edges,
        &"10003".parse().unwrap(),
    )
    .unwrap();
    assert_eq!(actual, routes.candidates[0].amount_out);
    assert_ne!(
        state[&v3.pool_id].v3.as_ref().unwrap().sqrt_price_x96,
        v3.v3.unwrap().sqrt_price_x96
    );
}

#[test]
fn mixed_dex_version_path_keeps_each_adapter_and_fee() {
    let base: Pool =
        serde_json::from_str::<Snapshot>(include_str!("../../../test/fixtures/evm/baseline.json"))
            .unwrap()
            .pools[0]
            .clone();
    let mut pools = Vec::new();
    for (index, name) in ["pancake-v1", "pancake-v2", "sushiswap-v2"]
        .into_iter()
        .enumerate()
    {
        let profile = xone_domain::protocols::protocol(name).unwrap();
        let mut p = base.clone();
        p.protocol = name.into();
        p.adapter_version = profile.adapter.into();
        p.fee_numerator = profile.fee_numerator;
        p.fee_denominator = profile.fee_denominator;
        p.pool_id = format!("0x{:040x}", index + 1);
        p.token0 = format!("T{index}");
        p.token1 = format!("T{}", index + 1);
        p.v2 = Some(xone_domain::protocols::V2State {
            swap: None,
            factory: format!("0x{}", "44".repeat(20)),
            code_sha256: "aa".repeat(32),
        });
        pools.push(p);
    }
    let paths = search(
        &Graph::build(&pools).unwrap(),
        &pools,
        "T0",
        "T3",
        &"10000".parse().unwrap(),
        Budget {
            split_parts: 20,
            max_hops: 3,
            max_candidates: 8,
            max_expansions: 100,
        },
    )
    .unwrap();
    assert_eq!(paths.candidates.len(), 1);
    let path = &paths.candidates[0];
    assert_eq!(
        path.edges
            .iter()
            .map(|e| e.protocol.as_str())
            .collect::<Vec<_>>(),
        vec!["pancake-v1", "pancake-v2", "sushiswap-v2"]
    );
    let mut state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect();
    let out = evaluate_path(&mut state, &path.edges, &"10000".parse().unwrap()).unwrap();
    assert_eq!(out, path.amount_out);
    let mut corrupt = path.edges.clone();
    corrupt[0].protocol = "sushiswap-v2".into();
    assert!(evaluate_path(&mut state, &corrupt, &"10000".parse().unwrap()).is_err());
}
