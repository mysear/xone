//! Original two-pool swaps/collections, carried paths and clock-only transitions.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, sync::OnceLock};
use xone_domain::{validate_pool_dependencies, Amount, Pool};
use xone_protocol_math::managed_v2_pool::{collect_at, quote_at, reclock};
#[path = "../src/shared_vault.rs"]
mod shared_vault;
type Bank = BTreeMap<String, Pool>;
fn fixtures() -> &'static (Value, Value) {
    static DATA: OnceLock<(Value, Value)> = OnceLock::new();
    DATA.get_or_init(|| {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(base.join("test/contracts/managed-v2-scheduled-shared.json")).unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let thin: Value = serde_json::from_slice(
            &std::fs::read(base.join("test/contracts/managed-v2-scheduled-shared-pools.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            thin["original_raw"]["sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(thin["original_raw"]["bytes"], bytes.len());
        assert_eq!(raw["cases"].as_array().unwrap().len(), 52);
        assert_eq!(thin["cases"].as_array().unwrap().len(), 52);
        for c in thin["cases"].as_array().unwrap() {
            let original = raw["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["id"] == c["id"])
                .unwrap();
            for key in [
                "id",
                "namespace",
                "operation",
                "pool_name",
                "before",
                "after",
                "transaction",
                "expected_native",
            ] {
                assert_eq!(
                    c[key], original[key],
                    "exact original native case field {key}"
                );
            }
        }
        (raw, thin)
    })
}
fn bank(id: &str) -> Bank {
    fixtures().1["banks"][id]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let p: Pool = serde_json::from_value(r["pool"].clone()).unwrap();
            (p.pool_id.clone(), p)
        })
        .collect()
}
fn selected(c: &Value) -> String {
    fixtures().1["banks"][c["before"].as_str().unwrap()]["pools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == c["pool_name"])
        .unwrap()["pool"]["pool_id"]
        .as_str()
        .unwrap()
        .into()
}
fn step(current: &mut Bank, c: &Value) {
    validate_pool_dependencies(current.values()).unwrap();
    let id = selected(c);
    let before = &current[&id];
    let block = c["params"]["execution_block"].as_u64().unwrap();
    let time = c["params"]["execution_timestamp"].as_u64().unwrap();
    let next = if c["operation"] == "swap" {
        let amount: Amount = c["params"]["amount"].as_str().unwrap().parse().unwrap();
        let assets = &before.multi_asset.as_ref().unwrap().assets;
        let i = c["params"]["index_in"].as_u64().unwrap() as usize;
        let j = c["params"]["index_out"].as_u64().unwrap() as usize;
        let q = quote_at(
            before,
            &amount,
            &assets[i].token,
            &assets[j].token,
            block,
            time,
        )
        .unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        assert_eq!(
            q.amount_out.to_string(),
            c["expected_native"]["actual_amount_out"].as_str().unwrap(),
            "{} original actual recipient credit",
            c["id"]
        );
        assert_eq!(
            c["expected_native"]["actual_amount_in"],
            c["params"]["amount"]
        );
        q.next_pool
    } else {
        let q = collect_at(before, block, time).unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        assert_eq!(
            q.accrual.total_bpt.to_string(),
            c["expected_native"]["minted_bpt"].as_str().unwrap()
        );
        assert_eq!(
            q.accrual.manager_bpt.to_string(),
            c["expected_native"]["manager_bpt_delta"].as_str().unwrap()
        );
        assert_eq!(
            q.accrual.protocol_bpt.to_string(),
            c["expected_native"]["collector_bpt_delta"]
                .as_str()
                .unwrap()
        );
        q.next_pool
    };
    shared_vault::propagate(current, &next)
        .unwrap_or_else(|e| panic!("{} peer propagation: {e}", c["id"]));
    current.insert(next.pool_id.clone(), next);
    validate_pool_dependencies(current.values()).unwrap();
}
fn compare(current: &Bank, expected: &Value, origin: &Bank, label: &str) {
    let pools: Vec<Pool> = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(pools.len(), 2);
    assert_eq!(current.len(), 2);
    for p in pools {
        let original = &origin[&p.pool_id];
        let m = original
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let mut e = serde_json::to_value(&p).unwrap();
        let state = &mut e["multi_asset"]["managed_v2"];
        state["block_number"] = json!(m.block_number);
        state["block_hash"] = json!(m.block_hash);
        state["timestamp"] = json!(m.timestamp);
        let actual = &current[&p.pool_id];
        actual.validate().unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            e,
            "{label}: all own and sibling state fields, actual wallets and original source H"
        );
    }
}
#[test]
fn all_48_swaps_and_four_collections_match_both_full_original_pool_poststates() {
    for c in fixtures().1["cases"].as_array().unwrap() {
        let origin = bank(c["before"].as_str().unwrap());
        let mut current = origin.clone();
        step(&mut current, c);
        compare(
            &current,
            &c["expected_pools_after"],
            &origin,
            c["id"].as_str().unwrap(),
        );
    }
}
#[test]
fn twelve_carried_abab_and_final_collection_sequences_never_reset_source_or_wallets() {
    let (_, thin) = fixtures();
    let cases = thin["cases"].as_array().unwrap();
    let mut count = 0;
    for cp in thin["checkpoints"].as_array().unwrap() {
        let mut cursor = cp["bank"].as_str().unwrap().to_string();
        let origin = bank(&cursor);
        let mut current = origin.clone();
        let mut steps = 0;
        while let Some(c) = cases.iter().find(|c| c["before"] == cursor) {
            step(&mut current, c);
            compare(
                &current,
                &c["expected_pools_after"],
                &origin,
                c["id"].as_str().unwrap(),
            );
            cursor = c["after"].as_str().unwrap().into();
            steps += 1;
            count += 1;
        }
        assert!(
            steps == 4 || steps == 6,
            "complete original A/B/A/B with optional final two collections"
        );
    }
    assert_eq!(
        count, 52,
        "all distinct original executions carried exactly once"
    );
}
#[test]
fn all_twelve_native_clock_only_transitions_reclock_each_own_schedule_and_aum() {
    let (raw, thin) = fixtures();
    for branch in raw["branches"].as_object().unwrap().values() {
        for op in branch["clock_operations"].as_array().unwrap() {
            let before = thin["banks"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, b)| {
                    b["namespace"] == branch["mode"]
                        && b["header"]["hash"] == op["pre_header"]["hash"]
                })
                .unwrap();
            let after = thin["banks"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, b)| {
                    b["namespace"] == branch["mode"] && b["header"]["hash"] == op["header"]["hash"]
                })
                .unwrap();
            let origin = bank(before.0);
            let mut current = origin.clone();
            let block = u64::from_str_radix(
                op["header"]["number"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("0x"),
                16,
            )
            .unwrap();
            let time = op["timestamp"].as_u64().unwrap();
            let p = current.values().next().unwrap();
            let projected = reclock(p, block, time).unwrap();
            shared_vault::propagate(&mut current, &projected).unwrap();
            current.insert(projected.pool_id.clone(), projected);
            let expected: Vec<Value> = after.1["pools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    let mut value = p["pool"].clone();
                    value["multi_asset"]["managed_v2"]["execution_context"] =
                        json!({"block_number":block,"timestamp":time});
                    value
                })
                .collect();
            validate_pool_dependencies(current.values()).unwrap();
            compare(&current, &json!(expected), &origin, after.0);
        }
    }
}

