use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{meteora, Amount, Pool};
use xone_routing::exact_out::{evaluate_path, search};
use xone_routing::{Budget, Edge, Funds, Graph, SearchExecution};
const SENDER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
const RECIPIENT: &str = "4pKpaEbYfLmCba11Zz9o1V2Ter2FR1mdoeaQ1abjqXxE";
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/meteora-exact-out.json"
    ))
    .unwrap()
}
fn edge(p: &Pool, zero: bool) -> Edge {
    Edge {
        from_token: None,
        protocol: p.protocol.clone(),
        pool_id: p.pool_id.clone(),
        zero_for_one: zero,
        to: if zero {
            p.token1.clone()
        } else {
            p.token0.clone()
        },
    }
}
#[test]
fn meteora_output_paths_and_shared_sequences_match_original_complete_state() {
    let f = data();
    let (mut full, mut rejected) = (0, 0);
    for c in f["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["steps"][0]["pool"].clone()).unwrap();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p)]);
        for s in c["steps"].as_array().unwrap() {
            let p = state.values().next().unwrap();
            let e = edge(p, s["zero_for_one"].as_bool().unwrap());
            let before = state.clone();
            let target: Amount = s["target"].as_str().unwrap().parse().unwrap();
            let q = evaluate_path(
                &mut state,
                &[e],
                &target,
                Funds {
                    sender: SENDER,
                    recipient: RECIPIENT,
                },
            );
            if s["passed"] == false {
                assert!(q.is_err());
                assert_eq!(state, before);
                rejected += 1;
            } else {
                let q = q.unwrap_or_else(|e| panic!("case {} {e}", c["index"]));
                assert_eq!(q.amount_out, target);
                assert!(q.target_output.is_none());
                assert_eq!(q.factory, meteora::PROGRAM);
                assert_eq!(
                    q.amount_in.to_string(),
                    s["required_input"].as_str().unwrap()
                );
                let mut expected: Pool = serde_json::from_value(s["after"].clone()).unwrap();
                expected.meteora.as_mut().unwrap().derived = true;
                assert!(
                    state[&expected.pool_id] == expected,
                    "case {} after",
                    c["index"]
                );
                full += 1;
            }
        }
    }
    assert_eq!((full, rejected), (162, 22));
}
#[test]
fn meteora_exact_out_search_is_case_sensitive_and_rejects_internal_funds_and_multihop() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][8]["steps"][0]["pool"].clone()).unwrap();
    let e = edge(&p, true);
    let state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    for account in p.meteora.as_ref().unwrap().internal_accounts(&p).unwrap() {
        let mut candidate = state.clone();
        assert_eq!(
            evaluate_path(
                &mut candidate,
                std::slice::from_ref(&e),
                &"3".parse().unwrap(),
                Funds {
                    sender: SENDER,
                    recipient: &account
                }
            )
            .unwrap_err()
            .0,
            "EXACT_OUT_UNMODELED_SELF_FUNDING"
        );
        assert_eq!(state, candidate);
    }
    let distinct = p
        .meteora
        .as_ref()
        .unwrap()
        .internal_accounts(&p)
        .unwrap()
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .find(|s| {
            xone_domain::raydium::validate_pubkey(s).is_ok()
                && !p
                    .meteora
                    .as_ref()
                    .unwrap()
                    .internal_accounts(&p)
                    .unwrap()
                    .contains(s)
        })
        .unwrap();
    let mut candidate = state.clone();
    evaluate_path(
        &mut candidate,
        std::slice::from_ref(&e),
        &"3".parse().unwrap(),
        Funds {
            sender: SENDER,
            recipient: &distinct,
        },
    )
    .unwrap();
    let mut multi = vec![e.clone(), e.clone()];
    multi[1].pool_id = RECIPIENT.into();
    let mut candidate = state.clone();
    assert!(evaluate_path(
        &mut candidate,
        &multi,
        &"3".parse().unwrap(),
        Funds {
            sender: SENDER,
            recipient: RECIPIENT
        }
    )
    .is_err());
    assert_eq!(candidate, state);
    let paths = search(
        &Graph::build(std::slice::from_ref(&p)).unwrap(),
        std::slice::from_ref(&p),
        &p.token0,
        &p.token1,
        &"1000000".parse().unwrap(),
        Budget {
            split_parts: 1,
            max_hops: 3,
            max_candidates: 8,
            max_expansions: 64,
        },
        SearchExecution {
            funds: Some(Funds {
                sender: SENDER,
                recipient: RECIPIENT,
            }),
            control: None,
        },
    )
    .unwrap();
    assert_eq!(paths.candidates.len(), 1);
    assert_eq!(paths.candidates[0].edges.len(), 1);
}
