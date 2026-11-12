use base64::{engine::general_purpose::STANDARD, Engine};
use xone_execution_plan::solana_wsol;
const BASE: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
#[test]
fn seeded_wsol_matches_official_system_encoding_without_new_signer() {
    assert_eq!(
        solana_wsol::address(BASE, "xone-test").unwrap(),
        "8iGTMMCyU6zU7qsdxkFiWbr3nBVxybG2ciW74RY9cogW"
    );
    let ix = solana_wsol::create(BASE, "xone-test", 2039280).unwrap();
    let raw = STANDARD.decode(&ix[0].data_base64).unwrap();
    assert_eq!(&raw[..4], &3u32.to_le_bytes());
    assert_eq!(u64::from_le_bytes(raw[36..44].try_into().unwrap()), 9);
    assert_eq!(&raw[44..53], b"xone-test");
    assert_eq!(u64::from_le_bytes(raw[53..61].try_into().unwrap()), 2039280);
    assert_eq!(u64::from_le_bytes(raw[61..69].try_into().unwrap()), 165);
    assert_eq!(ix[0].accounts.len(), 2);
    assert!(!ix[0].accounts[1].is_signer);
    let init = STANDARD.decode(&ix[1].data_base64).unwrap();
    assert_eq!(init[0], 18);
    assert_eq!(init.len(), 33);
    let close = solana_wsol::close(BASE, "xone-test").unwrap();
    assert_eq!(close.data_base64, "CQ==");
    assert_eq!(close.accounts[0].pubkey, ix[0].accounts[1].pubkey);
    assert_eq!(close.accounts[1].pubkey, BASE);
    assert_eq!(close.accounts[2].pubkey, BASE);
    assert!(close.accounts[2].is_signer);
}
#[test]
fn temporary_address_rejects_empty_oversize_or_non_ascii_seeds() {
    for seed in ["", "é", "12345678901234567890123456789012345"] {
        assert!(solana_wsol::address(BASE, seed).is_err());
    }
}
