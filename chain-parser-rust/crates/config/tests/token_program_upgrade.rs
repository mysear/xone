use serde_json::{json, Value};
use xone_config::*;

fn pool_value(kind: &str) -> Value {
    let raw = match kind {
        "raydium_cpmm" => include_str!("../../../test/contracts/raydium-state.json"),
        "raydium_clmm" => include_str!("../../../test/contracts/rayclmm-state.json"),
        _ => include_str!("../../../test/contracts/orca-state.json"),
    };
    let p: Value = serde_json::from_str(raw).unwrap();
    let s = &p[kind];
    let mut accounts = vec![
        p["pool_id"].clone(),
        s["vault0"].clone(),
        s["vault1"].clone(),
        s[if kind == "orca_whirlpool" {
            "whirlpools_config"
        } else {
            "amm_config"
        }]
        .clone(),
        p["token0"].clone(),
        p["token1"].clone(),
    ];
    if kind == "raydium_clmm" {
        accounts.push(s["observation"].clone());
    }
    let mut identity = json!({"expected_programdata":s["programdata"],"expected_program_code_sha256":s["program_code_sha256"],"expected_deployed_slot":s["deployed_slot"],"expected_token_program_code_sha256":s["token_program_code_sha256"]});
    if kind != "raydium_cpmm" {
        identity["tick_array_starts"] = if kind == "raydium_clmm" {
            json!([-3840, 0, 3840])
        } else {
            json!([-5632, 0, 5632])
        };
        identity["max_steps"] = json!(512);
    }
    let mut result = json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"fee_numerator":p["fee_numerator"],"fee_denominator":1000000,"program_id":s["program_id"],"accounts":accounts});
    result[kind] = identity;
    result
}

#[test]
fn token_upgrade_configuration_is_explicit_pinned_and_preserves_immutable_defaults() {
    for kind in ["raydium_cpmm", "raydium_clmm", "orca_whirlpool"] {
        let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
            .unwrap()
            .config;
        cfg.chains[0].confirmation_tag = "finalized".into();
        let legacy = pool_value(kind);
        let pool: PoolConfig = serde_json::from_value(legacy.clone()).unwrap();
        assert!(!serde_json::to_value(&pool).unwrap()[kind]
            .as_object()
            .unwrap()
            .contains_key("token_program_upgrade"));
        cfg.chains[0].pools = vec![pool];
        cfg.validate("chain-parser-rust").unwrap();
        let mut upgraded = legacy;
        upgraded[kind]["token_program_upgrade"] =
            json!({"programdata":CLASSIC_TOKEN_PROGRAMDATA,"deployed_slot":17});
        cfg.chains[0].pools = vec![serde_json::from_value(upgraded.clone()).unwrap()];
        cfg.validate("chain-parser-rust").unwrap();
        for invalid in [
            json!({"programdata":CLASSIC_TOKEN_PROGRAMDATA,"deployed_slot":0}),
            json!({"programdata":cfg.chains[0].pools[0].id,"deployed_slot":17}),
        ] {
            let mut changed = upgraded.clone();
            changed[kind]["token_program_upgrade"] = invalid;
            cfg.chains[0].pools = vec![serde_json::from_value(changed).unwrap()];
            assert!(
                cfg.validate("chain-parser-rust").is_err(),
                "accepted invalid token binding for {kind}"
            );
        }
        upgraded[kind]["token_program_upgrade"]["unreviewed_mode"] = json!(true);
        assert!(serde_json::from_value::<PoolConfig>(upgraded).is_err());
    }
}
