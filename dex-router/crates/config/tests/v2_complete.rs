use serde_json::{json, Value};
use xone_config::{load, Config};
fn configured() -> Value {
    let base = load("../../config/config-test.yaml", "dex-router")
        .unwrap()
        .config;
    let mut v = serde_json::to_value(base).unwrap();
    v["chains"][0]["pools"] = json!([{"id":format!("0x{}","1".repeat(40)),"protocol":"uniswap-v2-complete","token0":format!("0x{}","2".repeat(40)),"token1":format!("0x{}","3".repeat(40)),"fee_numerator":3,"fee_denominator":1000,"program_id":"","accounts":[],"v2":{"expected_factory":format!("0x{}","4".repeat(40)),"expected_code_sha256":xone_domain::v2_swap::PAIR_CODE_SHA256}}]);
    v
}
#[test]
fn complete_v2_configuration_pins_exact_layout_and_canonical_identity() {
    let c: Config = serde_json::from_value(configured()).unwrap();
    c.validate("dex-router").unwrap();
    for bad in 0..8 {
        let mut v = configured();
        let p = &mut v["chains"][0]["pools"][0];
        match bad {
            0 => p["v2"]["expected_code_sha256"] = "a".repeat(64).into(),
            1 => p["v2"] = Value::Null,
            2 => p["v2"]["expected_factory"] = format!("0x{}", "0".repeat(40)).into(),
            3 => p["token0"] = format!("0x{}", "0".repeat(40)).into(),
            4 => {
                let t = p["token0"].clone();
                p["token0"] = p["token1"].clone();
                p["token1"] = t;
            }
            5 => p["program_id"] = "unexpected-solana-program".into(),
            6 => p["accounts"] = json!(["unused"]),
            _ => p["fee_numerator"] = 4.into(),
        };
        let c: Config = serde_json::from_value(v).unwrap();
        assert!(c.validate("dex-router").is_err(), "bad={bad}");
    }
}
