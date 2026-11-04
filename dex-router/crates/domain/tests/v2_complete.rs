use prost::Message;
use serde_json::{json, Value};
use xone_domain::{
    hash, scope_hash,
    wire_conversion::{snapshot_from_proto, snapshot_to_proto},
    BlockRef, Pool, Snapshot,
};
const PAIR: &str = "8b5db55fa9ab3b9527508d4abe0b39eb588bf310270c8e04b3f38214e8ba63b4";
const FACTORY: &str = "3abc53f12a9cb8ae37ebfada9efc261c1ab4c2759d161e341a49bf67df3f8321";
fn value() -> Value {
    json!({"chain_key":"evm:31337","pool_id":format!("0x{}","1".repeat(40)),"protocol":"uniswap-v2-complete","token0":format!("0x{}","2".repeat(40)),"token1":format!("0x{}","3".repeat(40)),"reserve0":"1000","reserve1":"2000","fee_numerator":3,"fee_denominator":1000,"adapter_version":"uniswap-v2/core-1.0.1-full-swap-1","token_behavior":"standard","v2":{"factory":format!("0x{}","4".repeat(40)),"code_sha256":PAIR,"swap":{"raw_balance0":"1050","raw_balance1":"2000","block_timestamp_last":1790812700,"price0_cumulative_last":"340282366920938463463374607431768211456","price1_cumulative_last":"680564733841876926926749214863536422912","unlocked":true,"factory_code_sha256":FACTORY,"bank_block_number":12,"bank_block_hash":format!("0x{}","a".repeat(64)),"bank_unix_timestamp":1790812800}}})
}
fn snapshot(pool: Pool) -> Snapshot {
    let mut s: Snapshot =
        serde_json::from_str(include_str!("../../../test/fixtures/evm/baseline.json")).unwrap();
    s.reference.block = BlockRef {
        chain_key: "evm:31337".into(),
        number_or_slot: 12,
        hash: format!("0x{}", "a".repeat(64)),
        parent_hash: format!("0x{}", "b".repeat(64)),
        timestamp_ms: 1790812800000,
    };
    s.reference.scope_hash = scope_hash(std::slice::from_ref(&pool)).unwrap();
    s.reference.adapter_set_hash = hash(&vec![pool.adapter_version.clone()]).unwrap();
    s.pools = vec![pool];
    s
}
#[test]
fn complete_v2_state_preserves_unsynced_balances_and_full_wire_widths() {
    let pool: Pool = serde_json::from_value(value()).expect("complete V2 must deserialize");
    pool.validate().unwrap();
    let s = snapshot(pool);
    s.validate(
        &s.reference.owner_service,
        s.reference.owner_assignment_version,
    )
    .unwrap();
    let w = snapshot_to_proto(&s).unwrap();
    assert_eq!(snapshot_from_proto(w).unwrap(), s);
}
#[test]
fn complete_v2_rejects_missing_state_unknown_identity_and_legacy_profile_smuggling() {
    for bad in 0..16 {
        let mut p = value();
        match bad {
            0 => {
                p["v2"].as_object_mut().unwrap().remove("swap");
            }
            1 => p["v2"]["code_sha256"] = "a".repeat(64).into(),
            2 => p["v2"]["swap"]["factory_code_sha256"] = "a".repeat(64).into(),
            3 => p["v2"]["swap"]["unlocked"] = false.into(),
            4 => {
                p["reserve0"] = (num_bigint::BigUint::from(1u8) << 112usize)
                    .to_string()
                    .into()
            }
            5 => {
                p["v2"]["swap"]["raw_balance0"] = (num_bigint::BigUint::from(1u8) << 256usize)
                    .to_string()
                    .into()
            }
            6 => p["v2"]["swap"]["bank_block_hash"] = "0x00".into(),
            7 => p["v2"]["swap"]["bank_unix_timestamp"] = u64::MAX.into(),
            8 => p["token_behavior"] = "fee-on-transfer".into(),
            9 => p["token0"] = p["token1"].clone(),
            10 => p["v2"]["swap"]["unknown"] = 1.into(),
            11 => p["adapter_version"] = "uniswap-v2/1".into(),
            12 => p["v2"]["factory"] = format!("0x{}", "0".repeat(40)).into(),
            13..=15 => {
                let name = match bad {
                    13 => "uniswap-v2",
                    14 => "pancake-v2",
                    _ => "sushiswap-v2",
                };
                p["protocol"] = name.into();
                let profile = xone_domain::protocols::protocol(name).unwrap();
                p["adapter_version"] = profile.adapter.into();
                if profile.fee_denominator != 0 {
                    p["fee_numerator"] = profile.fee_numerator.into();
                    p["fee_denominator"] = profile.fee_denominator.into();
                }
                let mut old = p.clone();
                old["v2"].as_object_mut().unwrap().remove("swap");
                serde_json::from_value::<Pool>(old)
                    .unwrap()
                    .validate()
                    .expect("legacy fixture remains valid without new state");
            }
            _ => unreachable!(),
        }
        let parsed = serde_json::from_value::<Pool>(p);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "bad={bad}"
        );
    }
}
#[test]
fn complete_v2_snapshot_requires_exact_bank_number_hash_and_timestamp() {
    let pool: Pool = serde_json::from_value(value()).unwrap();
    for bad in 0..3 {
        let mut s = snapshot(pool.clone());
        match bad {
            0 => s.reference.block.number_or_slot += 1,
            1 => s.reference.block.hash = format!("0x{}", "c".repeat(64)),
            _ => s.reference.block.timestamp_ms += 1,
        };
        assert!(
            s.validate(
                &s.reference.owner_service,
                s.reference.owner_assignment_version
            )
            .is_err(),
            "bad={bad}"
        );
    }
}
#[test]
fn legacy_v2_wire_keeps_field_twenty_absent() {
    let mut p = value();
    p["protocol"] = "uniswap-v2".into();
    p["adapter_version"] = "uniswap-v2/1".into();
    p["v2"].as_object_mut().unwrap().remove("swap");
    let pool: Pool = serde_json::from_value(p).unwrap();
    pool.validate().unwrap();
    let wire = snapshot_to_proto(&snapshot(pool.clone()))
        .unwrap()
        .pools
        .remove(0)
        .v2
        .unwrap();
    assert_eq!(
        hex::encode(wire.encode_to_vec()),
        format!(
            "0a2a{}1220{}",
            hex::encode(pool.v2.as_ref().unwrap().factory.as_bytes()),
            PAIR
        )
    );
    assert!(!serde_json::to_value(pool).unwrap()["v2"]
        .as_object()
        .unwrap()
        .contains_key("swap"));
}

