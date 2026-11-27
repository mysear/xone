use super::{err, json, quantity_u64, rpc, verify_execution_header, Api, Result, Value};
use std::collections::BTreeMap;
use xone_simulation::evm_fixture::FixtureState;

pub(super) async fn verify(
    api: &Api,
    chain: &str,
    fixture: &FixtureState,
    source: &Value,
) -> Result<()> {
    let mismatch = || err("EVM_NODE_RECEIPT_ANCESTOR_MISMATCH");
    let declared: BTreeMap<_, _> = fixture
        .block_hashes
        .iter()
        .map(|row| (row.number, row.hash.as_str()))
        .collect();
    let Some((&oldest, _)) = declared.first_key_value() else {
        return Ok(());
    };
    let source_height = fixture.state_anchor.block_number;
    if oldest
        < fixture
            .execution_environment
            .block_number
            .saturating_sub(256)
        || declared
            .last_key_value()
            .is_some_and(|(&newest, _)| newest > source_height)
        || declared
            .get(&source_height)
            .is_some_and(|&hash| hash != fixture.state_anchor.block_hash)
    {
        return Err(mismatch());
    }
    let mut hash = source["parentHash"]
        .as_str()
        .ok_or_else(mismatch)?
        .to_owned();
    for height in (oldest..source_height).rev() {
        if declared
            .get(&height)
            .is_some_and(|&expected| expected != hash)
        {
            return Err(mismatch());
        }
        if height == oldest {
            break;
        }
        let header = rpc(api, chain, "eth_getBlockByHash", json!([hash, false])).await?;
        if header["hash"] != hash || quantity_u64(&header["number"])? != height {
            return Err(mismatch());
        }
        verify_execution_header(&header, super::header_fork(&header))?;
        hash = header["parentHash"]
            .as_str()
            .ok_or_else(mismatch)?
            .to_owned();
    }
    Ok(())
}
