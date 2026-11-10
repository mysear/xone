use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{Budget, Edge, Funds, Graph, SearchExecution};
fn data() -> Value {
    serde_json::from_str(include_str!("../../../test/contracts/lfj-exact-out.json")).unwrap()
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
fn lfj_routes_separate_target_actual_and_surplus_without_clamping_state() {
    let f = data();
    let funds = Funds {
        sender: f["deployment"]["sender"].as_str().unwrap(),
        recipient: f["deployment"]["recipient"].as_str().unwrap(),
    };
    let mut extra = 0;
    for c in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["full_target"] == true)
    {
        let p: Pool = serde_json::from_value(c["pool"].clone()).unwrap();
        let target: Amount = c["target_output"].as_str().unwrap().parse().unwrap();
        let e = edge(&p, c["zero_for_one"].as_bool().unwrap());
        let mut state = BTreeMap::from([(p.pool_id.clone(), p)]);
        let q = xone_routing::exact_out::evaluate_path(&mut state, &[e], &target, funds).unwrap();
        assert_eq!(q.output_target(), &target);
        assert_eq!(
            q.amount_out.to_string(),
            c["actual_output"].as_str().unwrap()
        );
        assert_eq!(q.amounts.last(), Some(&q.amount_out));
        assert_eq!(
            serde_json::to_value(state.values().next().unwrap()).unwrap(),
            c["actual_pool"]
        );
        if c["output_surplus"] != "0" {
            assert_eq!(q.target_output, Some(target));
            extra += 1;
        } else {
            assert!(q.target_output.is_none());
            assert!(serde_json::to_value(q)
                .unwrap()
                .get("target_output")
                .is_none());
        }
    }
    assert_eq!(extra, 60);
}
#[test]
fn lfj_target_search_keeps_funded_minimums_and_rejects_internal_or_repeated_paths() {
    let f = data();
    let p: Pool = serde_json::from_value(
        f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["original"]["active_offset"] == 500 && c["zero_for_one"] == true)
            .unwrap()["pool"]
            .clone(),
    )
    .unwrap();
    let pools = vec![p.clone()];
    let graph = Graph::build(&pools).unwrap();
    let target: Amount = "7".parse().unwrap();
    let funds = Funds {
        sender: f["deployment"]["sender"].as_str().unwrap(),
        recipient: f["deployment"]["recipient"].as_str().unwrap(),
    };
    let q = xone_routing::exact_out::search(
        &graph,
        &pools,
        &p.token0,
        &p.token1,
        &target,
        Budget {
            max_hops: 3,
            max_candidates: 8,
            max_expansions: 100,
            split_parts: 4,
        },
        SearchExecution {
            control: None,
            funds: Some(funds),
        },
    )
    .unwrap();
    assert_eq!(q.candidates.len(), 1);
    assert_eq!(q.candidates[0].output_target(), &target);
    assert_eq!(q.candidates[0].amount_out.to_string(), "8");
    let base = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    for recipient in [
        &p.pool_id,
        &p.lfj.as_ref().unwrap().factory,
        &p.lfj.as_ref().unwrap().implementation,
    ] {
        let mut state = base.clone();
        assert!(xone_routing::exact_out::evaluate_path(
            &mut state,
            &[edge(&p, true)],
            &target,
            Funds {
                sender: funds.sender,
                recipient
            }
        )
        .is_err());
        assert_eq!(state, base);
    }
    let mut state = base.clone();
    assert!(xone_routing::exact_out::evaluate_path(
        &mut state,
        &[edge(&p, true), edge(&p, false)],
        &target,
        funds
    )
    .is_err());
    assert_eq!(state, base);
}
