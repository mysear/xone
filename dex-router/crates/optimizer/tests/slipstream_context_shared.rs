use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation_with_funds, AllocatedRoute};
use xone_routing::{evaluate_path_with_funds, Edge, Funds, PathQuote};
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/slipstream-context-state.json"
    ))
    .unwrap()
}
fn path(p: &Pool, zero: bool, input: Amount) -> PathQuote {
    PathQuote {
        amount_in: input,
        amount_out: Amount::zero(),
        tokens: if zero {
            vec![p.token0.clone(), p.token1.clone()]
        } else {
            vec![p.token1.clone(), p.token0.clone()]
        },
        edges: vec![Edge {
            from_token: None,
            protocol: p.protocol.clone(),
            pool_id: p.pool_id.clone(),
            zero_for_one: zero,
            to: if zero {
                p.token1.clone()
            } else {
                p.token0.clone()
            },
        }],
    }
}
#[test]
fn ordered_shared_allocations_use_actual_new_fee_after_each_original_oracle_write() {
    let file = data();
    for sequence in file["sequences"].as_array().unwrap() {
        let index = sequence["initial_case_index"].as_u64().unwrap() as usize;
        let p: Pool = serde_json::from_value(file["cases"][index]["pool"].clone()).unwrap();
        let origin = p.slipstream_context.as_ref().unwrap().view_origin.clone();
        let recipient = "0x000000000000000000000000000000000000beef";
        let funds = Funds {
            sender: &origin,
            recipient,
        };
        let zero = file["cases"][index]["zero_for_one"].as_bool().unwrap();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let mut routes = vec![];
        let mut gross = Amount::zero();
        for step in sequence["steps"].as_array().unwrap() {
            let input: Amount = step["input"].as_str().unwrap().parse().unwrap();
            let route = path(&p, zero, input.clone());
            let actual = evaluate_path_with_funds(&mut state, &route.edges, &input, funds).unwrap();
            assert_eq!(actual.to_string(), step["amount_out"].as_str().unwrap());
            assert_eq!(
                serde_json::to_value(&state[&p.pool_id]).unwrap(),
                step["expected_pool"]
            );
            gross = Amount::checked(gross.0 + &actual.0).unwrap();
            routes.push(AllocatedRoute {
                path: route,
                amount_in: input,
                amount_out: Amount::zero(),
            });
        }
        let result = evaluate_allocation_with_funds(&[p], &routes, &Amount::zero(), funds).unwrap();
        assert_eq!(result.gross_buy_amount, gross);
        assert_eq!(result.refund, Amount::zero());
    }
}
#[test]
fn contextual_failures_and_foreign_pool_wallets_never_install_partial_state() {
    let file = data();
    let case = file["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["mode"] == "dynamic")
        .unwrap();
    let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
    let origin = p.slipstream_context.as_ref().unwrap().view_origin.clone();
    let input: Amount = "1000000".parse().unwrap();
    let route = path(&p, true, input.clone());
    let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let initial = state.clone();
    for recipient in [
        p.pool_id.clone(),
        p.pool_id.to_uppercase().replacen("0X", "0x", 1),
    ] {
        assert_eq!(
            evaluate_path_with_funds(
                &mut state,
                &route.edges,
                &input,
                Funds {
                    sender: &origin,
                    recipient: &recipient
                }
            )
            .err()
            .unwrap()
            .0,
            "SLIPSTREAM_UNMODELED_SELF_FUNDING"
        );
        assert_eq!(state, initial);
    }
    let unknown = "0x0000000000000000000000000000000000000001";
    assert_eq!(
        evaluate_path_with_funds(
            &mut state,
            &route.edges,
            &input,
            Funds {
                sender: unknown,
                recipient: "0x000000000000000000000000000000000000beef"
            }
        )
        .err()
        .unwrap()
        .0,
        "SLIPSTREAM_ORIGIN_SCOPE_UNAVAILABLE"
    );
    assert_eq!(state, initial);
}

#[test]
fn contextual_output_token_must_match_pool_direction_without_mutation() {
    let f = data();
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["mode"] == "dynamic")
        .unwrap();
    let p: Pool = serde_json::from_value(case["pool"].clone()).unwrap();
    let origin = p.slipstream_context.as_ref().unwrap().view_origin.clone();
    let funds = Funds {
        sender: &origin,
        recipient: "0x000000000000000000000000000000000000beef",
    };
    let input: Amount = "1000000".parse().unwrap();
    let original = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    for zero in [true, false] {
        let valid = path(&p, zero, input.clone());
        let mut good_state = original.clone();
        assert!(evaluate_path_with_funds(&mut good_state, &valid.edges, &input, funds).is_ok());
        for output in [
            valid.tokens[0].as_str(),
            "0x000000000000000000000000000000000000cafe",
        ] {
            let mut bad = valid.clone();
            bad.edges[0].to = output.to_owned();
            let mut state = original.clone();
            let error =
                evaluate_path_with_funds(&mut state, &bad.edges, &input, funds).unwrap_err();
            assert_eq!(error.0, "PATH_OUTPUT_TOKEN_MISMATCH");
            assert_eq!(state, original);
        }
    }
}
