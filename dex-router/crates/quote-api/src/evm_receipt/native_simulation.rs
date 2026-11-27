use super::{
    balance, balance_call, check_execution_context, err, hash, json, quantity_u64, rpc,
    source_state, token_balance, verify_execution_header, Api, BalancePoint, Instant, Result,
    Value,
};
use crate::local_simulation::Failure;
use xone_domain::singleton::ZERO_ADDRESS;
use xone_execution_plan::{Context, Transaction};
use xone_simulation::evm_fixture::{
    EvmStatus, FixtureState, SimulationOutcome, TransactionContext,
};

pub(crate) async fn compare(
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
        .map_err(|_| Failure::new("SIMULATION_BUSY", "node_simulation"))?;
    tokio::time::timeout_at(deadline.into(), observe(api, fixture, context, replay))
        .await
        .map_err(|_| Failure::new("SIMULATION_TIMEOUT", "node_simulation"))?
        .map_err(|error| Failure::new(&error.0, "node_simulation"))
}

pub(crate) async fn recheck(
    api: &Api,
    fixture: &FixtureState,
    deadline: Instant,
) -> std::result::Result<(), Failure> {
    let _permit = api
        .permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| Failure::new("SIMULATION_BUSY", "node_simulation"))?;
    tokio::time::timeout_at(deadline.into(), check_canonical(api, fixture))
        .await
        .map_err(|_| Failure::new("SIMULATION_TIMEOUT", "node_simulation"))?
        .map_err(|error| Failure::new(&error.0, "node_simulation"))
}

async fn check_canonical(api: &Api, fixture: &FixtureState) -> Result<()> {
    let env = &fixture.execution_environment;
    let current = rpc(
        api,
        &fixture.state_anchor.chain_key,
        "eth_getBlockByNumber",
        json!([format!("0x{:x}", env.block_number), false]),
    )
    .await?;
    if current["hash"] != env.block_hash || current["parentHash"] != fixture.state_anchor.block_hash
    {
        return Err(err("EVM_NODE_SIMULATION_REORGED"));
    }
    Ok(())
}

async fn observe(
    api: &Api,
    fixture: &FixtureState,
    context: &Context,
    replay: &Value,
) -> Result<Value> {
    let bad = || err("EVM_NODE_SIMULATION_MISMATCH");
    let unsupported = || err("EVM_NODE_SIMULATION_UNSUPPORTED");
    let env = &fixture.execution_environment;
    let source = &fixture.state_anchor;
    let chain = &context.chain_key;
    let transaction: Transaction =
        serde_json::from_value(replay["transaction"].clone()).map_err(|_| unsupported())?;
    let tx: TransactionContext =
        serde_json::from_value(replay["transactionContext"].clone()).map_err(|_| unsupported())?;
    let local: SimulationOutcome =
        serde_json::from_value(replay["outcome"].clone()).map_err(|_| unsupported())?;
    if env.fork != "cancun"
        || source.chain_key != *chain
        || transaction.chain_key != *chain
        || transaction.from != context.caller
        || tx.chain_id != env.chain_id
        || local.approval.is_some()
        || local.allowance_reset.is_some()
        || local.wrap.is_some()
        || local.op_fees.is_some()
    {
        return Err(unsupported());
    }
    let beacon_root = env
        .parent_beacon_block_root
        .as_ref()
        .ok_or_else(unsupported)?;
    let chain_id = format!("0x{:x}", env.chain_id);
    let height = format!("0x{:x}", env.block_number);
    let request = |method, params| rpc(api, chain, method, params);
    let (observed_chain, genesis, execution) = tokio::try_join!(
        request("eth_chainId", json!([])),
        request("eth_getBlockByNumber", json!(["0x0", false])),
        request("eth_getBlockByNumber", json!([height, false])),
    )?;
    if observed_chain != chain_id
        || genesis["hash"] != source.genesis_hash
        || execution["hash"] != env.block_hash
        || execution["parentBeaconBlockRoot"] != *beacon_root
    {
        return Err(bad());
    }
    check_execution_context(&execution, fixture).map_err(|_| bad())?;
    verify_execution_header(&execution, &env.fork).map_err(|_| bad())?;
    let state_root = source_state::verify(api, chain, fixture).await?;

    let mut call = json!({
        "chainId":chain_id,"from":transaction.from,"to":transaction.to,
        "input":transaction.data,"value":format!("0x{:x}",transaction.value.0),
        "gas":format!("0x{:x}",tx.gas_limit),"nonce":format!("0x{:x}",tx.nonce),
    });
    if let Some(priority) = &tx.max_priority_fee_per_gas {
        call["maxFeePerGas"] = format!("0x{:x}", tx.gas_price.0).into();
        call["maxPriorityFeePerGas"] = format!("0x{:x}", priority.0).into();
    } else {
        call["gasPrice"] = format!("0x{:x}", tx.gas_price.0).into();
    }
    let options = json!({
        "validation":true,"returnFullTransactions":true,"blockStateCalls":[{
            "blockOverrides":{
                "number":height,"time":execution["timestamp"],"gasLimit":execution["gasLimit"],
                "feeRecipient":env.coinbase,"prevRandao":env.prevrandao,
                "baseFeePerGas":execution["baseFeePerGas"],"beaconRoot":beacon_root
            },
            "calls":[call]
        }]
    });
    let anchor = json!({"blockHash":source.block_hash,"requireCanonical":true});
    let mut requests = vec![json!([options, anchor])];
    if context.sell_token != ZERO_ADDRESS && context.buy_token != ZERO_ADDRESS {
        for (token, owner) in [
            (&context.sell_token, &context.caller),
            (&context.buy_token, &context.recipient),
        ] {
            let mut params = requests[0].clone();
            let mut probe = balance_call(context, token, owner);
            probe["gasPrice"] = execution["baseFeePerGas"].clone();
            params[0]["blockStateCalls"][0]["calls"]
                .as_array_mut()
                .ok_or_else(bad)?
                .push(probe);
            requests.push(params);
        }
    }
    let simulated = request("eth_simulateV1", requests[0].clone()).await?;
    let outcome = check_swap(&simulated, fixture, &call, &local, 1)?;
    let mut settlement = Value::Null;
    if let [_, sell_request, buy_request] = requests.as_slice() {
        let point = BalancePoint::Transaction(&env.block_hash, 0);
        let (sell_before, buy_before, sell, buy) = tokio::try_join!(
            balance(api, context, &context.sell_token, &context.caller, point),
            balance(api, context, &context.buy_token, &context.recipient, point),
            request("eth_simulateV1", sell_request.clone()),
            request("eth_simulateV1", buy_request.clone()),
        )?;
        check_swap(&sell, fixture, &call, &local, 2)?;
        check_swap(&buy, fixture, &call, &local, 2)?;
        let sell_after = post_balance(&sell, sell_request)?;
        let buy_after = post_balance(&buy, buy_request)?;
        if sell_before != local.balances.caller_sell_before
            || sell_after != local.balances.caller_sell_after
            || buy_before != local.balances.recipient_buy_before
            || buy_after != local.balances.recipient_buy_after
        {
            return Err(bad());
        }
        settlement = json!({"callerSellBefore":sell_before,"callerSellAfter":sell_after,
            "recipientBuyBefore":buy_before,"recipientBuyAfter":buy_after,
            "actualInputConsumed":local.actual_input_consumed,"actualBuyAmount":local.actual_buy_amount});
    }
    check_canonical(api, fixture).await?;
    let mut evidence = json!({
        "authority":"CANONICAL_RPC_SIMULATED","sourceBlockHash":source.block_hash,
        "sourceStateRoot":state_root,"executionContextBlockHash":env.block_hash,
        "bindingHash":replay["bindingHash"],"requestHash":hash(&requests)?,
        "gasUsed":local.execution.gas_used,"logsHash":local.execution.logs_hash,
        "returnData":outcome["returnData"],"localReceiptHash":local.receipt_hash,"settlement":settlement,
    });
    evidence["evidenceHash"] = hash(&evidence)?.into();
    Ok(evidence)
}

