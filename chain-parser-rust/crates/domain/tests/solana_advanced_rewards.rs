use serde_json::{json, Value};
use xone_domain::{orca::OrcaWhirlpoolState, raydium_clmm::RaydiumCLMMState, Pool};
fn corpus(orca: bool) -> Value {
    serde_json::from_str(if orca {
        include_str!("../../../test/contracts/solana-advanced/orca-rewards.json")
    } else {
        include_str!("../../../test/contracts/solana-advanced/raydium-clmm-rewards.json")
    })
    .unwrap()
}
#[test]
fn all_reward_fields_and_tick_growth_survive_wire_roundtrip() {
    for orca in [true, false] {
        let c = corpus(orca);
        let p: Pool = serde_json::from_value(c["base_pool"].clone()).unwrap();
        p.validate().unwrap();
        if orca {
            let s = p.orca_whirlpool.unwrap();
            assert_eq!(
                OrcaWhirlpoolState::from_proto(s.to_proto().unwrap()).unwrap(),
                s
            );
        } else {
            let s = p.raydium_clmm.unwrap();
            assert_eq!(
                RaydiumCLMMState::from_proto(s.to_proto().unwrap()).unwrap(),
                s
            );
        }
    }
}
#[test]
fn legacy_profiles_reject_reward_state_and_new_profiles_require_complete_arrays() {
    for orca in [true, false] {
        let c = corpus(orca);
        let sk = if orca {
            "orca_whirlpool"
        } else {
            "raydium_clmm"
        };
        for change in [
            "legacy",
            "missing-rewards",
            "short-tick",
            "wide-growth",
            "uninitialized-growth",
            "unsupported-control",
        ] {
            let mut p = c["base_pool"].clone();
            match change {
                "legacy" => {
                    p["protocol"] = json!(if orca {
                        "orca-whirlpools-static"
                    } else {
                        "raydium-clmm-static"
                    });
                    p["adapter_version"] = json!(if orca {
                        "orca-whirlpools/cc683ec9-spl-static-1"
                    } else {
                        "raydium-clmm/6d7a0f82-spl-static-1"
                    });
                }
                "missing-rewards" => {
                    p[sk].as_object_mut().unwrap().remove("reward_infos");
                }
                "short-tick" => {
                    p[sk]["tick_arrays"][0]["ticks"][0]["reward_growths_outside"] = json!(["1"])
                }
                "wide-growth" => {
                    p[sk]["tick_arrays"][0]["ticks"][0]["reward_growths_outside"][0] =
                        json!("340282366920938463463374607431768211456")
                }
                "uninitialized-growth" => {
                    p[sk]["tick_arrays"][0]["ticks"][0]["reward_growths_outside"][2] = json!("1")
                }
                _ if orca => p[sk]["reward_infos"][1]["extension"][0] = json!(1),
                _ => p[sk]["reward_infos"][0]["reward_state"] = json!(4),
            }
            let p: Pool = serde_json::from_value(p).unwrap();
            assert!(
                p.validate().is_err(),
                "accepted unsupported state: {orca} {change}"
            );
        }
    }
}
#[test]
fn empty_advanced_fields_remain_omitted_on_legacy_state() {
    for orca in [true, false] {
        let mut p = corpus(orca)["base_pool"].clone();
        let sk = if orca {
            "orca_whirlpool"
        } else {
            "raydium_clmm"
        };
        p["protocol"] = json!(if orca {
            "orca-whirlpools-static"
        } else {
            "raydium-clmm-static"
        });
        p["adapter_version"] = json!(if orca {
            "orca-whirlpools/cc683ec9-spl-static-1"
        } else {
            "raydium-clmm/6d7a0f82-spl-static-1"
        });
        p[sk].as_object_mut().unwrap().remove("reward_infos");
        for a in p[sk]["tick_arrays"].as_array_mut().unwrap() {
            for t in a["ticks"].as_array_mut().unwrap() {
                t.as_object_mut().unwrap().remove("reward_growths_outside");
            }
        }
        let typed: Pool = serde_json::from_value(p.clone()).unwrap();
        typed.validate().unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), p);
    }
}
