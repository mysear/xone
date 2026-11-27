//! Same-bank preparation for Whirlpool position reward collection.
use super::*;
use xone_domain::{orca as o, spl_token_2022 as token22};
use xone_protocol_math::orca_adaptive_swap::accrue_reward_growth;

const POSITION_SIZE: usize = 216;
const POSITION_REWARDS: usize = 144;
const REWARD_SLOT_SIZE: usize = 24;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimRequest {
    pub authority: String,
    pub position: String,
    pub position_token_account: String,
    pub recipient: String,
    pub reward_account: String,
    pub reward_index: u8,
    pub reward_token_program: String,
    #[serde(default)]
    pub update_before_collect: bool,
}

fn validate_position(
    pool: &Pool,
    request: &ClaimRequest,
    position: &[u8],
    nft: &Account,
) -> Result<()> {
    let invalid = || err("ORCA_REWARD_POSITION_INVALID");
    if position[..8] != m::discriminator("Position") || m::key(position, 8) != pool.pool_id {
        return Err(invalid());
    }
    let position_mint = &position[40..72];
    if position_mint == [0; 32]
        || m::pda(&[b"position", position_mint], o::PROGRAM_ID)? != request.position
        || nft.executable
        || !match nft.owner.as_str() {
            TOKEN => nft.data.len() == 165,
            token22::PROGRAM => {
                nft.data.len() == 165
                    || (nft.data.len() == 170 && nft.data[165..] == [2, 7, 0, 0, 0])
            }
            _ => false,
        }
        || &nft.data[..32] != position_mint
        || m::u64_at(&nft.data, 64) != 1
        || !matches!(nft.data[108], 1 | 2)
        || m::u32_at(&nft.data, 109) != 0
    {
        return Err(invalid());
    }
    let delegated = m::u32_at(&nft.data, 72);
    if delegated > 1 {
        return Err(invalid());
    }
    if delegated == 1 && m::key(&nft.data, 76) == request.authority {
        if m::u64_at(&nft.data, 121) != 1 {
            return Err(err("ORCA_REWARD_AUTHORITY_INVALID"));
        }
    } else if m::key(&nft.data, 32) != request.authority {
        return Err(err("ORCA_REWARD_AUTHORITY_INVALID"));
    }
    Ok(())
}

fn inside_growth(
    global: u128,
    lower: Option<u128>,
    upper: Option<u128>,
    current: i32,
    range: [i32; 2],
) -> u128 {
    let below = lower.map_or(global, |growth| {
        if current < range[0] {
            global.wrapping_sub(growth)
        } else {
            growth
        }
    });
    let above = upper.map_or(0, |growth| {
        if current < range[1] {
            growth
        } else {
            global.wrapping_sub(growth)
        }
    });
    global.wrapping_sub(below).wrapping_sub(above)
}

fn update_checkpoint(position: &mut [u8], offset: usize, growth: u128, liquidity: u128) -> u64 {
    let checkpoint = m::u128_at(position, offset);
    let delta = growth.wrapping_sub(checkpoint);
    let (owed, checkpoint) = match liquidity.checked_mul(delta) {
        Some(product) => {
            let owed = (product >> 64) as u64;
            let converted = (u128::from(owed) << 64).div_ceil(liquidity);
            (owed, checkpoint.wrapping_add(converted))
        }
        None => (0, growth),
    };
    let total = m::u64_at(position, offset + 16).wrapping_add(owed);
    position[offset..offset + 16].copy_from_slice(&checkpoint.to_le_bytes());
    position[offset + 16..offset + 24].copy_from_slice(&total.to_le_bytes());
    owed
}

