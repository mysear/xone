use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::KeyPair;
use serde_json::Value;
use xone_execution_plan::solana_signed::verify;
#[test]
fn signature_verifier_requires_exact_message_payer_and_real_ed25519_signature() {
    let data: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../test/contracts/sbf-preparations.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut unsigned = STANDARD
        .decode(data["meteora"]["unsignedTransaction"].as_str().unwrap())
        .unwrap();
    let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).unwrap();
    let caller = bs58::encode(key.public_key().as_ref()).into_string();
    // The fixture has a single payer at the first canonical account key.
    let offset = 69;
    unsigned[offset..offset + 32].copy_from_slice(key.public_key().as_ref());
    let mut signed = unsigned.clone();
    signed[1..65].copy_from_slice(key.sign(&unsigned[65..]).as_ref());
    let info = verify(&unsigned, &signed, &caller).unwrap();
    assert_eq!(info.account_keys[0], caller);
    assert!(!info.signature.is_empty());
    let mut tamper = signed.clone();
    tamper[70] ^= 1;
    assert!(verify(&unsigned, &tamper, &caller).is_err());
    let mut tamper = signed.clone();
    tamper[1] ^= 1;
    assert_eq!(
        verify(&unsigned, &tamper, &caller).unwrap_err().0,
        "WALLET_SIGNATURE_INVALID"
    );
    assert!(verify(&unsigned, &unsigned, &caller).is_err());
    assert!(verify(
        &unsigned,
        &signed,
        "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso"
    )
    .is_err());
}
