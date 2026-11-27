use super::{err, hash, json, Api, Instant, Result, Value};
use crate::{local_simulation::Failure, transaction_lifecycle::rpc};
use xone_domain::{singleton::ZERO_ADDRESS, Amount};
use xone_execution_plan::Context;
use xone_simulation::evm_fixture::{
    EvmStatus, FixtureState, SimulationOutcome, BEACON_ROOTS_ADDRESS,
};

const EMPTY_WITHDRAWALS_ROOT: &str =
    "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";

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
    let transactions = block["transactions"].as_array().ok_or_else(bad)?;
    let index = usize::try_from(quantity_u64(&receipt["transactionIndex"])?).map_err(|_| bad())?;
    let next = index.checked_add(1).ok_or_else(bad)?;
    let cumulative = quantity_u64(&receipt["cumulativeGasUsed"])?;
    let block_gas = quantity_u64(&block["gasUsed"])?;
    let withdrawals = block["withdrawals"].as_array().ok_or_else(bad)?;
    let withdrawals_root = block["withdrawalsRoot"].as_str().ok_or_else(bad)?;
    // Withdrawals happen after the final transaction. A whole-block balance
    // therefore cannot stand in for the last transaction's post-state.
    if withdrawals.is_empty() != (withdrawals_root == EMPTY_WITHDRAWALS_ROOT)
        || (!withdrawals.is_empty() && next == transactions.len())
    {
        return Err(bad());
    }
    if transactions.get(index) != Some(&json!(tx))
        || block["difficulty"] != "0x0"
        || block["uncles"] != json!([])
        || receipt["transactionHash"] != *tx
        || receipt["blockHash"] != env.block_hash
        || receipt["blockNumber"] != height
        || receipt["from"] != replay["transaction"]["from"]
        || receipt["to"] != replay["transaction"]["to"]
        || receipt["gasUsed"] != format!("0x{:x}", local.execution.gas_used)
        || cumulative < local.execution.gas_used
        || cumulative > block_gas
        || (index == 0 && cumulative != local.execution.gas_used)
        || (next == transactions.len() && cumulative != block_gas)
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
    let logs = receipt["logs"].as_array().ok_or_else(bad)?;
    let log_start = logs
        .first()
        .map(|log| quantity_u64(&log["logIndex"]))
        .transpose()?
        .unwrap_or(0);
    if index == 0 && log_start != 0 {
        return Err(bad());
    }
    let logs = logs
        .iter()
        .enumerate()
        .map(|(offset, log)| {
            if log["blockHash"] != env.block_hash
                || log["transactionHash"] != *tx
                || log["blockNumber"] != height
                || log["transactionIndex"] != receipt["transactionIndex"]
                || log_start.checked_add(offset as u64) != Some(quantity_u64(&log["logIndex"])?)
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
    if let Some(root) = &env.parent_beacon_block_root {
        if block["parentBeaconBlockRoot"] != *root {
            return Err(bad());
        }
        let account = fixture
            .accounts
            .iter()
            .find(|row| row.address == BEACON_ROOTS_ADDRESS)
            .ok_or_else(bad)?;
        let source_anchor = json!({"blockHash":source,"requireCanonical":true});
        if request("eth_getCode", json!([BEACON_ROOTS_ADDRESS, source_anchor])).await?
            != account.code
        {
            return Err(bad());
        }
        if account.code != "0x" {
            let [timestamp_slot, root_slot] = env
                .beacon_root_slots()
                .map_err(|_| bad())?
                .ok_or_else(bad)?;
            let storage = |slot| {
                request(
                    "eth_getStorageAt",
                    json!([
                        BEACON_ROOTS_ADDRESS, format!("0x{slot:x}"),
                        {"blockHash":execution,"requireCanonical":true},
                    ]),
                )
            };
            let (stored_timestamp, stored_root) =
                tokio::try_join!(storage(timestamp_slot), storage(root_slot))?;
            if stored_timestamp != format!("0x{:064x}", env.timestamp) || stored_root != *root {
                return Err(bad());
            }
        }
    }
    let single = transactions.len() == 1;
    let before = if single {
        BalancePoint::Block(source)
    } else {
        BalancePoint::Transaction(execution, index)
    };
    let after = if next == transactions.len() {
        BalancePoint::Block(execution)
    } else {
        BalancePoint::Transaction(execution, next)
    };
    if !single {
        let caller = fixture
            .accounts
            .iter()
            .find(|account| account.address == context.caller)
            .ok_or_else(bad)?;
        let (nonce_before, nonce_after) = tokio::try_join!(
            nonce(api, chain, &context.caller, before),
            nonce(api, chain, &context.caller, after),
        )?;
        // An included transaction increments its sender nonce even on revert.
        // This also rejects a tracer that silently ignores the transaction index.
        if nonce_before != caller.nonce || Some(nonce_after) != caller.nonce.checked_add(1) {
            return Err(bad());
        }
    }
    let (sell_before, sell_after, buy_before, buy_after) = tokio::try_join!(
        balance(api, context, &context.sell_token, &context.caller, before),
        balance(api, context, &context.sell_token, &context.caller, after),
        balance(api, context, &context.buy_token, &context.recipient, before),
        balance(api, context, &context.buy_token, &context.recipient, after),
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
        "authority":"CANONICAL_RPC_OBSERVED",
        "scope":if single {"SINGLE_TRANSACTION_BLOCK"} else {"TRANSACTION_INDEX"},
        "transactionHash":tx,"sourceBlockHash":source,"blockHash":execution,
        "transactionIndex":index,"success":local.execution.status == EvmStatus::Success,
        "gasUsed":local.execution.gas_used,"logsHash":local.execution.logs_hash,
        "localReceiptHash":local.receipt_hash,"actualInputConsumed":local.actual_input_consumed,
        "actualBuyAmount":local.actual_buy_amount,"balances":local.balances,
    });
    if let Some(root) = &env.parent_beacon_block_root {
        evidence["parentBeaconBlockRoot"] = root.clone().into();
    }
    if !withdrawals.is_empty() {
        evidence["withdrawalsRoot"] = withdrawals_root.into();
    }
    evidence["evidenceHash"] = hash(&evidence)?.into();
    Ok(evidence)
}