fn check_swap<'a>(
    simulated: &'a Value,
    fixture: &FixtureState,
    call: &Value,
    local: &SimulationOutcome,
    count: usize,
) -> Result<&'a Value> {
    let bad = || err("EVM_NODE_SIMULATION_MISMATCH");
    let block = only(simulated).ok_or_else(bad)?;
    check_execution_context(block, fixture).map_err(|_| bad())?;
    if block["parentBeaconBlockRoot"]
        != json!(fixture.execution_environment.parent_beacon_block_root)
        || block.get("requestsHash").is_some()
    {
        return Err(bad());
    }
    let transactions = block["transactions"]
        .as_array()
        .filter(|items| items.len() == count)
        .ok_or_else(bad)?;
    for (key, expected) in call.as_object().ok_or_else(bad)? {
        if key == "chainId" && call.get("gasPrice").is_some() {
            continue;
        }
        if &transactions[0][key] != expected {
            return Err(bad());
        }
    }
    let calls = block["calls"]
        .as_array()
        .filter(|items| items.len() == count)
        .ok_or_else(bad)?;
    let gas = calls.iter().try_fold(0_u64, |total, outcome| {
        total
            .checked_add(quantity_u64(&outcome["gasUsed"])?)
            .ok_or_else(bad)
    })?;
    let outcome = &calls[0];
    let logs = outcome["logs"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|log| json!({"address":log["address"],"topics":log["topics"],"data":log["data"]}))
        .collect::<Vec<_>>();
    let logs_hash = hash(&logs)?;
    if outcome["status"] != "0x1"
        || local.execution.status != EvmStatus::Success
        || !local.matches_plan
        || quantity_u64(&outcome["gasUsed"])? != local.execution.gas_used
        || quantity_u64(&block["gasUsed"])? != gas
        || outcome["returnData"] != local.execution.output
        || local.execution.logs_hash.as_deref() != Some(&logs_hash)
    {
        return Err(bad());
    }
    Ok(outcome)
}

fn post_balance(result: &Value, params: &Value) -> Result<xone_domain::Amount> {
    let probe = &params[0]["blockStateCalls"][0]["calls"][1];
    let executed = &result[0]["transactions"][1];
    let outcome = &result[0]["calls"][1];
    if executed["input"] != probe["data"]
        || executed["from"] != probe["from"]
        || executed["to"] != probe["to"]
        || executed["gas"] != probe["gas"]
        || executed["gasPrice"] != probe["gasPrice"]
        || executed["value"] != "0x0"
        || outcome["status"] != "0x1"
        || outcome["logs"] != json!([])
    {
        return Err(err("EVM_NODE_SIMULATION_MISMATCH"));
    }
    token_balance(&outcome["returnData"])
}

fn only(value: &Value) -> Option<&Value> {
    value.as_array().filter(|values| values.len() == 1)?.first()
}
