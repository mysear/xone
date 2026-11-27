//! Same-bank preparation for Raydium CLMM position fees and rewards.
use super::*;
use xone_domain::{raydium_clmm as r, spl_token_2022 as token22};
use xone_protocol_math::raydium_clmm::position::{self, Position, PositionAccrual};

const POSITION_LEN: usize = 281;
const FEE_GROWTH_OFFSET: usize = 97;
const FEE_OWED_OFFSET: usize = 129;
const REWARD_OFFSET: usize = 145;
const EPOCH_OFFSET: usize = 217;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaimRequest {
    pub authority: String,
    pub position: String,
    pub position_token_account: String,
    pub recipient: String,
    pub fee_accounts: [String; 2],
    pub reward_accounts: Vec<String>,
}

fn position_state(
    pool: &Pool,
    request: &ClaimRequest,
    position: &[u8],
    nft: &Account,
) -> Result<Position> {
    let invalid = || err("RAYCLMM_POSITION_INVALID");
    let mint = &position[9..41];
    let seeds: &[&[u8]] = &[b"position", mint];
    if position[..8] != m::discriminator("PersonalPositionState")
        || m::key(position, 41) != pool.pool_id
        || mint == [0; 32]
        || m::pda(seeds, r::PROGRAM_ID)? != request.position
        || rayclmm::bump(seeds, r::PROGRAM_ID, &request.position)? != position[8]
        || position[225..].iter().any(|byte| *byte != 0)
        || nft.executable
        || !match nft.owner.as_str() {
            TOKEN => nft.data.len() == 165,
            token22::PROGRAM => {
                nft.data.len() == 165
                    || (nft.data.len() == 170 && nft.data[165..] == [2, 7, 0, 0, 0])
            }
            _ => false,
        }
        || &nft.data[..32] != mint
        || m::key(&nft.data, 32) != request.authority
        || m::u64_at(&nft.data, 64) != 1
        || !matches!(nft.data[108], 1 | 2)
        || nft.data[109..113] != [0; 4]
    {
        return Err(invalid());
    }
    Ok(Position {
        lower_tick: m::i32_at(position, 73),
        upper_tick: m::i32_at(position, 77),
        liquidity: m::u128_at(position, 81),
        fees: std::array::from_fn(|i| PositionAccrual {
            growth_inside_last_x64: m::u128_at(position, FEE_GROWTH_OFFSET + 16 * i),
            owed: m::u64_at(position, FEE_OWED_OFFSET + 8 * i),
        }),
        rewards: std::array::from_fn(|i| PositionAccrual {
            growth_inside_last_x64: m::u128_at(position, REWARD_OFFSET + 24 * i),
            owed: m::u64_at(position, REWARD_OFFSET + 24 * i + 16),
        }),
        recent_epoch: m::u64_at(position, EPOCH_OFFSET),
    })
}

struct Transfer<'a> {
    vault: &'a str,
    destination: &'a str,
    program: &'a str,
    balance: u64,
    fee: Option<token22::TransferFee>,
    native: bool,
}

fn transfer<'a>(
    bank: &'a BTreeMap<String, Option<Account>>,
    p: &Pool,
    request: &ClaimRequest,
    addresses: (&str, &'a str, &'a str),
    epoch: u64,
) -> Result<Transfer<'a>> {
    let (mint, vault, destination) = addresses;
    let mint_account = get(bank, mint)?;
    let program = mint_account.owner.as_str();
    if mint_account.executable
        || mint_account.data.len() < 82
        || mint_account.data[45] != 1
        || m::u32_at(&mint_account.data, 0) > 1
        || m::u32_at(&mint_account.data, 46) > 1
        || [vault, request.position_token_account.as_str()].contains(&destination)
    {
        return Err(err("RAYCLMM_REWARD_ACCOUNT_INVALID"));
    }
    let fee = match program {
        TOKEN if mint_account.data.len() == 82 => None,
        token22::PROGRAM => Some(token22::TransferFee::from_mint(&mint_account.data, epoch)?),
        _ => return Err(err("RAYCLMM_REWARD_MINT_UNSUPPORTED")),
    };
    let balance = token(get(bank, vault)?, mint, &p.pool_id, program)?;
    token(get(bank, destination)?, mint, &request.recipient, program)?;
    Ok(Transfer {
        vault,
        destination,
        program,
        balance,
        fee,
        native: mint == xone_domain::pump_amm::WSOL,
    })
}