#[derive(Clone, Copy)]
enum BalancePoint<'a> {
    Block(&'a str),
    Transaction(&'a str, usize),
}

async fn trace_account(
    api: &Api,
    chain: &str,
    owner: &str,
    block: &str,
    index: usize,
) -> Result<Value> {
    let result = rpc(
        api, chain, "debug_traceCall",
        json!([
            {"to":owner,"gas":"0x5208"}, {"blockHash":block,"requireCanonical":true},
            {"tracer":"prestateTracer","tracerConfig":{"disableCode":true,"disableStorage":true,"includeEmpty":true},"txIndex":format!("0x{index:x}")},
        ]),
    ).await?;
    result
        .get(owner)
        .filter(|v| v.is_object())
        .cloned()
        .ok_or_else(|| err("EVM_NODE_RECEIPT_BALANCE"))
}

async fn nonce(api: &Api, chain: &str, owner: &str, point: BalancePoint<'_>) -> Result<u64> {
    match point {
        BalancePoint::Block(block) => quantity_u64(
            &rpc(
                api,
                chain,
                "eth_getTransactionCount",
                json!([owner, {"blockHash":block,"requireCanonical":true}]),
            )
            .await?,
        ),
        BalancePoint::Transaction(block, index) => {
            let account = trace_account(api, chain, owner, block, index).await?;
            match account.get("nonce") {
                Some(value) => value
                    .as_u64()
                    .ok_or_else(|| err("EVM_NODE_RECEIPT_QUANTITY")),
                None => Ok(0),
            }
        }
    }
}

async fn balance(
    api: &Api,
    context: &Context,
    token: &str,
    owner: &str,
    point: BalancePoint<'_>,
) -> Result<Amount> {
    let chain = &context.chain_key;
    let native = token == ZERO_ADDRESS;
    let method = if native { "eth_getBalance" } else { "eth_call" };
    let call = if native {
        json!(owner)
    } else {
        let data = format!("0x70a08231000000000000000000000000{}", &owner[2..]);
        json!({"from":context.caller,"to":token,"data":data,"gas":"0x30d40"})
    };
    let value = match point {
        BalancePoint::Block(block) => {
            let value = rpc(
                api,
                chain,
                method,
                json!([call, {"blockHash":block,"requireCanonical":true}]),
            )
            .await?;
            if native {
                return quantity(&value);
            }
            value
        }
        BalancePoint::Transaction(block, index) if native => {
            let account = trace_account(api, chain, owner, block, index).await?;
            return quantity(&account["balance"]);
        }
        BalancePoint::Transaction(block, index) => {
            let trace = rpc(api, chain, "debug_traceCall", json!([
                call, {"blockHash":block,"requireCanonical":true},
                {"tracer":"callTracer","tracerConfig":{"onlyTopCall":true},"txIndex":format!("0x{index:x}")},
            ])).await?;
            if trace.get("error").is_some() {
                return Err(err("EVM_NODE_RECEIPT_BALANCE"));
            }
            trace["output"].clone()
        }
    };
    let raw = value
        .as_str()
        .and_then(|v| v.strip_prefix("0x"))
        .filter(|v| v.len() == 64)
        .ok_or_else(|| err("EVM_NODE_RECEIPT_BALANCE"))?;
    let bytes = hex::decode(raw).map_err(|_| err("EVM_NODE_RECEIPT_BALANCE"))?;
    Amount::from_be_bytes(&bytes)
}

fn quantity_u64(value: &Value) -> Result<u64> {
    u64::try_from(quantity(value)?.0).map_err(|_| err("EVM_NODE_RECEIPT_QUANTITY"))
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
