//! Same-bank preparation for already accrued Whirlpool position rewards.
use super::*;
use xone_domain::{orca as o, spl_token_2022 as token22};

const POSITION_SIZE: usize = 216;
const POSITION_REWARDS: usize = 144;
const REWARD_SLOT_SIZE: usize = 24;
const OBSERVED_TOKEN22_SLOT: u64 = 460497271;
const OBSERVED_TOKEN22_CODE: &str =
    "15a2e615a42e20f31714bbaaee43002ae6f1cda2e8d7118243117ab3c1342e2d";

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
}

fn position_owed(
    pool: &Pool,
    request: &ClaimRequest,
    position: &[u8],
    nft: &Account,
) -> Result<(usize, u64)> {
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
    let offset = POSITION_REWARDS + REWARD_SLOT_SIZE * usize::from(request.reward_index) + 16;
    Ok((offset, m::u64_at(position, offset)))
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
    let (owed_offset, owed) = position_owed(pool, request, position, nft)?;
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
    let mut position_after = position.to_vec();
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
    let instructions = [
        Instruction {
            program_id: orca::COMPUTE_BUDGET.into(),
            accounts: vec![],
            data_base64: STANDARD.encode(budget_data),
        },
        Instruction {
            program_id: o::PROGRAM_ID.into(),
            accounts,
            data_base64: STANDARD.encode(data),
        },
    ];
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
    result["preparationHash"] = hash(&result)?.into();
    Ok(SolanaPreparation {
        result,
        accounts: Some(snapshot),
    })
}
