use revm::primitives::{keccak256, Address, B256, U256};
use secp256k1::{Message, Secp256k1, SecretKey};

pub fn encoded(bytes: &[u8], list: bool) -> Vec<u8> {
    if !list && bytes.len() == 1 && bytes[0] < 128 {
        return bytes.to_vec();
    }
    let offset = if list { 0xc0 } else { 0x80 };
    let mut out = if bytes.len() < 56 {
        vec![offset + bytes.len() as u8]
    } else {
        let n = bytes.len().to_be_bytes();
        let start = n.iter().position(|b| *b != 0).unwrap();
        let mut h = vec![offset + 55 + (n.len() - start) as u8];
        h.extend(&n[start..]);
        h
    };
    out.extend(bytes);
    out
}
pub fn number(n: U256) -> Vec<u8> {
    let b = n.to_be_bytes::<32>();
    encoded(&b[b.iter().position(|n| *n != 0).unwrap_or(32)..], false)
}
pub fn payer() -> Address {
    Address::from_slice(&hex::decode("7e5f4552091a69125d5dfcb7b8c2659029395bdf").unwrap())
}
pub fn fields(kind: u8) -> Vec<Vec<u8>> {
    let mut f = vec![number(U256::ZERO)];
    if kind == 2 {
        f.push(number(U256::from(2)));
    }
    f.extend([
        number(U256::from(3_000_000_000u64)),
        number(U256::from(60_000)),
        encoded(&[0x22; 20], false),
        number(U256::ZERO),
        encoded(&[0, 1, 2], false),
    ]);
    if kind > 0 {
        f.insert(0, number(U256::from(10)));
        f.push(encoded(&[], true));
    }
    f
}
pub fn signed_fields(kind: u8, fields: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut unsigned = fields.concat();
    if kind == 0 {
        unsigned.extend([number(U256::from(10)), vec![0x80], vec![0x80]].concat());
    }
    let mut signing = encoded(&unsigned, true);
    if kind > 0 {
        signing.insert(0, kind);
    }
    let mut key = [0; 32];
    key[31] = 1;
    let sig = Secp256k1::new().sign_ecdsa_recoverable(
        &Message::from_digest(keccak256(signing).into()),
        &SecretKey::from_slice(&key).unwrap(),
    );
    let (id, sig) = sig.serialize_compact();
    let mut parts = fields.to_vec();
    parts.push(number(U256::from(
        id.to_i32() as u64 + if kind == 0 { 55 } else { 0 },
    )));
    parts.push(number(U256::from_be_slice(&sig[..32])));
    parts.push(number(U256::from_be_slice(&sig[32..])));
    parts
}
pub fn signed(kind: u8, fields: &[Vec<u8>]) -> Vec<u8> {
    let mut wire = encoded(&signed_fields(kind, fields).concat(), true);
    if kind > 0 {
        wire.insert(0, kind);
    }
    wire
}
pub fn hash(wire: &[u8]) -> B256 {
    keccak256(wire)
}
