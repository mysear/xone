use serde_json::Value;
use xone_domain::Amount;
use xone_execution_preparation::op_packed_cost::{
    data_cost, decode_packed_attributes, operator_cost, DataFeeRule, OperatorFeeRule,
};

fn attributes(format: &str, base: &str, scalar: u32, constant: u64) -> Vec<u8> {
    let (selector, length) = match format {
        "ecotone" => ("440a5e20", 164),
        "isthmus" => ("098999be", 176),
        "jovian" => ("3db6be2b", 178),
        _ => unreachable!(),
    };
    let mut data = hex::decode(selector).unwrap();
    data.resize(length, 0);
    data[4..8].copy_from_slice(&1_000_000u32.to_be_bytes());
    data[36..68].copy_from_slice(&base.parse::<Amount>().unwrap().to_be_bytes().unwrap());
    if length >= 176 {
        data[164..168].copy_from_slice(&scalar.to_be_bytes());
        data[168..176].copy_from_slice(&constant.to_be_bytes());
    }
    data
}

#[test]
fn packed_formats_are_exact_and_operator_unknown_is_not_zero() {
    for (format, length) in [("ecotone", 164), ("isthmus", 176), ("jovian", 178)] {
        let a = attributes(format, "1", 0, 0);
        let p = decode_packed_attributes(&a).unwrap();
        let expected: Option<u64> = if format == "ecotone" { None } else { Some(0) };
        assert_eq!(
            serde_json::to_value(&p).unwrap()["operator_fee_constant"],
            serde_json::to_value(expected).unwrap()
        );
        for n in [0, 3, 4, 163, 165, 175, 177, 179, length + 1] {
            let mut changed = a.clone();
            changed.resize(n, 0);
            assert!(decode_packed_attributes(&changed).is_err());
        }
        let mut changed = a.clone();
        changed[0] ^= 1;
        assert!(decode_packed_attributes(&changed).is_err());
        let result = operator_cost(&p, OperatorFeeRule::Jovian, 1000);
        if expected.is_some() {
            assert_eq!(result.unwrap().operator_fee_native, Amount::zero());
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn packed_cost_uses_explicit_model_floor_and_rejects_wrong_types_and_budget() {
    let p = decode_packed_attributes(&attributes("ecotone", "1", 0, 0)).unwrap();
    let ecotone = data_cost(&p, DataFeeRule::Ecotone, &[2, 0], 2).unwrap();
    assert_eq!(ecotone.l1_gas_used, 20);
    assert_eq!(ecotone.l1_data_fee_native, "20".parse().unwrap());
    let fjord = data_cost(&p, DataFeeRule::Fjord, &[2, 0], 2).unwrap();
    assert_eq!(fjord.fastlz_bytes, 3);
    assert_eq!(fjord.estimated_da_bytes_scaled, Some(100_000_000));
    assert_eq!(fjord.l1_gas_used, 1600);
    assert_eq!(fjord.l1_data_fee_native, "1600".parse().unwrap());
    for wire in [vec![], vec![0], vec![3], vec![4], vec![0x7e], vec![0x80]] {
        assert!(data_cost(&p, DataFeeRule::Fjord, &wire, 100).is_err());
    }
    assert!(data_cost(&p, DataFeeRule::Fjord, &[2], 0).is_err());
    assert!(data_cost(&p, DataFeeRule::Fjord, &[2, 0], 1).is_err());
    let p = decode_packed_attributes(&attributes("jovian", "0", 0, 0)).unwrap();
    assert!(data_cost(&p, DataFeeRule::Ecotone, &[2], 1).is_err());
    assert!(data_cost(&p, DataFeeRule::Fjord, &[4], 1).is_ok());
    assert_eq!(
        data_cost(&p, DataFeeRule::Fjord, &[2], 1)
            .unwrap()
            .l1_data_fee_native,
        Amount::zero()
    );
    assert!(operator_cost(&p, OperatorFeeRule::Isthmus, 1).is_err());
}

#[test]
fn operator_units_floor_constant_and_max_width_remain_distinct() {
    let p = decode_packed_attributes(&attributes("isthmus", "0", 1, 7)).unwrap();
    assert_eq!(
        operator_cost(&p, OperatorFeeRule::Isthmus, 999_999)
            .unwrap()
            .operator_fee_native,
        "7".parse().unwrap()
    );
    assert_eq!(
        operator_cost(&p, OperatorFeeRule::Isthmus, 1_000_001)
            .unwrap()
            .operator_fee_native,
        "8".parse().unwrap()
    );
    assert_eq!(
        operator_cost(&p, OperatorFeeRule::Jovian, 1_000_001)
            .unwrap()
            .operator_fee_native,
        "100000107".parse().unwrap()
    );
    let p = decode_packed_attributes(&attributes("jovian", "0", u32::MAX, u64::MAX)).unwrap();
    let fee = operator_cost(&p, OperatorFeeRule::Jovian, u64::MAX).unwrap();
    let expected = num_bigint::BigUint::from(u64::MAX) * u32::MAX * 100u32 + u64::MAX;
    assert_eq!(fee.operator_fee_native.0, expected);
    let maximum = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    let p = decode_packed_attributes(&attributes("ecotone", maximum, 0, 0)).unwrap();
    assert!(data_cost(&p, DataFeeRule::Fjord, &[2], 1).is_err());
}

#[test]
#[ignore = "requires current normal Go source and unchanged original op-geth vectors"]
fn original_op_geth_packed_costs_and_current_public_signed_users_match_rust() {
    let doc: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_PACKED_VECTORS").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(doc["passed"], true);
    assert_eq!(doc["public_signed_user_transactions"], 49);
    let rows = doc["cases"].as_array().unwrap();
    assert!(rows.len() > 500);
    let mut public = 0;
    let mut overflow = 0;
    for row in rows {
        let p =
            decode_packed_attributes(&hex::decode(row["attributes"].as_str().unwrap()).unwrap())
                .unwrap();
        let wire = hex::decode(row["encoded_transaction"].as_str().unwrap()).unwrap();
        let rule = match row["data_rule"].as_str().unwrap() {
            "Ecotone" => DataFeeRule::Ecotone,
            "Fjord" => DataFeeRule::Fjord,
            _ => unreachable!(),
        };
        let result = data_cost(&p, rule, &wire, wire.len());
        if row["overflow"] == true {
            assert!(result.is_err(), "{}", row["id"]);
            overflow += 1;
            continue;
        }
        let c = result.unwrap_or_else(|e| panic!("{}: {}", row["id"], e));
        assert_eq!(
            c.l1_data_fee_native,
            row["fee"].as_str().unwrap().parse().unwrap(),
            "{}",
            row["id"]
        );
        assert_eq!(c.l1_gas_used.to_string(), row["l1_gas_used"]);
        assert_eq!(
            c.fastlz_bytes,
            row["fastlz_bytes"].as_u64().unwrap(),
            "{}",
            row["id"]
        );
        assert_eq!(c.zero_bytes, row["zeroes"].as_u64().unwrap());
        assert_eq!(c.nonzero_bytes, row["ones"].as_u64().unwrap());
        if rule == DataFeeRule::Fjord {
            assert_eq!(
                c.estimated_da_bytes_scaled.unwrap().to_string(),
                row["estimated_da_bytes_scaled"]
            );
        }
        if let Some(rule) = row["operator_rule"].as_str() {
            let rule = match rule {
                "Isthmus" => OperatorFeeRule::Isthmus,
                "Jovian" => OperatorFeeRule::Jovian,
                _ => unreachable!(),
            };
            let gas = row["gas_used"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                operator_cost(&p, rule, gas).unwrap().operator_fee_native,
                row["operator_fee"].as_str().unwrap().parse().unwrap()
            );
        }
        if !row["transaction_hash"].is_null() {
            public += 1;
            assert_eq!(c.l1_data_fee_native.to_string(), row["receipt_fee"]);
            assert_eq!(c.l1_gas_used.to_string(), row["receipt_gas"]);
        }
    }
    assert_eq!(public, 49);
    assert!(overflow > 0);
}

#[test]
#[ignore = "requires current normal versioned Go source and original upgrade vectors"]
fn normal_versioned_op_sources_feed_rust_across_four_real_upgrade_boundaries() {
    let read = |name| -> Value {
        serde_json::from_slice(&std::fs::read(std::env::var(name).unwrap()).unwrap()).unwrap()
    };
    let source = read("XONE_OP_UPGRADE_INPUTS");
    let vectors = read("XONE_OP_UPGRADE_VECTORS");
    assert_eq!(source["passed"], true);
    assert_eq!(source["projection_mutations_rejected"], 60);
    assert_eq!(source["old_archive_compatible"], true);
    assert_eq!(source["parser_fee_math_added"], false);
    assert_eq!(source["production_ready"], false);
    assert_eq!(vectors["public_signed_users"], 128);
    let blocks = source["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 8);
    let mut users = 0;
    let mut upgrades = 0;
    for block in blocks {
        let p = &block["inputs"];
        assert_eq!(
            p["authority"],
            "RPC_HEADER_BODY_RECEIPTS_COMMITMENT_NOT_CONSENSUS"
        );
        assert_eq!(p["chain_key"], "evm:10");
        let transactions = p["user_transactions"].as_array().unwrap();
        if p["upgrade_block"] == true {
            assert!(transactions.is_empty());
            upgrades += 1;
            continue;
        }
        let attributes = hex::decode(
            p["attributes_calldata"]
                .as_str()
                .unwrap()
                .strip_prefix("0x")
                .unwrap(),
        )
        .unwrap();
        let parameters = decode_packed_attributes(&attributes).unwrap();
        let rule = match p["data_fee_rule"].as_str().unwrap() {
            "Ecotone" => DataFeeRule::Ecotone,
            "Fjord" => DataFeeRule::Fjord,
            _ => panic!("unknown source data rule"),
        };
        for tx in transactions {
            let original = vectors["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["transaction_hash"] == tx["hash"])
                .unwrap();
            let wire = hex::decode(
                tx["encoded_transaction"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("0x")
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(hex::encode(&wire), original["encoded_transaction"]);
            assert_eq!(hex::encode(&attributes), original["attributes"]);
            assert_eq!(block["name"], original["block_name"]);
            let c = data_cost(&parameters, rule, &wire, wire.len()).unwrap();
            assert_eq!(c.l1_data_fee_native.to_string(), original["fee"]);
            assert_eq!(c.l1_gas_used.to_string(), original["l1_gas_used"]);
            assert_eq!(
                serde_json::to_value(&c).unwrap()["parameters"]["attributes_sha256"],
                p["attributes_sha256"]
            );
            let op_rule = match p["operator_fee_rule"].as_str().unwrap() {
                "NotActive" => None,
                "Isthmus" => Some(OperatorFeeRule::Isthmus),
                "Jovian" => Some(OperatorFeeRule::Jovian),
                _ => panic!("unknown source operator rule"),
            };
            if let Some(rule) = op_rule {
                let gas = tx["gas_used"].as_u64().unwrap();
                assert_eq!(gas.to_string(), original["gas_used"]);
                assert_eq!(
                    operator_cost(&parameters, rule, gas)
                        .unwrap()
                        .operator_fee_native
                        .to_string(),
                    original["operator_fee"]
                );
            } else {
                assert!(operator_cost(
                    &parameters,
                    OperatorFeeRule::Isthmus,
                    tx["gas_used"].as_u64().unwrap()
                )
                .is_err());
            }
            users += 1;
        }
    }
    assert_eq!(users, 128);
    assert_eq!(upgrades, 4);
}
