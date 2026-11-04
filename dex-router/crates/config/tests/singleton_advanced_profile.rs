use xone_config::*;
#[test]
fn advanced_config_has_the_same_callback_capability_guards_as_runtime() {
    let mut cfg = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    let doc: serde_json::Value = serde_json::from_str(include_str!(
        "../../../test/contracts/five-family-evm-wire.json"
    ))
    .unwrap();
    let pool = doc["snapshot"]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["protocol"] == "uniswap-v4-advanced")
        .unwrap();
    let mut state: xone_domain::singleton::SingletonAdvancedState =
        serde_json::from_value(pool["singleton_advanced"].clone()).unwrap();
    let configure = |state: &xone_domain::singleton::SingletonAdvancedState| -> PoolConfig {
        serde_json::from_value(serde_json::json!({"id":format!("{}/{}",state.snapshot.manager,state.snapshot.pool_id),"protocol":"uniswap-v4-advanced","token0":state.currency0,"token1":state.currency1,"fee_numerator":state.key_fee,"fee_denominator":1000000,"program_id":"","accounts":[],"singleton_advanced":{"protocol":state.protocol,"manager":state.snapshot.manager,"expected_manager_code_sha256":state.snapshot.manager_code_sha256,"pool_id":state.snapshot.pool_id,"currency0":state.currency0,"currency1":state.currency1,"key_fee":state.key_fee,"tick_spacing":state.snapshot.core.tick_spacing,"hooks":state.snapshot.hooks,"parameters":state.snapshot.parameters,"tick_word_radius":1,"max_initialized_ticks":32,"max_steps":512}})).unwrap()
    };
    cfg.chains[0].pools = vec![configure(&state)];
    cfg.validate("dex-router").unwrap();
    state.snapshot.hooks = "0x0000000000000000000000000000000000000040".into();
    state.snapshot.pool_id = state.key_id().unwrap();
    cfg.chains[0].pools = vec![configure(&state)];
    assert!(
        cfg.validate("dex-router").is_err(),
        "swap callback cannot be admitted by config"
    );
}
