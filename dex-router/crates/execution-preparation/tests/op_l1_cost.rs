use serde_json::Value;
use xone_domain::Amount;
use xone_execution_preparation::op_l1_cost::{data_cost, decode_parameters};

fn attributes(base: &str, overhead: &str, scalar: &str) -> Vec<u8> {
    let mut data = vec![0x01, 0x5d, 0x8e, 0xb9];
    for value in ["1", "1686068903", base, "0", "0", "0", overhead, scalar] {
        data.extend(value.parse::<Amount>().unwrap().to_be_bytes().unwrap());
    }
    data
}

#[test]
fn regolith_uses_complete_wire_counts_floor_and_explicit_zero_without_signature_padding() {
    let a = attributes("7", "3", "684000");
    let p = decode_parameters("evm:10", 1_686_068_903, &a).unwrap();
    let c = data_cost(&p, &[2, 0, 0, 1], 4).unwrap();
    assert_eq!(c.zero_bytes, 2);
    assert_eq!(c.nonzero_bytes, 2);
    assert_eq!(c.calldata_gas, 40);
    assert_eq!(c.l1_gas_used, "43");
    assert_eq!(c.l1_data_fee_native, "205".parse().unwrap());
    for (base, scalar) in [("0", "1"), ("1", "0"), ("1", "1")] {
        let p = decode_parameters("evm:10", 1_686_068_903, &attributes(base, "0", scalar)).unwrap();
        assert_eq!(
            data_cost(&p, &[2], 1).unwrap().l1_data_fee_native,
            Amount::zero()
        );
    }
}

#[test]
fn attributes_keep_chain_activation_abi_and_l1_time_boundaries_closed() {
    let a = attributes("7", "3", "684000");
    for (chain, start) in [("evm:10", 1_686_068_903), ("evm:8453", 1_686_789_347)] {
        assert!(decode_parameters(chain, start - 1, &a).is_err());
        assert!(decode_parameters(chain, start, &a).is_ok());
        assert!(decode_parameters(chain, 1_710_374_400, &a).is_ok());
        assert!(decode_parameters(chain, 1_710_374_401, &a).is_err());
        assert!(decode_parameters(chain, u64::MAX, &a).is_err());
    }
    assert!(decode_parameters("evm:1", 1_686_068_903, &a).is_err());
    for n in [0, 3, 4, 259, 261, 1024] {
        let mut b = a.clone();
        b.resize(n, 0);
        assert!(decode_parameters("evm:10", 1_686_068_903, &b).is_err());
    }
    for index in [0, 4, 36, 132] {
        let mut b = a.clone();
        b[index] = 255;
        assert!(decode_parameters("evm:10", 1_686_068_903, &b).is_err());
    }
    let mut b = a.clone();
    b[36..68].copy_from_slice(
        &"1686068904"
            .parse::<Amount>()
            .unwrap()
            .to_be_bytes()
            .unwrap(),
    );
    assert!(decode_parameters("evm:10", 1_686_068_903, &b).is_err());
}

#[test]
fn data_fee_rejects_deposit_future_types_and_byte_budget_and_checks_final_uint256() {
    let p = decode_parameters("evm:10", 1_686_068_903, &attributes("1", "0", "1000000")).unwrap();
    for wire in [vec![], vec![0x7e], vec![3], vec![4], vec![0], vec![0x80]] {
        assert!(data_cost(&p, &wire, 100).is_err());
    }
    assert!(data_cost(&p, &[2], 0).is_err());
    assert!(data_cost(&p, &[2, 0], 1).is_err());
    let maximum = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    let p = decode_parameters(
        "evm:10",
        1_686_068_903,
        &attributes(maximum, "0", "1000000"),
    )
    .unwrap();
    assert!(data_cost(&p, &[2], 1).is_err());
    let p = decode_parameters("evm:10", 1_686_068_903, &attributes("1", maximum, "0")).unwrap();
    let cost = data_cost(&p, &[2], 1).unwrap();
    assert_eq!(cost.l1_data_fee_native, Amount::zero());
    assert_eq!(
        cost.l1_gas_used,
        "115792089237316195423570985008687907853269984665640564039457584007913129639951"
    );
    let p = decode_parameters("evm:10", 1_686_068_903, &attributes(maximum, "0", "1")).unwrap();
    // The product exceeds uint256, but the final divided fee fits.
    assert!(data_cost(&p, &[2], 1).is_ok());
}

