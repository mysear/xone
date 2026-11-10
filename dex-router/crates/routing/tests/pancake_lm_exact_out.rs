use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use xone_domain::{Amount, Pool};
use xone_routing::{Budget, Edge, Funds, Graph, SearchControl, SearchExecution};
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-lm-exact-out.json"
    ))
    .unwrap()
}
fn edge(p: &Pool, zero: bool) -> Edge {
    Edge {
        pool_id: p.pool_id.clone(),
        protocol: p.protocol.clone(),
        zero_for_one: zero,
        to: if zero {
            p.token1.clone()
        } else {
            p.token0.clone()
        },
        from_token: None,
    }
}
#[test]
fn legacy_lm_exact_output_routes_match_original_sequences_and_full_ledger_transitions() {
    let f = data();
    let funds = Funds {
        sender: f["deployment"]["caller"]["address"].as_str().unwrap(),
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for seq in f["sequences"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p)]);
        for step in seq["steps"].as_array().unwrap() {
            let p = state.values().next().unwrap();
            let e = edge(p, step["zero_for_one"].as_bool().unwrap());
            let target: Amount = step["target_output"].as_str().unwrap().parse().unwrap();
            let q =
                xone_routing::exact_out::evaluate_path(&mut state, &[e], &target, funds).unwrap();
            assert_eq!(q.amount_out, target);
            assert_eq!(
                q.amount_in.to_string(),
                step["consumed_in"].as_str().unwrap()
            );
            assert_eq!(
                serde_json::to_value(state.values().next().unwrap()).unwrap(),
                step["expected_pool"]
            );
        }
    }
}
#[test]
fn legacy_lm_exact_output_search_rejects_internal_funds_partial_targets_and_cancellation() {
    let f = data();
    let c = &f["cases"][0];
    let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
    let pools = vec![p.clone()];
    let graph = Graph::build(&pools).unwrap();
    let target: Amount = "1000000000".parse().unwrap();
    let caller = f["deployment"]["caller"]["address"].as_str().unwrap();
    let funds = Funds {
        sender: caller,
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for zero in [true, false] {
        let e = edge(&p, zero);
        let (sell, buy) = if zero {
            (&p.token0, &p.token1)
        } else {
            (&p.token1, &p.token0)
        };
        let budget = Budget {
            max_hops: 3,
            max_candidates: 8,
            max_expansions: 100,
            split_parts: 4,
        };
        let q = xone_routing::exact_out::search(
            &graph,
            &pools,
            sell,
            buy,
            &target,
            budget,
            SearchExecution {
                control: None,
                funds: Some(funds),
            },
        )
        .unwrap();
        assert_eq!(q.candidates.len(), 1);
        assert_eq!(q.candidates[0].amount_out, target);
        assert!(q.excluded_pools.is_empty());
        let base = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        for recipient in [
            &p.pool_id,
            &p.pancake_lm.as_ref().unwrap().lm.address,
            &p.pancake_lm.as_ref().unwrap().core.factory,
        ] {
            let mut state = base.clone();
            assert!(xone_routing::exact_out::evaluate_path(
                &mut state,
                std::slice::from_ref(&e),
                &target,
                Funds {
                    sender: caller,
                    recipient
                }
            )
            .is_err());
            assert_eq!(state, base);
        }
        let mut state = base.clone();
        assert!(xone_routing::exact_out::evaluate_path(
            &mut state,
            &[e.clone(), edge(&p, !zero)],
            &target,
            funds
        )
        .is_err());
        assert_eq!(state, base);
        let cancel = AtomicBool::new(true);
        assert_eq!(
            xone_routing::exact_out::search(
                &graph,
                &pools,
                sell,
                buy,
                &target,
                budget,
                SearchExecution {
                    control: Some(SearchControl {
                        cancel: &cancel,
                        deadline: Instant::now() + Duration::from_secs(1)
                    }),
                    funds: Some(funds)
                }
            )
            .unwrap_err()
            .0,
            "BUDGET_EXHAUSTED"
        );
    }
    let partial: Amount = "1000000000000".parse().unwrap();
    let q = xone_routing::exact_out::search(
        &graph,
        &pools,
        &p.token0,
        &p.token1,
        &partial,
        Budget {
            max_hops: 1,
            max_candidates: 8,
            max_expansions: 100,
            split_parts: 1,
        },
        SearchExecution {
            control: None,
            funds: Some(funds),
        },
    )
    .unwrap();
    assert!(q.candidates.is_empty());
}
