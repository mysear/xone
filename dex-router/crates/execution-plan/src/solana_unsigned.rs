//! Single-signer legacy Solana wire format. No RPC, signing or broadcast.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::collections::BTreeMap;
use xone_domain::{err, Result};
fn bad() -> xone_domain::Error {
    err("SOLANA_UNSIGNED_INVALID")
}
fn pubkey(s: &str) -> Result<[u8; 32]> {
    let raw: [u8; 32] = bs58::decode(s)
        .into_vec()
        .map_err(|_| bad())?
        .try_into()
        .map_err(|_| bad())?;
    if bs58::encode(raw).into_string() != s {
        return Err(bad());
    }
    Ok(raw)
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolanaAccountMeta {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolanaInstruction {
    pub program_id: String,
    pub accounts: Vec<SolanaAccountMeta>,
    pub data_base64: String,
}
fn sv(n: usize, out: &mut Vec<u8>) -> Result<()> {
    let mut n = u16::try_from(n).map_err(|_| bad())?;
    loop {
        let mut b = (n & 127) as u8;
        n >>= 7;
        if n > 0 {
            b |= 128;
        }
        out.push(b);
        if n == 0 {
            return Ok(());
        }
    }
}
pub fn compile_message(payer: &str, blockhash: &str, ix: &SolanaInstruction) -> Result<Vec<u8>> {
    compile_message_many(payer, blockhash, std::slice::from_ref(ix))
}
pub fn compile_message_many(
    payer: &str,
    blockhash: &str,
    instructions: &[SolanaInstruction],
) -> Result<Vec<u8>> {
    if instructions.is_empty() || instructions.len() > 8 {
        return Err(bad());
    }
    if curve25519_dalek::edwards::CompressedEdwardsY(pubkey(payer)?)
        .decompress()
        .is_none()
    {
        return Err(err("SOLANA_UNSIGNED_SIGNER_OFF_CURVE"));
    }
    let mut flags = BTreeMap::<String, (bool, bool)>::from([(payer.to_string(), (true, true))]);
    for ix in instructions {
        flags.entry(ix.program_id.clone()).or_default();
        for a in &ix.accounts {
            let f = flags.entry(a.pubkey.clone()).or_default();
            f.0 |= a.is_signer;
            f.1 |= a.is_writable;
        }
    }
    if instructions
        .iter()
        .any(|ix| flags[&ix.program_id].0 || flags[&ix.program_id].1)
    {
        return Err(bad());
    }
    if flags.iter().any(|(id, (signer, _))| *signer && id != payer) {
        return Err(bad());
    }
    let mut keys = vec![payer.to_string()];
    for writable in [true, false] {
        keys.extend(
            flags
                .iter()
                .filter(|(id, (_, w))| *id != payer && *w == writable)
                .map(|(id, _)| id.clone()),
        );
    }
    if keys.len() > 256 {
        return Err(bad());
    }
    let readonly = keys.iter().filter(|id| !flags[*id].1).count();
    let mut out = vec![1, 0, u8::try_from(readonly).map_err(|_| bad())?];
    sv(keys.len(), &mut out)?;
    for key in &keys {
        out.extend(pubkey(key)?);
    }
    out.extend(pubkey(blockhash)?);
    sv(instructions.len(), &mut out)?;
    let index = |id: &str| -> Result<u8> {
        u8::try_from(keys.iter().position(|key| key == id).ok_or_else(bad)?).map_err(|_| bad())
    };
    for ix in instructions {
        out.push(index(&ix.program_id)?);
        sv(ix.accounts.len(), &mut out)?;
        for a in &ix.accounts {
            out.push(index(&a.pubkey)?);
        }
        let data = STANDARD.decode(&ix.data_base64).map_err(|_| bad())?;
        sv(data.len(), &mut out)?;
        out.extend(data);
    }
    Ok(out)
}

pub fn unsigned_packet(message: &[u8]) -> Result<Vec<u8>> {
    if message.len() < 3 || message[0] != 1 || message[1] != 0 {
        return Err(bad());
    }
    let mut wire = vec![1];
    wire.extend([0; 64]);
    wire.extend(message);
    if wire.len() > 1232 {
        return Err(err("PREPARATION_TRANSACTION_TOO_LARGE"));
    }
    Ok(wire)
}
