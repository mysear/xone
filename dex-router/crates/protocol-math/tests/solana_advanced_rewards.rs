use serde_json::{json, Value};
use xone_domain::orca::OrcaWhirlpoolState;

#[test]
fn initialized_rewards_are_typed_state_not_ignored_json() {
    let mut p: Value =
        serde_json::from_str(include_str!("../../../test/contracts/orca-state.json")).unwrap();
    let s = &mut p["orca_whirlpool"];
    s["reward_infos"] = json!(vec![
        json!({
            "mint":"11111111111111111111111111111111", "vault":"11111111111111111111111111111111",
            "extension": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
            "emissions_per_second_x64":"0", "growth_global_x64":"0"
        });
        3
    ]);
    let decoded = serde_json::from_value::<OrcaWhirlpoolState>(s.clone());
    assert!(
        decoded.is_ok(),
        "reward fields must survive typed decoding: {decoded:?}"
    );
    assert_eq!(
        serde_json::to_value(decoded.unwrap()).unwrap()["reward_infos"],
        s["reward_infos"]
    );
}

fn corpus(family: &str) -> Value {
    serde_json::from_str(match family {
        "orca" => include_str!("../../../test/contracts/solana-advanced/orca-rewards.json"),
        _ => include_str!("../../../test/contracts/solana-advanced/raydium-clmm-rewards.json"),
    })
    .unwrap()
}

#[test]
fn full_swaps_match_original_native_output_and_entire_poststate() {
    for family in ["orca", "raydium-clmm"] {
        let c = corpus(family);
        assert_eq!(c["reference"]["sbf_executed"], false);
        assert_eq!(c["reference"]["token_program_executed"], false);
        let mut successful_crossings = 0;
        for case in c["cases"]
            .as_array()
            .unwrap()
            .iter()
            .chain(c["sequences"].as_array().unwrap())
        {
            let p: xone_domain::Pool = serde_json::from_value(case["pool"].clone()).unwrap();
            let before = p.clone();
            let input = case["input"].as_str().unwrap().parse().unwrap();
            let left = case["zero_for_one"].as_bool().unwrap();
            let q = xone_protocol_math::quote_exact_in(&p, &input, left);
            assert_eq!(p, before, "input state mutated");
            if case["expected"]["reverted"] == true {
                assert!(
                    q.is_err(),
                    "{} {} expected native rejection",
                    family,
                    case["name"]
                );
                continue;
            }
            let q = q.unwrap_or_else(|e| panic!("{} {}: {e}", family, case["name"]));
            assert_eq!(
                q.amount_out.to_string(),
                case["expected"]["amount_out"].as_str().unwrap(),
                "{} {} output",
                family,
                case["name"]
            );
            assert_eq!(q.consumed_in, input);
            assert_eq!(q.remaining_in, xone_domain::Amount::zero());
            let actual = serde_json::to_value(&q.next_pool).unwrap();
            if actual != case["expected"]["next_pool"] {
                panic!(
                    "{} {} full poststate mismatch: {}",
                    family,
                    case["name"],
                    first_difference(&actual, &case["expected"]["next_pool"], "")
                );
            }
            let sk = if family == "orca" {
                "orca_whirlpool"
            } else {
                "raydium_clmm"
            };
            if actual[sk]["tick_arrays"] != case["pool"][sk]["tick_arrays"] {
                successful_crossings += 1;
            }
        }
        assert!(
            successful_crossings >= 2,
            "native corpus must exercise actual tick crossings"
        );
    }
}

#[test]
fn reward_kernels_match_native_overflow_cap_zero_liquidity_and_crossings() {
    for family in ["orca", "raydium-clmm"] {
        for case in corpus(family)["kernels"].as_array().unwrap() {
            let mut outside: Vec<xone_domain::Amount> =
                serde_json::from_value(case["outside"].clone()).unwrap();
            let rewards = if family == "orca" {
                let mut s = serde_json::from_value(case["pool"]["orca_whirlpool"].clone()).unwrap();
                xone_protocol_math::orca::update_rewards(&mut s).unwrap();
                xone_protocol_math::orca::cross_reward_growths(&s.reward_infos, &mut outside)
                    .unwrap();
                assert_eq!(s.reward_last_updated_timestamp, s.bank_unix_timestamp);
                serde_json::to_value(s.reward_infos).unwrap()
            } else {
                let mut s = serde_json::from_value(case["pool"]["raydium_clmm"].clone()).unwrap();
                xone_protocol_math::raydium_clmm::update_rewards(&mut s).unwrap();
                xone_protocol_math::raydium_clmm::cross_reward_growths(
                    &s.reward_infos,
                    &mut outside,
                )
                .unwrap();
                assert_eq!(s.recent_epoch, s.bank_epoch);
                serde_json::to_value(s.reward_infos).unwrap()
            };
            assert_eq!(
                rewards, case["expected"]["reward_infos"],
                "{} {} rewards",
                family, case["name"]
            );
            assert_eq!(
                serde_json::to_value(outside).unwrap(),
                case["expected"]["outside"],
                "{} {} crossing",
                family,
                case["name"]
            );
        }
    }
}

fn first_difference(left: &Value, right: &Value, path: &str) -> String {
    if let (Some(a), Some(b)) = (left.as_object(), right.as_object()) {
        for key in a.keys().chain(b.keys()) {
            if a.get(key) != b.get(key) {
                return first_difference(&left[key], &right[key], &format!("{path}/{key}"));
            }
        }
    }
    if let (Some(a), Some(b)) = (left.as_array(), right.as_array()) {
        for i in 0..a.len().max(b.len()) {
            if a.get(i) != b.get(i) {
                return first_difference(&left[i], &right[i], &format!("{path}/{i}"));
            }
        }
    }
    format!("{path}: production={left} native={right}")
}
