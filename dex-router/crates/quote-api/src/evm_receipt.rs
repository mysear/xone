use super::{err, hash, json, Api, Instant, Result, Value};
use crate::{local_simulation::Failure, transaction_lifecycle::rpc};
use xone_domain::{singleton::ZERO_ADDRESS, Amount};
use xone_execution_plan::Context;
use xone_simulation::evm_fixture::{EvmStatus, FixtureState, SimulationOutcome};

pub(super) async fn compare(
    api: &Api,
    fixture: &FixtureState,
    context: &Context,
    replay: &Value,
    deadline: Instant,
) -> std::result::Result<Value, Failure> {
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| Failure::new("SIMULATION_BUSY", "node_receipt"))?;
    tokio::time::timeout_at(deadline.into(), observe(api, fixture, context, replay))
        .await
        .map_err(|_| Failure::new("SIMULATION_TIMEOUT", "node_receipt"))?
        .map_err(|error| Failure::new(&error.0, "node_receipt"))
}

async fn observe(
    api: &Api,
    fixture: &FixtureState,
    context: &Context,
    replay: &Value,
) -> Result<Value> {
    let bad = || err("EVM_NODE_RECEIPT_MISMATCH");
    let env = &fixture.execution_environment;
    let local: SimulationOutcome =
        serde_json::from_value(replay["outcome"].clone()).map_err(|_| bad())?;
    let tx = local.signed_transaction_hash.as_ref().ok_or_else(bad)?;
    let chain = &context.chain_key;
    let request = |method, params| rpc(api, chain, method, params);
    let height = format!("0x{:x}", env.block_number);
    let (chain_id, genesis, block, receipt) = tokio::try_join!(
        request("eth_chainId", json!([])),
        request("eth_getBlockByNumber", json!(["0x0", false])),
        request("eth_getBlockByNumber", json!([height, false])),
        request("eth_getTransactionReceipt", json!([tx])),
    )?;
    if quantity(&chain_id)? != env.chain_id.to_string().parse::<Amount>()?
        || genesis["hash"] != fixture.state_anchor.genesis_hash
        || block["hash"] != env.block_hash
        || block["parentHash"] != fixture.state_anchor.block_hash
        || block["number"] != height
        || block["timestamp"] != format!("0x{:x}", env.timestamp)
        || block["gasLimit"] != format!("0x{:x}", env.gas_limit)
        || block["miner"] != env.coinbase
        || block["mixHash"] != env.prevrandao
        || quantity(&block["baseFeePerGas"])? != env.base_fee
        || env
            .excess_blob_gas
            .map(|gas| format!("0x{gas:x}"))
            .as_deref()
            != block["excessBlobGas"].as_str()
    {
        return Err(bad());
    }
    // Exclude beneficiary rewards, withdrawals and other transactions when
    // attributing whole-block balance changes to the signed swap.
    if block["transactions"] != json!([tx])
        || block["difficulty"] != "0x0"
        || block["uncles"] != json!([])
        || block["withdrawals"] != json!([])
        || receipt["transactionHash"] != *tx
        || receipt["blockHash"] != env.block_hash
        || receipt["blockNumber"] != height
        || receipt["transactionIndex"] != "0x0"
        || receipt["from"] != replay["transaction"]["from"]
        || receipt["to"] != replay["transaction"]["to"]
        || receipt["gasUsed"] != format!("0x{:x}", local.execution.gas_used)
        || receipt["cumulativeGasUsed"] != receipt["gasUsed"]
        || block["gasUsed"] != receipt["gasUsed"]
        || receipt["status"]
            != if local.execution.status == EvmStatus::Success {
                "0x1"
            } else {
                "0x0"
            }
        || quantity(&receipt["effectiveGasPrice"])?
            != local
                .native_fees
                .as_ref()
                .ok_or_else(bad)?
                .effective_gas_price
    {
        return Err(bad());
    }
    let logs = receipt["logs"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .enumerate()
        .map(|(index, log)| {
            if log["blockHash"] != env.block_hash
                || log["transactionHash"] != *tx
                || log["blockNumber"] != height
                || log["transactionIndex"] != "0x0"
                || log["logIndex"] != format!("0x{index:x}")
                || log["removed"] != false
            {
                return Err(bad());
            }
            Ok(json!({"address":log["address"],"topics":log["topics"],"data":log["data"]}))
        })
        .collect::<Result<Vec<_>>>()?;
    if local.execution.logs_hash.as_deref() != Some(hash(&logs)?.as_str()) {
        return Err(bad());
    }
    let source = &fixture.state_anchor.block_hash;
    let execution = &env.block_hash;
    let (sell_before, sell_after, buy_before, buy_after) = tokio::try_join!(
        balance(api, chain, &context.sell_token, &context.caller, source),
        balance(api, chain, &context.sell_token, &context.caller, execution),
        balance(api, chain, &context.buy_token, &context.recipient, source),
        balance(
            api,
            chain,
            &context.buy_token,
            &context.recipient,
            execution
        ),
    )?;
    if sell_before != local.balances.caller_sell_before
        || sell_after != local.balances.caller_sell_after
        || buy_before != local.balances.recipient_buy_before
        || buy_after != local.balances.recipient_buy_after
    {
        return Err(bad());
    }
    let current = request("eth_getBlockByNumber", json!([height, false])).await?;
    if current["hash"] != env.block_hash {
        return Err(err("EVM_NODE_RECEIPT_REORGED"));
    }
    let mut evidence = json!({
        "authority":"CANONICAL_RPC_OBSERVED","scope":"SINGLE_TRANSACTION_BLOCK",
        "transactionHash":tx,"sourceBlockHash":source,"blockHash":execution,
        "transactionIndex":0,"success":local.execution.status == EvmStatus::Success,
        "gasUsed":local.execution.gas_used,"logsHash":local.execution.logs_hash,
        "localReceiptHash":local.receipt_hash,"actualInputConsumed":local.actual_input_consumed,
        "actualBuyAmount":local.actual_buy_amount,"balances":local.balances,
    });
    evidence["evidenceHash"] = hash(&evidence)?.into();
    Ok(evidence)
}

async fn balance(api: &Api, chain: &str, token: &str, owner: &str, block: &str) -> Result<Amount> {
    let anchor = json!({"blockHash":block,"requireCanonical":true});
    if token == ZERO_ADDRESS {
        return quantity(&rpc(api, chain, "eth_getBalance", json!([owner, anchor])).await?);
    }
    let data = format!("0x70a08231000000000000000000000000{}", &owner[2..]);
    let value = rpc(
        api,
        chain,
        "eth_call",
        json!([{"to":token,"data":data},anchor]),
    )
    .await?;
    let raw = value
        .as_str()
        .and_then(|v| v.strip_prefix("0x"))
        .filter(|v| v.len() == 64)
        .ok_or_else(|| err("EVM_NODE_RECEIPT_BALANCE"))?;
    let bytes = hex::decode(raw).map_err(|_| err("EVM_NODE_RECEIPT_BALANCE"))?;
    Amount::from_be_bytes(&bytes)
}

fn quantity(value: &Value) -> Result<Amount> {
    let invalid = || err("EVM_NODE_RECEIPT_QUANTITY");
    let raw = value
        .as_str()
        .and_then(|v| v.strip_prefix("0x"))
        .ok_or_else(invalid)?;
    if raw.is_empty()
        || raw.len() > 64
        || (raw.len() > 1 && raw.starts_with('0'))
        || !raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    let padded = format!("{raw:0>64}");
    Amount::from_be_bytes(&hex::decode(padded).map_err(|_| invalid())?)
}