fn add_post(
    post: &mut BTreeMap<String, (Vec<u8>, u64)>,
    bank: &BTreeMap<String, Option<Account>>,
    transfer: &Transfer<'_>,
    gross: u64,
) -> Result<Value> {
    let fee = transfer.fee.map_or(0, |schedule| schedule.fee(gross));
    let net = gross - fee;
    for (address, amount, debit) in [
        (transfer.vault, gross, true),
        (transfer.destination, net, false),
    ] {
        let account = get(bank, address)?;
        let (data, lamports) = post
            .entry(address.into())
            .or_insert_with(|| (account.data.clone(), account.lamports));
        let change = |value: u64| {
            (if debit {
                value.checked_sub(amount)
            } else {
                value.checked_add(amount)
            })
            .ok_or_else(|| err("RAYCLMM_POSITION_SETTLEMENT_BALANCE"))
        };
        let balance = change(m::u64_at(data, 64))?;
        data[64..72].copy_from_slice(&balance.to_le_bytes());
        if transfer.native {
            *lamports = change(*lamports)?;
        } else if !debit && transfer.fee.is_some() {
            token22::TransferFeeAccount::increment_withheld(data, fee)?;
        }
    }
    Ok(
        json!({"vault":transfer.vault,"account":transfer.destination,"program":transfer.program,
        "gross":gross.to_string(),"net":net.to_string(),"transferFee":fee.to_string()}),
    )
}

