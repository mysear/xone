use serde_json::{json, Value};
use xone_config::*;
fn configured() -> Value {
    let base = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    let mut value = serde_json::to_value(base).unwrap();
    value["simulation"] = json!({"backend":"revm-fixture","rpc_url":"","max_gas":15_000_000,"max_state_reads":4096,"timeout_ms":1000,"allowed_targets":["0x0000000000000000000000000000000000000001"],"state_file":"fixtures/evm-state.json","max_state_bytes":8_388_608,"max_instructions":2_000_000,"max_memory_bytes":67_108_864,"concurrency":2});
    value
}

fn priced() -> Value {
    let mut v = configured();
    let chain = v["chains"][0]["key"].clone();
    let genesis = format!("0x{}", "ab".repeat(32));
    v["chains"][0]["genesis_hash"] = genesis.clone().into();
    v["simulation"]["native_valuation"] = json!({"observations_file":"fixtures/prices.json","policies":[{"source_id":"configured-oracle","chain_key":chain,"genesis_hash":genesis,"token":"0x0000000000000000000000000000000000000010","signers":{"a":"11".repeat(32),"b":"22".repeat(32),"c":"33".repeat(32)},"threshold":2,"max_age_ms":60000,"max_validity_ms":60000}]});
    v
}
#[test]
fn oracle_authorities_are_optional_and_restricted_to_configured_local_evm_chains() {
    let base: Config = serde_json::from_value(configured()).unwrap();
    assert!(base.simulation.native_valuation.is_none());
    assert!(serde_json::to_value(&base).unwrap()["simulation"]
        .get("native_valuation")
        .is_none());
    let c: Config = serde_json::from_value(priced()).unwrap();
    c.validate("dex-router").unwrap();
    for field in [
        "backend",
        "genesis",
        "chain",
        "path",
        "duplicate",
        "keys",
        "single",
        "expiry",
        "unknown",
    ] {
        let mut v = priced();
        match field {
            "backend" => v["simulation"]["backend"] = "disabled".into(),
            "genesis" => {
                v["simulation"]["native_valuation"]["policies"][0]["genesis_hash"] =
                    format!("0x{}", "cd".repeat(32)).into()
            }
            "chain" => {
                v["simulation"]["native_valuation"]["policies"][0]["chain_key"] = "evm:999".into()
            }
            "path" => v["simulation"]["native_valuation"]["observations_file"] = "".into(),
            "duplicate" => {
                let p = v["simulation"]["native_valuation"]["policies"][0].clone();
                v["simulation"]["native_valuation"]["policies"]
                    .as_array_mut()
                    .unwrap()
                    .push(p);
            }
            "keys" => {
                v["simulation"]["native_valuation"]["policies"][0]["signers"]["b"] =
                    "11".repeat(32).into()
            }
            "single" => v["simulation"]["native_valuation"]["policies"][0]["threshold"] = 1.into(),
            "expiry" => {
                v["simulation"]["native_valuation"]["policies"][0]["max_age_ms"] = 3600001u64.into()
            }
            _ => v["simulation"]["native_valuation"]["trustCallerRates"] = true.into(),
        }
        let parsed = serde_json::from_value::<Config>(v);
        assert!(
            parsed.is_err() || parsed.unwrap().validate("dex-router").is_err(),
            "{field}"
        );
    }
}
#[test]
#[ignore = "requires current Go oracle configuration exports"]
fn current_go_oracle_authority_config_and_public_economic_hashes_match() {
    let directory =
        std::path::PathBuf::from(std::env::var("XONE_NATIVE_VALUATION_CONFIG_VECTORS").unwrap());
    let v: Value =
        serde_json::from_slice(&std::fs::read(directory.join("vectors.json")).unwrap()).unwrap();
    let rows = v["vectors"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let name = row["file"].as_str().unwrap();
        assert!(
            name.starts_with("config-")
                && name.ends_with(".json")
                && !name.contains('/')
                && !name.contains('\\')
        );
        let c = load(directory.join(name), "dex-router").unwrap();
        assert_eq!(c.public_hash, row["public_hash"].as_str().unwrap());
        assert_eq!(c.economic_hash, row["economic_hash"].as_str().unwrap());
    }
}

#[test]
fn candidate_execution_cost_config_is_opt_in_bounded_and_single_route_only() {
    let mut v = priced();
    assert!(v["simulation"].get("candidate_cost_max_plans").is_none());
    v["routing"]["max_splits"] = 1.into();
    v["simulation"]["candidate_cost_max_plans"] = 16.into();
    serde_json::from_value::<Config>(v.clone())
        .unwrap()
        .validate("dex-router")
        .unwrap();
    for mode in ["limit", "backend", "valuation", "splits"] {
        let mut bad = v.clone();
        match mode {
            "limit" => bad["simulation"]["candidate_cost_max_plans"] = 17.into(),
            "backend" => bad["simulation"]["backend"] = "disabled".into(),
            "valuation" => {
                bad["simulation"]
                    .as_object_mut()
                    .unwrap()
                    .remove("native_valuation");
            }
            _ => bad["routing"]["max_splits"] = 2.into(),
        }
        assert!(
            serde_json::from_value::<Config>(bad)
                .unwrap()
                .validate("dex-router")
                .is_err(),
            "{mode}"
        );
    }
}
