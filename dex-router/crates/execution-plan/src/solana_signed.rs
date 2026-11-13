//! Verify a wallet signature without allowing any mutation of the prepared legacy message.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use xone_domain::{err, Result};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedTransaction {
    pub signature: String,
    pub transaction_sha256: String,
    pub message_sha256: String,
    pub account_keys: Vec<String>,
}
fn bad() -> xone_domain::Error {
    err("SIGNED_TRANSACTION_INVALID")
}
fn count(raw: &[u8], at: &mut usize) -> Result<usize> {
    let mut value = 0usize;
    for shift in [0, 7, 14] {
        let byte = *raw.get(*at).ok_or_else(bad)?;
        *at += 1;
        value |= usize::from(byte & 127) << shift;
        if byte & 128 == 0 {
            if shift > 0 && byte == 0 {
                return Err(bad());
            }
            return Ok(value);
        }
    }
    Err(bad())
}
pub fn verify(unsigned: &[u8], signed: &[u8], caller: &str) -> Result<SignedTransaction> {
    if unsigned.len() != signed.len()
        || !(134..=1232).contains(&unsigned.len())
        || unsigned[0] != 1
        || signed[0] != 1
        || unsigned[1..65] != [0; 64]
        || unsigned[65..] != signed[65..]
        || unsigned[65..68][..2] != [1, 0]
    {
        return Err(bad());
    }
    let mut cursor = 68;
    let n = count(unsigned, &mut cursor)?;
    if !(1..=256).contains(&n)
        || usize::from(unsigned[67]) >= n
        || cursor + n * 32 + 33 > unsigned.len()
    {
        return Err(bad());
    }
    let public = xone_domain::raydium::validate_pubkey(caller)?;
    let mut keys = Vec::new();
    for _ in 0..n {
        let raw = unsigned.get(cursor..cursor + 32).ok_or_else(bad)?;
        keys.push(bs58::encode(raw).into_string());
        cursor += 32;
    }
    if keys.first().map(String::as_str) != Some(caller)
        || keys.iter().collect::<std::collections::BTreeSet<_>>().len() != keys.len()
    {
        return Err(bad());
    }
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public)
        .verify(&signed[65..], &signed[1..65])
        .map_err(|_| err("WALLET_SIGNATURE_INVALID"))?;
    Ok(SignedTransaction {
        signature: bs58::encode(&signed[1..65]).into_string(),
        transaction_sha256: format!("{:x}", Sha256::digest(signed)),
        message_sha256: format!("{:x}", Sha256::digest(&signed[65..])),
        account_keys: keys,
    })
}