fn project_update(
    pool: &Pool,
    bank: &BTreeMap<String, Option<Account>>,
    bank_pool: &[u8],
    position: &mut [u8],
    reward_index: usize,
) -> Result<([String; 2], Vec<u8>, u64)> {
    let state = pool.orca_whirlpool.as_ref().ok_or_else(bad)?;
    let current = state.tick_current_index;
    let spacing = state.tick_spacing as i32;
    let range = [m::i32_at(position, 88), m::i32_at(position, 92)];
    let liquidity = m::u128_at(position, 72);
    if liquidity == 0
        || range[0] >= range[1]
        || range
            .iter()
            .any(|tick| !(o::MIN_TICK..=o::MAX_TICK).contains(tick) || tick % spacing != 0)
    {
        return Err(err("ORCA_REWARD_POSITION_RANGE"));
    }
    let bounds = range.map(|tick| {
        let start = tick.div_euclid(88 * spacing) * (88 * spacing);
        let array = state
            .tick_arrays
            .iter()
            .find(|a| a.start_tick_index == start && a.layout != "sparse")
            .ok_or_else(|| err("ORCA_REWARD_TICK_ARRAY_MISSING"))?;
        let address = m::pda(
            &[
                b"tick_array",
                &m::pubkey(&pool.pool_id)?,
                start.to_string().as_bytes(),
            ],
            o::PROGRAM_ID,
        )?;
        if address != array.address {
            return Err(err("ORCA_REWARD_TICK_ARRAY_MISMATCH"));
        }
        raw(
            bank,
            &address,
            o::PROGRAM_ID,
            &orca::array_bytes(pool, array)?,
        )
        .map_err(|_| err("ORCA_REWARD_TICK_ARRAY_MISMATCH"))?;
        Ok((address, array.ticks.iter().find(|entry| entry.tick == tick)))
    });
    let [lower, upper] = bounds;
    let (lower_address, lower) = lower?;
    let (upper_address, upper) = upper?;
    let as_u128 = |value: &Amount| u128::try_from(&value.0).map_err(|_| bad());
    for (side, global) in [&state.fee_growth_global0, &state.fee_growth_global1]
        .into_iter()
        .enumerate()
    {
        let outside = |tick: Option<&o::OrcaTick>| {
            tick.map(|tick| as_u128([&tick.fee_growth_outside0, &tick.fee_growth_outside1][side]))
                .transpose()
        };
        let growth = inside_growth(
            as_u128(global)?,
            outside(lower)?,
            outside(upper)?,
            current,
            range,
        );
        update_checkpoint(position, 96 + side * 24, growth, liquidity);
    }
    let mut pool_after = bank_pool.to_vec();
    let elapsed = state.bank_unix_timestamp - state.reward_last_updated_timestamp;
    let pool_liquidity = as_u128(&state.liquidity)?;
    pool_after[261..269].copy_from_slice(&state.bank_unix_timestamp.to_le_bytes());
    let mut accrued_reward = 0;
    for (index, reward) in state.reward_infos.iter().enumerate() {
        let growth = if reward.initialized() {
            let global = accrue_reward_growth(
                as_u128(&reward.growth_global_x64)?,
                as_u128(&reward.emissions_per_second_x64)?,
                elapsed,
                pool_liquidity,
            );
            let offset = 381 + index * 128;
            pool_after[offset..offset + 16].copy_from_slice(&global.to_le_bytes());
            let outside = |tick: Option<&o::OrcaTick>| {
                tick.map(|tick| as_u128(&tick.reward_growths_outside[index]))
                    .transpose()
            };
            inside_growth(global, outside(lower)?, outside(upper)?, current, range)
        } else {
            0
        };
        let accrued = update_checkpoint(
            position,
            POSITION_REWARDS + index * REWARD_SLOT_SIZE,
            growth,
            liquidity,
        );
        if index == reward_index {
            accrued_reward = accrued;
        }
    }
    Ok(([lower_address, upper_address], pool_after, accrued_reward))
}

