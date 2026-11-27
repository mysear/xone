//! Whirlpool swap preparation against a captured bank.
use super::*;
use xone_domain::orca as o;
use xone_domain::spl_token_2022::PROGRAM as TOKEN22;
pub(super) const COMPUTE_BUDGET: &str = "ComputeBudget111111111111111111111111111111";
pub(super) const COMPUTE_LIMIT: u32 = 1_400_000;
pub(super) fn owned<'a>(
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
fn amount(bytes: &mut [u8], offset: usize, value: &Amount) -> Result<()> {
    let n = value.0.to_bytes_le();
    if n.len() > 16 {
        return Err(bad());
    }
    bytes[offset..offset + 16].fill(0);
    bytes[offset..offset + n.len()].copy_from_slice(&n);
    Ok(())
}
fn key(bytes: &mut [u8], offset: usize, value: &str) -> Result<()> {
    if value == SYSTEM {
        bytes[offset..offset + 32].fill(0);
    } else {
        bytes[offset..offset + 32].copy_from_slice(&m::pubkey(value)?);
    }
    Ok(())
}
pub(super) fn pool_bytes(p: &Pool, observed: &[u8]) -> Result<Vec<u8>> {
    let s = p.orca_whirlpool.as_ref().ok_or_else(bad)?;
    let mut b = vec![0; 653];
    b[..8].copy_from_slice(&m::discriminator("Whirlpool"));
    key(&mut b, 8, &s.whirlpools_config)?;
    b[40] = s.whirlpool_bump as u8;
    for (off, value) in [
        (41, s.tick_spacing),
        (43, s.fee_tier_index),
        (45, s.fee_rate),
        (47, s.protocol_fee_rate),
    ] {
        b[off..off + 2].copy_from_slice(&(value as u16).to_le_bytes());
    }
    amount(&mut b, 49, &s.liquidity)?;
    amount(&mut b, 65, &s.sqrt_price_x64)?;
    b[81..85].copy_from_slice(&s.tick_current_index.to_le_bytes());
    for (off, v) in [
        (85, s.protocol_fee_owed0),
        (93, s.protocol_fee_owed1),
        (261, s.reward_last_updated_timestamp),
    ] {
        b[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }
    key(&mut b, 101, &p.token0)?;
    key(&mut b, 133, &s.vault0)?;
    amount(&mut b, 165, &s.fee_growth_global0)?;
    key(&mut b, 181, &p.token1)?;
    key(&mut b, 213, &s.vault1)?;
    amount(&mut b, 245, &s.fee_growth_global1)?;
    if s.reward_infos.is_empty() {
        // Legacy static views omit only the inactive slot-zero admin authority.
        // Mint, vault, emissions, growth and the two control extensions stay zero.
        b[333..365].copy_from_slice(&observed[333..365]);
    } else {
        for (i, r) in s.reward_infos.iter().enumerate() {
            let off = 269 + i * 128;
            key(&mut b, off, &r.mint)?;
            key(&mut b, off + 32, &r.vault)?;
            b[off + 64..off + 96].copy_from_slice(&r.extension);
            amount(&mut b, off + 96, &r.emissions_per_second_x64)?;
            amount(&mut b, off + 112, &r.growth_global_x64)?;
        }
    }
    Ok(b)
}
pub(super) fn array_bytes(pool: &Pool, a: &o::OrcaTickArray) -> Result<Vec<u8>> {
    let s = pool.orca_whirlpool.as_ref().ok_or_else(bad)?;
    let dynamic = a.layout == "dynamic";
    let mut b = if dynamic {
        let mut b = vec![0; 60];
        b[..8].copy_from_slice(&m::discriminator("DynamicTickArray"));
        key(&mut b, 12, &pool.pool_id)?;
        amount(&mut b, 44, &a.initialized_bitmap)?;
        b
    } else {
        let mut b = vec![0; 12];
        b[..8].copy_from_slice(&m::discriminator("TickArray"));
        b
    };
    b[8..12].copy_from_slice(&a.start_tick_index.to_le_bytes());
    for i in 0..88 {
        let index = i64::from(a.start_tick_index) + i64::from(i) * i64::from(s.tick_spacing);
        let tick = a.ticks.iter().find(|t| i64::from(t.tick) == index);
        let mut raw = vec![0; if dynamic && tick.is_none() { 1 } else { 113 }];
        if let Some(t) = tick {
            raw[0] = 1;
            raw[1..17].copy_from_slice(
                &t.liquidity_net
                    .parse::<i128>()
                    .map_err(|_| bad())?
                    .to_le_bytes(),
            );
            amount(&mut raw, 17, &t.liquidity_gross)?;
            amount(&mut raw, 33, &t.fee_growth_outside0)?;
            amount(&mut raw, 49, &t.fee_growth_outside1)?;
            for (i, r) in t.reward_growths_outside.iter().enumerate() {
                amount(&mut raw, 65 + 16 * i, r)?;
            }
        }
        b.extend(raw);
    }
    if !dynamic {
        b.extend(m::pubkey(&pool.pool_id)?);
    }
    Ok(b)
}
fn adaptive_oracle_bytes(
    s: &xone_domain::orca_adaptive_state::OrcaAdaptiveState,
) -> Result<Vec<u8>> {
    let mut b = vec![0; 254];
    b[..8].copy_from_slice(&m::discriminator("Oracle"));
    key(&mut b, 8, &s.whirlpool)?;
    for (off, value) in [
        (40, s.trade_enable_timestamp),
        (82, s.last_reference_update_timestamp),
        (90, s.last_major_swap_timestamp),
    ] {
        b[off..off + 8].copy_from_slice(&value.to_le_bytes());
    }
    for (off, value) in [
        (48, s.filter_period),
        (50, s.decay_period),
        (52, s.reduction_factor),
        (62, s.tick_group_size),
        (64, s.major_swap_threshold_ticks),
    ] {
        let value = u16::try_from(value).map_err(|_| bad())?;
        b[off..off + 2].copy_from_slice(&value.to_le_bytes());
    }
    for (off, value) in [
        (54, s.adaptive_fee_control_factor),
        (58, s.max_volatility_accumulator),
        (98, s.volatility_reference),
        (106, s.volatility_accumulator),
    ] {
        b[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }
    b[102..106].copy_from_slice(&s.tick_group_index_reference.to_le_bytes());
    b[66..82].copy_from_slice(&s.constants_reserved);
    b[110..126].copy_from_slice(&s.variables_reserved);
    b[126..254].copy_from_slice(&s.reserved);
    Ok(b)
}
pub(super) async fn resolve(
    config: &SolanaRpcConfig,
    s: &SolanaQuote,
    request: SolanaRequest,
    capture: Capture,
) -> Result<Value> {
    let p = &s.pool;
    let state = p.orca_whirlpool.as_ref().ok_or_else(bad)?;
    let token_state = state.token_2022.as_ref();
    let asset0 = token_state.and_then(|t| t.token0.as_ref());
    let asset1 = token_state.and_then(|t| t.token1.as_ref());
    let program0 = if asset0.is_some() { TOKEN22 } else { TOKEN };
    let program1 = if asset1.is_some() { TOKEN22 } else { TOKEN };
    let (input, output) = if s.zero {
        ((p.token0.as_str(), program0), (p.token1.as_str(), program1))
    } else {
        ((p.token1.as_str(), program1), (p.token0.as_str(), program0))
    };
    let addresses = classic_funding::Addresses::with_programs(s, &request, input, output)?;
    let token2022 = token_state.is_some();
    let using_funding = request.create_destination_ata
        || request.wrap_source_sol
        || request.refund_wrapped_source
        || request.unwrap_destination_sol;
    let mut ids = vec![
        p.pool_id.clone(),
        state.whirlpools_config.clone(),
        state.vault0.clone(),
        state.vault1.clone(),
        p.token0.clone(),
        p.token1.clone(),
        state.oracle.clone(),
        o::PROGRAM_ID.into(),
        state.programdata.clone(),
        TOKEN.into(),
        m::pda(&[&m::pubkey(TOKEN)?], LOADER)?,
        addresses.source.clone(),
        addresses.destination.clone(),
        s.context.caller.clone(),
        s.context.recipient.clone(),
        xone_domain::raydium::CLOCK.into(),
        RENT.into(),
        COMPUTE_BUDGET.into(),
    ];
    ids.extend(state.tick_arrays.iter().map(|a| a.address.clone()));
    if let Some(t) = token_state {
        ids.extend([TOKEN22.into(), t.programdata.clone(), MEMO.into()]);
    }
    if using_funding {
        ids.extend([
            SYSTEM.into(),
            ASSOCIATED.into(),
            m::pda(&[&m::pubkey(ASSOCIATED)?], LOADER)?,
        ]);
    }
    let mut seen = std::collections::BTreeSet::new();
    ids.retain(|id| seen.insert(id.clone()));
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
    let slot = response["context"]["slot"]
        .as_u64()
        .filter(|v| *v >= state.bank_slot)
        .ok_or_else(|| err("PREPARATION_STALE_RPC"))?;
    let values = response["value"]
        .as_array()
        .filter(|a| a.len() == ids.len())
        .ok_or_else(bad)?;
    let bank = ids
        .iter()
        .zip(values)
        .map(|(id, v)| Ok((id.clone(), account(v)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    code(
        &bank,
        o::PROGRAM_ID,
        &state.program_code_sha256,
        slot,
        Some(state.deployed_slot),
    )?;
    code(&bank, TOKEN, &state.token_program_code_sha256, slot, None)?;
    if let Some(t) = token_state {
        if get(&bank, TOKEN22)?.owner != LOADER || get(&bank, MEMO)?.owner != IMMUTABLE {
            return Err(err("PREPARATION_PROGRAM_CHANGED"));
        }
        code(
            &bank,
            TOKEN22,
            &t.program_code_sha256,
            slot,
            Some(t.deployed_slot),
        )?;
        code(&bank, MEMO, MEMO_CODE, slot, None)?;
    }
    let budget = get(&bank, COMPUTE_BUDGET)?;
    if !budget.executable
        || budget.owner != "NativeLoader1111111111111111111111111111111"
        || budget.data != b"compute_budget_program"
    {
        return Err(err("PREPARATION_PROGRAM_CHANGED"));
    }
    if using_funding {
        code(&bank, ASSOCIATED, ASSOCIATED_CODE, slot, None)?;
        let a = get(&bank, SYSTEM)?;
        if !a.executable || a.owner != "NativeLoader1111111111111111111111111111111" {
            return Err(bad());
        }
    }
    let observed = owned(&bank, &p.pool_id, o::PROGRAM_ID, 653)?;
    raw(&bank, &p.pool_id, o::PROGRAM_ID, &pool_bytes(p, observed)?)?;
    let cfg = owned(&bank, &state.whirlpools_config, o::PROGRAM_ID, 108)?;
    if cfg[..8] != m::discriminator("WhirlpoolsConfig")
        || m::u16_at(cfg, 104) > 2500
        || m::u16_at(cfg, 106) & !1 != 0
    {
        return Err(bad());
    }
    let pool_key = m::pubkey(&p.pool_id)?;
    if m::pda(
        &[
            b"whirlpool",
            &m::pubkey(&state.whirlpools_config)?,
            &m::pubkey(&p.token0)?,
            &m::pubkey(&p.token1)?,
            &(state.fee_tier_index as u16).to_le_bytes(),
        ],
        o::PROGRAM_ID,
    )? != p.pool_id
        || m::pda(&[b"oracle", &pool_key], o::PROGRAM_ID)? != state.oracle
    {
        return Err(bad());
    }
    if let Some(adaptive) = &state.adaptive {
        raw(
            &bank,
            &state.oracle,
            o::PROGRAM_ID,
            &adaptive_oracle_bytes(adaptive)?,
        )?;
    } else if let Some(a) = bank.get(&state.oracle).and_then(Option::as_ref) {
        if a.owner != SYSTEM || a.executable || !a.data.is_empty() {
            return Err(err("PREPARATION_ORCA_ORACLE_UNSUPPORTED"));
        }
    }
    let clock = owned(&bank, xone_domain::raydium::CLOCK, SYSVAR, 40)?;
    if slot != state.bank_slot
        || m::u64_at(clock, 0) != slot
        || m::u64_at(clock, 16) != state.bank_epoch
        || m::u64_at(clock, 32) != state.bank_unix_timestamp
    {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    for (mint, vault, expected, decimals, asset) in [
        (
            &p.token0,
            &state.vault0,
            state.raw_vault0,
            state.mint0_decimals,
            asset0,
        ),
        (
            &p.token1,
            &state.vault1,
            state.raw_vault1,
            state.mint1_decimals,
            asset1,
        ),
    ] {
        if let Some(asset) = asset {
            raw(
                &bank,
                mint,
                TOKEN22,
                &hex::decode(&asset.mint_data).map_err(|_| bad())?,
            )?;
            raw(
                &bank,
                vault,
                TOKEN22,
                &hex::decode(&asset.vault_data).map_err(|_| bad())?,
            )?;
            continue;
        }
        let mint_raw = owned(&bank, mint, TOKEN, 82)?;
        if mint_raw[45] != 1
            || u32::from(mint_raw[44]) != decimals
            || u32::from_le_bytes(mint_raw[..4].try_into().map_err(|_| bad())?) > 1
            || u32::from_le_bytes(mint_raw[46..50].try_into().map_err(|_| bad())?) > 1
        {
            return Err(bad());
        }
        let a = get(&bank, vault)?;
        owned(&bank, vault, TOKEN, 165)?;
        // Vaults can contain an observed unsolicited SOL donation. Swap never
        // syncs it: validate backing without replacing the captured lamports.
        let mut value = Account {
            owner: a.owner.clone(),
            data: a.data.clone(),
            lamports: a.lamports,
            executable: a.executable,
        };
        if mint == xone_domain::pump_amm::WSOL {
            let backing = m::u64_at(&a.data, 113)
                .checked_add(expected)
                .ok_or_else(bad)?;
            if a.lamports < backing {
                return Err(bad());
            }
            value.lamports = backing;
        }
        if token(&value, mint, &p.pool_id, TOKEN)? != expected {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    for a in &state.tick_arrays {
        if m::pda(
            &[
                b"tick_array",
                &pool_key,
                a.start_tick_index.to_string().as_bytes(),
            ],
            o::PROGRAM_ID,
        )? != a.address
        {
            return Err(bad());
        }
        if a.layout == "sparse" {
            raw(&bank, &a.address, SYSTEM, &[])?;
        } else {
            raw(&bank, &a.address, o::PROGRAM_ID, &array_bytes(p, a)?)?;
        }
    }
    let span = i64::from(state.tick_spacing) * 88;
    let current = (i64::from(state.tick_current_index)
        + if s.zero {
            0
        } else {
            i64::from(state.tick_spacing)
        })
    .div_euclid(span)
        * span;
    let mut arrays = Vec::new();
    for i in 0..3 {
        let start = current + if s.zero { -i * span } else { i * span };
        if let Some(a) = state
            .tick_arrays
            .iter()
            .find(|a| i64::from(a.start_tick_index) == start)
        {
            arrays.push(a.address.clone());
        } else {
            break;
        }
    }
    let last_array = arrays
        .last()
        .cloned()
        .ok_or_else(|| err("PREPARATION_ORCA_ARRAY_WINDOW"))?;
    // The original builder deduplicates account keys and accepts a shorter
    // proven window. Repeating its last account fills the three ABI slots
    // without fabricating an unobserved array outside the quote snapshot.
    while arrays.len() < 3 {
        arrays.push(last_array.clone());
    }
    let required = if s.exact { s.limit } else { s.amount };
    let mut funding =
        classic_funding::prepare(&rpc, &bank, s, &request, &addresses, required).await?;
    if token2022 {
        let q = if s.exact {
            xone_protocol_math::orca::quote_exact_out(p, &Amount(s.amount.into()), s.zero)?
        } else {
            xone_protocol_math::orca::quote_exact_in(p, &Amount(s.amount.into()), s.zero)?
        };
        if q.amount_out != Amount(s.expected_output.into())
            || q.consumed_in != Amount(s.expected_input.into())
        {
            return Err(err("PREPARATION_QUOTE_INVALID"));
        }
        let next = q.next_pool.orca_whirlpool.as_ref().ok_or_else(bad)?;
        let (before, after) = if s.zero {
            (state.raw_vault1, next.raw_vault1)
        } else {
            (state.raw_vault0, next.raw_vault0)
        };
        let gross = before.checked_sub(after).ok_or_else(bad)?;
        let fee = gross.checked_sub(s.expected_output).ok_or_else(bad)?;
        funding.result["expectedDestinationTransferFee"] = fee.to_string().into();
        funding.result["expectedOrcaVaults"] = json!([
            {"amount":next.raw_vault0.to_string(),"token2022Data":next.token_2022.as_ref().and_then(|t| t.token0.as_ref()).map(|a|&a.vault_data)},
            {"amount":next.raw_vault1.to_string(),"token2022Data":next.token_2022.as_ref().and_then(|t| t.token1.as_ref()).map(|a|&a.vault_data)},
        ]);
    }
    funding.result["observedSlot"] = slot.into();
    if !funding.result["blockers"]
        .as_array()
        .ok_or_else(bad)?
        .is_empty()
    {
        return Ok(funding.result);
    }
    let recent_blockhash = rpc.latest_blockhash(slot).await?;
    let blockhash = recent_blockhash.hash.as_str();
    let last = recent_blockhash.last_valid_block_height;
    let discriminator = if token2022 {
        &b"global:swap_v2"[..]
    } else {
        &b"global:swap"[..]
    };
    let mut data = Sha256::digest(discriminator)[..8].to_vec();
    data.extend(s.amount.to_le_bytes());
    data.extend(s.limit.to_le_bytes());
    let limit: u128 = if s.zero {
        4295048016
    } else {
        79226673515401279992447579055
    };
    data.extend(limit.to_le_bytes());
    data.extend([u8::from(!s.exact), u8::from(s.zero)]);
    if token2022 {
        data.push(0);
    }
    let (account0, account1) = if s.zero {
        (&addresses.source, &addresses.destination)
    } else {
        (&addresses.destination, &addresses.source)
    };
    let mut metas = if token2022 {
        vec![
            meta(program0, false, false),
            meta(program1, false, false),
            meta(MEMO, false, false),
            meta(&s.context.caller, true, false),
            meta(&p.pool_id, false, true),
            meta(&p.token0, false, false),
            meta(&p.token1, false, false),
            meta(account0, false, true),
            meta(&state.vault0, false, true),
            meta(account1, false, true),
            meta(&state.vault1, false, true),
        ]
    } else {
        vec![
            meta(TOKEN, false, false),
            meta(&s.context.caller, true, false),
            meta(&p.pool_id, false, true),
            meta(account0, false, true),
            meta(&state.vault0, false, true),
            meta(account1, false, true),
            meta(&state.vault1, false, true),
        ]
    };
    metas.extend(arrays.iter().map(|a| meta(a, false, true)));
    metas.push(meta(&state.oracle, false, state.adaptive.is_some()));
    let mut limit_data = vec![2];
    limit_data.extend(COMPUTE_LIMIT.to_le_bytes());
    let mut instructions = vec![Instruction {
        program_id: COMPUTE_BUDGET.into(),
        accounts: Vec::new(),
        data_base64: STANDARD.encode(limit_data),
    }];
    instructions.append(&mut funding.prefix);
    instructions.push(Instruction {
        program_id: o::PROGRAM_ID.into(),
        accounts: metas,
        data_base64: STANDARD.encode(data),
    });
    instructions.append(&mut funding.suffix);
    let message = compile_message_many(&s.context.caller, blockhash, &instructions)?;
    let (cost, fee_slot) = rpc.message_fee(&message, &recent_blockhash).await?;
    if !funding.finish(s, &request, cost)? {
        return Ok(funding.result);
    }
    let mut result = funding.result;
    let wire = unsigned_packet(&message)?;
    result["ready"] = true.into();
    result["recentBlockhash"] = blockhash.into();
    result["blockhashObservedSlot"] = recent_blockhash.observed_slot.into();
    result["lastValidBlockHeight"] = last.into();
    result["computeUnitLimit"] = COMPUTE_LIMIT.into();
    result["instructions"] = json!(instructions);
    if request.simulate {
        let response=rpc.call("simulateTransaction",json!([STANDARD.encode(&wire),{"encoding":"base64","commitment":"finalized","minContextSlot":fee_slot,"sigVerify":false,"replaceRecentBlockhash":false,"accounts":{"encoding":"base64","addresses":[addresses.source,addresses.destination,s.context.caller]}}])).await?;
        let observed = response["context"]["slot"]
            .as_u64()
            .filter(|v| *v >= fee_slot)
            .ok_or_else(bad)?;
        let v = &response["value"];
        if v.get("err") != Some(&Value::Null) {
            return Err(err("PREPARATION_SIMULATION_FAILED"));
        }
        let after = v["accounts"]
            .as_array()
            .filter(|a| a.len() == 3)
            .ok_or_else(bad)?;
        let before = [&addresses.source, &addresses.destination, &s.context.caller].map(|key| {
            values[ids
                .iter()
                .position(|id| id == key)
                .expect("captured account")]
            .clone()
        });
        let (paid, received) =
            verify_simulated_settlement(&s.context, &result, &before, after, cost)?;
        result["simulation"] = json!({"level":"RPC_SIMULATED","success":true,"observedSlot":observed,"actualInput":paid.to_string(),"actualOutput":received.to_string(),"unitsConsumed":v["unitsConsumed"],"independentProductionVerification":false});
    }
    result["messageBase64"] = STANDARD.encode(message).into();
    result["unsignedTransaction"] = STANDARD.encode(wire).into();
    result["requiredSigners"] = json!([s.context.caller]);
    result["preparationHash"] = hash(&result)?.into();
    Ok(result)
}
