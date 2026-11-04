//! Runtime-read service-local original getter projection, not synthetic expected math.
use num_bigint::BigInt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use xone_domain::{Amount, Pool};
use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};
fn bank() -> &'static Value {
    static BANK: OnceLock<Value> = OnceLock::new();
    BANK.get_or_init(|| {
        let raw_bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/managed-v2-aum.json"
        ))
        .unwrap();
        assert_eq!(raw_bytes.len(), 7_905_624);
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw_bytes)),
            "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66"
        );
        let raw: Value = serde_json::from_slice(&raw_bytes).unwrap();
        let thin: Value = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test/contracts/managed-v2-aum-pools.json"
            ))
            .unwrap(),
        )
        .unwrap();
        assert_raw_bank_projection(&thin, &raw);
        thin
    })
}
// This precondition runs before every native differential test can access the
// thin cases. Expectations and provenance come from this project's raw objects.
fn assert_raw_bank_projection(thin: &Value, raw: &Value) {
    fn resolve<'a>(raw: &'a Value, link: &Value, kind: &str) -> &'a Value {
        let object = &raw["objects"][link["ref"].as_str().expect("object reference")];
        assert_eq!(object["kind"], kind);
        &object["value"]
    }
    fn hex_u64(v: &Value) -> u64 {
        u64::from_str_radix(v.as_str().unwrap().strip_prefix("0x").unwrap(), 16).unwrap()
    }
    fn delta(after: &Value, before: &Value) -> String {
        (after.as_str().unwrap().parse::<BigInt>().unwrap()
            - before.as_str().unwrap().parse::<BigInt>().unwrap())
        .to_string()
    }
    assert_eq!(thin["original_bank"]["bytes"], 7_905_624);
    assert_eq!(
        thin["original_bank"]["sha256"],
        "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66"
    );
    assert_eq!(thin["original_bank"]["reference"], raw["reference"]);
    let cases = thin["cases"].as_array().unwrap();
    let originals = raw["cases"].as_array().unwrap();
    assert_eq!(cases.len(), originals.len());
    assert_eq!(cases.len(), 27);
    let mut seen = std::collections::BTreeSet::new();
    for c in cases {
        let name = c["name"].as_str().unwrap();
        assert!(seen.insert(name), "duplicate case {name}");
        let original = originals.iter().find(|r| r["name"] == name).unwrap();
        assert_eq!(c["scenario"], original["scenario"], "{name}");
        assert_eq!(c["operation"], original["operation"], "{name}");
        assert_eq!(
            c["source_refs"]["scenario_source_h"], original["source_h"],
            "{name}"
        );
        assert_eq!(
            c["source_refs"]["bank_sha256"],
            thin["original_bank"]["sha256"]
        );
        for key in ["before", "after", "transaction"] {
            assert_eq!(
                c["source_refs"][key], original["execution"][key],
                "{name} {key}"
            );
        }
        let before = resolve(raw, &original["execution"]["before"], "snapshot");
        let after = resolve(raw, &original["execution"]["after"], "snapshot");
        let tx = resolve(raw, &original["execution"]["transaction"], "transaction");
        assert_eq!(tx["pre_header"], before["header"]);
        assert_eq!(tx["execution_header"], after["header"]);
        let header = resolve(raw, &tx["execution_header"], "header");
        let status = hex_u64(&tx["receipt"]["status"]);
        assert_eq!(c["params"]["execution_block"], hex_u64(&header["number"]));
        assert_eq!(
            c["params"]["execution_timestamp"],
            hex_u64(&header["timestamp"])
        );
        assert_eq!(c["expected_native"]["status"], status);
        assert_eq!(c["expected_native"]["expectation"], original["expectation"]);
        if status == 0 {
            assert_eq!(
                c["expected_native"]["rejection"],
                "original-transaction-reverted"
            );
        }
        match original["operation"].as_str().unwrap() {
            "swap" => {
                assert_eq!(
                    c["expected_native"]["quantity"],
                    original["expectation"]["raw_out"]
                );
                assert_eq!(c["params"]["amount"], original["expectation"]["raw_in"]);
                for index in ["index_in", "index_out"] {
                    assert_eq!(c["params"][index], original["expectation"][index]);
                }
            }
            "collect" => assert_eq!(
                c["expected_native"]["quantity"],
                delta(
                    &after["state"]["total_supply"],
                    &before["state"]["total_supply"]
                )
            ),
            "withdraw" | "deposit" => assert_eq!(
                c["expected_native"]["quantity"],
                original["expectation"]["amount"]
            ),
            other => panic!("unrecognized original operation {other}"),
        }
        for (field, snap) in [
            ("source_pool", before),
            ("observed_pool_after", after),
            ("expected_pool_after", after),
        ] {
            let m = &c[field]["multi_asset"]["managed_v2"];
            let state = &snap["state"];
            for key in [
                "total_supply",
                "virtual_supply",
                "actual_supply",
                "pending_aum",
            ] {
                assert_eq!(m[key], state[key], "{name} {field} {key}");
            }
            for (key, role) in [
                ("manager_bpt_balance", "manager"),
                ("protocol_collector_bpt_balance", "collector"),
            ] {
                let wallet = state["roles"][role].as_str().unwrap();
                assert_eq!(m[key], state["bpt_balances"][wallet]);
            }
            if field == "expected_pool_after" {
                assert_eq!(
                    m["execution_context"],
                    json!({"block_number":hex_u64(&header["number"]),"timestamp":hex_u64(&header["timestamp"])})
                );
                let source = &c["source_pool"]["multi_asset"]["managed_v2"];
                for key in ["block_number", "timestamp", "block_hash"] {
                    assert_eq!(m[key], source[key]);
                }
            } else {
                let h = resolve(raw, &snap["header"], "header");
                assert_eq!(m["block_number"], hex_u64(&h["number"]));
                assert_eq!(m["timestamp"], hex_u64(&h["timestamp"]));
                assert_eq!(m["block_hash"], h["hash"]);
                assert!(m.get("execution_context").is_none());
            }
        }
        assert_eq!(
            c["expected_native"]["observed_bpt_deltas"]["total_supply"],
            delta(
                &after["state"]["total_supply"],
                &before["state"]["total_supply"]
            )
        );
    }
}
#[test]
fn all_eight_original_vault_swaps_match_entire_published_managed_state() {
    let mut seen = 0;
    for c in bank()["cases"].as_array().unwrap() {
        if c["operation"] != "swap" {
            continue;
        }
        let p: Pool = serde_json::from_value(c["source_pool"].clone()).unwrap();
        let param = &c["params"];
        let input: Amount = param["amount"].as_str().unwrap().parse().unwrap();
        let multi = p.multi_asset.as_ref().unwrap();
        let token_in = multi.assets[param["index_in"].as_u64().unwrap() as usize]
            .token
            .clone();
        let token_out = multi.assets[param["index_out"].as_u64().unwrap() as usize]
            .token
            .clone();
        let q = quote_at(
            &p,
            &input,
            &token_in,
            &token_out,
            param["execution_block"].as_u64().unwrap(),
            param["execution_timestamp"].as_u64().unwrap(),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        assert_eq!(
            q.amount_out.to_string(),
            c["expected_native"]["quantity"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        assert_eq!(
            q.next_pool,
            serde_json::from_value::<Pool>(c["expected_pool_after"].clone()).unwrap(),
            "{}",
            c["name"]
        );
        seen += 1;
    }
    assert_eq!(seen, 8);
}
#[test]
fn all_seventeen_original_collections_match_supply_wallets_and_original_revert() {
    let (mut success, mut rejected) = (0, 0);
    for c in bank()["cases"].as_array().unwrap() {
        if c["operation"] != "collect" {
            continue;
        }
        let p: Pool = serde_json::from_value(c["source_pool"].clone()).unwrap();
        let param = &c["params"];
        let result = collect_at(
            &p,
            param["execution_block"].as_u64().unwrap(),
            param["execution_timestamp"].as_u64().unwrap(),
        );
        if c["expected_native"]["status"] == 0 {
            assert!(result.is_err(), "{}", c["name"]);
            rejected += 1;
            continue;
        }
        let q = result.unwrap_or_else(|e| panic!("{}: {e}", c["name"]));
        assert_eq!(
            q.accrual.total_bpt.to_string(),
            c["expected_native"]["quantity"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(
            q.next_pool,
            serde_json::from_value::<Pool>(c["expected_pool_after"].clone()).unwrap(),
            "{}",
            c["name"]
        );
        success += 1;
    }
    assert_eq!((success, rejected), (16, 1));
}
#[test]
fn all_original_before_observed_after_and_derived_pools_validate_without_faking_bank() {
    let mut seen = 0;
    for c in bank()["cases"].as_array().unwrap() {
        for field in ["source_pool", "observed_pool_after", "expected_pool_after"] {
            let p: Pool = serde_json::from_value(c[field].clone()).unwrap();
            p.validate()
                .unwrap_or_else(|e| panic!("{} {field}: {e}", c["name"]));
            seen += 1;
        }
        let before = &c["source_pool"]["multi_asset"]["managed_v2"];
        let derived = &c["expected_pool_after"]["multi_asset"]["managed_v2"];
        for field in ["timestamp", "block_number", "block_hash"] {
            assert_eq!(before[field], derived[field], "{} {field}", c["name"]);
        }
    }
    assert_eq!(seen, 81);
}

#[test]
fn raw_bank_precondition_rejects_thin_reference_and_native_expectation_drift() {
    let thin = bank();
    let raw: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/managed-v2-aum.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let collection = thin["cases"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c["operation"] == "collect")
        .unwrap();
    let mut mutations = Vec::new();
    let mut changed = thin.clone();
    changed["original_bank"]["reference"]
        .as_object_mut()
        .unwrap()
        .remove("identities");
    mutations.push(changed);
    for key in ["before", "after", "transaction"] {
        let mut changed = thin.clone();
        changed["cases"][0]["source_refs"][key]["ref"] = "0".repeat(64).into();
        mutations.push(changed);
    }
    for (index, key, value) in [
        (0, "status", json!(0)),
        (0, "quantity", json!("1")),
        (collection, "quantity", json!("1")),
    ] {
        let mut changed = thin.clone();
        changed["cases"][index]["expected_native"][key] = value;
        mutations.push(changed);
    }
    for changed in mutations {
        assert!(
            std::panic::catch_unwind(|| assert_raw_bank_projection(&changed, &raw)).is_err(),
            "thin drift bypassed raw precondition"
        );
    }
}

#[test]
fn original_vault_pause_does_not_pause_public_aum_collection_and_expires_independently() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/contracts/managed-v2-aum-paused-vault.json"
    ))
    .unwrap();
    let branch: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(branch["schema_version"], 1);
    assert_eq!(
        branch["public_adapter_version"],
        xone_domain::managed_v2_state::ADAPTER_VERSION
    );
    assert_eq!(
        branch["original_bank_sha256"],
        "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66"
    );
    assert_eq!(branch["cases"].as_array().unwrap().len(), 3);
    let mut verified = (0, 0, 0);
    for case in branch["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(case["source_pool"].clone()).unwrap();
        let before = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
        assert!(!before.paused);
        assert!(before.vault_paused);
        let clock = &case["params"];
        let block = clock["execution_block"].as_u64().unwrap();
        let time = clock["execution_timestamp"].as_u64().unwrap();
        let expected: Pool = serde_json::from_value(case["expected_pool_after"].clone()).unwrap();
        let (next, amount) = if case["operation"] == "collect" {
            let transition = collect_at(&p, block, time).unwrap();
            verified.1 += 1;
            (transition.next_pool, transition.accrual.total_bpt)
        } else {
            let s = p.multi_asset.as_ref().unwrap();
            let input: Amount = clock["amount"].as_str().unwrap().parse().unwrap();
            let i = clock["index_in"].as_u64().unwrap() as usize;
            let j = clock["index_out"].as_u64().unwrap() as usize;
            let quoted = quote_at(
                &p,
                &input,
                &s.assets[i].token,
                &s.assets[j].token,
                block,
                time,
            );
            if case["expected_native"]["status"] == 0 {
                assert!(
                    quoted.is_err(),
                    "original paused Vault transaction reverted"
                );
                verified.0 += 1;
                continue;
            }
            let quoted = quoted.unwrap();
            verified.2 += 1;
            assert_eq!(quoted.consumed_in, input);
            assert_eq!(quoted.remaining_in, Amount::zero());
            let execution_pool: Pool =
                serde_json::from_value(case["execution_source_pool"].clone()).unwrap();
            let current = xone_protocol_math::multi_asset::quote_exact_in_tokens(
                &execution_pool,
                &input,
                &s.assets[i].token,
                &s.assets[j].token,
            )
            .unwrap();
            assert_eq!(
                current.amount_out, quoted.amount_out,
                "normal source clock must use expired Vault flag"
            );
            (quoted.next_pool, quoted.amount_out)
        };
        assert_eq!(
            amount.to_string(),
            case["expected_native"]["quantity"].as_str().unwrap()
        );
        assert_eq!(
            next, expected,
            "{}: complete original pause/fee/wallet/clock poststate",
            case["name"]
        );
        assert!(
            next.multi_asset.as_ref().unwrap().to_proto().is_err(),
            "derived transition cannot publish as original source"
        );
        let m = next
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(
            (m.block_number, m.timestamp, &m.block_hash),
            (before.block_number, before.timestamp, &before.block_hash)
        );
    }
    assert_eq!(verified, (1, 1, 1));
}

#[test]
fn original_recovery_effective_fee_zero_swap_collection_and_exit_source_views() {
    let raw = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/contracts/managed-v2-aum-recovery.json"
    ))
    .unwrap();
    let branch: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        branch["public_adapter_version"],
        xone_domain::managed_v2_state::ADAPTER_VERSION
    );
    assert_eq!(
        branch["original_bank_sha256"],
        "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66"
    );
    let cases = branch["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    let mut coverage = (0, 0, 0);
    for case in &cases[3..] {
        let before: Pool = serde_json::from_value(case["source_pool"].clone()).unwrap();
        let observed: Pool = serde_json::from_value(case["observed_pool_after"].clone()).unwrap();
        before.validate().unwrap();
        observed.validate().unwrap();
        let bm = before
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let am = observed
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        assert_eq!(
            bm.total_supply, am.total_supply,
            "Recovery operations mint no BPT"
        );
        if case["operation"] == "control" {
            assert_eq!(
                case["expected_native"]["model_execution"], false,
                "canonical control observation is not a local control kernel"
            );
            if case["name"] == "enable-recovery" {
                assert!(!bm.recovery_mode && am.recovery_mode);
                assert_eq!(am.management_aum_fee, Amount::zero());
                assert_eq!(am.pending_aum, Amount::zero());
            } else {
                assert!(bm.recovery_mode && !am.recovery_mode);
                assert_eq!(am.management_aum_fee, "10000000000000000".parse().unwrap());
                assert_eq!(am.pending_aum, Amount::zero());
            }
            coverage.0 += 1;
            continue;
        }
        assert!(bm.recovery_mode);
        assert_eq!(bm.management_aum_fee, Amount::zero());
        assert_eq!(bm.pending_aum, Amount::zero());
        let block = case["params"]["execution_block"].as_u64().unwrap();
        let time = case["params"]["execution_timestamp"].as_u64().unwrap();
        let (next, amount) = if case["operation"] == "collect" {
            let transition = collect_at(&before, block, time).unwrap();
            coverage.1 += 1;
            assert_eq!(
                transition
                    .next_pool
                    .multi_asset
                    .as_ref()
                    .unwrap()
                    .managed_v2
                    .as_ref()
                    .unwrap()
                    .last_collection_timestamp,
                u32::try_from(time).unwrap()
            );
            (transition.next_pool, transition.accrual.total_bpt)
        } else {
            let assets = &before.multi_asset.as_ref().unwrap().assets;
            let input: Amount = case["params"]["amount"].as_str().unwrap().parse().unwrap();
            let i = case["params"]["index_in"].as_u64().unwrap() as usize;
            let j = case["params"]["index_out"].as_u64().unwrap() as usize;
            let transition = quote_at(
                &before,
                &input,
                &assets[i].token,
                &assets[j].token,
                block,
                time,
            )
            .unwrap();
            coverage.2 += 1;
            assert_eq!(transition.consumed_in, input);
            assert_eq!(transition.remaining_in, Amount::zero());
            (transition.next_pool, transition.amount_out)
        };
        assert_eq!(
            amount.to_string(),
            case["expected_native"]["quantity"].as_str().unwrap()
        );
        let expected: Pool = serde_json::from_value(case["expected_pool_after"].clone()).unwrap();
        assert_eq!(next, expected, "{} full Recovery poststate", case["name"]);
    }
    assert_eq!(coverage, (2, 1, 1));
}
