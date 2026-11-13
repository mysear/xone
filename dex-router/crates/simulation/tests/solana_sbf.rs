use serde_json::{json, Value};
use std::path::PathBuf;
use xone_domain::{hash, Pool};
use xone_simulation::solana_sbf::{Engine, RuntimeManifest};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn manifest(name: &str) -> PathBuf {
    root().join(format!("test/contracts/sbf-runtime/{name}.json"))
}
fn preparation(name: &str) -> Value {
    let v: Value = serde_json::from_slice(
        &std::fs::read(root().join("test/contracts/sbf-preparations.json")).unwrap(),
    )
    .unwrap();
    v[name].clone()
}
fn reseal(value: &mut Value) {
    value.as_object_mut().unwrap().remove("preparationHash");
    value["preparationHash"] = hash(value).unwrap().into();
}
#[tokio::test]
async fn isolated_sbf_executes_actual_unsigned_wire_and_enforces_real_compute_limit() {
    for name in ["meteora", "token2022", "raydium"] {
        let manifest_path = manifest(name);
        let configuration: RuntimeManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        let bank: Value =
            serde_json::from_slice(&std::fs::read(&configuration.banks[0].path).unwrap()).unwrap();
        let pool: Pool = serde_json::from_value(bank["pool"].clone()).unwrap();
        let engine = Engine::load(&manifest_path).unwrap();
        assert!(engine.available(&pool, bank["genesis"].as_str().unwrap()));
        let prepared = preparation(name);
        let caller = bank["caller"].as_str().unwrap();
        let recipient = if prepared["unwrapDestinationSol"] == true {
            caller
        } else {
            bank["recipient"].as_str().unwrap()
        };
        let zero = if name == "raydium" {
            true
        } else {
            name == "meteora"
        };
        let (sell, buy) = if zero {
            (&pool.token0, &pool.token1)
        } else {
            (&pool.token1, &pool.token0)
        };
        let result = engine
            .simulate(&pool, caller, recipient, sell, buy, &prepared)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(result["success"], true);
        assert_eq!(result["actualInput"], prepared["quotedInput"]);
        assert_eq!(result["actualOutput"], prepared["quotedOutput"]);
        assert_eq!(result["sourceBankUnchanged"], true);
        assert_eq!(result["level"], "LOCAL_SBF_SIMULATED");
        assert_eq!(result["independentProductionVerification"], false);
        assert_eq!(result["submitted"], false);
        assert!(result["unitsConsumed"].as_u64().unwrap() > 0);
        let mut bad = prepared.clone();
        bad["quotedOutput"] = "999".into();
        reseal(&mut bad);
        assert_eq!(
            engine
                .simulate(&pool, caller, recipient, sell, buy, &bad)
                .await
                .unwrap_err()
                .0,
            "SBF_AMOUNT_MISMATCH"
        );
        if name == "raydium" {
            let mut limited = configuration.clone();
            limited.max_compute_units = 1;
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("low.json");
            std::fs::write(&path, serde_json::to_vec(&limited).unwrap()).unwrap();
            let engine = Engine::load(&path).unwrap();
            assert_eq!(
                engine
                    .simulate(&pool, caller, recipient, sell, buy, &prepared)
                    .await
                    .unwrap_err()
                    .0,
                "SBF_TRANSACTION_FAILED"
            );
        }
    }
}
#[test]
fn sbf_manifest_refuses_runtime_bank_or_path_changes_before_execution() {
    let original: Value =
        serde_json::from_slice(&std::fs::read(manifest("raydium")).unwrap()).unwrap();
    for index in 0..5 {
        let mut value = original.clone();
        match index {
            0 => value["python_executable"]["sha256"] = json!("0".repeat(64)),
            1 => {
                let first = value["runtime_files"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .next()
                    .unwrap()
                    .clone();
                value["runtime_files"][first] = json!("0".repeat(64));
            }
            2 => value["runtime_files"]["../escape.py"] = json!("0".repeat(64)),
            3 => value["banks"][0]["sha256"] = json!("0".repeat(64)),
            4 => value["max_compute_units"] = json!(0),
            _ => unreachable!(),
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bad.json");
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Engine::load(&path).is_err(), "case {index}");
    }
}
