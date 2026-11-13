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

#[tokio::test]
async fn capture_only_runtime_binds_raw_snapshot_and_rejects_ambient_or_missing_dependencies() {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let manifest_path = manifest("rpc-capture");
    let engine = Engine::load(&manifest_path).unwrap();
    let bank: Value = serde_json::from_slice(
        &std::fs::read(root().join("test/contracts/sbf-runtime/token2022-bank.json")).unwrap(),
    )
    .unwrap();
    let pool: Pool = serde_json::from_value(bank["pool"].clone()).unwrap();
    let t = pool.meteora.as_ref().unwrap();
    let prepared = preparation("token2022");
    let mut ids = std::collections::BTreeSet::new();
    for d in prepared["instructions"].as_array().unwrap() {
        ids.insert(d["programId"].as_str().unwrap().to_owned());
        for a in d["accounts"].as_array().unwrap() {
            ids.insert(a["pubkey"].as_str().unwrap().to_owned());
        }
    }
    ids.insert(xone_domain::raydium::CLOCK.into());
    ids.insert(t.programdata.clone());
    ids.insert(t.token2022.as_ref().unwrap().programdata.clone());
    ids.insert(
        xone_domain::meteora::pda(
            &[
                &xone_domain::meteora::pubkey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
                    .unwrap(),
            ],
            "BPFLoaderUpgradeab1e11111111111111111111111",
        )
        .unwrap(),
    );
    let rent = "SysvarRent111111111111111111111111111111111";
    let mut accounts = serde_json::Map::new();
    for key in ids {
        accounts.insert(key.clone(), bank["accounts"][&key].clone());
    }
    accounts.insert(rent.into(),json!({"owner":"Sysvar1111111111111111111111111111111111111","executable":false,"lamports":1,"data":["MBsAAAAAAAAAAAAAAADwPzI=","base64"]}));
    let snapshot = json!({"schemaVersion":1,"chainKey":pool.chain_key,"genesisHash":bank["genesis"],"observedSlot":t.slot,"accounts":accounts});
    let caller = bank["caller"].as_str().unwrap();
    let recipient = bank["recipient"].as_str().unwrap();
    for variant in 0..7 {
        let mut capture = snapshot.clone();
        let mut p = prepared.clone();
        match variant {
            0 => {}
            1 => capture["observedSlot"] = json!(t.slot + 1),
            2 => {
                capture["accounts"].as_object_mut().unwrap().remove(rent);
            }
            3 => {
                let mut data = STANDARD
                    .decode(
                        capture["accounts"][&t.programdata]["data"][0]
                            .as_str()
                            .unwrap(),
                    )
                    .unwrap();
                data[45] ^= 1;
                capture["accounts"][&t.programdata]["data"][0] = STANDARD.encode(data).into();
            }
            4 => {
                let key = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
                let mut data = STANDARD
                    .decode(capture["accounts"][key]["data"][0].as_str().unwrap())
                    .unwrap();
                data[0] ^= 1;
                capture["accounts"][key]["data"][0] = STANDARD.encode(data).into();
            }
            5 => {
                let key = p["sourceTokenAccount"].as_str().unwrap().to_owned();
                capture["accounts"][key] = json!({"owner":"11111111111111111111111111111111","lamports":1,"executable":false,"data":["","base64"]});
            }
            6 => {}
            _ => unreachable!(),
        }
        p["accountSnapshot"] =
            json!({"snapshotHash":hash(&capture).unwrap(),"observedSlot":capture["observedSlot"]});
        if variant == 6 {
            p["accountSnapshot"]["snapshotHash"] = "0".repeat(64).into();
        }
        reseal(&mut p);
        let result = engine
            .simulate_captured(
                &pool,
                caller,
                recipient,
                &pool.token1,
                &pool.token0,
                &p,
                &capture,
            )
            .await;
        let expected = match variant {
            0 => None,
            1 | 6 => Some("SBF_CAPTURE_BINDING_INVALID"),
            2 => Some("SBF_RENT_REQUIRED"),
            3 => Some("SBF_BANK_PROGRAM_MISMATCH"),
            4 => Some("SBF_AMBIENT_PROGRAM_MISMATCH"),
            5 => Some("SBF_TRANSACTION_FAILED"),
            _ => unreachable!(),
        };
        if let Some(expected) = expected {
            assert_eq!(result.unwrap_err().0, expected, "variant {variant}");
        } else {
            let result = result.unwrap();
            assert_eq!(result["actualOutput"], "1000");
            assert_eq!(result["bankSha256"], p["accountSnapshot"]["snapshotHash"]);
            assert_eq!(result["level"], "RPC_CAPTURE_SBF_SIMULATED");
        }
    }
}
