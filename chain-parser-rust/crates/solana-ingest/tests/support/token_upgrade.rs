//! Loader identity regression shared by the three normal protocol decoders.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use xone_config::PoolConfig;
use xone_domain::{Pool, Result};
use xone_solana_ingest::{raydium::RpcAccount, rpc::TOKEN_PROGRAM};
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const IMMUTABLE: &str = "BPFLoader2111111111111111111111111111111111";
const DATA: &str = "3gvYRKWyXRR9xKWe1ZjPhLY5ZJRN7KDB4rFZFGoJfFk2";

pub fn verify_upgradeable_token(
    pool: &PoolConfig,
    accounts: &BTreeMap<String, RpcAccount>,
    profile: &str,
    decode: impl Fn(&PoolConfig, &BTreeMap<String, RpcAccount>) -> Result<Pool>,
    required: impl Fn(&PoolConfig) -> Result<Vec<String>>,
) {
    let original = decode(pool, accounts).unwrap();
    let original_dependencies = required(pool).unwrap();
    let mut encoded = serde_json::to_value(pool).unwrap();
    let mut code = b"\x7fELF".to_vec();
    code.extend_from_slice(&accounts[TOKEN_PROGRAM].data);
    encoded[profile]["expected_token_program_code_sha256"] =
        serde_json::json!(format!("{:x}", Sha256::digest(&code)));
    encoded[profile]["token_program_upgrade"] =
        serde_json::json!({"programdata":DATA,"deployed_slot":17});
    let upgraded: PoolConfig = serde_json::from_value(encoded.clone()).unwrap();
    let mut observed = accounts.clone();
    let token = observed.get_mut(TOKEN_PROGRAM).unwrap();
    token.owner = LOADER.into();
    token.data = 2u32.to_le_bytes().to_vec();
    token.data.extend(bs58::decode(DATA).into_vec().unwrap());
    let mut data = vec![0; 45];
    data[..4].copy_from_slice(&3u32.to_le_bytes());
    data[4..12].copy_from_slice(&17u64.to_le_bytes());
    data.extend_from_slice(&code);
    observed.insert(
        DATA.into(),
        RpcAccount {
            owner: LOADER.into(),
            data,
            executable: false,
            lamports: 1,
        },
    );
    let result = decode(&upgraded, &observed).unwrap();
    assert_eq!(result.reserve0, original.reserve0);
    assert_eq!(result.reserve1, original.reserve1);
    let dependencies = required(&upgraded).unwrap();
    assert_eq!(dependencies.len(), original_dependencies.len() + 1);
    assert_eq!(
        dependencies
            .iter()
            .filter(|key| key.as_str() == DATA)
            .count(),
        1
    );
    for mutation in [
        "owner",
        "executable",
        "program-tag",
        "program-short",
        "pointer",
        "data-owner",
        "data-executable",
        "data-tag",
        "data-short",
        "authority-option",
        "code",
        "slot",
        "missing",
    ] {
        let mut changed = observed.clone();
        match mutation {
            "owner" => changed.get_mut(TOKEN_PROGRAM).unwrap().owner = IMMUTABLE.into(),
            "executable" => changed.get_mut(TOKEN_PROGRAM).unwrap().executable = false,
            "program-tag" => changed.get_mut(TOKEN_PROGRAM).unwrap().data[0] = 3,
            "program-short" => changed.get_mut(TOKEN_PROGRAM).unwrap().data.truncate(35),
            "pointer" => changed.get_mut(TOKEN_PROGRAM).unwrap().data[4] ^= 1,
            "data-owner" => changed.get_mut(DATA).unwrap().owner = IMMUTABLE.into(),
            "data-executable" => changed.get_mut(DATA).unwrap().executable = true,
            "data-tag" => changed.get_mut(DATA).unwrap().data[0] = 2,
            "data-short" => changed.get_mut(DATA).unwrap().data.truncate(45),
            "authority-option" => changed.get_mut(DATA).unwrap().data[12] = 2,
            "code" => changed.get_mut(DATA).unwrap().data[45] ^= 1,
            "slot" => {
                changed.get_mut(DATA).unwrap().data[4..12].copy_from_slice(&18u64.to_le_bytes())
            }
            "missing" => {
                changed.remove(DATA);
            }
            _ => unreachable!(),
        }
        assert!(
            decode(&upgraded, &changed).is_err(),
            "accepted token mutation {mutation}"
        );
    }
    let mut header_pin = encoded.clone();
    header_pin[profile]["expected_token_program_code_sha256"] = serde_json::json!(format!(
        "{:x}",
        Sha256::digest(&observed[TOKEN_PROGRAM].data)
    ));
    let header_pin: PoolConfig = serde_json::from_value(header_pin).unwrap();
    assert!(
        decode(&header_pin, &observed).is_err(),
        "36-byte program header was accepted as executable code identity"
    );
    let mut inline = observed.clone();
    inline.get_mut(TOKEN_PROGRAM).unwrap().owner = IMMUTABLE.into();
    inline.get_mut(TOKEN_PROGRAM).unwrap().data = code;
    assert!(
        decode(&upgraded, &inline).is_err(),
        "declared upgradeable mode accepted immutable program"
    );
    let mut future = encoded.clone();
    future[profile]["token_program_upgrade"]["deployed_slot"] = serde_json::json!(101);
    let future: PoolConfig = serde_json::from_value(future).unwrap();
    let mut changed = observed.clone();
    changed.get_mut(DATA).unwrap().data[4..12].copy_from_slice(&101u64.to_le_bytes());
    assert!(
        decode(&future, &changed).is_err(),
        "future token deployment accepted at bank 100"
    );
    encoded[profile]
        .as_object_mut()
        .unwrap()
        .remove("token_program_upgrade");
    let undeclared: PoolConfig = serde_json::from_value(encoded).unwrap();
    assert!(
        decode(&undeclared, &observed).is_err(),
        "old immutable profile silently widened"
    );
}