pub async fn prepare(
    config: &SolanaRpcConfig,
    pool: &Pool,
    request: &ClaimRequest,
) -> Result<SolanaPreparation> {
    pool.validate()?;
    let state = pool.orca_whirlpool.as_ref().ok_or_else(bad)?;
    let reward = state
        .reward_infos
        .get(usize::from(request.reward_index))
        .filter(|reward| reward.initialized())
        .ok_or_else(|| err("ORCA_REWARD_NOT_INITIALIZED"))?;
    let v2 = request.reward_token_program == token22::PROGRAM;
    if !v2 && request.reward_token_program != TOKEN {
        return Err(err("ORCA_REWARD_TOKEN_PROGRAM_UNSUPPORTED"));
    }
    for key in [
        &request.authority,
        &request.position,
        &request.position_token_account,
        &request.recipient,
        &request.reward_account,
    ] {
        m::pubkey(key)?;
    }
    if [
        reward.vault.as_str(),
        request.position_token_account.as_str(),
    ]
    .contains(&request.reward_account.as_str())
    {
        return Err(bad());
    }

    let mut ids = vec![
        pool.pool_id.clone(),
        state.program_id.clone(),
        state.programdata.clone(),
        request.authority.clone(),
        request.position.clone(),
        request.position_token_account.clone(),
        request.reward_account.clone(),
        reward.mint.clone(),
        reward.vault.clone(),
        request.reward_token_program.clone(),
        m::pda(&[&m::pubkey(&request.reward_token_program)?], LOADER)?,
        xone_domain::raydium::CLOCK.into(),
        orca::COMPUTE_BUDGET.into(),
        RENT.into(),
    ];
    if v2 {
        ids.push(MEMO.into());
    }
    if request.update_before_collect {
        ids.extend(
            state
                .tick_arrays
                .iter()
                .filter(|a| a.layout != "sparse")
                .map(|a| a.address.clone()),
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    ids.retain(|key| seen.insert(key.clone()));
    let rpc = Rpc {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| bad())?,
        url: config.url.clone(),
        capture: Capture::default(),
        chain_key: pool.chain_key.clone(),
        genesis_hash: config.genesis_hash.clone(),
    };
    if rpc.call("getGenesisHash", json!([])).await?.as_str() != Some(config.genesis_hash.as_str()) {
        return Err(err("PREPARATION_WRONG_CHAIN"));
    }
    let observed = rpc.call("getMultipleAccounts",json!([ids,{"encoding":"base64","commitment":"finalized","minContextSlot":state.bank_slot}])).await?;
    if observed["context"]["slot"].as_u64() != Some(state.bank_slot) {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    let values = observed["value"]
        .as_array()
        .filter(|v| v.len() == ids.len())
        .ok_or_else(bad)?;
    let bank = ids
        .iter()
        .zip(values)
        .map(|(key, value)| {
            Ok((
                key.clone(),
                account(value).map_err(|_| err("ORCA_REWARD_CAPTURE_INVALID"))?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let slot = state.bank_slot;
    if get(&bank, o::PROGRAM_ID)?.owner != LOADER
        || get(&bank, &request.reward_token_program)?.owner != LOADER
    {
        return Err(err("PREPARATION_PROGRAM_CHANGED"));
    }
    code(
        &bank,
        o::PROGRAM_ID,
        &state.program_code_sha256,
        slot,
        Some(state.deployed_slot),
    )?;
    let (token_digest, token_deployment) = if v2 {
        let (digest, deployment) = state
            .token_2022
            .as_ref()
            .map_or((OBSERVED_TOKEN22_CODE, OBSERVED_TOKEN22_SLOT), |proof| {
                (proof.program_code_sha256.as_str(), proof.deployed_slot)
            });
        if get(&bank, MEMO)?.owner != IMMUTABLE {
            return Err(err("PREPARATION_PROGRAM_CHANGED"));
        }
        code(&bank, token22::PROGRAM, digest, slot, Some(deployment))?;
        code(&bank, MEMO, MEMO_CODE, slot, None)?;
        (digest, Some(deployment))
    } else {
        code(&bank, TOKEN, &state.token_program_code_sha256, slot, None)?;
        (state.token_program_code_sha256.as_str(), None)
    };
    let budget = get(&bank, orca::COMPUTE_BUDGET)?;
    if !budget.executable
        || budget.owner != "NativeLoader1111111111111111111111111111111"
        || budget.data != b"compute_budget_program"
    {
        return Err(err("PREPARATION_PROGRAM_CHANGED"));
    }
    let clock = orca::owned(&bank, xone_domain::raydium::CLOCK, SYSVAR, 40)?;
    if m::u64_at(clock, 0) != slot
        || m::u64_at(clock, 16) != state.bank_epoch
        || m::u64_at(clock, 32) != state.bank_unix_timestamp
    {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    let observed_pool = orca::owned(&bank, &pool.pool_id, o::PROGRAM_ID, 653)?;
    raw(
        &bank,
        &pool.pool_id,
        o::PROGRAM_ID,
        &orca::pool_bytes(pool, observed_pool)?,
    )?;
    let expected_pool = m::pda(
        &[
            b"whirlpool",
            &m::pubkey(&state.whirlpools_config)?,
            &m::pubkey(&pool.token0)?,
            &m::pubkey(&pool.token1)?,
            &(state.fee_tier_index as u16).to_le_bytes(),
        ],
        o::PROGRAM_ID,
    )?;
    if pool.pool_id != expected_pool {
        return Err(err("ORCA_REWARD_POOL_IDENTITY"));
    }
    let payer = get(&bank, &request.authority)?;
    if !empty_system(payer) {
        return Err(err("ORCA_REWARD_PAYER_INVALID"));
    }
    let position = orca::owned(&bank, &request.position, o::PROGRAM_ID, POSITION_SIZE)
        .map_err(|_| err("ORCA_REWARD_POSITION_INVALID"))?;
    let nft = get(&bank, &request.position_token_account)?;
    validate_position(pool, request, position, nft)?;
    let reward_index = usize::from(request.reward_index);
    let owed_offset = POSITION_REWARDS + REWARD_SLOT_SIZE * reward_index + 16;
    let mut position_after = position.to_vec();
    let update = if request.update_before_collect {
        Some(project_update(
            pool,
            &bank,
            observed_pool,
            &mut position_after,
            reward_index,
        )?)
    } else {
        None
    };
    let owed = m::u64_at(&position_after, owed_offset);
    let mint = orca::owned(
        &bank,
        &reward.mint,
        &request.reward_token_program,
        if v2 { 278 } else { 82 },
    )
    .map_err(|_| err("ORCA_REWARD_MINT_INVALID"))?;
    if mint[45] != 1 || m::u32_at(mint, 0) > 1 || m::u32_at(mint, 46) > 1 {
        return Err(err("ORCA_REWARD_MINT_INVALID"));
    }
    let vault = get(&bank, &reward.vault)?;
    let destination = get(&bank, &request.reward_account)?;
    let vault_balance = token(
        vault,
        &reward.mint,
        &pool.pool_id,
        &request.reward_token_program,
    )
    .map_err(|_| err("ORCA_REWARD_VAULT_INVALID"))?;
    let before_balance = token(
        destination,
        &reward.mint,
        &request.recipient,
        &request.reward_token_program,
    )
    .map_err(|_| err("ORCA_REWARD_DESTINATION_INVALID"))?;
    let gross = owed.min(vault_balance);
    let fee = if v2 {
        token22::TransferFee::from_mint(mint, state.bank_epoch)?.fee(gross)
    } else {
        0
    };
    let net = gross - fee;
    let native = reward.mint == xone_domain::pump_amm::WSOL;
    let vault_lamports = vault
        .lamports
        .checked_sub(if native { gross } else { 0 })
        .ok_or_else(bad)?;
    let destination_lamports = destination
        .lamports
        .checked_add(if native { net } else { 0 })
        .ok_or_else(bad)?;
    position_after[owed_offset..owed_offset + 8].copy_from_slice(&(owed - gross).to_le_bytes());
    let mut vault_after = vault.data.clone();
    vault_after[64..72].copy_from_slice(&(vault_balance - gross).to_le_bytes());
    let mut destination_after = destination.data.clone();
    destination_after[64..72].copy_from_slice(
        &before_balance
            .checked_add(net)
            .ok_or_else(bad)?
            .to_le_bytes(),
    );
    if v2 {
        token22::TransferFeeAccount::increment_withheld(&mut destination_after, fee)?;
    }
    let mut accounts = vec![
        meta(&pool.pool_id, false, false),
        meta(&request.authority, true, false),
        meta(&request.position, false, true),
        meta(&request.position_token_account, false, false),
        meta(&request.reward_account, false, true),
    ];
    let mut data = Sha256::digest(if v2 {
        &b"global:collect_reward_v2"[..]
    } else {
        &b"global:collect_reward"[..]
    })[..8]
        .to_vec();
    data.push(request.reward_index);
    if v2 {
        accounts.push(meta(&reward.mint, false, false));
        data.push(0);
    }
    accounts.push(meta(&reward.vault, false, true));
    accounts.push(meta(&request.reward_token_program, false, false));
    if v2 {
        accounts.push(meta(MEMO, false, false));
    }
    let mut budget_data = vec![2];
    budget_data.extend(orca::COMPUTE_LIMIT.to_le_bytes());
    let mut instructions = vec![Instruction {
        program_id: orca::COMPUTE_BUDGET.into(),
        accounts: vec![],
        data_base64: STANDARD.encode(budget_data),
    }];
    if let Some(([lower, upper], _, _)) = &update {
        instructions.push(Instruction {
            program_id: o::PROGRAM_ID.into(),
            accounts: vec![
                meta(&pool.pool_id, false, true),
                meta(&request.position, false, true),
                meta(lower, false, false),
                meta(upper, false, false),
            ],
            data_base64: STANDARD.encode(&Sha256::digest(b"global:update_fees_and_rewards")[..8]),
        });
    }
    instructions.push(Instruction {
        program_id: o::PROGRAM_ID.into(),
        accounts,
        data_base64: STANDARD.encode(data),
    });
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
    let expiry = latest["value"]["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(bad)?;
    let message = compile_message_many(&request.authority, blockhash, &instructions)?;
    let fee_reply = rpc
        .call(
            "getFeeForMessage",
            json!([STANDARD.encode(&message),{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if fee_reply["context"]["slot"]
        .as_u64()
        .is_none_or(|v| v < slot)
    {
        return Err(bad());
    }
    let fee_lamports = fee_reply["value"]
        .as_u64()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    if payer.lamports < fee_lamports {
        return Err(err("INSUFFICIENT_FEE_BALANCE"));
    }
    let packet = unsigned_packet(&message)?;
    let snapshot = rpc
        .capture
        .0
        .lock()
        .map_err(|_| err("PREPARATION_CAPTURE_LOCK"))?
        .take()
        .ok_or_else(bad)?;
    let mut result = json!({
        "operation":"ORCA_COLLECT_REWARD",
        "ready":true,"signed":false,"submitted":false,"observedSlot":slot,
        "poolId":pool.pool_id,"position":request.position,"authority":request.authority,"recipient":request.recipient,
        "positionTokenAccount":request.position_token_account,"rewardIndex":request.reward_index,"rewardMint":reward.mint,
        "rewardVault":reward.vault,"rewardAccount":request.reward_account,"rewardTokenProgram":request.reward_token_program,
        "grossReward":gross.to_string(),"netReward":net.to_string(),"transferFee":fee.to_string(),
        "remainingRewardOwed":(owed-gross).to_string(),"feeLamports":fee_lamports.to_string(),
        "computeUnitLimit":orca::COMPUTE_LIMIT,
        "rewardTokenCodeSha256":token_digest,"rewardTokenDeployedSlot":token_deployment,
        "expectedPostAccounts":[
            {"address":request.position,"owner":o::PROGRAM_ID,"dataSha256":format!("{:x}",Sha256::digest(position_after)),"lamports":get(&bank,&request.position)?.lamports.to_string()},
            {"address":reward.vault,"owner":request.reward_token_program,"dataSha256":format!("{:x}",Sha256::digest(vault_after)),"lamports":vault_lamports.to_string()},
            {"address":request.reward_account,"owner":request.reward_token_program,"dataSha256":format!("{:x}",Sha256::digest(destination_after)),"lamports":destination_lamports.to_string()}
        ],
        "recentBlockhash":blockhash,"lastValidBlockHeight":expiry,"requiredSigners":[request.authority],
        "instructions":instructions,"messageBase64":STANDARD.encode(message),"unsignedTransaction":STANDARD.encode(packet),
        "accountSnapshot":snapshot.summary()?,
    });
    if let Some(([lower, upper], pool_after, accrued)) = update {
        result["rewardUpdate"] = json!({"lowerTickArray":lower,"upperTickArray":upper,"accruedReward":accrued.to_string()});
        result["expectedPostAccounts"]
            .as_array_mut()
            .ok_or_else(bad)?
            .push(json!({
                "address":pool.pool_id,"owner":o::PROGRAM_ID,
                "dataSha256":format!("{:x}",Sha256::digest(pool_after)),
                "lamports":get(&bank, &pool.pool_id)?.lamports.to_string()
            }));
    }
    result["preparationHash"] = hash(&result)?.into();
    Ok(SolanaPreparation {
        result,
        accounts: Some(snapshot),
    })
}
