use super::{err, hash, json, Api, Instant, Result, Value};
use crate::{local_simulation::Failure, transaction_lifecycle::rpc};
use alloy_rlp::{Bytes, Encodable, Header, RlpEncodable};
use alloy_trie::root::{ordered_trie_root, ordered_trie_root_with_encoder};
use num_traits::CheckedSub;
use std::collections::BTreeMap;
use tiny_keccak::{Hasher, Keccak};
use xone_domain::{singleton::ZERO_ADDRESS, Amount};
use xone_execution_plan::Context;
use xone_simulation::evm_fixture::{
    EvmStatus, FixtureState, SimulationOutcome, BEACON_ROOTS_ADDRESS,
};

#[derive(RlpEncodable)]
struct Withdrawal {
    index: u64,
    validator_index: u64,
    address: [u8; 20],
    amount: u64,
}

fn parse_withdrawals(block: &Value) -> Result<Vec<Withdrawal>> {
    let bad = || err("EVM_NODE_RECEIPT_MISMATCH");
    let withdrawals = block["withdrawals"].as_array().ok_or_else(bad)?;
    if withdrawals.len() > 16 {
        return Err(bad());
    }
    let withdrawals = withdrawals
        .iter()
        .map(|row| {
            Ok(Withdrawal {
                index: quantity_u64(&row["index"])?,
                validator_index: quantity_u64(&row["validatorIndex"])?,
                address: receipt_bytes(&row["address"])?,
                amount: quantity_u64(&row["amount"])?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if block["withdrawalsRoot"] != format!("{:#x}", ordered_trie_root(&withdrawals)) {
        return Err(bad());
    }
    Ok(withdrawals)
}

fn verify_execution_header(block: &Value, fork: &str) -> Result<()> {
    let bad = || err("EVM_NODE_RECEIPT_MISMATCH");
    let mut fields = vec![
        ("parentHash", Some(32)),
        ("sha3Uncles", Some(32)),
        ("miner", Some(20)),
        ("stateRoot", Some(32)),
        ("transactionsRoot", Some(32)),
        ("receiptsRoot", Some(32)),
        ("logsBloom", Some(256)),
        ("difficulty", None),
        ("number", None),
        ("gasLimit", None),
        ("gasUsed", None),
        ("timestamp", None),
        ("extraData", Some(0)),
        ("mixHash", Some(32)),
        ("nonce", Some(8)),
        ("baseFeePerGas", None),
        ("withdrawalsRoot", Some(32)),
    ];
    match fork {
        "shanghai" => {}
        "cancun" => fields.extend([
            ("blobGasUsed", None),
            ("excessBlobGas", None),
            ("parentBeaconBlockRoot", Some(32)),
        ]),
        _ => return Err(bad()),
    }
    let mut payload = Vec::new();
    for (field, length) in fields {
        let bytes = if let Some(length) = length {
            let value = block[field]
                .as_str()
                .and_then(|value| value.strip_prefix("0x"))
                .filter(|value| {
                    if length == 0 {
                        value.len() <= 64
                    } else {
                        value.len() == length * 2
                    }
                })
                .ok_or_else(bad)?;
            hex::decode(value).map_err(|_| bad())?
        } else {
            let number = quantity(&block[field])?.0;
            if number.bits() == 0 {
                Vec::new()
            } else {
                number.to_bytes_be()
            }
        };
        bytes.as_slice().encode(&mut payload);
    }
    let mut raw = Vec::new();
    Header {
        list: true,
        payload_length: payload.len(),
    }
    .encode(&mut raw);
    raw.extend(payload);
    let mut hash = [0; 32];
    let mut keccak = Keccak::v256();
    keccak.update(&raw);
    keccak.finalize(&mut hash);
    if block["hash"] != format!("0x{}", hex::encode(hash)) {
        return Err(bad());
    }
    Ok(())
}

#[derive(RlpEncodable)]
struct ReceiptLog {
    address: [u8; 20],
    topics: Vec<[u8; 32]>,
    data: Bytes,
}

#[derive(RlpEncodable)]
struct ReceiptPayload {
    status: u8,
    cumulative_gas: u64,
    bloom: [u8; 256],
    logs: Vec<ReceiptLog>,
}

fn receipt_hex(value: &Value) -> Result<&str> {
    value
        .as_str()
        .and_then(|value| value.strip_prefix("0x"))
        .ok_or_else(|| err("EVM_NODE_RECEIPT_MISMATCH"))
}

fn receipt_bytes<const N: usize>(value: &Value) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    hex::decode_to_slice(receipt_hex(value)?, &mut bytes)
        .map_err(|_| err("EVM_NODE_RECEIPT_MISMATCH"))?;
    Ok(bytes)
}

fn committed_receipt<'a>(
    block: &Value,
    transactions: &[Value],
    receipts: &'a Value,
    target: &str,
    fork: &str,
) -> Result<(usize, &'a Value)> {
    let bad = || err("EVM_NODE_RECEIPT_MISMATCH");
    let receipts = receipts.as_array().ok_or_else(bad)?;
    if receipts.len() != transactions.len() {
        return Err(bad());
    }
    let mut previous_gas = 0;
    let mut log_index = 0;
    let mut found = None;
    let mut encoded = Vec::with_capacity(receipts.len());
    for (index, (receipt, transaction)) in receipts.iter().zip(transactions).enumerate() {
        let status = quantity_u64(&receipt["status"])?;
        let kind = quantity_u64(&receipt["type"])?;
        let cumulative_gas = quantity_u64(&receipt["cumulativeGasUsed"])?;
        let gas = quantity_u64(&receipt["gasUsed"])?;
        receipt_bytes::<32>(transaction)?;
        if receipt["transactionHash"] != *transaction
            || receipt["blockHash"] != block["hash"]
            || receipt["blockNumber"] != block["number"]
            || quantity_u64(&receipt["transactionIndex"])? != index as u64
            || status > 1
            || kind > if fork == "cancun" { 3 } else { 2 }
            || gas == 0
            || cumulative_gas.checked_sub(previous_gas) != Some(gas)
        {
            return Err(bad());
        }
        previous_gas = cumulative_gas;
        if transaction == target && found.replace((index, receipt)).is_some() {
            return Err(bad());
        }
        let logs = receipt["logs"]
            .as_array()
            .ok_or_else(bad)?
            .iter()
            .map(|log| {
                if log["blockHash"] != block["hash"]
                    || log["blockNumber"] != block["number"]
                    || log["transactionHash"] != *transaction
                    || log["transactionIndex"] != receipt["transactionIndex"]
                    || quantity_u64(&log["logIndex"])? != log_index
                    || log["removed"] != false
                {
                    return Err(bad());
                }
                log_index += 1;
                Ok(ReceiptLog {
                    address: receipt_bytes(&log["address"])?,
                    topics: log["topics"]
                        .as_array()
                        .ok_or_else(bad)?
                        .iter()
                        .map(receipt_bytes::<32>)
                        .collect::<Result<_>>()?,
                    data: hex::decode(receipt_hex(&log["data"])?)
                        .map_err(|_| bad())?
                        .into(),
                })
            })
            .collect::<Result<_>>()?;
        encoded.push((
            kind as u8,
            ReceiptPayload {
                status: status as u8,
                cumulative_gas,
                bloom: receipt_bytes(&receipt["logsBloom"])?,
                logs,
            },
        ));
    }
    let root = ordered_trie_root_with_encoder(&encoded, |(kind, receipt), output| {
        if *kind != 0 {
            output.push(*kind);
        }
        receipt.encode(output);
    });
    if previous_gas != quantity_u64(&block["gasUsed"])?
        || block["receiptsRoot"] != format!("{root:#x}")
    {
        return Err(bad());
    }
    found.ok_or_else(bad)
}

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
    let fees = local.native_fees.as_ref().ok_or_else(bad)?;
    let chain = &context.chain_key;
    let request = |method, params| rpc(api, chain, method, params);
    let height = format!("0x{:x}", env.block_number);
    let (chain_id, genesis, block, receipts) = tokio::try_join!(
        request("eth_chainId", json!([])),
        request("eth_getBlockByNumber", json!(["0x0", false])),
        request("eth_getBlockByNumber", json!([height, false])),
        request("eth_getBlockReceipts", json!([env.block_hash])),
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
    let withdrawals = parse_withdrawals(&block)?;
    verify_execution_header(&block, &env.fork)?;
    let (index, receipt) = committed_receipt(&block, transactions, &receipts, tx, &env.fork)?;
    let next = index + 1;
    if block["difficulty"] != "0x0"
        || block["uncles"] != json!([])
        || receipt["from"] != replay["transaction"]["from"]
        || receipt["to"] != replay["transaction"]["to"]
        || receipt["gasUsed"] != format!("0x{:x}", local.execution.gas_used)
        || receipt["status"]
            != if local.execution.status == EvmStatus::Success {
                "0x1"
            } else {
                "0x0"
            }
        || quantity(&receipt["effectiveGasPrice"])? != fees.effective_gas_price
    {
        return Err(bad());
    }
    let logs = receipt["logs"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|log| json!({"address":log["address"],"topics":log["topics"],"data":log["data"]}))
        .collect::<Vec<_>>();
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
    let pre_withdrawals = if !withdrawals.is_empty() && next == transactions.len() {
        Some(reverse_withdrawals(api, chain, execution, &withdrawals).await?)
    } else {
        None
    };
    let before = if single && pre_withdrawals.is_none() {
        BalancePoint::Block(source, None)
    } else {
        BalancePoint::Transaction(execution, index)
    };
    let after = if next == transactions.len() {
        BalancePoint::Block(execution, pre_withdrawals.as_ref())
    } else {
        BalancePoint::Transaction(execution, next)
    };
    let nonce = fixture
        .accounts
        .iter()
        .find(|account| account.address == context.caller)
        .ok_or_else(bad)?
        .nonce;
    let next_nonce = nonce.checked_add(1).ok_or_else(bad)?;
    let ((payer_before, sell_before, buy_before), (payer_after, sell_after, buy_after)) = tokio::try_join!(
        settlement_balances(api, context, before, nonce),
        settlement_balances(api, context, after, next_nonce),
    )?;
    if payer_before != fees.caller_before
        || payer_after != fees.caller_after
        || sell_before != local.balances.caller_sell_before
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
        "scope": if pre_withdrawals.is_some() {
            "TRANSACTION_BEFORE_WITHDRAWALS"
        } else if single {
            "SINGLE_TRANSACTION_BLOCK"
        } else {
            "TRANSACTION_INDEX"
        },
        "transactionHash":tx,"sourceBlockHash":source,"blockHash":execution,
        "transactionIndex":index,"success":local.execution.status == EvmStatus::Success,
        "receiptsRoot":block["receiptsRoot"],
        "gasUsed":local.execution.gas_used,"logsHash":local.execution.logs_hash,
        "payerNativeBefore":payer_before,"payerNativeAfter":payer_after,
        "localReceiptHash":local.receipt_hash,"actualInputConsumed":local.actual_input_consumed,
        "actualBuyAmount":local.actual_buy_amount,"balances":local.balances,
    });
    if let Some(root) = &env.parent_beacon_block_root {
        evidence["parentBeaconBlockRoot"] = root.clone().into();
    }
    if !withdrawals.is_empty() {
        evidence["withdrawalsRoot"] = block["withdrawalsRoot"].clone();
    }
    evidence["evidenceHash"] = hash(&evidence)?.into();
    Ok(evidence)
}

