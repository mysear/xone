use super::{
    err, header_fork, json, keccak256, quantity, quantity_u64, receipt_bytes, receipt_hex, rpc,
    verify_execution_header, Api, Result, Value,
};
use alloy_primitives::{Bytes, B256, U256};
use alloy_trie::{proof::verify_proof, Nibbles, TrieAccount};
use xone_simulation::evm_fixture::{FixtureAccount, FixtureState};

fn mismatch() -> xone_domain::Error {
    err("EVM_NODE_RECEIPT_SOURCE_STATE_MISMATCH")
}

fn nodes(value: &Value) -> Result<Vec<Bytes>> {
    let nodes = value.as_array().ok_or_else(mismatch)?;
    if nodes.len() > 65 {
        return Err(mismatch());
    }
    nodes
        .iter()
        .map(|node| {
            let raw = receipt_hex(node)?;
            if raw.is_empty() || raw.len() > 2048 {
                return Err(mismatch());
            }
            Ok(hex::decode(raw).map_err(|_| mismatch())?.into())
        })
        .collect()
}

fn verify_account(root: B256, fixture: &FixtureAccount, proof: &Value) -> Result<()> {
    let address = receipt_bytes::<20>(&proof["address"])?;
    if proof["address"] != fixture.address
        || quantity_u64(&proof["nonce"])? != fixture.nonce
        || quantity(&proof["balance"])? != fixture.balance
    {
        return Err(mismatch());
    }
    let account_proof = nodes(&proof["accountProof"])?;
    let storage_root = B256::from(receipt_bytes::<32>(&proof["storageHash"])?);
    let storage = proof["storageProof"].as_array().ok_or_else(mismatch)?;
    if storage.len() != fixture.storage.len() {
        return Err(mismatch());
    }
    let expected = if fixture.exists {
        let code = fixture.code.strip_prefix("0x").ok_or_else(mismatch)?;
        let code_hash: B256 = keccak256(&hex::decode(code).map_err(|_| mismatch())?).into();
        if receipt_bytes::<32>(&proof["codeHash"])? != code_hash.0 {
            return Err(mismatch());
        }
        let balance = fixture.balance.to_be_bytes()?;
        Some(alloy_rlp::encode(TrieAccount {
            nonce: fixture.nonce,
            balance: U256::from_be_bytes(balance),
            storage_root,
            code_hash,
        }))
    } else {
        None
    };
    verify_proof(
        root,
        Nibbles::unpack(keccak256(&address)),
        expected,
        &account_proof,
    )
    .map_err(|_| mismatch())?;
    for (entry, proof) in fixture.storage.iter().zip(storage) {
        let slot = receipt_bytes::<32>(&json!(entry.slot))?;
        let value = receipt_bytes::<32>(&json!(entry.value))?;
        if receipt_bytes::<32>(&proof["key"])? != slot
            || quantity(&proof["value"])?.to_be_bytes()? != value
        {
            return Err(mismatch());
        }
        let encoded = (value != [0; 32]).then(|| alloy_rlp::encode(U256::from_be_bytes(value)));
        verify_proof(
            storage_root,
            Nibbles::unpack(keccak256(&slot)),
            encoded,
            &nodes(&proof["proof"])?,
        )
        .map_err(|_| mismatch())?;
    }
    Ok(())
}

pub(super) async fn verify(api: &Api, chain: &str, fixture: &FixtureState) -> Result<Value> {
    let anchor = &fixture.state_anchor;
    let source = rpc(
        api,
        chain,
        "eth_getBlockByHash",
        json!([anchor.block_hash, false]),
    )
    .await?;
    if source["hash"] != anchor.block_hash
        || source["parentHash"] != anchor.parent_hash
        || quantity_u64(&source["number"])? != anchor.block_number
        || quantity_u64(&source["timestamp"])? != anchor.timestamp
    {
        return Err(mismatch());
    }
    verify_execution_header(&source, header_fork(&source))?;
    super::ancestors::verify(api, chain, fixture, &source).await?;
    let root = B256::from(receipt_bytes::<32>(&source["stateRoot"])?);
    let block = json!({"blockHash":anchor.block_hash,"requireCanonical":true});
    for account in &fixture.accounts {
        let slots: Vec<_> = account.storage.iter().map(|entry| &entry.slot).collect();
        let proof = rpc(
            api,
            chain,
            "eth_getProof",
            json!([account.address, slots, block]),
        )
        .await?;
        verify_account(root, account, &proof)?;
    }
    Ok(source["stateRoot"].clone())
}