#[test]
fn complete_v2_mandatory_zero_valued_json_and_wire_fields_have_presence() {
    for name in [
        "raw_balance0",
        "raw_balance1",
        "block_timestamp_last",
        "price0_cumulative_last",
        "price1_cumulative_last",
        "unlocked",
        "factory_code_sha256",
        "bank_block_number",
        "bank_block_hash",
        "bank_unix_timestamp",
    ] {
        let mut v = value();
        v["v2"]["swap"].as_object_mut().unwrap().remove(name);
        assert!(
            serde_json::from_value::<Pool>(v).is_err(),
            "missing JSON {name}"
        );
    }
    let pool: Pool = serde_json::from_value(value()).unwrap();
    for missing in 0..7 {
        let mut w = snapshot_to_proto(&snapshot(pool.clone())).unwrap();
        let swap = w.pools[0].v2.as_mut().unwrap().swap.as_mut().unwrap();
        match missing {
            0 => swap.block_timestamp_last = None,
            1 => swap.bank_block_number = None,
            2 => swap.bank_unix_timestamp = None,
            3 => swap.raw_balance0 = None,
            4 => swap.raw_balance1 = None,
            5 => swap.price0_cumulative_last = None,
            _ => swap.price1_cumulative_last = None,
        };
        assert!(snapshot_from_proto(w).is_err(), "missing wire {missing}");
    }
    let mut s = snapshot(pool);
    s.reference.block.number_or_slot = 0;
    s.reference.block.timestamp_ms = 0;
    let state = s.pools[0].v2.as_mut().unwrap().swap.as_mut().unwrap();
    state.block_timestamp_last = 0;
    state.bank_block_number = 0;
    state.bank_unix_timestamp = 0;
    assert_eq!(
        snapshot_from_proto(snapshot_to_proto(&s).unwrap()).unwrap(),
        s
    );
}
