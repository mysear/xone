//! Original MetaUSD, initialized EIP-1167 clones, real ERC20 bookkeeping.
use num_bigint::BigUint;
use serde_json::Value;
use sha2::{Digest, Sha256};
use xone_protocol_math::curve_multi;

fn n(v: &Value) -> BigUint {
    v.as_str().unwrap().parse().unwrap()
}
fn pair(v: &Value) -> Vec<BigUint> {
    v.as_array().unwrap().iter().map(n).collect()
}
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../../test/contracts/curve-meta-direct.json"
    ))
    .unwrap()
}
fn quote(row: &Value) -> xone_domain::Result<curve_multi::CurveQuote> {
    curve_multi::quote_meta_exact_in(
        &pair(&row["balances"]),
        &n(&row["amp"]),
        &n(&row["meta_rate"]),
        &n(&row["base_virtual_price"]),
        &n(&row["fee"]),
        &n(&row["admin_fee"]),
        &n(&row["input"]),
        row["index_in"].as_u64().unwrap() as usize,
        row["index_out"].as_u64().unwrap() as usize,
    )
}
fn compare(row: &Value) {
    let q = quote(row).unwrap();
    let i = row["index_in"].as_u64().unwrap() as usize;
    let j = row["index_out"].as_u64().unwrap() as usize;
    assert_eq!(q.amount_out, n(&row["expected"]["amount_out"]));
    assert_eq!(q.next_balances, pair(&row["expected"]["next_balances"]));
    let mut raw = pair(&row["stored_balances"]);
    let mut admin = pair(&row["admin_balances"]);
    raw[i] += n(&row["input"]);
    raw[j] -= &q.amount_out;
    admin[j] += &q.admin_fee_amount;
    assert_eq!(raw, pair(&row["expected"]["next_stored_balances"]));
    assert_eq!(admin, pair(&row["expected"]["next_admin_balances"]));
    for k in 0..2 {
        assert_eq!(raw[k], &q.next_balances[k] + &admin[k]);
    }
    assert_eq!(row["expected"]["status"], 1);
}
#[test]
fn original_clone_identity_and_32_sequential_exchanges() {
    let c = corpus();
    assert_eq!(
        c["reference"]["revision"],
        "5272fd2b1199f185d9cd94822be9262c387dc2c2"
    );
    assert_eq!(
        c["reference"]["runtime_sha256"],
        "c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d"
    );
    assert_eq!(c["reference"]["allow_unlimited_contract_size"], false);
    let scenarios = c["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 4);
    for (s, decimals) in scenarios.iter().zip([0u64, 6, 8, 18]) {
        assert_eq!(s["meta_decimals"], decimals);
        let h = s["implementation_runtime_hex"]
            .as_str()
            .unwrap()
            .trim_start_matches("0x");
        let bytes: Vec<u8> = (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(bytes.len(), 20060);
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            c["reference"]["runtime_sha256"].as_str().unwrap()
        );
        assert_eq!(
            s["pool_runtime_hex"].as_str().unwrap(),
            format!(
                "0x363d3d373d3d3d363d73{}5af43d82803e903d91602b57fd5bf3",
                s["implementation"].as_str().unwrap()[2..].to_lowercase()
            )
        );
        assert_eq!(
            s["seed"]["method"],
            "add_liquidity(uint256[2],uint256,address)"
        );
        assert_eq!(s["seed"]["status"], 1);
        let steps = s["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 8);
        for (k, row) in steps.iter().enumerate() {
            assert_eq!(row["index_in"], (k % 2) as u64);
            assert_eq!(
                n(&row["meta_rate"]),
                BigUint::from(10u8).pow(36 - decimals as u32)
            );
            assert_eq!(row["amp"], "12300");
            assert_eq!(row["timestamp"], 1767225600u64 + k as u64);
            assert_eq!(
                row["base_virtual_price"],
                scenarios[0]["steps"][k]["base_virtual_price"]
            );
            if k > 0 {
                for (before, after) in [
                    ("balances", "next_balances"),
                    ("stored_balances", "next_stored_balances"),
                    ("admin_balances", "next_admin_balances"),
                ] {
                    assert_eq!(row[before], steps[k - 1]["expected"][after]);
                }
            }
            compare(row);
        }
    }
}
#[test]
fn native_zero_dust_overflow_and_transfer_rollback_probes() {
    let c = corpus();
    let probes = c["probes"].as_array().unwrap();
    assert_eq!(probes.len(), 8);
    for row in probes {
        if row["expected"]["reverted"] == true {
            assert_eq!(row["expected"]["status"], 0);
            for (before, after) in [
                ("balances", "next_balances"),
                ("stored_balances", "next_stored_balances"),
                ("admin_balances", "next_admin_balances"),
            ] {
                assert_eq!(row[before], row["expected"][after]);
            }
            assert_eq!(
                row["wallet_balances"],
                row["expected"]["next_wallet_balances"]
            );
            if row["kind"] == "arithmetic" {
                assert!(quote(row).is_err());
            } else {
                assert!(quote(row).is_ok());
            }
        } else {
            compare(row);
        }
    }
}
