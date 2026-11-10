//! Each service reads its own original bank and complete Pool projection.
use num_bigint::BigUint;
use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::OnceLock};
use xone_domain::{multi_asset::MultiAssetState, wire, Amount, Pool};

fn fixtures() -> &'static (Value, Value) {
    static FIXTURES: OnceLock<(Value, Value)> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = std::fs::read(base.join("test/contracts/managed-v2-scheduled.json")).unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let pools: Value = serde_json::from_slice(
            &std::fs::read(base.join("test/contracts/managed-v2-scheduled-pools.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            pools["original_raw"]["sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(pools["original_raw"]["bytes"], bytes.len());
        assert_eq!(pools["states"].as_object().unwrap().len(), 234);
        (raw, pools)
    })
}
fn raw_state<'a>(raw: &'a Value, id: &'a str) -> (&'a str, &'a Value) {
    let (namespace, local) = id.split_once('/').unwrap();
    let state = raw["branches"][namespace]["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == local)
        .unwrap();
    (namespace, state)
}
fn amount_hex(v: &Value) -> Amount {
    Amount(
        BigUint::parse_bytes(v.as_str().unwrap().trim_start_matches("0x").as_bytes(), 16).unwrap(),
    )
}
#[test]
fn all_original_source_headers_raw_words_getters_and_code_identities_match() {
    let (raw, pools) = fixtures();
    for (id, value) in pools["states"].as_object().unwrap() {
        let (ns, state) = raw_state(raw, id);
        let p: Pool = serde_json::from_value(value.clone()).unwrap();
        p.validate().unwrap_or_else(|e| panic!("{id}: {e}"));
        let multi = p.multi_asset.as_ref().unwrap();
        let m = multi.managed_v2.as_ref().unwrap();
        assert!(
            m.execution_context.is_none(),
            "{id}: actual source must stay canonical"
        );
        assert_eq!(m.block_hash, state["header"]["hash"].as_str().unwrap());
        for (actual, key) in [(m.block_number, "number"), (m.timestamp, "timestamp")] {
            assert_eq!(
                actual,
                u64::from_str_radix(
                    state["header"][key]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("0x"),
                    16
                )
                .unwrap(),
                "{id}: source {key}"
            );
        }
        let schedule = m.schedule.as_ref().unwrap();
        assert_eq!(
            schedule.pool_state_word,
            amount_hex(&state["packed"]["pool_word"]),
            "{id}: pool slot9"
        );
        for (token, observed) in schedule
            .tokens
            .iter()
            .zip(state["packed"]["tokens"].as_array().unwrap())
        {
            assert_eq!(token.token, observed["token"].as_str().unwrap());
            let word = if ns == "two" {
                &observed["raw_word"]
            } else {
                &observed["word"]
            };
            assert_eq!(
                token.state_word,
                amount_hex(word),
                "{id}: raw token mapping11"
            );
        }
        let view = schedule.at(m.timestamp).unwrap();
        let weights = if ns == "two" {
            &state["current_weights"]
        } else {
            &state["weights"]
        };
        let fee = if ns == "two" {
            &state["current_fee"]
        } else {
            &state["fee"]
        };
        assert_eq!(
            serde_json::to_value(&view.weights).unwrap(),
            *weights,
            "{id}: original weights"
        );
        assert_eq!(view.weights, multi.weights, "{id}: public weights");
        assert_eq!(
            view.swap_fee.to_string(),
            fee.as_str().unwrap(),
            "{id}: original fee"
        );
        assert_eq!(p.fee_numerator.to_string(), view.swap_fee.to_string());
        for (asset, scale) in multi.assets.iter().zip(view.scaling) {
            assert_eq!(asset.scaling, scale, "{id}: scaling");
            let account = state["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["address"] == asset.token)
                .unwrap();
            assert_eq!(asset.code_sha256, account["code_sha256"].as_str().unwrap());
        }
        let account = state["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["address"] == p.pool_id)
            .unwrap();
        assert_eq!(multi.code_sha256, account["code_sha256"].as_str().unwrap());
        let encoded = multi.to_proto().unwrap().encode_to_vec();
        let decoded = wire::MultiAssetState::decode(encoded.as_slice()).unwrap();
        assert_eq!(
            MultiAssetState::from_proto(decoded).unwrap(),
            *multi,
            "{id}: complete field40 wire roundtrip"
        );
    }
}
#[test]
fn original_nonunit_weight_sums_and_derived_context_remain_distinct() {
    let (_, pools) = fixtures();
    let mut deltas = std::collections::BTreeSet::new();
    for value in pools["states"].as_object().unwrap().values() {
        let p: Pool = serde_json::from_value(value.clone()).unwrap();
        let multi = p.multi_asset.as_ref().unwrap();
        let sum: BigUint = multi.weights.iter().map(|w| &w.0).sum();
        deltas.insert(num_bigint::BigInt::from(sum) - num_bigint::BigInt::from(10u64.pow(18)));
    }
    for delta in [-2, -1, 0, 1, 2] {
        assert!(deltas.contains(&num_bigint::BigInt::from(delta)));
    }
    for c in pools["cases"].as_array().unwrap() {
        let p: Pool = serde_json::from_value(c["expected_pool_after"].clone()).unwrap();
        p.validate().unwrap();
        let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
        assert!(m.execution_context.is_some());
        assert!(
            m.to_proto().is_err(),
            "derived computation state cannot become canonical wire"
        );
    }
}