#[test]
#[ignore = "requires actual API-selected original VM vectors; mandatory in the shared service gate"]
fn actual_api_selected_two_route_full_poststates() {
    let path = std::path::PathBuf::from(
        std::env::var_os("XONE_SCHEDULED_SHARED_SELECTED_VECTORS")
            .expect("actual selected original VM vectors required"),
    );
    assert!(
        path.is_absolute(),
        "explicit absolute controlled report path"
    );
    let report: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        report["original_shared_sha256"],
        fixtures().1["original_raw"]["sha256"]
    );
    let executions = report["executions"].as_array().unwrap();
    assert!(!executions.is_empty());
    let mut steps = 0;
    for execution in executions {
        let pools: Vec<Pool> = serde_json::from_value(execution["source_pools"].clone()).unwrap();
        assert_eq!(pools.len(), 2);
        let origin: Bank = pools.into_iter().map(|p| (p.pool_id.clone(), p)).collect();
        let mut current = origin.clone();
        assert_eq!(execution["steps"].as_array().unwrap().len(), 2);
        let mut total_in = Amount::zero();
        let mut total_out = Amount::zero();
        for (index, raw) in execution["steps"].as_array().unwrap().iter().enumerate() {
            let mut c = raw.clone();
            c["id"] = json!(format!("{}/route-{}", execution["id"], index));
            step(&mut current, &c);
            compare(
                &current,
                &c["expected_pools_after"],
                &origin,
                c["id"].as_str().unwrap(),
            );
            let input: Amount = c["expected_native"]["actual_amount_in"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            let output: Amount = c["expected_native"]["actual_amount_out"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            total_in.0 += input.0;
            total_out.0 += output.0;
            steps += 1;
        }
        assert_eq!(
            total_in.to_string(),
            execution["actual_amount_in"].as_str().unwrap()
        );
        assert_eq!(
            total_out.to_string(),
            execution["actual_amount_out"].as_str().unwrap()
        );
    }
    assert_eq!(
        steps,
        report["selected_transactions"].as_u64().unwrap() as usize,
        "every actual selected original swap compared completely"
    );
}
