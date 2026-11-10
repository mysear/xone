use serde_json::Value;
use std::collections::BTreeMap;
use xone_domain::{Amount, Pool};
use xone_optimizer::{evaluate_allocation_with_funds, AllocatedRoute};
use xone_routing::{evaluate_path_with_funds, Edge, Funds, PathQuote};
fn data() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-lm-state.json"
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
fn shared_lm_routes_carry_original_rewards_fee_and_oracle_through_every_step() {
    let f = data();
    let funds = Funds {
        sender: "0x0000000000000000000000000000000000001111",
        recipient: "0x000000000000000000000000000000000000beef",
    };
    for seq in f["sequences"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(seq["pool"].clone()).unwrap();
        let mut state = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
        let (mut routes, mut gross) = (Vec::new(), Amount::zero());
        for (index, step) in seq["steps"].as_array().unwrap().iter().enumerate() {
            let input: Amount = serde_json::from_value(step["input"].clone()).unwrap();
            let route = path(&p, step["zero_for_one"].as_bool().unwrap(), input.clone());
            let out = evaluate_path_with_funds(&mut state, &route.edges, &input, funds).unwrap();
            assert_eq!(out.to_string(), step["amount_out"].as_str().unwrap());
            let expected: Pool = serde_json::from_value(step["expected_pool"].clone()).unwrap();
            assert_eq!(state[&p.pool_id], expected);
            if index < 2 {
                gross = Amount::checked(gross.0 + &out.0).unwrap();
                routes.push(AllocatedRoute {
                    path: route,
                    amount_in: input,
                    amount_out: Amount::zero(),
                });
            }
        }
        let result = evaluate_allocation_with_funds(&[p], &routes, &Amount::zero(), funds).unwrap();
        assert_eq!(result.gross_buy_amount, gross);
        assert_eq!(result.refund, Amount::zero());
    }
}
#[test]
fn lm_internal_wallet_alias_and_failed_amount_leave_all_pool_state_unchanged() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][2]["pool"].clone()).unwrap();
    let s = p.pancake_lm.as_ref().unwrap();
    let input: Amount = "1000000000".parse().unwrap();
    let route = path(&p, true, input.clone());
    let initial = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let mut state = initial.clone();
    for recipient in [
        &p.pool_id,
        &s.lm.address,
        &s.master_chef.address,
        &s.core.factory,
    ] {
        let error = evaluate_path_with_funds(
            &mut state,
            &route.edges,
            &input,
            Funds {
                sender: "0x0000000000000000000000000000000000001111",
                recipient,
            },
        )
        .unwrap_err();
        assert!(error.0.contains("PANCAKE_LM_UNMODELED_SELF_FUNDING"));
        assert_eq!(state, initial);
    }
    let huge: Amount = "1000000000000000000000000000".parse().unwrap();
    assert!(evaluate_path_with_funds(
        &mut state,
        &route.edges,
        &huge,
        Funds {
            sender: "0x0000000000000000000000000000000000001111",
            recipient: "0x000000000000000000000000000000000000beef"
        }
    )
    .is_err());
    assert_eq!(state, initial);
}

#[test]
fn lm_output_token_must_match_pool_direction_without_mutation() {
    let f = data();
    let p: Pool = serde_json::from_value(f["cases"][2]["pool"].clone()).unwrap();
    let input: Amount = "1000000000".parse().unwrap();
    let original = BTreeMap::from([(p.pool_id.clone(), p.clone())]);
    let funds = Funds {
        sender: "0x0000000000000000000000000000000000001111",
        recipient: "0x000000000000000000000000000000000000beef",
    };
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
