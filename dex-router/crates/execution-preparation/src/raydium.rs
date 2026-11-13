//! Pinned CP-Swap classic-SPL adapter. No implicit account creation or SOL conversion.
use super::*;
use xone_domain::raydium as r;
fn owned<'a>(
    bank: &'a BTreeMap<String, Option<Account>>,
    key: &str,
    owner: &str,
    size: usize,
) -> Result<&'a [u8]> {
    let a = get(bank, key)?;
    if a.owner != owner || a.executable || a.data.len() != size {
        return Err(bad());
    }
    Ok(&a.data)
}
fn bump(seeds: &[&[u8]], program: &str, address: &str) -> Result<u8> {
    let target = m::pubkey(address)?;
    for nonce in (0..=255u8).rev() {
        let mut h = Sha256::new();
        for seed in seeds {
            h.update(seed);
        }
        h.update([nonce]);
        h.update(m::pubkey(program)?);
        h.update(b"ProgramDerivedAddress");
        let digest: [u8; 32] = h.finalize().into();
        if digest == target {
            return Ok(nonce);
        }
    }
    Err(bad())
}
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
    let state = p.raydium_cpmm.as_ref().ok_or_else(bad)?;
    let (input, output, iv, ov) = if s.zero {
        (&p.token0, &p.token1, &state.vault0, &state.vault1)
    } else {
        (&p.token1, &p.token0, &state.vault1, &state.vault0)
    };
    let source = request
        .source_token_account
        .unwrap_or(ata(&s.context.caller, input, TOKEN)?);
    let destination =
        request
            .destination_token_account
            .unwrap_or(ata(&s.context.recipient, output, TOKEN)?);
    m::pubkey(&source)?;
    m::pubkey(&destination)?;
    let pool_key = m::pubkey(&p.pool_id)?;
    let observation = m::pda(&[b"observation", &pool_key], r::PROGRAM_ID)?;
    let token_pd = m::pda(&[&m::pubkey(TOKEN)?], LOADER)?;
    let mut ids = vec![
        p.pool_id.clone(),
        state.amm_config.clone(),
        state.vault0.clone(),
        state.vault1.clone(),
        p.token0.clone(),
        p.token1.clone(),
        observation.clone(),
        r::PROGRAM_ID.into(),
        state.programdata.clone(),
        TOKEN.into(),
        token_pd,
        source.clone(),
        destination.clone(),
        s.context.caller.clone(),
        s.context.recipient.clone(),
        r::CLOCK.into(),
        state.authority.clone(),
        RENT.into(),
    ];
    let mut unique = std::collections::BTreeSet::new();
    ids.retain(|id| unique.insert(id.clone()));
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
    let response=rpc.call("getMultipleAccounts",json!([ids,{"encoding":"base64","commitment":"finalized","minContextSlot":state.bank_slot}])).await?;
    let slot = response["context"]["slot"].as_u64().ok_or_else(bad)?;
    if slot < state.bank_slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let values = response["value"]
        .as_array()
        .filter(|v| v.len() == ids.len())
        .ok_or_else(bad)?;
    let bank = ids
        .iter()
        .zip(values)
        .map(|(key, v)| Ok((key.clone(), account(v)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    code(
        &bank,
        r::PROGRAM_ID,
        &state.program_code_sha256,
        slot,
        Some(state.deployed_slot),
    )?;
    code(&bank, TOKEN, &state.token_program_code_sha256, slot, None)?;
    let raw = owned(&bank, &p.pool_id, r::PROGRAM_ID, 637)?;
    let cfg = owned(&bank, &state.amm_config, r::PROGRAM_ID, 236)?;
    if raw[..8] != [247, 237, 227, 245, 215, 195, 222, 70]
        || cfg[..8] != [218, 244, 33, 104, 203, 203, 43, 111]
        || raw[329] & !7 != 0
        || raw[329] & 4 != 0
        || raw[390] > 1
        || cfg[9] > 1
        || raw[391..397]
            .iter()
            .chain(raw[413..].iter())
            .any(|b| *b != 0)
        || cfg[124..].iter().any(|b| *b != 0)
    {
        return Err(err("PREPARATION_ACCOUNT_INVALID"));
    }
    let index = u16::from_le_bytes(cfg[10..12].try_into().map_err(|_| bad())?).to_be_bytes();
    if m::pda(&[b"amm_config", &index], r::PROGRAM_ID)? != state.amm_config
        || cfg[8] != bump(&[b"amm_config", &index], r::PROGRAM_ID, &state.amm_config)?
        || m::pda(&[b"vault_and_lp_mint_auth_seed"], r::PROGRAM_ID)? != state.authority
        || u32::from(raw[328]) != state.authority_bump
        || raw[330] != 9
        || m::key(raw, 136) != m::pda(&[b"pool_lp_mint", &pool_key], r::PROGRAM_ID)?
        || m::key(raw, 296) != observation
    {
        return Err(bad());
    }
    for (offset, want) in [
        (8, &state.amm_config),
        (72, &state.vault0),
        (104, &state.vault1),
        (168, &p.token0),
        (200, &p.token1),
    ] {
        if m::key(raw, offset) != *want {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    for (offset, want) in [
        (341, state.protocol_fees0),
        (349, state.protocol_fees1),
        (357, state.fund_fees0),
        (365, state.fund_fees1),
        (373, state.open_time),
        (381, state.recent_epoch),
        (397, state.creator_fees0),
        (405, state.creator_fees1),
    ] {
        if m::u64_at(raw, offset) != want {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    if u32::from(raw[331]) != state.mint0_decimals
        || u32::from(raw[332]) != state.mint1_decimals
        || u32::from(raw[389]) != state.creator_fee_on
        || (raw[390] == 1) != state.creator_fee_enabled
    {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    for (offset, want) in [
        (12, p.fee_numerator),
        (20, state.protocol_fee_rate),
        (28, state.fund_fee_rate),
        (108, state.creator_fee_rate),
        (116, state.creator_fee_share_rate),
    ] {
        if m::u64_at(cfg, offset) != want {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    let clock = owned(
        &bank,
        r::CLOCK,
        "Sysvar1111111111111111111111111111111111111",
        40,
    )?;
    if m::u64_at(clock, 0) != slot
        || slot != state.bank_slot
        || m::u64_at(clock, 16) != state.bank_epoch
        || m::u64_at(clock, 32) != state.bank_unix_timestamp
    {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    let obs = owned(&bank, &observation, r::PROGRAM_ID, 4075)?;
    if obs[..8] != m::discriminator("ObservationState")
        || obs[8] > 1
        || m::u16_at(obs, 9) >= 100
        || m::key(obs, 11) != p.pool_id
    {
        return Err(bad());
    }
    for (i, (mint, vault, balance, decimals)) in [
        (
            &p.token0,
            &state.vault0,
            state.raw_vault0,
            state.mint0_decimals,
        ),
        (
            &p.token1,
            &state.vault1,
            state.raw_vault1,
            state.mint1_decimals,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let data = owned(&bank, mint, TOKEN, 82)?;
        if data[45] != 1
            || u32::from(data[44]) != decimals
            || m::key(raw, 232 + i * 32) != TOKEN
            || m::pda(
                &[b"pool_vault", &pool_key, &m::pubkey(mint)?],
                r::PROGRAM_ID,
            )? != *vault
            || token(get(&bank, vault)?, mint, &state.authority, TOKEN)? != balance
        {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    let required = if s.exact { s.limit } else { s.amount };
    let before_in = bank
        .get(&source)
        .and_then(Option::as_ref)
        .map(|a| token(a, input, &s.context.caller, TOKEN))
        .transpose()?;
    let before_out = bank
        .get(&destination)
        .and_then(Option::as_ref)
        .map(|a| token(a, output, &s.context.recipient, TOKEN))
        .transpose()?;
    let mut blockers = Vec::new();
    if before_in.is_none() {
        blockers.push("SOURCE_ACCOUNT_MISSING");
    } else if before_in.is_some_and(|n| n < required) {
        blockers.push("INSUFFICIENT_TOKEN_BALANCE");
    }
    if before_out.is_none() {
        blockers.push("DESTINATION_ACCOUNT_MISSING");
    }
    let payer = get(&bank, &s.context.caller)?;
    if payer.owner != SYSTEM || payer.executable || !payer.data.is_empty() {
        return Err(bad());
    }
    let mut result = json!({"quoteId":s.quote_view_id,"quoteViewId":s.quote_view_id,"planHash":s.plan_hash,"snapshotRef":s.snapshot,"observedSlot":slot,"ready":false,"blockers":blockers,"sourceTokenAccount":source,"destinationTokenAccount":destination,"sourceTokenProgram":TOKEN,"destinationTokenProgram":TOKEN,"destinationAccountSize":165,"requiredInput":required.to_string(),"quotedInput":s.expected_input.to_string(),"quotedOutput":s.expected_output.to_string(),"accountCreation":false,"sourceAccountCreation":false,"wrapLamports":"0","rentLamports":"0","destinationRentLamports":"0","rentRefundLamports":"0","expectedDestinationTransferFee":"0","expectedNativeRefundLamports":"0","sourceWsolRetained":false,"refundWrappedSource":false,"unwrapDestinationSol":false,"temporaryWsolAccount":Value::Null,"expectedResidualWsol":"0","settlementAsset":output,"payerLamports":payer.lamports.to_string(),"simulationRequired":true,"signed":false,"submitted":false,"unsignedTransaction":Value::Null});
    if !blockers.is_empty() {
        return Ok(result);
    }
    let latest = rpc
        .call(
            "getLatestBlockhash",
            json!([{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if latest["context"]["slot"].as_u64().ok_or_else(bad)? < slot {
        return Err(err("PREPARATION_STALE_RPC"));
    }
    let blockhash = latest["value"]["blockhash"].as_str().ok_or_else(bad)?;
    let last = latest["value"]["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(bad)?;
    let instruction_name = if s.exact {
        "global:swap_base_output"
    } else {
        "global:swap_base_input"
    };
    let discriminator = Sha256::digest(instruction_name.as_bytes());
    let mut data = discriminator[..8].to_vec();
    for n in if s.exact {
        [s.limit, s.amount]
    } else {
        [s.amount, s.limit]
    } {
        data.extend(n.to_le_bytes());
    }
    let accounts = vec![
        meta(&s.context.caller, true, true),
        meta(&state.authority, false, false),
        meta(&state.amm_config, false, false),
        meta(&p.pool_id, false, true),
        meta(&source, false, true),
        meta(&destination, false, true),
        meta(iv, false, true),
        meta(ov, false, true),
        meta(TOKEN, false, false),
        meta(TOKEN, false, false),
        meta(input, false, false),
        meta(output, false, false),
        meta(&observation, false, true),
    ];
    let instructions = vec![Instruction {
        program_id: r::PROGRAM_ID.into(),
        accounts,
        data_base64: STANDARD.encode(data),
    }];
    let message = compile_message_many(&s.context.caller, blockhash, &instructions)?;
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
    if payer.lamports < cost {
        result["blockers"] = json!(["INSUFFICIENT_FEE_BALANCE"]);
        return Ok(result);
    }
    let wire = unsigned_packet(&message)?;
    result["ready"] = true.into();
    result["feeLamports"] = cost.to_string().into();
    result["totalFundingLamports"] = cost.to_string().into();
    result["expectedPayerLamportDelta"] = (-i128::from(cost)).to_string().into();
    result["recentBlockhash"] = blockhash.into();
    result["lastValidBlockHeight"] = last.into();
    result["instructions"] = json!(instructions);
    if request.simulate {
        let response=rpc.call("simulateTransaction",json!([STANDARD.encode(&wire),{"encoding":"base64","commitment":"finalized","minContextSlot":slot,"sigVerify":false,"replaceRecentBlockhash":false,"accounts":{"encoding":"base64","addresses":[source,destination]}}])).await?;
        let observed = response["context"]["slot"]
            .as_u64()
            .filter(|n| *n >= slot)
            .ok_or_else(bad)?;
        let value = &response["value"];
        if value.get("err") != Some(&Value::Null) {
            return Err(err("PREPARATION_SIMULATION_FAILED"));
        }
        let accounts = value["accounts"]
            .as_array()
            .filter(|v| v.len() == 2)
            .ok_or_else(bad)?;
        let paid = before_in
            .ok_or_else(bad)?
            .checked_sub(token(
                &account(&accounts[0])?.ok_or_else(bad)?,
                input,
                &s.context.caller,
                TOKEN,
            )?)
            .ok_or_else(bad)?;
        let received = token(
            &account(&accounts[1])?.ok_or_else(bad)?,
            output,
            &s.context.recipient,
            TOKEN,
        )?
        .checked_sub(before_out.ok_or_else(bad)?)
        .ok_or_else(bad)?;
        if paid != s.expected_input || received != s.expected_output {
            return Err(err("PREPARATION_SIMULATION_AMOUNT_MISMATCH"));
        }
        result["simulation"] = json!({"level":"RPC_SIMULATED","success":true,"observedSlot":observed,"actualInput":paid.to_string(),"actualOutput":received.to_string(),"unitsConsumed":value["unitsConsumed"],"independentProductionVerification":false});
    }
    result["messageBase64"] = STANDARD.encode(message).into();
    result["unsignedTransaction"] = STANDARD.encode(wire).into();
    result["requiredSigners"] = json!([s.context.caller]);
    result["preparationHash"] = hash(&result)?.into();
    Ok(result)
}