#[test]
#[ignore = "requires current pinned original op-geth vectors and retained signed public wires"]
fn current_original_op_geth_and_public_receipts_match_rust_data_fees() {
    let path =
        std::env::var("XONE_OP_L1_COST_VECTORS").expect("current reference vectors required");
    let doc: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let rows = doc["cases"].as_array().unwrap();
    assert!(rows.len() >= 60);
    assert_eq!(doc["public_signed_user_transactions"], 3);
    for row in rows {
        let a = hex::decode(row["attributes"].as_str().unwrap()).unwrap();
        let wire = hex::decode(row["encoded_transaction"].as_str().unwrap()).unwrap();
        let p = decode_parameters(
            row["chain_key"].as_str().unwrap(),
            row["timestamp"].as_u64().unwrap(),
            &a,
        )
        .unwrap();
        let result = data_cost(&p, &wire, wire.len());
        if row["overflow"] == true {
            assert!(result.is_err(), "{}", row["id"]);
            continue;
        }
        let result = result.unwrap();
        assert_eq!(
            result.l1_data_fee_native,
            row["fee"].as_str().unwrap().parse().unwrap(),
            "{}",
            row["id"]
        );
        assert_eq!(
            result.l1_gas_used,
            row["gas"].as_str().unwrap(),
            "{}",
            row["id"]
        );
        assert_eq!(result.zero_bytes, row["zeroes"].as_u64().unwrap());
        assert_eq!(result.nonzero_bytes, row["ones"].as_u64().unwrap());
    }
}

#[test]
#[ignore = "requires current normal Go header/body/replay output and independent op-geth vectors"]
fn current_normal_go_committed_inputs_feed_rust_fee_kernel_without_rpc_fee_fields() {
    let source: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_L1_SOURCE_INPUTS").expect("normal Go source output"))
            .unwrap(),
    )
    .unwrap();
    let vectors: Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var("XONE_OP_L1_COST_VECTORS").expect("original op-geth reference"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(source["passed"], true);
    assert_eq!(source["production_ready"], false);
    assert_eq!(source["fee_arithmetic_performed_by_parser"], false);
    assert_eq!(source["projection_mutations_rejected"], 12);
    let inputs = &source["inputs"];
    assert_eq!(inputs["version"], "op-regolith-committed-fee-inputs/1");
    assert_eq!(
        inputs["authority"],
        "RPC_HEADER_BODY_COMMITMENT_NOT_CONSENSUS"
    );
    assert_eq!(inputs["chain_key"], "evm:10");
    let attributes = hex::decode(
        inputs["attributes_calldata"]
            .as_str()
            .unwrap()
            .strip_prefix("0x")
            .unwrap(),
    )
    .unwrap();
    let p = decode_parameters(
        inputs["chain_key"].as_str().unwrap(),
        inputs["execution_timestamp"].as_u64().unwrap(),
        &attributes,
    )
    .unwrap();
    let transactions = inputs["user_transactions"].as_array().unwrap();
    assert_eq!(transactions.len(), 3);
    for (i, tx) in transactions.iter().enumerate() {
        let original = vectors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["transaction_hash"] == tx["hash"])
            .expect("same retained signed public transaction");
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
        assert_eq!(tx["index"], i + 1);
        let cost = data_cost(&p, &wire, wire.len()).unwrap();
        assert_eq!(cost.attributes_sha256, inputs["attributes_sha256"]);
        assert_eq!(cost.l1_gas_used, original["gas"]);
        assert_eq!(
            cost.l1_data_fee_native,
            original["fee"].as_str().unwrap().parse().unwrap()
        );
    }
}
