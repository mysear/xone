use xone_config::*;
#[test]
fn single_file_is_explicit_and_head_first() {
    let cfg = load("../../config/config-test.yaml", "chain-parser-rust").unwrap();
    assert_eq!(cfg.config.startup.mode, "HEAD_FIRST");
    assert!(!cfg.public_hash.is_empty());
}
#[test]
fn duplicate_alias_and_env_rejected() {
    for raw in ["a: 1\na: 2\n", "a: &x [1]\nb: *x\n", "a: ${HOME}\n"] {
        assert!(decode_document(raw.as_bytes()).is_err(), "{raw}");
    }
}

#[test]
fn single_file_without_development_credentials() {
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("../../config/config-test.yaml").unwrap())
            .unwrap();
    value["service"]["mode"] = "rpc".into();
    value["api"].as_object_mut().unwrap().remove("auth_token");
    value.as_object_mut().unwrap().remove("routing");
    value.as_object_mut().unwrap().remove("simulation");
    for chain in value["chains"].as_array_mut().unwrap() {
        chain.as_object_mut().unwrap().remove("fixture_path");
        chain["rpc_urls"] = serde_json::json!(["http://127.0.0.1:8899"]);
    }
    let dir = std::env::temp_dir().join(format!("xone-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config-dev.yaml");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = load(&path, "chain-parser-rust");
    std::fs::remove_dir_all(dir).unwrap();
    let config = result.unwrap();
    assert!(config.token.is_empty());
}

#[test]
fn explicit_auth_and_removed_debug_config() {
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("../../config/config-test.yaml").unwrap())
            .unwrap();
    let service = value["service"]["name"].as_str().unwrap().to_owned();
    let dir = std::env::temp_dir().join(format!("xone-auth-config-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config-test.yaml");
    let write =
        |doc: &serde_json::Value| std::fs::write(&path, serde_json::to_vec(doc).unwrap()).unwrap();
    write(&value);
    let without = load(&path, &service).unwrap();
    value["api"]["auth_token"] = "explicit-business-api-token-123456".into();
    value["journal"]["source_auth_tokens"] =
        serde_json::json!({"http://127.0.0.1:8091":"upstream-only-token-1234567890"});
    write(&value);
    let with = load(&path, &service).unwrap();
    assert_eq!(without.public_hash, with.public_hash);
    assert_eq!(without.economic_hash, with.economic_hash);
    assert_eq!(with.token, "explicit-business-api-token-123456");
    value["service"]["mode"] = "fixture".into();
    write(&value);
    assert!(load(&path, &service).is_err());
    value["service"]["mode"] = "rpc".into();
    value["chains"][0]["fixture_path"] = "sample.json".into();
    write(&value);
    assert!(load(&path, &service).is_err());
    write(&serde_json::json!({"schema_version":1,"release_id":"old","files":[]}));
    assert!(load(&path, &service).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn optional_v3_factory_digest_is_validated_for_every_profile() {
    let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    cfg.chains[0].key = "evm:31337".into();
    cfg.service.name = "chain-parser-go".into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].owner_service = "chain-parser-go".into();
    let p = serde_json::json!({"id":format!("0x{}","3".repeat(40)),"protocol":"uniswap-v3","token0":format!("0x{}","1".repeat(40)),"token1":format!("0x{}","2".repeat(40)),"fee_numerator":3000,"fee_denominator":1000000,"program_id":"","accounts":[],"v3":{"expected_factory":format!("0x{}","4".repeat(40)),"expected_code_sha256":"a".repeat(64),"tick_word_radius":1,"max_initialized_ticks":32,"max_steps":512}});
    for name in ["uniswap-v3", "pancake-v3"] {
        for digest in [
            String::new(),
            "b".repeat(64),
            "aa".into(),
            "g".repeat(64),
            "B".repeat(64),
        ] {
            let mut value = p.clone();
            value["protocol"] = name.into();
            value["v3"]["expected_factory_code_sha256"] = digest.clone().into();
            cfg.chains[0].pools = vec![serde_json::from_value(value).unwrap()];
            assert_eq!(
                cfg.validate("chain-parser-go").is_ok(),
                digest.is_empty() || digest == "b".repeat(64),
                "{name} digest {digest}"
            );
        }
    }
}

#[test]
fn singleton_configuration_binds_manager_key_and_budget() {
    let mut cfg = load("../../config/config-test.yaml", "chain-parser-rust")
        .unwrap()
        .config;
    cfg.service.name = "chain-parser-go".into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].key = "evm:31337".into();
    cfg.chains[0].owner_service = "chain-parser-go".into();
    let file: serde_json::Value =
        serde_json::from_str(include_str!("../../../test/contracts/singleton-state.json")).unwrap();
    for p in file["pools"].as_array().unwrap() {
        let s = &p["singleton_cl"];
        let value = serde_json::json!({"id":p["pool_id"],"protocol":p["protocol"],"token0":p["token0"],"token1":p["token1"],"fee_numerator":p["fee_numerator"],"fee_denominator":1000000,"program_id":"","accounts":[],"singleton_cl":{"manager":s["manager"],"expected_manager_code_sha256":s["manager_code_sha256"],"pool_id":s["pool_id"],"lp_fee":s["lp_fee"],"tick_spacing":s["core"]["tick_spacing"],"parameters":s["parameters"],"tick_word_radius":1,"max_initialized_ticks":32,"max_steps":512}});
        cfg.chains[0].pools = vec![serde_json::from_value(value.clone()).unwrap()];
        cfg.validate("chain-parser-go").unwrap();
        let mut wrong = value;
        wrong["singleton_cl"]["pool_id"] = "0xdead".into();
        cfg.chains[0].pools = vec![serde_json::from_value(wrong).unwrap()];
        assert!(cfg.validate("chain-parser-go").is_err());
    }
}
