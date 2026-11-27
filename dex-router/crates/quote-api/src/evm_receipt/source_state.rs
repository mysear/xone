use super::{
    err, header_fork, json, keccak256, quantity, quantity_u64, receipt_bytes, receipt_hex, rpc,
    verify_execution_header, Api, Result, Value,
};
use alloy_primitives::{Bytes, B256, U256};
use alloy_trie::{proof::verify_proof, Nibbles, TrieAccount};
use xone_simulation::evm_fixture::{
    FixtureAccount, FixtureState, StorageEntry, BEACON_ROOTS_ADDRESS,
};

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

fn verify_account(root: B256, owner: &str, exists: bool, proof: &Value) -> Result<TrieAccount> {
    if proof["address"] != owner {
        return Err(mismatch());
    }
    let address = receipt_bytes::<20>(&proof["address"])?;
    let account = TrieAccount {
        nonce: quantity_u64(&proof["nonce"])?,
        balance: U256::from_be_bytes(quantity(&proof["balance"])?.to_be_bytes()?),
        storage_root: receipt_bytes::<32>(&proof["storageHash"])?.into(),
        code_hash: receipt_bytes::<32>(&proof["codeHash"])?.into(),
    };
    if !exists && (account.nonce != 0 || !account.balance.is_zero()) {
        return Err(mismatch());
    }
    verify_proof(
        root,
        Nibbles::unpack(keccak256(&address)),
        exists.then(|| alloy_rlp::encode(account)),
        &nodes(&proof["accountProof"])?,
    )
    .map_err(|_| mismatch())?;
    Ok(if exists {
        account
    } else {
        TrieAccount::default()
    })
}

fn verify_storage(storage_root: B256, entries: &[StorageEntry], proof: &Value) -> Result<()> {
    prove_storage(storage_root, entries, proof, |entry, value| {
        if receipt_bytes::<32>(&json!(entry.value))? != value {
            return Err(mismatch());
        }
        Ok(())
    })
}

