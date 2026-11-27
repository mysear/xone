use super::{
    check_execution_context, err, header_fork, json, quantity, quantity_u64, receipt_bytes, rpc,
    source_state, verify_execution_header, Api, Instant, Result, Value,
};
use crate::local_simulation::Failure;
use xone_domain::BlockRef;
use xone_simulation::evm_fixture::{
    ExecutionEnvironment, FixtureBlockHash, FixtureState, StateAnchor, StorageEntry,
    BEACON_ROOTS_ADDRESS,
};

pub(crate) async fn refresh(
    api: &Api,
    template: &FixtureState,
    block: &BlockRef,
    deadline: Instant,
) -> std::result::Result<FixtureState, Failure> {
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| Failure::new("SIMULATION_BUSY", "rpc_state"))?;
    tokio::time::timeout_at(deadline.into(), load(api, template, block))
        .await
        .map_err(|_| Failure::new("SIMULATION_TIMEOUT", "rpc_state"))?
        .map_err(|error| Failure::new(&error.0, "rpc_state"))
}

async fn load(api: &Api, template: &FixtureState, block: &BlockRef) -> Result<FixtureState> {
    let unsupported = || err("EVM_NODE_SIMULATION_UNSUPPORTED");
    let chain = &block.chain_key;
    let height = block
        .number_or_slot
        .checked_add(1)
        .ok_or_else(unsupported)?;
    let (source, execution) = tokio::try_join!(
        rpc(
            api,
            chain,
            "eth_getBlockByNumber",
            json!([format!("0x{:x}", block.number_or_slot), false])
        ),
        rpc(
            api,
            chain,
            "eth_getBlockByNumber",
            json!([format!("0x{height:x}"), false])
        ),
    )?;
    if source["hash"] != block.hash || source["parentHash"] != block.parent_hash {
        return Err(err("EVM_NODE_SIMULATION_REORGED"));
    }
    if execution.is_null() || header_fork(&execution) != "cancun" {
        return Err(unsupported());
    }
    verify_execution_header(&source, header_fork(&source))?;
    verify_execution_header(&execution, "cancun")?;
    let timestamp = quantity_u64(&source["timestamp"])?;
    if quantity_u64(&source["number"])? != block.number_or_slot
        || timestamp.checked_mul(1000) != Some(block.timestamp_ms)
        || execution["parentHash"] != block.hash
    {
        return Err(err("EVM_NODE_SIMULATION_REORGED"));
    }
    let field = |key: &str| {
        execution[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(unsupported)
    };
    let mut fixture = template.clone();
    fixture.state_anchor = StateAnchor {
        chain_key: chain.clone(),
        genesis_hash: template.state_anchor.genesis_hash.clone(),
        block_number: block.number_or_slot,
        block_hash: block.hash.clone(),
        parent_hash: block.parent_hash.clone(),
        timestamp,
        base_fee_per_gas: Some(quantity(&source["baseFeePerGas"])?),
    };
    fixture.execution_environment = ExecutionEnvironment {
        chain_id: template.execution_environment.chain_id,
        fork: "cancun".into(),
        block_number: height,
        block_hash: field("hash")?,
        parent_hash: block.hash.clone(),
        timestamp: quantity_u64(&execution["timestamp"])?,
        coinbase: field("miner")?,
        gas_limit: quantity_u64(&execution["gasLimit"])?,
        base_fee: quantity(&execution["baseFeePerGas"])?,
        prevrandao: field("mixHash")?,
        excess_blob_gas: Some(quantity_u64(&execution["excessBlobGas"])?),
        parent_beacon_block_root: Some(field("parentBeaconBlockRoot")?),
    };
    check_execution_context(&execution, &fixture)?;
    let slots = fixture
        .execution_environment
        .beacon_root_slots()
        .map_err(|_| unsupported())?
        .ok_or_else(unsupported)?;
    let beacon = fixture
        .accounts
        .iter_mut()
        .find(|account| account.address == BEACON_ROOTS_ADDRESS)
        .ok_or_else(unsupported)?;
    for slot in slots {
        let slot = format!("0x{slot:064x}");
        if !beacon.storage.iter().any(|entry| entry.slot == slot) {
            beacon.storage.push(StorageEntry {
                slot,
                value: format!("0x{:064x}", 0),
            });
        }
    }
    fixture.block_hashes = ancestors(api, template, block, &source).await?;
    source_state::refresh_accounts(api, &mut fixture, &source).await?;
    Ok(fixture)
}

async fn ancestors(
    api: &Api,
    template: &FixtureState,
    block: &BlockRef,
    source: &Value,
) -> Result<Vec<FixtureBlockHash>> {
    let bad = || err("EVM_NODE_RECEIPT_ANCESTOR_MISMATCH");
    let template_height = template.state_anchor.block_number;
    let mut wanted = template
        .block_hashes
        .iter()
        .map(|entry| {
            template_height
                .checked_sub(entry.number)
                .filter(|distance| *distance < 256)
                .and_then(|distance| block.number_or_slot.checked_sub(distance))
                .ok_or_else(bad)
        })
        .collect::<Result<Vec<_>>>()?;
    wanted.sort_unstable();
    let oldest = wanted.first().copied();
    let mut rows = Vec::with_capacity(wanted.len());
    let mut hash = block.hash.clone();
    let mut height = block.number_or_slot;
    let mut header = source.clone();
    for number in wanted.into_iter().rev() {
        while height > number {
            hash = format!(
                "0x{}",
                hex::encode(receipt_bytes::<32>(&header["parentHash"])?)
            );
            height -= 1;
            if Some(height) != oldest {
                header = rpc(
                    api,
                    &block.chain_key,
                    "eth_getBlockByHash",
                    json!([hash, false]),
                )
                .await?;
                if header["hash"] != hash || quantity_u64(&header["number"])? != height {
                    return Err(bad());
                }
                verify_execution_header(&header, header_fork(&header))?;
            }
        }
        rows.push(FixtureBlockHash {
            number,
            hash: hash.clone(),
        });
    }
    rows.reverse();
    Ok(rows)
}
