use super::{err, json, keccak256, receipt_bytes, receipt_hex, rpc, Api, Result, Value};
use alloy_trie::root::ordered_trie_root_with_encoder;
use tokio::task::JoinSet;

const MAX_TRANSACTIONS: usize = 4096;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const READ_CONCURRENCY: usize = 4;

pub(super) async fn verify(api: &Api, chain: &str, block: &Value, hashes: &[Value]) -> Result<()> {
    let mismatch = || err("EVM_NODE_RECEIPT_MISMATCH");
    let over_budget = || err("EVM_NODE_RECEIPT_BODY_LIMIT");
    if hashes.len() > MAX_TRANSACTIONS {
        return Err(over_budget());
    }
    let mut remaining = hashes.iter().enumerate();
    let mut wires = vec![Vec::new(); hashes.len()];
    let mut running = JoinSet::new();
    let mut bytes_left = MAX_BODY_BYTES;
    loop {
        while running.len() < READ_CONCURRENCY {
            let Some((index, hash)) = remaining.next() else {
                break;
            };
            let expected = receipt_bytes::<32>(hash)?;
            let params = json!([hash]);
            let chain = chain.to_owned();
            let api = api.clone();
            running.spawn(async move {
                let raw = rpc(&api, &chain, "eth_getRawTransactionByHash", params).await?;
                let wire = hex::decode(receipt_hex(&raw)?).map_err(|_| mismatch())?;
                if wire.is_empty() || keccak256(&wire) != expected {
                    return Err(mismatch());
                }
                Ok((index, wire))
            });
        }
        let Some(result) = running.join_next().await else {
            break;
        };
        let (index, wire) = result.map_err(|_| mismatch())??;
        bytes_left = bytes_left.checked_sub(wire.len()).ok_or_else(over_budget)?;
        wires[index] = wire;
    }
    let root = ordered_trie_root_with_encoder(&wires, |wire, output| output.extend(wire));
    if block["transactionsRoot"] != format!("{root:#x}") {
        return Err(mismatch());
    }
    Ok(())
}
