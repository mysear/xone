//! Authenticate regular signed OP user envelopes before constructing a VM
//! transaction. Block/state provenance and canonicality remain separate.
use revm::primitives::{
    alloy_primitives::B512, keccak256, AccessListItem, Address, Bytes, OptimismFields, TxEnv,
    TxKind, B256, U256,
};
use xone_domain::{err, Result};

pub const VERSION: &str = "xone-op-signed-user-envelope/1";
pub const MAX_WIRE_BYTES: usize = 16 << 20;
const MAX_ACCESS_ITEMS: usize = 4096;

/// Fields are private: execution cannot substitute another caller, calldata,
/// gas or fee while retaining a cheaper envelope for the L1 byte charge.
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    env: TxEnv,
    transaction_hash: B256,
    transaction_type: u8,
}
impl AuthenticatedUser {
    pub fn transaction_hash(&self) -> B256 {
        self.transaction_hash
    }
    pub fn transaction_type(&self) -> u8 {
        self.transaction_type
    }
    pub fn environment(&self) -> &TxEnv {
        &self.env
    }
    pub(crate) fn into_environment(self) -> TxEnv {
        self.env
    }
}

#[derive(Clone, Copy)]
struct Node<'a> {
    encoded: &'a [u8],
    payload: &'a [u8],
    list: bool,
}
fn invalid() -> xone_domain::Error {
    err("OP_USER_WIRE_RLP")
}
fn node(input: &[u8]) -> Result<(Node<'_>, &[u8])> {
    let p = *input.first().ok_or_else(invalid)?;
    let (head, length, list) = match p {
        0..=0x7f => (0usize, 1usize, false),
        0x80..=0xb7 => (1, (p - 0x80) as usize, false),
        0xc0..=0xf7 => (1, (p - 0xc0) as usize, true),
        _ => {
            let n = if p < 0xc0 { p - 0xb7 } else { p - 0xf7 } as usize;
            if n > std::mem::size_of::<usize>() || input.len() <= n || input[1] == 0 {
                return Err(invalid());
            }
            let mut size = 0usize;
            for byte in &input[1..=n] {
                size = size
                    .checked_mul(256)
                    .and_then(|v| v.checked_add(*byte as usize))
                    .ok_or_else(invalid)?;
            }
            if size < 56 {
                return Err(invalid());
            }
            (n + 1, size, p >= 0xc0)
        }
    };
    let end = head
        .checked_add(length)
        .filter(|n| *n <= input.len())
        .ok_or_else(invalid)?;
    let payload = &input[head..end];
    if !list && head == 1 && length == 1 && payload[0] < 0x80 {
        return Err(invalid());
    }
    Ok((
        Node {
            encoded: &input[..end],
            payload,
            list,
        },
        &input[end..],
    ))
}
fn children(n: Node<'_>, max: usize) -> Result<Vec<Node<'_>>> {
    if !n.list {
        return Err(invalid());
    }
    let mut tail = n.payload;
    let mut out = Vec::new();
    while !tail.is_empty() {
        if out.len() == max {
            return Err(err("OP_USER_WIRE_ACCESS_BUDGET"));
        }
        let (item, rest) = node(tail)?;
        out.push(item);
        tail = rest;
    }
    Ok(out)
}
fn bytes(n: Node<'_>) -> Result<&[u8]> {
    if n.list {
        return Err(invalid());
    }
    Ok(n.payload)
}
fn integer(n: Node<'_>) -> Result<U256> {
    let b = bytes(n)?;
    if b.len() > 32 || b.first() == Some(&0) {
        return Err(invalid());
    }
    Ok(U256::from_be_slice(b))
}
fn u64_integer(n: Node<'_>) -> Result<u64> {
    integer(n)?.try_into().map_err(|_| invalid())
}
fn rlp_list(body: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(body.len() + 9);
    if body.len() < 56 {
        result.push(0xc0 + body.len() as u8);
    } else {
        let size = body.len().to_be_bytes();
        let length = size.iter().position(|n| *n != 0).unwrap();
        result.push(0xf7 + (size.len() - length) as u8);
        result.extend_from_slice(&size[length..]);
    }
    result.extend_from_slice(body);
    result
}
fn rlp_chain_id(chain: u64) -> Vec<u8> {
    if chain < 0x80 {
        return vec![chain as u8];
    }
    let bytes = chain.to_be_bytes();
    let start = bytes.iter().position(|b| *b != 0).unwrap();
    let mut out = vec![0x80 + (8 - start) as u8];
    out.extend_from_slice(&bytes[start..]);
    out
}
fn access_list(n: Node<'_>) -> Result<Vec<AccessListItem>> {
    let mut items = Vec::new();
    let mut budget = MAX_ACCESS_ITEMS;
    for item in children(n, MAX_ACCESS_ITEMS)? {
        let entry = children(item, 2)?;
        if entry.len() != 2 || bytes(entry[0])?.len() != 20 {
            return Err(invalid());
        }
        budget = budget.checked_sub(1).ok_or_else(invalid)?;
        let mut storage_keys = Vec::new();
        for slot in children(entry[1], budget)? {
            let value = bytes(slot)?;
            if value.len() != 32 {
                return Err(invalid());
            }
            budget = budget.checked_sub(1).ok_or_else(invalid)?;
            storage_keys.push(B256::from_slice(value));
        }
        items.push(AccessListItem {
            address: Address::from_slice(bytes(entry[0])?),
            storage_keys,
        });
    }
    Ok(items)
}

/// Only EIP-155 legacy, EIP-2930 and EIP-1559 are admitted here. Deposits,
/// unprotected legacy, blobs and authorization transactions need other rules.
/// Expected hash/caller are supplied by the bound source or execution plan;
/// a valid signature alone does not prove block inclusion or freshness.
pub fn authenticate_user(
    chain_id: u64,
    expected_hash: B256,
    expected_caller: Address,
    wire: &[u8],
    max_bytes: usize,
) -> Result<AuthenticatedUser> {
    if !matches!(chain_id, 10 | 8453) {
        return Err(err("OP_USER_CHAIN_UNSUPPORTED"));
    }
    if wire.is_empty() || wire.len() > max_bytes || wire.len() > MAX_WIRE_BYTES {
        return Err(err("OP_USER_WIRE_BUDGET"));
    }
    if keccak256(wire) != expected_hash {
        return Err(err("OP_USER_TRANSACTION_HASH"));
    }
    let (kind, payload) = match wire[0] {
        1 | 2 => (wire[0], &wire[1..]),
        0xc0..=0xff => (0, wire),
        _ => return Err(err("OP_USER_TYPE_UNSUPPORTED")),
    };
    let (root, tail) = node(payload)?;
    let fields = children(root, 12)?;
    let (sig, nonce, price, priority, gas, to, value, data, access) = match kind {
        0 if fields.len() == 9 => (6, 0, 1, None, 2, 3, 4, 5, None),
        1 if fields.len() == 11 => (8, 1, 2, None, 3, 4, 5, 6, Some(7)),
        2 if fields.len() == 12 => (9, 1, 3, Some(2), 4, 5, 6, 7, Some(8)),
        _ => return Err(invalid()),
    };
    if !tail.is_empty() {
        return Err(invalid());
    }
    let mut unsigned = fields[..sig]
        .iter()
        .flat_map(|n| n.encoded.iter().copied())
        .collect::<Vec<_>>();
    let parity = if kind == 0 {
        let v = u64_integer(fields[sig])?;
        let protected = chain_id * 2 + 35;
        if v != protected && v != protected + 1 {
            return Err(err("OP_USER_SIGNATURE_CHAIN"));
        }
        unsigned.extend(rlp_chain_id(chain_id));
        unsigned.extend([0x80, 0x80]);
        (v - protected) as u8
    } else {
        if u64_integer(fields[0])? != chain_id {
            return Err(err("OP_USER_SIGNATURE_CHAIN"));
        }
        let v = u64_integer(fields[sig])?;
        if v > 1 {
            return Err(err("OP_USER_SIGNATURE_PARITY"));
        }
        v as u8
    };
    let mut signing = rlp_list(&unsigned);
    if kind != 0 {
        signing.insert(0, kind);
    }
    let r = integer(fields[sig + 1])?;
    let s = integer(fields[sig + 2])?;
    let half_order = U256::from_be_slice(
        &hex::decode("7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a0").unwrap(),
    );
    if r.is_zero() || s.is_zero() || s > half_order {
        return Err(err("OP_USER_SIGNATURE"));
    }
    let mut signature = [0; 64];
    signature[..32].copy_from_slice(&r.to_be_bytes::<32>());
    signature[32..].copy_from_slice(&s.to_be_bytes::<32>());
    let recovered =
        revm::precompile::secp256k1::ecrecover(&B512::from(signature), parity, &keccak256(signing))
            .map_err(|_| err("OP_USER_SIGNATURE"))?;
    let caller = Address::from_slice(&recovered[12..]);
    if caller != expected_caller {
        return Err(err("OP_USER_CALLER"));
    }
    let nonce = u64_integer(fields[nonce])?;
    let gas_limit = u64_integer(fields[gas])?;
    let gas_price = integer(fields[price])?;
    let gas_priority_fee = priority.map(|i| integer(fields[i])).transpose()?;
    let target = bytes(fields[to])?;
    if nonce == u64::MAX
        || gas_limit < 21000
        || gas_priority_fee.is_some_and(|tip| tip > gas_price)
        || !matches!(target.len(), 0 | 20)
    {
        return Err(err("OP_USER_TRANSACTION_CONTEXT"));
    }
    let env = TxEnv {
        caller,
        gas_limit,
        gas_price,
        gas_priority_fee,
        transact_to: if target.is_empty() {
            TxKind::Create
        } else {
            TxKind::Call(Address::from_slice(target))
        },
        value: integer(fields[value])?,
        data: Bytes::copy_from_slice(bytes(fields[data])?),
        nonce: Some(nonce),
        chain_id: Some(chain_id),
        access_list: access
            .map(|i| access_list(fields[i]))
            .transpose()?
            .unwrap_or_default(),
        optimism: OptimismFields {
            enveloped_tx: Some(Bytes::copy_from_slice(wire)),
            ..Default::default()
        },
        ..Default::default()
    };
    Ok(AuthenticatedUser {
        env,
        transaction_hash: expected_hash,
        transaction_type: kind,
    })
}
