//! Raydium CLMM 6d7a0f82: classic SPL, static/dynamic fees, seeds and bitmap proofs.
//! Layouts and seeds come from the pinned program, including its 168-byte order-aware ticks.
use crate::raydium::{
    check_digest, digest, get, key_at, owned_data, pda, pubkey, tag_at, u64_at, RpcAccount, CLOCK,
};
use crate::rpc::TOKEN_PROGRAM;
use std::collections::{BTreeMap, BTreeSet};
use xone_config::{Chain, PoolConfig};
use xone_domain::raydium_clmm::{
    RaydiumCLMMArray, RaydiumCLMMBitmapExtension, RaydiumCLMMDynamicFee, RaydiumCLMMObservation,
    RaydiumCLMMRewardInfo, RaydiumCLMMState, RaydiumCLMMTick,
};
use xone_domain::{err, Amount, Pool, Result};

pub const PROGRAM_ID: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
pub const PROGRAM_DATA: &str = "HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx";
const UPGRADEABLE_LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const SYSVAR_OWNER: &str = "Sysvar1111111111111111111111111111111111111";
const NATIVE_MINT: &str = "So11111111111111111111111111111111111111112";
const POOL_LEN: usize = 1544;
const CONFIG_LEN: usize = 117;
// Pinned TickArrayBitmapExtension: discriminator, pool_id, then 14 * 8 u64 per side.
const BITMAP_EXTENSION_WORDS: usize = 14 * 8;
const BITMAP_EXTENSION_LEN: usize = 8 + 32 + 2 * BITMAP_EXTENSION_WORDS * 8;
const BITMAP_EXTENSION_SEED: &[u8] = b"pool_tick_array_bitmap_extension";
const TICK_LEN: usize = 168;
const ARRAY_LEN: usize = 8 + 32 + 4 + 60 * TICK_LEN + 1 + 8 + 107;
const OBSERVATION_LEN: usize = 8 + 1 + 8 + 2 + 32 + 100 * 44 + 32;
const MIN_TICK: i32 = -443636;
const MAX_TICK: i32 = 443636;

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(d[o..o + 2].try_into().unwrap())
}
fn i32_at(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn u128_at(d: &[u8], o: usize) -> u128 {
    u128::from_le_bytes(d[o..o + 16].try_into().unwrap())
}
fn amount(d: &[u8], o: usize) -> Result<Amount> {
    Amount::checked(u128_at(d, o).into())
}
fn zero(d: &[u8]) -> bool {
    d.iter().all(|b| *b == 0)
}
fn discriminator(d: &[u8], expected: [u8; 8]) -> Result<()> {
    if d[..8] != expected {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    Ok(())
}

/// Seven business accounts, four bank/code proofs, then three to seven array PDAs.
/// Bitmap, custom-seed and dynamic profiles append the mandatory extension PDA.
/// Fetching uses the configured pool; its authoritative seed is decoded from raw state.
/// Array spacing is read from the pool/config after fetching; starts still have to form
/// a unique, ascending arithmetic sequence before any RPC is made.
pub fn required_accounts(p: &PoolConfig) -> Result<Vec<String>> {
    if !matches!(
        p.protocol.as_str(),
        "raydium-clmm-static"
            | "raydium-clmm-rewards"
            | "raydium-clmm-token-fee"
            | "raydium-clmm-bitmap"
            | "raydium-clmm-custom-seed"
            | "raydium-clmm-dynamic"
    ) || p.program_id != PROGRAM_ID
        || p.accounts.len() != 7
        || p.accounts[0] != p.id
        || p.accounts[4] != p.token0
        || p.accounts[5] != p.token1
        || p.v2.is_some()
        || p.v3.is_some()
        || p.stable.is_some()
        || p.singleton_cl.is_some()
        || p.raydium_cpmm.is_some()
        || p.orca_whirlpool.is_some()
    {
        return Err(err("POOL_ACCOUNT_IDENTITY"));
    }
    let cfg = p
        .raydium_clmm
        .as_ref()
        .ok_or_else(|| err("RAYCLMM_CONFIG_REQUIRED"))?;
    check_digest(&cfg.expected_program_code_sha256)?;
    check_digest(&cfg.expected_token_program_code_sha256)?;
    if !(1..=8192).contains(&cfg.max_steps) || !(3..=7).contains(&cfg.tick_array_starts.len()) {
        return Err(err("RAYCLMM_CONFIG_PARAMETERS"));
    }
    if pubkey(&p.token0)? >= pubkey(&p.token1)? {
        return Err(err("RAYCLMM_MINT_ORDER"));
    }
    let program = pubkey(PROGRAM_ID)?;
    if cfg.expected_programdata != PROGRAM_DATA
        || pda(&[&program], &pubkey(UPGRADEABLE_LOADER)?)?.0 != PROGRAM_DATA
    {
        return Err(err("RAYCLMM_PROGRAMDATA_IDENTITY"));
    }
    let span = i64::from(cfg.tick_array_starts[1]) - i64::from(cfg.tick_array_starts[0]);
    if span <= 0 || span % 60 != 0 || span / 60 > 1000 {
        return Err(err("RAYCLMM_ARRAY_SEQUENCE"));
    }
    let mut ids = p.accounts.clone();
    ids.extend([
        PROGRAM_ID.into(),
        PROGRAM_DATA.into(),
        TOKEN_PROGRAM.into(),
        CLOCK.into(),
    ]);
    let pool = pubkey(&p.id)?;
    let index_range = if matches!(
        p.protocol.as_str(),
        "raydium-clmm-bitmap" | "raydium-clmm-custom-seed" | "raydium-clmm-dynamic"
    ) {
        -7680..=7679
    } else {
        -512..=511
    };
    for (i, start) in cfg.tick_array_starts.iter().enumerate() {
        let start64 = i64::from(*start);
        if start64 % span != 0
            || !index_range.contains(&(start64 / span))
            || start64 > i64::from(MAX_TICK)
            || start64 + span <= i64::from(MIN_TICK)
            || i > 0 && start64 != i64::from(cfg.tick_array_starts[i - 1]) + span
        {
            return Err(err("RAYCLMM_ARRAY_SEQUENCE"));
        }
        ids.push(pda(&[b"tick_array", &pool, &start.to_be_bytes()], &program)?.0);
    }
    if matches!(
        p.protocol.as_str(),
        "raydium-clmm-bitmap" | "raydium-clmm-custom-seed" | "raydium-clmm-dynamic"
    ) {
        ids.push(pda(&[BITMAP_EXTENSION_SEED, &pool], &program)?.0);
    }
    crate::token_program::append_dependency(&mut ids, cfg.token_program_upgrade.as_ref())?;
    let mut seen = BTreeSet::new();
    for id in &ids {
        pubkey(id)?;
        if !seen.insert(id) {
            return Err(err("RAYCLMM_ACCOUNT_ALIAS"));
        }
    }
    Ok(ids)
}

pub fn decode_raydium_clmm(
    ch: &Chain,
    p: &PoolConfig,
    accounts: &BTreeMap<String, RpcAccount>,
    bank_slot: u64,
) -> Result<Pool> {
    if !ch.key.starts_with("solana:") {
        return Err(err("PROTOCOL_CHAIN_FAMILY_MISMATCH"));
    }
    if ch.confirmation_tag != "finalized" {
        return Err(err("RAYCLMM_REQUIRES_FINALIZED"));
    }
    let ids = required_accounts(p)?;
    let identity = p
        .raydium_clmm
        .as_ref()
        .ok_or_else(|| err("RAYCLMM_CONFIG_REQUIRED"))?;
    let program_key = pubkey(PROGRAM_ID)?;
    let program = get(accounts, PROGRAM_ID)?;
    if !program.executable
        || program.owner != UPGRADEABLE_LOADER
        || program.data.len() != 36
        || tag_at(&program.data, 0) != 2
        || key_at(&program.data, 4) != PROGRAM_DATA
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    let pd = get(accounts, PROGRAM_DATA)?;
    if pd.executable
        || pd.owner != UPGRADEABLE_LOADER
        || pd.data.len() <= 45
        || tag_at(&pd.data, 0) != 3
        || pd.data[12] > 1
        || u64_at(&pd.data, 4) != identity.expected_deployed_slot
        || identity.expected_deployed_slot > bank_slot
        || digest(&pd.data[45..]) != identity.expected_program_code_sha256
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    crate::token_program::verify(
        accounts,
        &identity.expected_token_program_code_sha256,
        identity.token_program_upgrade.as_ref(),
        bank_slot,
    )?;
    let clock = owned_data(accounts, CLOCK, SYSVAR_OWNER, 40)?;
    if u64_at(clock, 0) != bank_slot {
        return Err(err("CLOCK_CONTEXT_MISMATCH"));
    }
    let timestamp = i64::from_le_bytes(clock[32..40].try_into().unwrap());
    // Source truncates to u32 for oracle observations. This profile excludes future wraparound.
    let bank_unix_timestamp =
        u32::try_from(timestamp).map_err(|_| err("CLOCK_TIMESTAMP_INVALID"))? as u64;
    let bank_epoch = u64_at(clock, 16);
    let state = owned_data(accounts, &p.id, PROGRAM_ID, POOL_LEN)?;
    let cfg = owned_data(accounts, &p.accounts[3], PROGRAM_ID, CONFIG_LEN)?;
    discriminator(state, [247, 237, 227, 245, 215, 195, 222, 70])?;
    discriminator(cfg, [218, 244, 33, 104, 203, 203, 43, 111])?;
    let seed_index = u16_at(state, 391);
    let dynamic_profile = p.protocol == "raydium-clmm-dynamic";
    if !dynamic_profile && (p.protocol == "raydium-clmm-custom-seed") != (seed_index > 0) {
        return Err(err("RAYCLMM_SEED_PROFILE"));
    }
    // DynamicFeeInfo uses 34 packed bytes, followed by 46 padding bytes.
    // Legacy profiles still require the full 80-byte default and zero reserved tails.
    // Only bytes 391..393 are the source u16LE seed; 393..397 remain padding.
    // Source padding5[325..389] and padding6[1032..1064] are opaque unused
    // arrays; original layout tests round-trip nonzero historical contents.
    if !zero(&state[273..277])
        || state[389] & !63 != 0
        || !zero(&state[393..397])
        || !zero(&state[if dynamic_profile { 1130 } else { 1096 }..])
        || !zero(&cfg[57..61])
        || !zero(&cfg[93..])
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    if (p.protocol == "raydium-clmm-static" && state[390] != 0)
        || (p.protocol == "raydium-clmm-token-fee" && !matches!(state[390], 1 | 2))
        || (matches!(
            p.protocol.as_str(),
            "raydium-clmm-bitmap" | "raydium-clmm-custom-seed" | "raydium-clmm-dynamic"
        ) && state[390] > 2)
    {
        return Err(err("RAYCLMM_FEE_MODE_UNSUPPORTED"));
    }
    if state[389] & 16 != 0 {
        return Err(err("POOL_SWAP_DISABLED"));
    }
    // Actual external_swap and swap_v2 both require_gt!, despite the disabled field comment.
    if bank_unix_timestamp <= u64_at(state, 1080) {
        return Err(err("POOL_NOT_OPEN"));
    }
    if u64_at(state, 1088) > bank_epoch {
        return Err(err("POOL_EPOCH_INVALID"));
    }
    let rewards = p.protocol == "raydium-clmm-rewards";
    let mut reward_infos = Vec::new();
    for reward in state[397..904].chunks_exact(169) {
        // Legacy profiles still allow only RewardInfo::new(authority).
        if !rewards && (!zero(&reward[..121]) || !zero(&reward[153..])) {
            return Err(err("RAYCLMM_REWARD_UNSUPPORTED"));
        }
        if rewards {
            reward_infos.push(RaydiumCLMMRewardInfo {
                reward_state: reward[0].into(),
                open_time: u64_at(reward, 1),
                end_time: u64_at(reward, 9),
                last_update_time: u64_at(reward, 17),
                emissions_per_second_x64: amount(reward, 25)?,
                reward_total_emitted: u64_at(reward, 41),
                reward_claimed: u64_at(reward, 49),
                token_mint: key_at(reward, 57),
                token_vault: key_at(reward, 89),
                authority: key_at(reward, 121),
                reward_growth_global_x64: amount(reward, 153)?,
            });
        }
    }
    let index = u16_at(cfg, 9).to_be_bytes();
    let (config_key, config_bump) = pda(&[b"amm_config", &index], &program_key)?;
    let config_bytes = pubkey(&p.accounts[3])?;
    let mint0_bytes = pubkey(&p.token0)?;
    let mint1_bytes = pubkey(&p.token1)?;
    let seed_bytes = seed_index.to_le_bytes();
    // PoolState::seeds uses an empty slice for legacy zero, never [0, 0].
    let seed: &[u8] = if seed_index == 0 { &[] } else { &seed_bytes };
    let (pool_key, pool_bump) = pda(
        &[b"pool", &config_bytes, &mint0_bytes, &mint1_bytes, seed],
        &program_key,
    )?;
    let pool_bytes = pubkey(&p.id)?;
    if config_key != p.accounts[3]
        || cfg[8] != config_bump
        || pool_key != p.id
        || state[8] != pool_bump
        || key_at(state, 9) != p.accounts[3]
        || key_at(state, 73) != p.token0
        || key_at(state, 105) != p.token1
        || key_at(state, 137) != p.accounts[1]
        || key_at(state, 169) != p.accounts[2]
        || key_at(state, 201) != p.accounts[6]
        || pda(&[b"observation", &pool_bytes], &program_key)?.0 != p.accounts[6]
    {
        return Err(err("POOL_ACCOUNT_IDENTITY"));
    }
    let tick_spacing = u16_at(cfg, 51);
    let trade_fee_rate = tag_at(cfg, 47);
    let protocol_fee_rate = tag_at(cfg, 43);
    let fund_fee_rate = tag_at(cfg, 53);
    if tick_spacing == 0
        || tick_spacing > 1000
        || u16_at(state, 235) != tick_spacing
        || trade_fee_rate >= 1_000_000
        || u64::from(protocol_fee_rate) + u64::from(fund_fee_rate) > 1_000_000
    {
        return Err(err("RAYCLMM_CONFIG_PARAMETERS"));
    }
    let raw = decode_vaults(p, accounts, state, &program_key, &pool_bytes)?;
    let span = 60 * i32::from(tick_spacing);
    let tick_current = i32_at(state, 269);
    if !(MIN_TICK..MAX_TICK).contains(&tick_current) {
        return Err(err("RAYCLMM_PRICE_TICK"));
    }
    let current_start = tick_current.div_euclid(span) * span;
    if !identity.tick_array_starts.contains(&current_start) {
        return Err(err("RAYCLMM_CURRENT_TICK_UNPROVEN"));
    }
    let bitmap = (0..16)
        .map(|i| u64_at(state, 904 + 8 * i))
        .collect::<Vec<_>>();
    let bitmap_extension = if matches!(
        p.protocol.as_str(),
        "raydium-clmm-bitmap" | "raydium-clmm-custom-seed" | "raydium-clmm-dynamic"
    ) {
        let address = pda(&[BITMAP_EXTENSION_SEED, &pool_bytes], &program_key)?.0;
        let data = owned_data(accounts, &address, PROGRAM_ID, BITMAP_EXTENSION_LEN)?;
        discriminator(data, [60, 150, 36, 219, 97, 128, 139, 153])?;
        if key_at(data, 8) != p.id {
            return Err(err("POOL_ACCOUNT_IDENTITY"));
        }
        Some(RaydiumCLMMBitmapExtension {
            address,
            positive: (0..BITMAP_EXTENSION_WORDS)
                .map(|i| u64_at(data, 40 + i * 8))
                .collect(),
            negative: (0..BITMAP_EXTENSION_WORDS)
                .map(|i| u64_at(data, 40 + (BITMAP_EXTENSION_WORDS + i) * 8))
                .collect(),
        })
    } else {
        None
    };
    let obs = owned_data(accounts, &p.accounts[6], PROGRAM_ID, OBSERVATION_LEN)?;
    discriminator(obs, [122, 174, 197, 53, 129, 9, 165, 132])?;
    if obs[8] > 1
        || u16_at(obs, 17) >= 100
        || key_at(obs, 19) != p.id
        || u64_at(obs, 9) > bank_epoch
        || !zero(&obs[4451..])
    {
        return Err(err("RAYCLMM_OBSERVATION_LAYOUT"));
    }
    let mut observations = Vec::with_capacity(100);
    for entry in obs[51..4451].chunks_exact(44) {
        if !zero(&entry[12..]) {
            return Err(err("RAYCLMM_OBSERVATION_LAYOUT"));
        }
        observations.push(RaydiumCLMMObservation {
            block_timestamp: tag_at(entry, 0),
            tick_cumulative: i64::from_le_bytes(entry[4..12].try_into().unwrap()),
        });
    }
    let mut s = RaydiumCLMMState {
        reward_infos,
        dynamic_fee: dynamic_profile.then(|| RaydiumCLMMDynamicFee {
            filter_period: u16_at(state, 1096).into(),
            decay_period: u16_at(state, 1098).into(),
            reduction_factor: u16_at(state, 1100).into(),
            dynamic_fee_control: tag_at(state, 1102),
            max_volatility_accumulator: tag_at(state, 1106),
            tick_spacing_index_reference: i32_at(state, 1110),
            volatility_reference: tag_at(state, 1114),
            volatility_accumulator: tag_at(state, 1118),
            last_update_timestamp: u64_at(state, 1122),
        }),
        seed_index: seed_index.into(),
        bitmap_extension,
        collect_fee_on: state[390].into(),
        program_id: PROGRAM_ID.into(),
        programdata: PROGRAM_DATA.into(),
        program_code_sha256: identity.expected_program_code_sha256.clone(),
        deployed_slot: identity.expected_deployed_slot,
        token_program_code_sha256: identity.expected_token_program_code_sha256.clone(),
        amm_config: p.accounts[3].clone(),
        pool_bump: state[8].into(),
        observation: p.accounts[6].clone(),
        vault0: p.accounts[1].clone(),
        vault1: p.accounts[2].clone(),
        raw_vault0: raw[0],
        raw_vault1: raw[1],
        mint0_decimals: state[233].into(),
        mint1_decimals: state[234].into(),
        bank_slot,
        bank_unix_timestamp,
        bank_epoch,
        trade_fee_rate,
        protocol_fee_rate,
        fund_fee_rate,
        tick_spacing: tick_spacing.into(),
        sqrt_price_x64: amount(state, 253)?,
        tick_current,
        liquidity: amount(state, 237)?,
        fee_growth_global0: amount(state, 277)?,
        fee_growth_global1: amount(state, 293)?,
        protocol_fees0: u64_at(state, 309),
        protocol_fees1: u64_at(state, 317),
        fund_fees0: u64_at(state, 1064),
        fund_fees1: u64_at(state, 1072),
        open_time: u64_at(state, 1080),
        recent_epoch: u64_at(state, 1088),
        bitmap,
        tick_arrays: Vec::new(),
        max_steps: identity.max_steps,
        observation_initialized: obs[8] == 1,
        observation_index: u16_at(obs, 17).into(),
        observation_recent_epoch: u64_at(obs, 9),
        observations,
    };
    let index_range = if s.bitmap_extension.is_some() {
        -7680..=7679
    } else {
        -512..=511
    };
    for (i, start) in identity.tick_array_starts.iter().enumerate() {
        if start % span != 0
            || !index_range.contains(&(start / span))
            || i > 0
                && i64::from(*start)
                    != i64::from(identity.tick_array_starts[i - 1]) + i64::from(span)
        {
            return Err(err("RAYCLMM_ARRAY_SEQUENCE"));
        }
        // Share the typed domain proof, especially its negative whole-group mapping.
        let active = s.array_initialized(start / span)?;
        s.tick_arrays.push(decode_array(
            accounts,
            &ids[11 + i],
            &p.id,
            *start,
            tick_spacing,
            bank_epoch,
            active,
            rewards,
        )?);
    }
    let pool = Pool {
        pancake_bsc_lm: None,
        pancake_lm: None,
        slipstream_context: None,
        lfj: None,
        infinity_bin: None,
        raydium_clmm: Some(s),
        orca_whirlpool: None,
        raydium_cpmm: None,
        singleton_cl: None,
        stable: None,
        weighted: None,
        multi_asset: None,
        singleton_advanced: None,
        v2: None,
        v3: None,
        chain_key: ch.key.clone(),
        pool_id: p.id.clone(),
        protocol: p.protocol.clone(),
        token0: p.token0.clone(),
        token1: p.token1.clone(),
        reserve0: Amount::zero(),
        reserve1: Amount::zero(),
        fee_numerator: trade_fee_rate.into(),
        fee_denominator: 1_000_000,
        adapter_version: xone_domain::protocols::protocol(&p.protocol)
            .ok_or_else(|| err("PROTOCOL_UNSUPPORTED"))?
            .adapter
            .into(),
        token_behavior: "standard".into(),
    };
    pool.validate()?;
    Ok(pool)
}

fn decode_vaults(
    p: &PoolConfig,
    accounts: &BTreeMap<String, RpcAccount>,
    state: &[u8],
    program: &[u8; 32],
    pool: &[u8; 32],
) -> Result<[u64; 2]> {
    let mut raw = [0; 2];
    for i in 0..2 {
        let mint_key = &p.accounts[4 + i];
        let mint = owned_data(accounts, mint_key, TOKEN_PROGRAM, 82)?;
        let vault = owned_data(accounts, &p.accounts[1 + i], TOKEN_PROGRAM, 165)?;
        if pda(&[b"pool_vault", pool, &pubkey(mint_key)?], program)?.0 != p.accounts[1 + i]
            || key_at(vault, 0) != *mint_key
            || key_at(vault, 32) != p.id
        {
            return Err(err("POOL_ACCOUNT_IDENTITY"));
        }
        // SPL COption is u32; None's payload may legally contain stale bytes.
        if mint[45] != 1
            || mint[44] != state[233 + i]
            || tag_at(mint, 0) > 1
            || tag_at(mint, 46) > 1
            || vault[108] != 1
            || tag_at(vault, 72) != 0
            || u64_at(vault, 121) != 0
            || tag_at(vault, 129) != 0
        {
            return Err(err("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED"));
        }
        raw[i] = u64_at(vault, 64);
        if mint_key == NATIVE_MINT {
            if mint[44] != 9
                || tag_at(vault, 109) != 1
                || get(accounts, &p.accounts[1 + i])?
                    .lamports
                    .checked_sub(u64_at(vault, 113))
                    .is_none_or(|b| b < raw[i])
            {
                return Err(err("NATIVE_VAULT_BACKING"));
            }
        } else if tag_at(vault, 109) != 0 {
            return Err(err("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED"));
        }
    }
    Ok(raw)
}

#[allow(clippy::too_many_arguments)]
fn decode_array(
    accounts: &BTreeMap<String, RpcAccount>,
    address: &str,
    pool: &str,
    start: i32,
    spacing: u16,
    bank_epoch: u64,
    active: bool,
    rewards: bool,
) -> Result<RaydiumCLMMArray> {
    let d = owned_data(accounts, address, PROGRAM_ID, ARRAY_LEN)?;
    discriminator(d, [192, 155, 85, 205, 49, 249, 129, 42])?;
    if key_at(d, 8) != pool
        || i32_at(d, 40) != start
        || u64_at(d, 10125) > bank_epoch
        || !zero(&d[10133..])
    {
        return Err(err("RAYCLMM_ARRAY_LAYOUT"));
    }
    let mut ticks = Vec::new();
    for (i, t) in d[44..10124].chunks_exact(TICK_LEN).enumerate() {
        // Source order_phase[116..124] is a historical cohort counter. With no
        // outstanding quantities, match_limit_order returns before using it.
        // Active orders, partial-fill ratio and unknown padding stay unsupported.
        if (!rewards && !zero(&t[68..116])) || !zero(&t[124..]) {
            return Err(err("RAYCLMM_TICK_STATE_UNSUPPORTED"));
        }
        let gross = u128_at(t, 20);
        let tick = i32_at(t, 0);
        let expected = i64::from(start) + i as i64 * i64::from(spacing);
        // Cleared ticks retain their index. Never-initialized slots have a default zero index.
        if gross == 0 {
            if !zero(&t[4..116]) || tick != 0 && i64::from(tick) != expected {
                return Err(err("RAYCLMM_TICK_STATE_UNSUPPORTED"));
            }
            continue;
        }
        if i64::from(tick) != expected || !(MIN_TICK..=MAX_TICK).contains(&tick) {
            return Err(err("RAYCLMM_TICK_INDEX"));
        }
        let net = i128::from_le_bytes(t[4..20].try_into().unwrap());
        if net.unsigned_abs() > gross {
            return Err(err("RAYCLMM_TICK_NET"));
        }
        ticks.push(RaydiumCLMMTick {
            reward_growths_outside: if rewards {
                (0..3)
                    .map(|i| amount(t, 68 + 16 * i))
                    .collect::<Result<Vec<_>>>()?
            } else {
                Vec::new()
            },
            tick,
            liquidity_gross: amount(t, 20)?,
            liquidity_net: net.to_string(),
            fee_growth_outside0: amount(t, 36)?,
            fee_growth_outside1: amount(t, 52)?,
        });
    }
    if ticks.len() != usize::from(d[10124]) || active == ticks.is_empty() {
        return Err(err("RAYCLMM_BITMAP_PROOF"));
    }
    Ok(RaydiumCLMMArray {
        address: address.into(),
        start_tick_index: start,
        recent_epoch: u64_at(d, 10125),
        ticks,
    })
}