pub async fn prepare(
    config: &SolanaRpcConfig,
    pool: &Pool,
    request: &ClaimRequest,
) -> Result<SolanaPreparation> {
    pool.validate()?;
    if pool.protocol != "raydium-clmm-rewards" {
        return Err(err("EXECUTION_ADAPTER_UNSUPPORTED"));
    }
    let state = pool.raydium_clmm.as_ref().ok_or_else(bad)?;
    let rewards: Vec<_> = state
        .reward_infos
        .iter()
        .filter(|reward| reward.initialized())
        .collect();
    if rewards.len() != request.reward_accounts.len()
        || rewards.is_empty()
        || state.reward_infos[..rewards.len()]
            .iter()
            .any(|reward| !reward.initialized())
    {
        return Err(err("RAYCLMM_REWARD_ACCOUNT_COUNT"));
    }
    let slot = state.bank_slot;
    let tokendata = m::pda(&[&m::pubkey(TOKEN)?], LOADER)?;
    let token22data = m::pda(&[&m::pubkey(rayclmm::TOKEN22)?], LOADER)?;
    let mut ids = vec![
        pool.pool_id.clone(),
        r::PROGRAM_ID.into(),
        state.programdata.clone(),
        state.amm_config.clone(),
        state.observation.clone(),
        TOKEN.into(),
        tokendata,
        rayclmm::TOKEN22.into(),
        token22data.clone(),
        MEMO.into(),
        rayclmm::BUDGET.into(),
        RENT.into(),
        xone_domain::raydium::CLOCK.into(),
        pool.token0.clone(),
        pool.token1.clone(),
        state.vault0.clone(),
        state.vault1.clone(),
        request.authority.clone(),
        request.position.clone(),
        request.position_token_account.clone(),
    ];
    ids.extend(state.tick_arrays.iter().map(|a| a.address.clone()));
    ids.extend(
        request
            .fee_accounts
            .iter()
            .chain(&request.reward_accounts)
            .cloned(),
    );
    for reward in &rewards {
        ids.push(reward.token_mint.clone());
        ids.push(reward.token_vault.clone());
    }
    let mut seen = std::collections::BTreeSet::new();
    ids.retain(|key| seen.insert(key.clone()));
    for id in &ids {
        m::pubkey(id)?;
    }
    m::pubkey(&request.recipient)?;
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
    let response = rpc
        .call(
            "getMultipleAccounts",
            json!([ids,{"encoding":"base64","commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if response["context"]["slot"].as_u64() != Some(slot) {
        return Err(err("QUOTE_CLOCK_CHANGED"));
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
    rayclmm::validate_programs(&bank, state, &token22data)?;
    let clock = rayclmm::owned(&bank, xone_domain::raydium::CLOCK, SYSVAR, 40)?;
    if [
        m::u64_at(clock, 0),
        m::u64_at(clock, 16),
        m::u64_at(clock, 32),
    ] != [slot, state.bank_epoch, state.bank_unix_timestamp]
    {
        return Err(err("QUOTE_CLOCK_CHANGED"));
    }
    let payer = get(&bank, &request.authority)?;
    if !empty_system(payer) {
        return Err(err("RAYCLMM_POSITION_PAYER_INVALID"));
    }
    let pool_data = rayclmm::owned(&bank, &pool.pool_id, r::PROGRAM_ID, 1544)?;
    raw(
        &bank,
        &pool.pool_id,
        r::PROGRAM_ID,
        &rayclmm::pool_bytes(pool, pool_data)?,
    )?;
    let pool_key = m::pubkey(&pool.pool_id)?;
    let seeds: &[&[u8]] = &[
        b"pool",
        &m::pubkey(&state.amm_config)?,
        &m::pubkey(&pool.token0)?,
        &m::pubkey(&pool.token1)?,
    ];
    if m::pda(seeds, r::PROGRAM_ID)? != pool.pool_id
        || u32::from(rayclmm::bump(seeds, r::PROGRAM_ID, &pool.pool_id)?) != state.pool_bump
    {
        return Err(err("RAYCLMM_POSITION_POOL_IDENTITY"));
    }
    let position_data = rayclmm::owned(&bank, &request.position, r::PROGRAM_ID, POSITION_LEN)?;
    let position = position_state(
        pool,
        request,
        position_data,
        get(&bank, &request.position_token_account)?,
    )?;
    let span = i32::try_from(state.tick_spacing).map_err(|_| bad())? * 60;
    let bound = |index: i32| -> Result<_> {
        if !(r::MIN_TICK..=r::MAX_TICK).contains(&index) {
            return Err(err("RAYCLMM_POSITION_TICKS"));
        }
        let start = index.div_euclid(span) * span;
        let array = state
            .tick_arrays
            .iter()
            .find(|array| array.start_tick_index == start)
            .ok_or_else(|| err("RAYCLMM_POSITION_TICKS"))?;
        let tick = array
            .ticks
            .iter()
            .find(|t| t.tick == index)
            .ok_or_else(|| err("RAYCLMM_POSITION_TICKS"))?;
        Ok((array, tick))
    };
    let (lower_array, lower) = bound(position.lower_tick)?;
    let (upper_array, upper) = bound(position.upper_tick)?;
    for array in &state.tick_arrays {
        if m::pda(
            &[
                b"tick_array",
                &pool_key,
                &array.start_tick_index.to_be_bytes(),
            ],
            r::PROGRAM_ID,
        )? != array.address
        {
            return Err(err("RAYCLMM_POSITION_TICKS"));
        }
        let observed = rayclmm::owned(&bank, &array.address, r::PROGRAM_ID, 10240)?;
        raw(
            &bank,
            &array.address,
            r::PROGRAM_ID,
            &rayclmm::array_bytes(pool, array, observed)?,
        )?;
    }

    let mut metas = vec![
        meta(&request.authority, true, false),
        meta(&request.position_token_account, false, false),
        meta(&request.position, false, true),
        meta(&pool.pool_id, false, true),
        meta(&pool.pool_id, false, false), // Deprecated protocol position is unchecked.
        meta(&state.vault0, false, true),
        meta(&state.vault1, false, true),
        meta(&lower_array.address, false, true),
        meta(&upper_array.address, false, true),
        meta(&request.fee_accounts[0], false, true),
        meta(&request.fee_accounts[1], false, true),
        meta(TOKEN, false, false),
        meta(rayclmm::TOKEN22, false, false),
        meta(MEMO, false, false),
        meta(&pool.token0, false, false),
        meta(&pool.token1, false, false),
    ];
    let fee_transfers = [
        transfer(
            &bank,
            pool,
            request,
            (&pool.token0, &state.vault0, &request.fee_accounts[0]),
            state.bank_epoch,
        )?,
        transfer(
            &bank,
            pool,
            request,
            (&pool.token1, &state.vault1, &request.fee_accounts[1]),
            state.bank_epoch,
        )?,
    ];
    if fee_transfers
        .each_ref()
        .map(|value| (value.program, value.balance))
        != [(TOKEN, state.raw_vault0), (TOKEN, state.raw_vault1)]
    {
        return Err(err("QUOTE_STATE_CHANGED"));
    }
    let mut reward_transfers = Vec::with_capacity(rewards.len());
    let mut balances = [0; 3];
    for (index, (reward, destination)) in rewards
        .into_iter()
        .zip(&request.reward_accounts)
        .enumerate()
    {
        let seed = m::pubkey(&reward.token_mint)?;
        if m::pda(&[b"pool_reward_vault", &pool_key, &seed], r::PROGRAM_ID)? != reward.token_vault {
            return Err(err("RAYCLMM_REWARD_VAULT_INVALID"));
        }
        let transfer = transfer(
            &bank,
            pool,
            request,
            (&reward.token_mint, &reward.token_vault, destination),
            state.bank_epoch,
        )?;
        balances[index] = transfer.balance;
        reward_transfers.push(transfer);
        metas.extend([
            meta(&reward.token_vault, false, true),
            meta(destination, false, true),
            meta(&reward.token_mint, false, false),
        ]);
    }
    let settlement = position::settle(state, &position, lower, upper, pool_data[389], balances)?;
    let updated = &settlement.next_position;
    let mut position_after = position_data.to_vec();
    for (i, fee) in updated.fees.iter().enumerate() {
        position_after[FEE_GROWTH_OFFSET + 16 * i..FEE_GROWTH_OFFSET + 16 * (i + 1)]
            .copy_from_slice(&fee.growth_inside_last_x64.to_le_bytes());
        position_after[FEE_OWED_OFFSET + 8 * i..FEE_OWED_OFFSET + 8 * (i + 1)]
            .copy_from_slice(&fee.owed.to_le_bytes());
    }
    for (i, reward) in updated.rewards.iter().enumerate() {
        let start = REWARD_OFFSET + 24 * i;
        position_after[start..start + 16]
            .copy_from_slice(&reward.growth_inside_last_x64.to_le_bytes());
        position_after[start + 16..start + 24].copy_from_slice(&reward.owed.to_le_bytes());
    }
    position_after[EPOCH_OFFSET..EPOCH_OFFSET + 8]
        .copy_from_slice(&updated.recent_epoch.to_le_bytes());
    let mut next_pool = pool.clone();
    next_pool.raydium_clmm = Some(settlement.next_pool);
    let mut post = BTreeMap::from([
        (
            request.position.clone(),
            (position_after, get(&bank, &request.position)?.lamports),
        ),
        (
            pool.pool_id.clone(),
            (
                rayclmm::pool_bytes(&next_pool, pool_data)?,
                get(&bank, &pool.pool_id)?.lamports,
            ),
        ),
    ]);
    let fees = fee_transfers
        .iter()
        .zip(settlement.fees)
        .map(|(transfer, gross)| add_post(&mut post, &bank, transfer, gross))
        .collect::<Result<Vec<_>>>()?;
    let rewards = reward_transfers
        .iter()
        .zip(settlement.rewards)
        .map(|(transfer, gross)| add_post(&mut post, &bank, transfer, gross))
        .collect::<Result<Vec<_>>>()?;
    let expected = post
        .iter()
        .map(|(address, (bytes, lamports))| {
            Ok(json!({"address":address,"owner":get(&bank,address)?.owner,
            "dataSha256":format!("{:x}", Sha256::digest(bytes)),"lamports":lamports.to_string()}))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut compute = vec![2];
    compute.extend(rayclmm::COMPUTE_LIMIT.to_le_bytes());
    let mut data = Sha256::digest(b"global:decrease_liquidity_v2")[..8].to_vec();
    data.extend([0; 32]);
    let instructions = [
        Instruction {
            program_id: rayclmm::BUDGET.into(),
            accounts: vec![],
            data_base64: STANDARD.encode(compute),
        },
        Instruction {
            program_id: r::PROGRAM_ID.into(),
            accounts: metas,
            data_base64: STANDARD.encode(data),
        },
    ];
    let latest = rpc
        .call(
            "getLatestBlockhash",
            json!([{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if latest["context"]["slot"]
        .as_u64()
        .is_none_or(|observed| observed < slot)
    {
        return Err(bad());
    }
    let blockhash = latest["value"]["blockhash"].as_str().ok_or_else(bad)?;
    let expiry = latest["value"]["lastValidBlockHeight"]
        .as_u64()
        .ok_or_else(bad)?;
    let message = compile_message_many(&request.authority, blockhash, &instructions)?;
    let fee = rpc
        .call(
            "getFeeForMessage",
            json!([STANDARD.encode(&message),{"commitment":"finalized","minContextSlot":slot}]),
        )
        .await?;
    if fee["context"]["slot"]
        .as_u64()
        .is_none_or(|observed| observed < slot)
    {
        return Err(bad());
    }
    let fee = fee["value"]
        .as_u64()
        .ok_or_else(|| err("PREPARATION_BLOCKHASH_EXPIRED"))?;
    if payer.lamports < fee {
        return Err(err("INSUFFICIENT_FEE_BALANCE"));
    }
    let snapshot = rpc
        .capture
        .0
        .lock()
        .map_err(|_| err("PREPARATION_CAPTURE_LOCK"))?
        .take()
        .ok_or_else(bad)?;
    let mut result = json!({
        "operation":"RAYCLMM_COLLECT_POSITION","ready":true,"signed":false,"submitted":false,
        "observedSlot":slot,"poolId":pool.pool_id,"position":request.position,"authority":request.authority,
        "recipient":request.recipient,"positionTokenAccount":request.position_token_account,
        "fees":fees,"rewards":rewards,"expectedPostAccounts":expected,
        "lowerTickArray":lower_array.address,"upperTickArray":upper_array.address,
        "feeLamports":fee.to_string(),"computeUnitLimit":rayclmm::COMPUTE_LIMIT,
        "recentBlockhash":blockhash,"lastValidBlockHeight":expiry,"requiredSigners":[request.authority],
        "instructions":instructions,"messageBase64":STANDARD.encode(&message),
        "unsignedTransaction":STANDARD.encode(unsigned_packet(&message)?),"accountSnapshot":snapshot.summary()?,
    });
    result["preparationHash"] = hash(&result)?.into();
    Ok(SolanaPreparation {
        result,
        accounts: Some(snapshot),
    })
}
