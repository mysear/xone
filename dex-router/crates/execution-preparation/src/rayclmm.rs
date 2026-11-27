//! Pinned classic SPL CLMM models with declared extension, seed and fee state.
use super::*;
use xone_domain::raydium_clmm as r;
const TOKEN22: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
// Explicitly admitted ancillary programs. Token2022 assets remain unsupported.
const TOKEN22_CODE: &str = "495e9d7680dd555cb126a6a8e5464af5be9b01f02f2cd70634352722d22e3cad";
const BUDGET: &str = "ComputeBudget111111111111111111111111111111";
const COMPUTE_LIMIT: u32 = 1_400_000;
fn supported(protocol: &str) -> bool {
    matches!(
        protocol,
        "raydium-clmm-static"
            | "raydium-clmm-rewards"
            | "raydium-clmm-token-fee"
            | "raydium-clmm-bitmap"
            | "raydium-clmm-custom-seed"
            | "raydium-clmm-dynamic"
    )
}
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
fn amount(b: &mut [u8], offset: usize, value: &Amount) -> Result<()> {
    let data = value.0.to_bytes_le();
    if data.len() > 16 {
        return Err(bad());
    }
    b[offset..offset + 16].fill(0);
    b[offset..offset + data.len()].copy_from_slice(&data);
    Ok(())
}
fn key(b: &mut [u8], offset: usize, value: &str) -> Result<()> {
    if value == SYSTEM {
        b[offset..offset + 32].fill(0);
    } else {
        b[offset..offset + 32].copy_from_slice(&m::pubkey(value)?);
    }
    Ok(())
}
fn bump(seeds: &[&[u8]], program: &str, address: &str) -> Result<u8> {
    let target = m::pubkey(address)?;
    for value in (0..=255u8).rev() {
        let mut h = Sha256::new();
        for seed in seeds {
            h.update(seed);
        }
        h.update([value]);
        h.update(m::pubkey(program)?);
        h.update(b"ProgramDerivedAddress");
        let bytes: [u8; 32] = h.finalize().into();
        if bytes == target {
            return Ok(value);
        }
    }
    Err(bad())
}
fn pool_bytes(p: &Pool, observed: &[u8]) -> Result<Vec<u8>> {
    let s = p.raydium_clmm.as_ref().ok_or_else(bad)?;
    let mut b = observed.to_vec();
    if b.len() != 1544
        || b[273..277] != [0; 4]
        || b[389] & !63 != 0
        || b[389] & 16 != 0
        || b[393..397] != [0; 4]
        || b[if s.dynamic_fee.is_some() { 1130 } else { 1096 }..]
            .iter()
            .any(|v| *v != 0)
    {
        return Err(err("PREPARATION_RAYCLMM_VERSION_UNSUPPORTED"));
    }
    b[..8].copy_from_slice(&m::discriminator("PoolState"));
    b[8] = s.pool_bump as u8;
    for (off, value) in [
        (9, &s.amm_config),
        (73, &p.token0),
        (105, &p.token1),
        (137, &s.vault0),
        (169, &s.vault1),
        (201, &s.observation),
    ] {
        key(&mut b, off, value)?;
    }
    b[233] = s.mint0_decimals as u8;
    b[234] = s.mint1_decimals as u8;
    b[235..237].copy_from_slice(&(s.tick_spacing as u16).to_le_bytes());
    amount(&mut b, 237, &s.liquidity)?;
    amount(&mut b, 253, &s.sqrt_price_x64)?;
    b[269..273].copy_from_slice(&s.tick_current.to_le_bytes());
    amount(&mut b, 277, &s.fee_growth_global0)?;
    amount(&mut b, 293, &s.fee_growth_global1)?;
    for (off, value) in [
        (309, s.protocol_fees0),
        (317, s.protocol_fees1),
        (1064, s.fund_fees0),
        (1072, s.fund_fees1),
        (1080, s.open_time),
        (1088, s.recent_epoch),
    ] {
        b[off..off + 8].copy_from_slice(&value.to_le_bytes());
    }
    b[390] = s.collect_fee_on as u8;
    b[391..393].copy_from_slice(&(s.seed_index as u16).to_le_bytes());
    for i in 0..3 {
        let off = 397 + 169 * i;
        if let Some(reward) = s.reward_infos.get(i) {
            b[off] = reward.reward_state as u8;
            for (delta, value) in [
                (1, reward.open_time),
                (9, reward.end_time),
                (17, reward.last_update_time),
                (41, reward.reward_total_emitted),
                (49, reward.reward_claimed),
            ] {
                b[off + delta..off + delta + 8].copy_from_slice(&value.to_le_bytes());
            }
            amount(&mut b, off + 25, &reward.emissions_per_second_x64)?;
            key(&mut b, off + 57, &reward.token_mint)?;
            key(&mut b, off + 89, &reward.token_vault)?;
            key(&mut b, off + 121, &reward.authority)?;
            amount(&mut b, off + 153, &reward.reward_growth_global_x64)?;
        } else {
            b[off..off + 121].fill(0);
            b[off + 153..off + 169].fill(0);
        }
    }
    if let Some(d) = &s.dynamic_fee {
        for (off, value) in [
            (1096, d.filter_period),
            (1098, d.decay_period),
            (1100, d.reduction_factor),
        ] {
            b[off..off + 2]
                .copy_from_slice(&u16::try_from(value).map_err(|_| bad())?.to_le_bytes());
        }
        for (off, value) in [
            (1102, d.dynamic_fee_control),
            (1106, d.max_volatility_accumulator),
            (1114, d.volatility_reference),
            (1118, d.volatility_accumulator),
        ] {
            b[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
        b[1110..1114].copy_from_slice(&d.tick_spacing_index_reference.to_le_bytes());
        b[1122..1130].copy_from_slice(&d.last_update_timestamp.to_le_bytes());
    }
    for (i, value) in s.bitmap.iter().enumerate() {
        b[904 + 8 * i..912 + 8 * i].copy_from_slice(&value.to_le_bytes());
    }
    Ok(b)
}
fn array_bytes(p: &Pool, a: &r::RaydiumCLMMArray, observed: &[u8]) -> Result<Vec<u8>> {
    let s = p.raydium_clmm.as_ref().ok_or_else(bad)?;
    let mut b = observed.to_vec();
    if b.len() != 10240 || b[10133..].iter().any(|v| *v != 0) {
        return Err(bad());
    }
    b[..8].copy_from_slice(&m::discriminator("TickArrayState"));
    key(&mut b, 8, &p.pool_id)?;
    b[40..44].copy_from_slice(&a.start_tick_index.to_le_bytes());
    for i in 0..60 {
        let off = 44 + i * 168;
        let index = i64::from(a.start_tick_index) + i as i64 * i64::from(s.tick_spacing);
        if b[off + 124..off + 168].iter().any(|v| *v != 0) {
            return Err(err("PREPARATION_RAYCLMM_ORDERS_UNSUPPORTED"));
        }
        if let Some(t) = a.ticks.iter().find(|t| i64::from(t.tick) == index) {
            b[off..off + 4].copy_from_slice(&t.tick.to_le_bytes());
            b[off + 4..off + 20].copy_from_slice(
                &t.liquidity_net
                    .parse::<i128>()
                    .map_err(|_| bad())?
                    .to_le_bytes(),
            );
            amount(&mut b, off + 20, &t.liquidity_gross)?;
            amount(&mut b, off + 36, &t.fee_growth_outside0)?;
            amount(&mut b, off + 52, &t.fee_growth_outside1)?;
            b[off + 68..off + 116].fill(0);
            for (j, value) in t.reward_growths_outside.iter().enumerate() {
                amount(&mut b, off + 68 + 16 * j, value)?;
            }
        } else {
            let actual = i32::from_le_bytes(b[off..off + 4].try_into().map_err(|_| bad())?);
            if actual != 0 && i64::from(actual) != index {
                return Err(bad());
            }
            b[off + 4..off + 116].fill(0);
        }
    }
    b[10124] = u8::try_from(a.ticks.len()).map_err(|_| bad())?;
    b[10125..10133].copy_from_slice(&a.recent_epoch.to_le_bytes());
    Ok(b)
}
fn observation_bytes(p: &Pool) -> Result<Vec<u8>> {
    let s = p.raydium_clmm.as_ref().ok_or_else(bad)?;
    let mut b = vec![0; 4483];
    b[..8].copy_from_slice(&m::discriminator("ObservationState"));
    b[8] = u8::from(s.observation_initialized);
    b[9..17].copy_from_slice(&s.observation_recent_epoch.to_le_bytes());
    b[17..19].copy_from_slice(&(s.observation_index as u16).to_le_bytes());
    key(&mut b, 19, &p.pool_id)?;
    if s.observations.len() != 100 {
        return Err(bad());
    }
    for (i, o) in s.observations.iter().enumerate() {
        let off = 51 + 44 * i;
        b[off..off + 4].copy_from_slice(&o.block_timestamp.to_le_bytes());
        b[off + 4..off + 12].copy_from_slice(&o.tick_cumulative.to_le_bytes());
    }
    Ok(b)
}
pub(super) async fn resolve(
    config: &SolanaRpcConfig,
    s: &SolanaQuote,
    request: SolanaRequest,
    capture: Capture,
) -> Result<Value> {
    let p = &s.pool;
    let state = p.raydium_clmm.as_ref().ok_or_else(bad)?;
    if !supported(&p.protocol) {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let (input, output) = if s.zero {
        (&p.token0, &p.token1)
    } else {
        (&p.token1, &p.token0)
    };
    let addresses = classic_funding::Addresses::new(s, &request, input, output)?;
    let using_funding = request.create_destination_ata
        || request.wrap_source_sol
        || request.refund_wrapped_source
        || request.unwrap_destination_sol;
    let mut ids = vec![
        p.pool_id.clone(),
        state.amm_config.clone(),
        state.vault0.clone(),
        state.vault1.clone(),
        state.observation.clone(),
        p.token0.clone(),
        p.token1.clone(),
        r::PROGRAM_ID.into(),
        state.programdata.clone(),
        TOKEN.into(),
        m::pda(&[&m::pubkey(TOKEN)?], LOADER)?,
        TOKEN22.into(),
        m::pda(&[&m::pubkey(TOKEN22)?], LOADER)?,
        MEMO.into(),
        BUDGET.into(),
        xone_domain::raydium::CLOCK.into(),
        RENT.into(),
        addresses.source.clone(),
        addresses.destination.clone(),
        s.context.caller.clone(),
        s.context.recipient.clone(),
    ];
    ids.extend(state.tick_arrays.iter().map(|a| a.address.clone()));
    if let Some(extension) = &state.bitmap_extension {
        ids.push(extension.address.clone());
    }
    if using_funding {
        ids.extend([
            SYSTEM.into(),
            ASSOCIATED.into(),
            m::pda(&[&m::pubkey(ASSOCIATED)?], LOADER)?,
        ]);
    }
    let mut unique = std::collections::BTreeSet::new();
    ids.retain(|key| unique.insert(key.clone()));
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
        .filter(|n| *n >= state.bank_slot)
        .ok_or_else(|| err("PREPARATION_STALE_RPC"))?;
    let values = response["value"]
        .as_array()
        .filter(|v| v.len() == ids.len())
        .ok_or_else(bad)?;
    let bank = ids
        .iter()
        .zip(values)
        .map(|(id, v)| Ok((id.clone(), account(v)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    code(
        &bank,
        r::PROGRAM_ID,
        &state.program_code_sha256,
        slot,
        Some(state.deployed_slot),
    )?;
    code(&bank, TOKEN, &state.token_program_code_sha256, slot, None)?;
    code(&bank, TOKEN22, TOKEN22_CODE, slot, None)?;
    code(&bank, MEMO, MEMO_CODE, slot, None)?;
    let budget = get(&bank, BUDGET)?;
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
    let observed = owned(&bank, &p.pool_id, r::PROGRAM_ID, 1544)?;
    raw(&bank, &p.pool_id, r::PROGRAM_ID, &pool_bytes(p, observed)?)?;
    let cfg = owned(&bank, &state.amm_config, r::PROGRAM_ID, 117)?;
    if cfg[..8] != m::discriminator("AmmConfig")
        || cfg[57..61] != [0; 4]
        || cfg[93..].iter().any(|v| *v != 0)
    {
        return Err(bad());
    }
    for (off, want) in [
        (43, state.protocol_fee_rate),
        (47, state.trade_fee_rate),
        (53, state.fund_fee_rate),
    ] {
        if u32::from_le_bytes(cfg[off..off + 4].try_into().map_err(|_| bad())?) != want {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
    }
    if u32::from(m::u16_at(cfg, 51)) != state.tick_spacing {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    let config_seed = m::u16_at(cfg, 9).to_be_bytes();
    let seed_bytes = u16::try_from(state.seed_index)
        .map_err(|_| bad())?
        .to_le_bytes();
    let seed: &[u8] = if state.seed_index == 0 {
        &[]
    } else {
        &seed_bytes
    };
    let pool_key = m::pubkey(&p.pool_id)?;
    let cfg_key = m::pubkey(&state.amm_config)?;
    let m0 = m::pubkey(&p.token0)?;
    let m1 = m::pubkey(&p.token1)?;
    if m::pda(&[b"amm_config", &config_seed], r::PROGRAM_ID)? != state.amm_config
        || bump(
            &[b"amm_config", &config_seed],
            r::PROGRAM_ID,
            &state.amm_config,
        )? != cfg[8]
        || m::pda(&[b"pool", &cfg_key, &m0, &m1, seed], r::PROGRAM_ID)? != p.pool_id
        || u32::from(bump(
            &[b"pool", &cfg_key, &m0, &m1, seed],
            r::PROGRAM_ID,
            &p.pool_id,
        )?) != state.pool_bump
        || m::pda(&[b"observation", &pool_key], r::PROGRAM_ID)? != state.observation
    {
        return Err(bad());
    }
    let clock = owned(
        &bank,
        xone_domain::raydium::CLOCK,
        "Sysvar1111111111111111111111111111111111111",
        40,
    )?;
    if slot != state.bank_slot
        || m::u64_at(clock, 0) != slot
        || m::u64_at(clock, 16) != state.bank_epoch
        || m::u64_at(clock, 32) != state.bank_unix_timestamp
    {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    raw(
        &bank,
        &state.observation,
        r::PROGRAM_ID,
        &observation_bytes(p)?,
    )?;
    for (mint, vault, expected, decimals) in [
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
    ] {
        let d = owned(&bank, mint, TOKEN, 82)?;
        if d[45] != 1
            || u32::from(d[44]) != decimals
            || u32::from_le_bytes(d[..4].try_into().map_err(|_| bad())?) > 1
            || u32::from_le_bytes(d[46..50].try_into().map_err(|_| bad())?) > 1
            || m::pda(
                &[b"pool_vault", &pool_key, &m::pubkey(mint)?],
                r::PROGRAM_ID,
            )? != *vault
        {
            return Err(bad());
        }
        let a = get(&bank, vault)?;
        owned(&bank, vault, TOKEN, 165)?;
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
    if let Some(extension) = &state.bitmap_extension {
        if extension.positive.len() != 112
            || extension.negative.len() != 112
            || m::pda(
                &[b"pool_tick_array_bitmap_extension", &pool_key],
                r::PROGRAM_ID,
            )? != extension.address
        {
            return Err(bad());
        }
        let mut bytes = vec![0; 1832];
        bytes[..8].copy_from_slice(&m::discriminator("TickArrayBitmapExtension"));
        key(&mut bytes, 8, &p.pool_id)?;
        for (i, value) in extension
            .positive
            .iter()
            .chain(&extension.negative)
            .enumerate()
        {
            bytes[40 + 8 * i..48 + 8 * i].copy_from_slice(&value.to_le_bytes());
        }
        raw(&bank, &extension.address, r::PROGRAM_ID, &bytes)?;
    }
    for a in &state.tick_arrays {
        if m::pda(
            &[b"tick_array", &pool_key, &a.start_tick_index.to_be_bytes()],
            r::PROGRAM_ID,
        )? != a.address
        {
            return Err(bad());
        }
        let observed = owned(&bank, &a.address, r::PROGRAM_ID, 10240)?;
        raw(
            &bank,
            &a.address,
            r::PROGRAM_ID,
            &array_bytes(p, a, observed)?,
        )?;
    }
    let span = i64::from(state.tick_spacing) * 60;
    let center = i64::from(state.tick_current).div_euclid(span) * span;
    let mut arrays = state
        .tick_arrays
        .iter()
        .filter(|a| {
            !a.ticks.is_empty()
                && if s.zero {
                    i64::from(a.start_tick_index) <= center
                } else {
                    i64::from(a.start_tick_index) >= center
                }
        })
        .map(|a| a.address.clone())
        .collect::<Vec<_>>();
    if s.zero {
        arrays.reverse();
    }
    if arrays.is_empty() {
        return Err(err("PREPARATION_RAYCLMM_ARRAY_WINDOW"));
    }
    let required = if s.exact { s.limit } else { s.amount };
    let mut funding =
        classic_funding::prepare(&rpc, &bank, s, &request, &addresses, required).await?;
    funding.result["observedSlot"] = slot.into();
    if !funding.result["blockers"]
        .as_array()
        .ok_or_else(bad)?
        .is_empty()
    {
        return Ok(funding.result);
    }
    let latest = rpc
        .call(
            "getLatestBlockhash",
            json!([{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if latest["context"]["slot"].as_u64().is_none_or(|v| v < slot) {
        return Err(bad());
    }
    let blockhash = latest["value"]["blockhash"].as_str().ok_or_else(bad)?;
    let last = latest["value"]["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(bad)?;
    let mut data = Sha256::digest(b"global:swap_v2")[..8].to_vec();
    data.extend(s.amount.to_le_bytes());
    data.extend(s.limit.to_le_bytes());
    let price: u128 = if s.zero {
        4295048017
    } else {
        79226673521066979257578248090
    };
    data.extend(price.to_le_bytes());
    data.push(u8::from(!s.exact));
    let (iv, ov) = if s.zero {
        (&state.vault0, &state.vault1)
    } else {
        (&state.vault1, &state.vault0)
    };
    let mut metas = vec![
        meta(&s.context.caller, true, false),
        meta(&state.amm_config, false, false),
        meta(&p.pool_id, false, true),
        meta(&addresses.source, false, true),
        meta(&addresses.destination, false, true),
        meta(iv, false, true),
        meta(ov, false, true),
        meta(&state.observation, false, true),
        meta(TOKEN, false, false),
        meta(TOKEN22, false, false),
        meta(MEMO, false, false),
        meta(input, false, false),
        meta(output, false, false),
    ];
    if let Some(extension) = &state.bitmap_extension {
        metas.push(meta(&extension.address, false, true));
    }
    metas.extend(arrays.iter().map(|key| meta(key, false, true)));
    let mut limit_data = vec![2];
    limit_data.extend(COMPUTE_LIMIT.to_le_bytes());
    let mut instructions = vec![Instruction {
        program_id: BUDGET.into(),
        accounts: Vec::new(),
        data_base64: STANDARD.encode(limit_data),
    }];
    instructions.append(&mut funding.prefix);
    instructions.push(Instruction {
        program_id: r::PROGRAM_ID.into(),
        accounts: metas,
        data_base64: STANDARD.encode(data),
    });
    instructions.append(&mut funding.suffix);
    let message = compile_message_many(&s.context.caller, blockhash, &instructions)?;
    let fee = rpc
        .call(
            "getFeeForMessage",
            json!([STANDARD.encode(&message),{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if fee["context"]["slot"].as_u64().is_none_or(|v| v < slot) {
        return Err(bad());
    }
    let cost = fee["value"]
        .as_u64()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    if !funding.finish(s, &request, cost)? {
        return Ok(funding.result);
    }
    let mut result = funding.result;
    let wire = unsigned_packet(&message)?;
    result["ready"] = true.into();
    result["recentBlockhash"] = blockhash.into();
    result["lastValidBlockHeight"] = last.into();
    result["computeUnitLimit"] = COMPUTE_LIMIT.into();
    result["instructions"] = json!(instructions);
    if request.simulate {
        let response=rpc.call("simulateTransaction",json!([STANDARD.encode(&wire),{"encoding":"base64","commitment":"finalized","minContextSlot":slot,"sigVerify":false,"replaceRecentBlockhash":false,"accounts":{"encoding":"base64","addresses":[addresses.source,addresses.destination,s.context.caller]}}])).await?;
        let observed = response["context"]["slot"]
            .as_u64()
            .filter(|v| *v >= slot)
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
