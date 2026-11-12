//! Single-signer temporary classic WSOL accounts; existing user accounts are never closed.
use crate::solana_unsigned::{SolanaAccountMeta as Meta, SolanaInstruction as Instruction};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
use xone_domain::{err, raydium::validate_pubkey, Result};
pub const SYSTEM: &str = "11111111111111111111111111111111";
pub const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const WSOL: &str = "So11111111111111111111111111111111111111112";
fn meta(pubkey: &str, is_signer: bool, is_writable: bool) -> Meta {
    Meta {
        pubkey: pubkey.into(),
        is_signer,
        is_writable,
    }
}
pub fn address(base: &str, seed: &str) -> Result<String> {
    if seed.is_empty() || seed.len() > 32 || !seed.is_ascii() {
        return Err(err("WSOL_TEMP_SEED_INVALID"));
    }
    let mut h = Sha256::new();
    h.update(validate_pubkey(base)?);
    h.update(seed.as_bytes());
    h.update(validate_pubkey(TOKEN)?);
    Ok(bs58::encode(h.finalize()).into_string())
}
pub fn create(base: &str, seed: &str, rent: u64) -> Result<Vec<Instruction>> {
    let temporary = address(base, seed)?;
    let mut data = 3u32.to_le_bytes().to_vec();
    data.extend(validate_pubkey(base)?);
    data.extend((seed.len() as u64).to_le_bytes());
    data.extend(seed.as_bytes());
    data.extend(rent.to_le_bytes());
    data.extend(165u64.to_le_bytes());
    data.extend(validate_pubkey(TOKEN)?);
    let mut init = vec![18u8];
    init.extend(validate_pubkey(base)?);
    Ok(vec![
        Instruction {
            program_id: SYSTEM.into(),
            accounts: vec![meta(base, true, true), meta(&temporary, false, true)],
            data_base64: STANDARD.encode(data),
        },
        Instruction {
            program_id: TOKEN.into(),
            accounts: vec![meta(&temporary, false, true), meta(WSOL, false, false)],
            data_base64: STANDARD.encode(init),
        },
    ])
}
pub fn close(base: &str, seed: &str) -> Result<Instruction> {
    // Re-derive instead of accepting a caller-supplied account to close.
    let temporary = address(base, seed)?;
    Ok(Instruction {
        program_id: TOKEN.into(),
        accounts: vec![
            meta(&temporary, false, true),
            meta(base, false, true),
            meta(base, true, false),
        ],
        data_base64: STANDARD.encode([9u8]),
    })
}
