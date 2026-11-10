#[test]
fn bsc4_config_requires_three_separate_pinned_predecessors() {
    let load_role = "chain-parser-rust";
    let role = "chain-parser-go";
    let mut cfg = xone_config::load("../../config/config-test.yaml", load_role)
        .unwrap()
        .config;
    cfg.service.name = role.into();
    cfg.state.engine = "pebble".into();
    cfg.chains[0].key = "evm:31337".into();
    cfg.chains[0].owner_service = role.into();
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/pancake-bsc4-state.json"
    ))
    .unwrap();
    for c in f["cases"].as_array().unwrap() {
        cfg.chains[0].pools = vec![serde_json::from_value(c["config"].clone()).unwrap()];
        cfg.validate(role).unwrap();
    }
    let source = cfg.chains[0].pools[0].clone();
    for mode in 0..6 {
        let mut bad = source.clone();
        match mode {
            0 => bad.pancake_bsc_lm = None,
            1 => {
                bad.pancake_bsc_lm.as_mut().unwrap().predecessors.pop();
            }
            2 => {
                let s = bad.pancake_bsc_lm.as_mut().unwrap();
                s.predecessors[1].address = s.predecessors[0].address.clone();
            }
            3 => bad.pancake_bsc_lm.as_mut().unwrap().predecessors[0]
                .code_sha256
                .clear(),
            4 => bad.protocol = "pancake-v3-lm-context".into(),
            _ => bad.pancake_bsc_lm.as_mut().unwrap().predecessors[0].address = bad.token0.clone(),
        };
        cfg.chains[0].pools = vec![bad];
        assert!(cfg.validate(role).is_err(), "{mode}");
    }
}