#[derive(Clone, Copy)]
enum BalancePoint<'a> {
    Block(&'a str, Option<&'a Value>),
    Transaction(&'a str, usize),
}

async fn reverse_withdrawals(
    api: &Api,
    chain: &str,
    block: &str,
    withdrawals: &[Withdrawal],
) -> Result<Value> {
    let unavailable = || err("EVM_NODE_RECEIPT_PRE_WITHDRAWAL_STATE_UNAVAILABLE");
    let mut credits = BTreeMap::<[u8; 20], u128>::new();
    for withdrawal in withdrawals {
        // Geth disables a precompile when any state override targets its address.
        if (withdrawal.address[..19] == [0; 19] && (1..=10).contains(&withdrawal.address[19]))
            || withdrawal.amount == 0
        {
            return Err(unavailable());
        }
        *credits.entry(withdrawal.address).or_default() +=
            u128::from(withdrawal.amount) * 1_000_000_000;
    }
    let anchor = json!({"blockHash":block,"requireCanonical":true});
    let mut overrides = serde_json::Map::new();
    for (address, credit) in credits {
        let owner = format!("0x{}", hex::encode(address));
        let get = |method| rpc(api, chain, method, json!([owner, anchor]));
        let after = quantity(&get("eth_getBalance").await?)?;
        let before = after
            .0
            .checked_sub(&credit.into())
            .ok_or_else(unavailable)?;
        if before.bits() == 0 {
            let (nonce, code) =
                tokio::try_join!(get("eth_getTransactionCount"), get("eth_getCode"))?;
            let code = code
                .as_str()
                .and_then(|value| value.strip_prefix("0x"))
                .filter(|value| {
                    value.len() % 2 == 0 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .ok_or_else(unavailable)?;
            // A balance override cannot remove an account created only by the withdrawal.
            if quantity_u64(&nonce)? == 0 && code.is_empty() {
                return Err(unavailable());
            }
        }
        overrides.insert(owner, json!({"balance":format!("0x{before:x}")}));
    }
    Ok(Value::Object(overrides))
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

async fn settlement_balances(
    api: &Api,
    context: &Context,
    point: BalancePoint<'_>,
    expected_nonce: u64,
) -> Result<(Amount, Amount, Amount)> {
    let owner = &context.caller;
    let chain = &context.chain_key;
    let (payer, nonce) = match point {
        BalancePoint::Block(block, _) => {
            let count = rpc(
                api,
                chain,
                "eth_getTransactionCount",
                json!([owner, {"blockHash":block,"requireCanonical":true}]),
            );
            let (payer, nonce) =
                tokio::try_join!(balance(api, context, ZERO_ADDRESS, owner, point), count)?;
            (payer, quantity_u64(&nonce)?)
        }
        BalancePoint::Transaction(block, index) => {
            let account = trace_account(api, chain, owner, block, index).await?;
            let nonce = match account.get("nonce") {
                Some(value) => value
                    .as_u64()
                    .ok_or_else(|| err("EVM_NODE_RECEIPT_QUANTITY")),
                None => Ok(0),
            }?;
            (quantity(&account["balance"])?, nonce)
        }
    };
    // An included transaction increments its sender nonce even on revert.
    // This also rejects a tracer that silently ignores the transaction index.
    if nonce != expected_nonce {
        return Err(err("EVM_NODE_RECEIPT_MISMATCH"));
    }
    let (sell, buy) = tokio::try_join!(
        async {
            if context.sell_token == ZERO_ADDRESS {
                Ok(payer.clone())
            } else {
                balance(api, context, &context.sell_token, owner, point).await
            }
        },
        async {
            if context.buy_token == ZERO_ADDRESS && context.recipient == *owner {
                Ok(payer.clone())
            } else {
                balance(api, context, &context.buy_token, &context.recipient, point).await
            }
        },
    )?;
    Ok((payer, sell, buy))
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
        BalancePoint::Block(block, overrides) => {
            let at = json!({"blockHash":block,"requireCanonical":true});
            if native {
                if let Some(amount) = overrides.and_then(|values| values.get(owner)) {
                    return quantity(&amount["balance"]);
                }
                return quantity(&rpc(api, chain, method, json!([call, at])).await?);
            }
            let params = match overrides {
                Some(overrides) => json!([call, at, overrides]),
                None => json!([call, at]),
            };
            rpc(api, chain, method, params).await?
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