fn prove_storage(
    storage_root: B256,
    entries: &[StorageEntry],
    proof: &Value,
    mut read: impl FnMut(&StorageEntry, [u8; 32]) -> Result<()>,
) -> Result<()> {
    let storage = proof["storageProof"].as_array().ok_or_else(mismatch)?;
    if storage.len() != entries.len() {
        return Err(mismatch());
    }
    for (entry, proof) in entries.iter().zip(storage) {
        let slot = receipt_bytes::<32>(&json!(entry.slot))?;
        let value = quantity(&proof["value"])?.to_be_bytes()?;
        if receipt_bytes::<32>(&proof["key"])? != slot {
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
        read(entry, value)?;
    }
    Ok(())
}

async fn account_proof(
    api: &Api,
    chain: &str,
    block_hash: &str,
    root: B256,
    account: &FixtureAccount,
) -> Result<(TrieAccount, Value)> {
    let slots: Vec<_> = account.storage.iter().map(|entry| &entry.slot).collect();
    let proof = rpc(
        api,
        chain,
        "eth_getProof",
        json!([account.address, slots, {"blockHash":block_hash,"requireCanonical":true}]),
    )
    .await?;
    let proved = verify_account(root, &account.address, account.exists, &proof)?;
    if account.exists && proved.code_hash.0 != code_hash(&account.code)? {
        return Err(mismatch());
    }
    Ok((proved, proof))
}

pub(super) async fn refresh_accounts(
    api: &Api,
    fixture: &mut FixtureState,
    source: &Value,
) -> Result<()> {
    let root = B256::from(receipt_bytes::<32>(&source["stateRoot"])?);
    let anchor = &fixture.state_anchor;
    for account in &mut fixture.accounts {
        let (proved, proof) =
            account_proof(api, &anchor.chain_key, &anchor.block_hash, root, account).await?;
        let mut storage = Vec::with_capacity(account.storage.len());
        prove_storage(
            proved.storage_root,
            &account.storage,
            &proof,
            |entry, value| {
                storage.push(StorageEntry {
                    slot: entry.slot.clone(),
                    value: format!("0x{}", hex::encode(value)),
                });
                Ok(())
            },
        )?;
        account.balance = xone_domain::Amount::from_be_bytes(&proved.balance.to_be_bytes::<32>())?;
        account.nonce = proved.nonce;
        account.storage = storage;
    }
    Ok(())
}

pub(super) async fn verify_beacon_storage(
    api: &Api,
    chain: &str,
    block: &Value,
    fixture: &FixtureState,
    beacon_root: &str,
) -> Result<()> {
    let mismatch = || err("EVM_NODE_RECEIPT_BEACON_STATE_MISMATCH");
    let env = &fixture.execution_environment;
    let account = fixture
        .accounts
        .iter()
        .find(|row| row.address == BEACON_ROOTS_ADDRESS)
        .ok_or_else(mismatch)?;
    if account.code == "0x" {
        return Ok(());
    }
    let [timestamp_slot, root_slot] = env
        .beacon_root_slots()
        .map_err(|_| mismatch())?
        .ok_or_else(mismatch)?;
    let entries = [
        StorageEntry {
            slot: format!("0x{timestamp_slot:064x}"),
            value: format!("0x{:064x}", env.timestamp),
        },
        StorageEntry {
            slot: format!("0x{root_slot:064x}"),
            value: beacon_root.into(),
        },
    ];
    let anchor = json!({"blockHash":block["hash"],"requireCanonical":true});
    let proof = rpc(
        api,
        chain,
        "eth_getProof",
        json!([
            BEACON_ROOTS_ADDRESS,
            [&entries[0].slot, &entries[1].slot],
            anchor
        ]),
    )
    .await?;
    let state_root = receipt_bytes::<32>(&block["stateRoot"])?.into();
    let bad = |_| mismatch();
    let proved = verify_account(state_root, BEACON_ROOTS_ADDRESS, true, &proof).map_err(bad)?;
    if proved.code_hash.0 != code_hash(&account.code)? {
        return Err(mismatch());
    }
    verify_storage(proved.storage_root, &entries, &proof).map_err(bad)
}

fn code_hash(value: &str) -> Result<[u8; 32]> {
    let code = value.strip_prefix("0x").ok_or_else(mismatch)?;
    Ok(keccak256(&hex::decode(code).map_err(|_| mismatch())?))
}

pub(super) async fn verify(api: &Api, chain: &str, fixture: &FixtureState) -> Result<Value> {
    let anchor = &fixture.state_anchor;
    let source = rpc(
        api,
        chain,
        "eth_getBlockByNumber",
        json!([format!("0x{:x}", anchor.block_number), false]),
    )
    .await?;
    if source["hash"] != anchor.block_hash {
        return Err(err("EVM_NODE_RECEIPT_REORGED"));
    }
    if source["parentHash"] != anchor.parent_hash
        || quantity_u64(&source["number"])? != anchor.block_number
        || quantity_u64(&source["timestamp"])? != anchor.timestamp
    {
        return Err(mismatch());
    }
    verify_execution_header(&source, header_fork(&source))?;
    if let Some(expected) = &anchor.base_fee_per_gas {
        if &quantity(&source["baseFeePerGas"])? != expected {
            return Err(mismatch());
        }
    }
    super::ancestors::verify(api, chain, fixture, &source).await?;
    let root = B256::from(receipt_bytes::<32>(&source["stateRoot"])?);
    for account in &fixture.accounts {
        let (proved, proof) = account_proof(api, chain, &anchor.block_hash, root, account).await?;
        if proved.nonce != account.nonce
            || proved.balance != U256::from_be_bytes(account.balance.to_be_bytes()?)
        {
            return Err(mismatch());
        }
        verify_storage(proved.storage_root, &account.storage, &proof)?;
    }
    Ok(source["stateRoot"].clone())
}
