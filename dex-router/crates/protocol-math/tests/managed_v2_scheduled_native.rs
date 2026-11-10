//! Original-bytecode wallet amounts and complete getter poststates.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::OnceLock};
use xone_domain::{Amount, Pool};
use xone_protocol_math::managed_v2_pool::{collect_at, quote_at, reclock};

fn fixtures() -> &'static Value {
    static THIN: OnceLock<Value> = OnceLock::new();
    THIN.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(root.join("test/contracts/managed-v2-scheduled.json")).unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let thin: Value = serde_json::from_slice(
            &std::fs::read(root.join("test/contracts/managed-v2-scheduled-pools.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(thin["original_raw"]["bytes"], bytes.len());
        assert_eq!(
            thin["original_raw"]["sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(raw["cases"].as_array().unwrap().len(), 53);
        assert_eq!(thin["cases"].as_array().unwrap().len(), 53);
        assert_eq!(thin["checkpoints"], raw["checkpoints"]);
        assert_eq!(thin["checkpoints"].as_array().unwrap().len(), 41);
        assert_eq!(thin["states"].as_object().unwrap().len(), 234);
        for indexed in raw["cases"].as_array().unwrap() {
            let c = thin["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == indexed["id"])
                .unwrap();
            for key in ["id", "before", "after", "transaction", "expected_raw_out"] {
                assert_eq!(c[key], indexed[key], "original case link {key}");
            }
        }
        thin
    })
}
fn pool(id: &Value) -> Pool {
    serde_json::from_value(fixtures()["states"][id.as_str().unwrap()].clone()).unwrap()
}
fn derived(before: &Pool, observed: &Pool) -> Value {
    let mut expected = serde_json::to_value(observed).unwrap();
    let source = before
        .multi_asset
        .as_ref()
        .unwrap()
        .managed_v2
        .as_ref()
        .unwrap();
    let m = &mut expected["multi_asset"]["managed_v2"];
    m["execution_context"] = json!({"block_number":m["block_number"],"timestamp":m["timestamp"]});
    m["block_number"] = json!(source.block_number);
    m["block_hash"] = json!(source.block_hash);
    m["timestamp"] = json!(source.timestamp);
    expected
}
#[test]
fn all_234_original_getter_views_and_41_checkpoints_are_exact() {
    let thin = fixtures();
    let mut drift = std::collections::BTreeSet::new();
    for (id, value) in thin["states"].as_object().unwrap() {
        let p: Pool = serde_json::from_value(value.clone()).unwrap();
        p.validate().unwrap_or_else(|e| panic!("{id}: {e}"));
        let s = p.multi_asset.as_ref().unwrap();
        let m = s.managed_v2.as_ref().unwrap();
        let view = m.schedule.as_ref().unwrap().at(m.timestamp).unwrap();
        assert_eq!(view.weights, s.weights, "{id}: original weights");
        assert_eq!(
            view.swap_fee.to_string(),
            p.fee_numerator.to_string(),
            "{id}: fee"
        );
        for (asset, scale) in s.assets.iter().zip(view.scaling) {
            assert_eq!(asset.scaling, scale, "{id}: decimals scaling");
        }
        let sum: num_bigint::BigUint = s.weights.iter().map(|w| &w.0).sum();
        drift.insert(num_bigint::BigInt::from(sum) - num_bigint::BigInt::from(10u64.pow(18)));
        let projected = reclock(&p, m.block_number, m.timestamp).unwrap();
        assert_eq!(
            serde_json::to_value(projected).unwrap(),
            derived(&p, &p),
            "{id}: view"
        );
    }
    for offset in [-2, -1, 0, 1, 2] {
        assert!(drift.contains(&num_bigint::BigInt::from(offset)));
    }
}
#[test]
fn all_53_original_wallet_outputs_and_full_swap_poststates_match() {
    let thin = fixtures();
    for c in thin["cases"].as_array().unwrap() {
        let before = pool(&c["before"]);
        let observed = pool(&c["after"]);
        let em = observed
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let amount: Amount = c["amount_in"].as_str().unwrap().parse().unwrap();
        let q = quote_at(
            &before,
            &amount,
            c["token_in"].as_str().unwrap(),
            c["token_out"].as_str().unwrap(),
            em.block_number,
            em.timestamp,
        )
        .unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        assert_eq!(
            q.amount_out.to_string(),
            c["expected_raw_out"].as_str().unwrap(),
            "{}",
            c["id"]
        );
        let expected = derived(&before, &observed);
        assert_eq!(
            expected, c["expected_pool_after"],
            "original after-H/context transform"
        );
        assert_eq!(
            serde_json::to_value(&q.next_pool).unwrap(),
            expected,
            "{} full swap state",
            c["id"]
        );
        q.next_pool.validate().unwrap();
    }
}
#[test]
fn both_original_nonzero_aum_collections_match_every_poststate_field() {
    let thin = fixtures();
    let cases = thin["collections"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for c in cases {
        let before = pool(&c["before"]);
        let observed = pool(&c["after"]);
        let em = observed
            .multi_asset
            .as_ref()
            .unwrap()
            .managed_v2
            .as_ref()
            .unwrap();
        let q = collect_at(&before, em.block_number, em.timestamp).unwrap();
        let expected = derived(&before, &observed);
        assert_eq!(expected, c["expected_pool_after"]);
        assert_eq!(
            serde_json::to_value(&q.next_pool).unwrap(),
            expected,
            "{} full collection state",
            c["id"]
        );
    }
}
