//! Eight original-bytecode cases, full public poststate and retained source H.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use xone_domain::{Amount, Pool};
use xone_protocol_math::managed_v2_pool::{collect_at, quote_at};

fn local_path(variable: &str, default: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os(variable) {
        let path = PathBuf::from(path);
        assert!(
            path.is_absolute(),
            "temporary fixture override must be absolute"
        );
        path
    } else {
        default.to_path_buf()
    }
}
fn fixtures() -> &'static (Value, Value) {
    static FIXTURES: OnceLock<(Value, Value)> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let bytes = std::fs::read(local_path(
            "XONE_CATALOG_RAW",
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../test/contracts/managed-v2-aum-catalog.json"),
        ))
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "b843e859a5e92efcc1880eed437e475042785a7967bf2277b6f9770cf93b2813"
        );
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let thin: Value = serde_json::from_slice(
            &std::fs::read(local_path(
                "XONE_CATALOG_PROJECTION",
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../test/contracts/managed-v2-aum-catalog-pools.json"),
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(thin["original_raw"]["bytes"], bytes.len());
        assert_eq!(
            thin["original_raw"]["sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(raw["cases"].as_array().unwrap().len(), 8);
        assert_eq!(thin["cases"].as_array().unwrap().len(), 8);
        assert_eq!(raw["banks"].as_object().unwrap().len(), 10);
        for (id, bank) in raw["banks"].as_object().unwrap() {
            assert_eq!(thin["banks"][id]["header"], bank["header"]);
            assert_eq!(bank["pools"].as_array().unwrap().len(), 4);
            for (i, p) in bank["pools"].as_array().unwrap().iter().enumerate() {
                assert_eq!(thin["banks"][id]["pools"][i]["source_pool"], p["pool"]);
                assert_eq!(
                    p["header"], bank["header"],
                    "common source H across all siblings/globals"
                );
            }
        }
        (raw, thin)
    })
}
fn hex_u64(v: &Value) -> u64 {
    u64::from_str_radix(v.as_str().unwrap().strip_prefix("0x").unwrap(), 16).unwrap()
}
fn run_case(scenario: &str, operation: &str) {
    let (raw, thin) = fixtures();
    let c = thin["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["scenario"] == scenario && c["operation"] == operation)
        .unwrap();
    let original = raw["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == c["name"])
        .unwrap();
    let source_bank = &raw["banks"][original["source_bank4pools"].as_str().unwrap()];
    let after_bank = &raw["banks"][original["expected_after_bank4pools"].as_str().unwrap()];
    let selected = |bank: &Value| {
        bank["pools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["scenario"] == scenario)
            .unwrap()["pool"]
            .clone()
    };
    assert_eq!(c["source_pool"], selected(source_bank));
    assert_eq!(c["observed_pool_after"], selected(after_bank));
    assert_eq!(c["expected_native"], original["expected_native"]);
    let block = hex_u64(&original["execution"]["execution_header"]["number"]);
    let time = hex_u64(&original["execution"]["execution_header"]["timestamp"]);
    let before: Pool = serde_json::from_value(c["source_pool"].clone()).unwrap();
    let observed: Pool = serde_json::from_value(c["observed_pool_after"].clone()).unwrap();
    before.validate().unwrap();
    observed.validate().unwrap();
    let bm = before
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert!(bm.execution_context.is_none());
    let mut derived = selected(after_bank);
    let source = &c["source_pool"]["multi_asset"]["managed_v2"];
    let m = &mut derived["multi_asset"]["managed_v2"];
    for key in ["block_number", "timestamp", "block_hash"] {
        m[key] = source[key].clone();
    }
    m["execution_context"] = json!({"block_number": block, "timestamp": time});
    assert_eq!(
        c["expected_pool_after"], derived,
        "exact observed poststate, only origin/context transformed"
    );
    let (next, quantity) = if operation == "swap" {
        let swap = &original["execution"]["args"][0];
        let input: Amount = swap["amount"].as_str().unwrap().parse().unwrap();
        let q = quote_at(
            &before,
            &input,
            swap["assetIn"].as_str().unwrap(),
            swap["assetOut"].as_str().unwrap(),
            block,
            time,
        )
        .unwrap();
        assert_eq!(q.consumed_in, input);
        assert_eq!(q.remaining_in, Amount::zero());
        (q.next_pool, q.amount_out)
    } else {
        assert_ne!(
            bm.pending_aum,
            Amount::zero(),
            "nonzero original 30-day accrual"
        );
        let q = collect_at(&before, block, time).unwrap();
        (q.next_pool, q.accrual.total_bpt)
    };
    assert_eq!(
        quantity.to_string(),
        c["expected_native"][if operation == "swap" {
            "amount_out"
        } else {
            "minted_bpt"
        }]
        .as_str()
        .unwrap()
    );
    let expected: Pool = serde_json::from_value(derived).unwrap();
    assert_eq!(
        next, expected,
        "{scenario}/{operation}: ALL public selected poststate fields"
    );
    let nm = next
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    assert_eq!(
        (nm.block_number, nm.timestamp, &nm.block_hash),
        (bm.block_number, bm.timestamp, &bm.block_hash)
    );
    let ctx = nm
        .execution_context
        .as_ref()
        .expect("every transition must remain derived even at source time");
    assert_eq!((ctx.block_number, ctx.timestamp), (block, time));
    assert!(
        next.multi_asset.as_ref().unwrap().to_proto().is_err(),
        "derived state cannot publish as original wire source"
    );
}
#[test]
fn two_swap() {
    run_case("two", "swap");
}
#[test]
fn three_swap() {
    run_case("three", "swap");
}
#[test]
fn five_swap() {
    run_case("five", "swap");
}
#[test]
fn fifty_swap() {
    run_case("fifty", "swap");
}
#[test]
fn two_collect() {
    run_case("two", "collect");
}
#[test]
fn three_collect() {
    run_case("three", "collect");
}
#[test]
fn five_collect() {
    run_case("five", "collect");
}
#[test]
fn fifty_collect() {
    run_case("fifty", "collect");
}
