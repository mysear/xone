//! Pinned Pump native SOL curve instruction/account adapter.
use super::*;
use xone_domain::pump as a;
pub(super) async fn resolve(
    config: &SolanaRpcConfig,
    s: &SolanaQuote,
    request: SolanaRequest,
    capture: Capture,
) -> Result<Value> {
    if request.create_destination_ata
        || request.wrap_source_sol
        || request.refund_wrapped_source
        || request.unwrap_destination_sol
    {
        return Err(err("PREPARATION_OPTIONS_UNSUPPORTED"));
    }
    let p = &s.pool;
    let state = p.pump.as_ref().ok_or_else(bad)?;
    if s.context.caller != state.user || s.context.recipient != state.user {
        return Err(err("PUMP_DECLARED_USER_REQUIRED"));
    }
    let output = if s.zero { &p.token1 } else { &p.token0 };
    let user_ata = ata(&state.user, &p.token0, TOKEN)?;
    let src = request.source_token_account.unwrap_or(if s.zero {
        user_ata.clone()
    } else {
        state.user.clone()
    });
    let dst = request.destination_token_account.unwrap_or(if s.zero {
        state.user.clone()
    } else {
        user_ata
    });
    m::pubkey(&src)?;
    m::pubkey(&dst)?;
    if (s.zero && dst != state.user) || (!s.zero && src != state.user) {
        return Err(err("PREPARATION_NATIVE_ACCOUNT_INVALID"));
    }
    let event = m::pda(&[b"__event_authority"], a::PROGRAM)?;
    let secondary = m::pda(&[b"bonding-curve-v2", &m::pubkey(&p.token0)?], a::PROGRAM)?;
    let global = xone_domain::pump::PumpState::global_id()?;
    let fee = xone_domain::pump::PumpState::fee_id()?;
    let global_volume = m::pda(&[b"global_volume_accumulator"], a::PROGRAM)?;
    let user_volume = m::pda(
        &[b"user_volume_accumulator", &m::pubkey(&state.user)?],
        a::PROGRAM,
    )?;
    let mut ids = state
        .accounts
        .iter()
        .map(|a| a.address.clone())
        .collect::<Vec<_>>();
    ids.extend([
        src.clone(),
        dst.clone(),
        state.user.clone(),
        state.fee_recipient.clone(),
        state.buyback_recipient.clone(),
        event.clone(),
        secondary.clone(),
        a::PROGRAM.into(),
        state.programdata.clone(),
        a::FEE_PROGRAM.into(),
        state.fee_programdata.clone(),
        TOKEN.into(),
        m::pda(&[&m::pubkey(TOKEN)?], LOADER)?,
        ASSOCIATED.into(),
        SYSTEM.into(),
        RENT.into(),
        xone_domain::raydium::CLOCK.into(),
    ]);
    let mut seen = std::collections::BTreeSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    if ids.len() > 100 {
        return Err(err("PREPARATION_ACCOUNT_LIMIT"));
    }
    let rpc = Rpc {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| bad())?,
        url: config.url.clone(),
        capture,
        chain_key: s.context.chain_key.clone(),
        genesis_hash: config.genesis_hash.clone(),
    };
    if rpc.call("getGenesisHash", json!([])).await?.as_str() != Some(config.genesis_hash.as_str()) {
        return Err(err("PREPARATION_WRONG_CHAIN"));
    }
    let response = rpc
        .call(
            "getMultipleAccounts",
            json!([ids,{"encoding":"base64","commitment":"finalized","minContextSlot":state.slot}]),
        )
        .await?;
    let slot = response["context"]["slot"].as_u64().ok_or_else(bad)?;
    if slot < state.slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let values = response["value"]
        .as_array()
        .filter(|v| v.len() == ids.len())
        .ok_or_else(bad)?;
    let bank = ids
        .iter()
        .zip(values)
        .map(|(id, value)| Ok((id.clone(), account(value)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    for item in &state.accounts {
        raw(
            &bank,
            &item.address,
            &item.owner,
            &hex::decode(&item.data).map_err(|_| bad())?,
        )?;
        if get(&bank, &item.address)?.lamports != item.lamports {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    code(
        &bank,
        a::PROGRAM,
        &state.program_code_sha256,
        slot,
        Some(state.deployed_slot),
    )?;
    code(
        &bank,
        a::FEE_PROGRAM,
        &state.fee_program_code_sha256,
        slot,
        Some(state.fee_deployed_slot),
    )?;
    code(&bank, TOKEN, &state.token_program_code_sha256, slot, None)?;
    code(&bank, ASSOCIATED, ASSOCIATED_CODE, slot, None)?;
    let system = get(&bank, SYSTEM)?;
    if !system.executable || system.owner != "NativeLoader1111111111111111111111111111111" {
        return Err(bad());
    }
    let clock = get(&bank, xone_domain::raydium::CLOCK)?;
    if clock.owner != "Sysvar1111111111111111111111111111111111111"
        || clock.executable
        || clock.data.len() != 40
        || m::u64_at(&clock.data, 0) != slot
        || slot != state.slot
        || m::u64_at(&clock.data, 32) != state.timestamp
    {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    let payer = get(&bank, &state.user)?;
    if payer.owner != SYSTEM || payer.executable || !payer.data.is_empty() {
        return Err(bad());
    }
    let token_key = if s.zero { &src } else { &dst };
    let balance = bank
        .get(token_key)
        .and_then(Option::as_ref)
        .map(|a| token(a, &p.token0, &state.user, TOKEN))
        .transpose()?;
    let required = if s.exact && !s.zero {
        s.limit
    } else {
        s.expected_input
    };
    let mut blockers = Vec::new();
    if balance.is_none() {
        blockers.push(if s.zero {
            "SOURCE_ACCOUNT_MISSING"
        } else {
            "DESTINATION_ACCOUNT_MISSING"
        });
    } else if s.zero && balance.is_some_and(|v| v < required) {
        blockers.push("INSUFFICIENT_TOKEN_BALANCE");
    }
    if !s.zero && payer.lamports < required {
        blockers.push("INSUFFICIENT_NATIVE_BALANCE");
    }
    let mut result = json!({"quoteId":s.quote_view_id,"planHash":s.plan_hash,"quoteViewId":s.quote_view_id,"snapshotRef":s.snapshot,"observedSlot":slot,"ready":false,"blockers":blockers,"sourceTokenAccount":src,"destinationTokenAccount":dst,"sourceNativeSol":!s.zero,"destinationNativeSol":s.zero,"sourceTokenProgram":if s.zero {TOKEN} else {SYSTEM},"destinationTokenProgram":if s.zero {SYSTEM} else {TOKEN},"requiredInput":required.to_string(),"quotedInput":s.expected_input.to_string(),"quotedOutput":s.expected_output.to_string(),"accountCreation":false,"sourceAccountCreation":false,"wrapLamports":"0","sourceWsolRetained":false,"refundWrappedSource":false,"unwrapDestinationSol":false,"temporaryWsolAccount":Value::Null,"temporarySeed":Value::Null,"settlementAsset":output,"rentRefundLamports":"0","expectedNativeRefundLamports":"0","expectedResidualWsol":"0","rentLamports":"0","payerLamports":payer.lamports.to_string(),"expectedDestinationTransferFee":"0","simulationRequired":true,"signed":false,"submitted":false,"unsignedTransaction":Value::Null});
    if !blockers.is_empty() {
        return Ok(result);
    }
    let block = rpc
        .call(
            "getLatestBlockhash",
            json!([{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if block["context"]["slot"].as_u64().ok_or_else(bad)? < slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let blockhash = block["value"]["blockhash"].as_str().ok_or_else(bad)?;
    let last = block["value"]["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(bad)?;
    let mut metas = vec![
        meta(&global, false, false),
        meta(&state.fee_recipient, false, true),
        meta(&p.token0, false, false),
        meta(&p.pool_id, false, true),
        meta(&state.vault, false, true),
        meta(token_key, false, true),
        meta(&state.user, true, true),
        meta(SYSTEM, false, false),
    ];
    if s.zero {
        metas.extend([
            meta(&state.creator_vault, false, true),
            meta(TOKEN, false, false),
        ]);
    } else {
        metas.extend([
            meta(TOKEN, false, false),
            meta(&state.creator_vault, false, true),
        ]);
    }
    metas.extend([meta(&event, false, false), meta(a::PROGRAM, false, false)]);
    if !s.zero {
        metas.extend([
            meta(&global_volume, false, false),
            meta(&user_volume, false, true),
        ]);
    }
    metas.extend([
        meta(&fee, false, false),
        meta(a::FEE_PROGRAM, false, false),
        meta(&secondary, false, false),
        meta(&state.buyback_recipient, false, true),
    ]);
    let (discriminator, first, second) = if s.zero {
        (
            [51, 230, 133, 164, 1, 127, 131, 173],
            if s.exact { s.expected_input } else { s.amount },
            if s.exact { s.amount } else { s.limit },
        )
    } else if s.exact {
        ([102, 6, 61, 18, 1, 218, 235, 234], s.amount, s.limit)
    } else {
        ([56, 252, 116, 8, 158, 223, 205, 95], s.amount, s.limit)
    };
    let mut data = discriminator.to_vec();
    data.extend(first.to_le_bytes());
    data.extend(second.to_le_bytes());
    if !s.zero {
        data.extend([0, 0]);
    }
    let instructions = vec![Instruction {
        program_id: a::PROGRAM.into(),
        accounts: metas,
        data_base64: STANDARD.encode(data),
    }];
    let message = compile_message_many(&state.user, blockhash, &instructions)?;
    let fee = rpc
        .call(
            "getFeeForMessage",
            json!([STANDARD.encode(&message),{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if fee["context"]["slot"].as_u64().ok_or_else(bad)? < slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let cost = fee["value"]
        .as_u64()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    let funding = cost
        .checked_add(if s.zero { 0 } else { required })
        .ok_or_else(bad)?;
    if payer.lamports < funding {
        result["blockers"] = json!(["INSUFFICIENT_FEE_BALANCE"]);
        return Ok(result);
    }
    let wire = unsigned_packet(&message)?;
    result["ready"] = true.into();
    result["instructions"] = json!(instructions);
    result["recentBlockhash"] = blockhash.into();
    result["lastValidBlockHeight"] = last.into();
    result["feeLamports"] = cost.to_string().into();
    result["totalFundingLamports"] = funding.to_string().into();
    result["expectedPayerLamportDelta"] = (if s.zero {
        i128::from(s.expected_output) - i128::from(cost)
    } else {
        -i128::from(s.expected_input) - i128::from(cost)
    })
    .to_string()
    .into();
    if request.simulate {
        let response=rpc.call("simulateTransaction",json!([STANDARD.encode(&wire),{"encoding":"base64","commitment":"finalized","minContextSlot":slot,"sigVerify":false,"replaceRecentBlockhash":false,"accounts":{"encoding":"base64","addresses":[src,dst,state.user]}}])).await?;
        let observed = response["context"]["slot"]
            .as_u64()
            .filter(|n| *n >= slot)
            .ok_or_else(bad)?;
        let v = &response["value"];
        if v.get("err") != Some(&Value::Null) {
            return Err(err("PREPARATION_SIMULATION_FAILED"));
        }
        let after = v["accounts"]
            .as_array()
            .filter(|v| v.len() == 3)
            .ok_or_else(bad)?;
        let before = [
            values[ids.iter().position(|v| v == &src).ok_or_else(bad)?].clone(),
            values[ids.iter().position(|v| v == &dst).ok_or_else(bad)?].clone(),
            values[ids.iter().position(|v| v == &state.user).ok_or_else(bad)?].clone(),
        ];
        let (paid, received) =
            verify_simulated_settlement(&s.context, &result, &before, after, cost)?;
        result["simulation"] = json!({"level":"RPC_SIMULATED","success":true,"observedSlot":observed,"actualInput":paid.to_string(),"actualOutput":received.to_string(),"unitsConsumed":v["unitsConsumed"],"independentProductionVerification":false});
    }
    result["messageBase64"] = STANDARD.encode(message).into();
    result["unsignedTransaction"] = STANDARD.encode(wire).into();
    result["requiredSigners"] = json!([state.user]);
    result["preparationHash"] = hash(&result)?.into();
    Ok(result)
}
