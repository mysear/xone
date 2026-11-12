use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_routing::{evaluate_path_with_funds, search, Budget, Funds, Graph};
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../test/contracts/meteora-mm.json")).unwrap()
}
#[test]
fn amount_specific_candidates_match_original_swaps_in_both_directions() {
    let f = data();
    let mut full = 0;
    for c in f["cases"].as_array().unwrap() {
        let s = &c["steps"][0];
        let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
        let input: Amount = s["input"].as_str().unwrap().parse().unwrap();
        let zero = s["zero_for_one"].as_bool().unwrap();
        let (a, b) = if zero {
            (&p.token0, &p.token1)
        } else {
            (&p.token1, &p.token0)
        };
        let g = Graph::build(std::slice::from_ref(&p)).unwrap();
        let q = search(
            &g,
            std::slice::from_ref(&p),
            a,
            b,
            &input,
            Budget {
                split_parts: 1,
                max_hops: 2,
                max_candidates: 4,
                max_expansions: 32,
            },
        )
        .unwrap();
        if s["passed"] == true {
            assert_eq!(q.candidates.len(), 1);
            assert_eq!(
                q.candidates[0].amount_out.to_string(),
                s["output"].as_str().unwrap()
            );
            full += 1;
        } else {
            assert!(q.candidates.is_empty());
        }
    }
    assert_eq!(full, 35);
}
#[test]
fn failed_path_is_atomic_and_pool_funding_aliases_are_rejected_case_sensitively() {
    let f = data();
    let s = &f["cases"][3]["steps"][0];
    let p: Pool = serde_json::from_value(s["pool"].clone()).unwrap();
    let g = Graph::build(std::slice::from_ref(&p)).unwrap();
    let path = &g.adjacency[&p.token0];
    let mut pools = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let before = pools.clone();
    let external = "6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n";
    for account in p.meteora.as_ref().unwrap().internal_accounts(&p).unwrap() {
        let result = evaluate_path_with_funds(
            &mut pools,
            path,
            &"1000000".parse().unwrap(),
            Funds {
                sender: external,
                recipient: &account,
            },
        );
        assert!(result.is_err());
        assert_eq!(pools, before);
    }
    assert!(evaluate_path_with_funds(
        &mut pools,
        path,
        &"1000000000000000".parse().unwrap(),
        Funds {
            sender: external,
            recipient: external
        }
    )
    .is_err());
    assert_eq!(pools, before);
}
#[test]
fn independent_pool_graph_rejects_overlapping_mutable_accounts() {
    let f = data();
    let mut p: Pool = serde_json::from_value(f["cases"][3]["steps"][0]["pool"].clone()).unwrap();
    let mut second = p.clone();
    second.pool_id = "6JdEP7ezaY73fdFvpsTFhtPnvNEBe2UP6r47gMajzs2n".into();
    assert_eq!(
        xone_domain::validate_pool_dependencies([&p, &second])
            .unwrap_err()
            .0,
        "METEORA_SHARED_MUTABLE_ACCOUNT"
    );
    p.meteora.as_mut().unwrap().max_steps = 1;
    let result = xone_protocol_math::quote_exact_in(&p, &"1000000".parse().unwrap(), true);
    assert_eq!(result.unwrap_err().0, "METEORA_STEP_BUDGET");
}
