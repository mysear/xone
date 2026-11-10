//! Two physical original pools per common source H; no sibling wallet synthesis.
use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::OnceLock};
use xone_domain::{multi_asset::MultiAssetState, validate_pool_dependencies, wire, Pool};
fn fixtures() -> &'static (Value, Value) {
    static FIXTURES: OnceLock<(Value, Value)> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            std::fs::read(base.join("test/contracts/managed-v2-scheduled-shared.json")).unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let thin: Value = serde_json::from_slice(
            &std::fs::read(base.join("test/contracts/managed-v2-scheduled-shared-pools.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(thin["original_raw"]["bytes"], bytes.len());
        assert_eq!(
            thin["original_raw"]["sha256"],
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(raw["banks"].as_array().unwrap().len(), 72);
        assert_eq!(thin["banks"].as_object().unwrap().len(), 72);
        assert_eq!(thin["cases"].as_array().unwrap().len(), 52);
        (raw, thin)
    })
}
#[test]
fn all_72_common_banks_preserve_full_sources_profiles_and_joint_cash_constraints() {
    let (raw, thin) = fixtures();
    for native in raw["banks"].as_array().unwrap() {
        let id = native["id"].as_str().unwrap();
        let bank = &thin["banks"][id];
        assert_eq!(bank["header"], native["header"]);
        let rows = bank["pools"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let pools: Vec<Pool> = rows
            .iter()
            .map(|r| serde_json::from_value(r["pool"].clone()).unwrap())
            .collect();
        validate_pool_dependencies(pools.iter()).unwrap_or_else(|e| panic!("{id}: {e}"));
        for (index, p) in pools.iter().enumerate() {
            p.validate().unwrap();
            let multi = p.multi_asset.as_ref().unwrap();
            let m = multi.managed_v2.as_ref().unwrap();
            assert!(m.execution_context.is_none());
            assert_eq!(m.block_hash, native["header"]["hash"].as_str().unwrap());
            assert_eq!(
                m.block_number,
                u64::from_str_radix(
                    native["header"]["number"]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("0x"),
                    16
                )
                .unwrap()
            );
            assert_eq!(
                m.timestamp,
                u64::from_str_radix(
                    native["header"]["timestamp"]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("0x"),
                    16
                )
                .unwrap()
            );
            assert_eq!(
                m.schedule.is_some(),
                p.protocol == xone_domain::managed_v2_schedule::PROTOCOL
            );
            if id.starts_with("mixed/") && index == 1 {
                assert_eq!(p.protocol, xone_domain::managed_v2_state::PROTOCOL);
                assert!(m.schedule.is_none());
            }
            if let Some(s) = &m.schedule {
                let view = s.at(m.timestamp).unwrap();
                assert_eq!(view.weights, multi.weights);
                assert_eq!(view.swap_fee.to_string(), p.fee_numerator.to_string());
            }
            let bytes = multi.to_proto().unwrap().encode_to_vec();
            let decoded = wire::MultiAssetState::decode(bytes.as_slice()).unwrap();
            assert_eq!(
                MultiAssetState::from_proto(decoded).unwrap(),
                *multi,
                "{id}: complete mixed/scheduled wire"
            );
        }
    }
}
#[test]
fn all_52_joint_after_banks_are_explicitly_derived_and_cannot_publish_as_source() {
    let (_, thin) = fixtures();
    for c in thin["cases"].as_array().unwrap() {
        let pools: Vec<Pool> = serde_json::from_value(c["expected_pools_after"].clone()).unwrap();
        assert_eq!(pools.len(), 2);
        validate_pool_dependencies(pools.iter()).unwrap_or_else(|e| panic!("{}: {e}", c["id"]));
        for p in pools {
            p.validate().unwrap();
            let m = p.multi_asset.as_ref().unwrap().managed_v2.as_ref().unwrap();
            assert!(m.execution_context.is_some());
            assert!(
                m.to_proto().is_err(),
                "derived common bank cannot enter canonical wire"
            );
        }
    }
}
