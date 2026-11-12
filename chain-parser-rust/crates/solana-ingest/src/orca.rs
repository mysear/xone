//! Orca Whirlpools cc683ec92acb5e7606caa3d8e514d9383e9c7c11.
//! Static fees with an explicit initialized-rewards profile; classic SPL Token only.
//!
//! Layouts and seeds follow the pinned program, not an emulated V3 pool. All
//! business accounts and executable identity proofs must come from one bank.
use crate::raydium::{
    check_digest, digest, get, key_at, owned_data, pda, pubkey, tag_at, u64_at, RpcAccount, CLOCK,
};
use crate::rpc::TOKEN_PROGRAM;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use xone_config::{Chain, PoolConfig};
use xone_domain::{
    err,
    orca::{OrcaRewardInfo, OrcaTick, OrcaTickArray, OrcaWhirlpoolState},
    Amount, Pool, Result,
};

pub const PROGRAM_ID: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
const PROFILE: &str = "orca-whirlpools-static";
const ADAPTER: &str = "orca-whirlpools/cc683ec9-spl-static-1";
const SYSTEM: &str = "11111111111111111111111111111111";
const SYSVAR_OWNER: &str = "Sysvar1111111111111111111111111111111111111";
const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const NATIVE_MINT: &str = "So11111111111111111111111111111111111111112";
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
fn amount(n: u128) -> Result<Amount> {
    Amount::checked(n.into())
}
fn discriminator(name: &str) -> [u8; 8] {
    Sha256::digest(format!("account:{name}").as_bytes())[..8]
        .try_into()
        .unwrap()
}
fn zero(d: &[u8]) -> bool {
    d.iter().all(|b| *b == 0)
}

pub fn oracle_address(p: &PoolConfig) -> Result<String> {
    Ok(pda(&[b"oracle", &pubkey(&p.id)?], &pubkey(PROGRAM_ID)?)?.0)
}

/// Only the already-declared Oracle PDA may have the RPC's null/system-empty
/// representation. Raydium's stricter account decoding remains unchanged.
pub fn decode_rpc_account(id: &str, oracle: &str, v: &Value) -> Result<RpcAccount> {
    if id != oracle {
        return RpcAccount::decode(v);
    }
    if v.is_null() {
        return Ok(RpcAccount {
            owner: SYSTEM.into(),
            data: Vec::new(),
            executable: false,
            lamports: 0,
        });
    }
    let encoded = v["data"]
        .as_array()
        .filter(|a| a.len() == 2 && a[1] == "base64")
        .and_then(|a| a[0].as_str())
        .ok_or_else(|| err("ACCOUNT_ENCODING"))?;
    let data = STANDARD
        .decode(encoded)
        .map_err(|_| err("ACCOUNT_ENCODING"))?;
    let lamports = v["lamports"]
        .as_u64()
        .ok_or_else(|| err("ACCOUNT_LAMPORTS_MISSING"))?;
    if v["owner"].as_str() != Some(SYSTEM)
        || v["executable"].as_bool() != Some(false)
        || !data.is_empty()
    {
        return Err(err("ORCA_INITIALIZED_OR_ADAPTIVE_ORACLE_UNSUPPORTED"));
    }
    Ok(RpcAccount {
        owner: SYSTEM.into(),
        data,
        executable: false,
        lamports,
    })
}

pub fn required_accounts(p: &PoolConfig) -> Result<Vec<String>> {
    if !matches!(
        p.protocol.as_str(),
        PROFILE
            | "orca-whirlpools-rewards"
            | "orca-whirlpools-adaptive"
            | "orca-whirlpools-adaptive-rewards"
            | "orca-whirlpools-adaptive-dynamic"
    ) || p.program_id != PROGRAM_ID
        || p.accounts.len() != 6
        || p.accounts[0] != p.id
        || p.accounts[4] != p.token0
        || p.accounts[5] != p.token1
        || p.raydium_cpmm.is_some()
        || p.v2.is_some()
        || p.v3.is_some()
        || p.stable.is_some()
        || p.singleton_cl.is_some()
    {
        return Err(err("POOL_ACCOUNT_IDENTITY"));
    }
    let cfg = p
        .orca_whirlpool
        .as_ref()
        .ok_or_else(|| err("ORCA_CONFIG_REQUIRED"))?;
    check_digest(&cfg.expected_program_code_sha256)?;
    check_digest(&cfg.expected_token_program_code_sha256)?;
    if !(1..=8192).contains(&cfg.max_steps) || !(3..=7).contains(&cfg.tick_array_starts.len()) {
        return Err(err("ORCA_ARRAY_WINDOW_OR_BUDGET"));
    }
    // Spacing is read from the pool later. Explicit starts already determine a
    // candidate span; check it now to bound and validate the requested batch.
    let starts = &cfg.tick_array_starts;
    let span = i64::from(starts[1]) - i64::from(starts[0]);
    if span <= 0 || span % 88 != 0 || span / 88 > u16::MAX.into() {
        return Err(err("ORCA_ARRAY_SEQUENCE"));
    }
    let min_start = i64::from(MIN_TICK).div_euclid(span) * span;
    for (i, start) in starts.iter().enumerate() {
        let start = i64::from(*start);
        if start % span != 0
            || start < min_start
            || start > MAX_TICK.into()
            || i > 0 && start != i64::from(starts[i - 1]) + span
        {
            return Err(err("ORCA_ARRAY_SEQUENCE"));
        }
    }
    if pubkey(&p.token0)? >= pubkey(&p.token1)? {
        return Err(err("ORCA_MINT_ORDER"));
    }
    let program = pubkey(PROGRAM_ID)?;
    if cfg.expected_programdata != pda(&[&program], &pubkey(LOADER)?)?.0 {
        return Err(err("ORCA_PROGRAMDATA_IDENTITY"));
    }
    let pool = pubkey(&p.id)?;
    let mut ids = p.accounts.clone();
    ids.extend([
        PROGRAM_ID.into(),
        cfg.expected_programdata.clone(),
        TOKEN_PROGRAM.into(),
        CLOCK.into(),
        oracle_address(p)?,
    ]);
    for start in starts {
        ids.push(
            pda(
                &[b"tick_array", &pool, start.to_string().as_bytes()],
                &program,
            )?
            .0,
        );
    }
    crate::token_program::append_dependency(&mut ids, cfg.token_program_upgrade.as_ref())?;
    let mut seen = BTreeSet::new();
    for id in &ids {
        pubkey(id)?;
        if !seen.insert(id) {
            return Err(err("ORCA_ACCOUNT_ALIAS"));
        }
    }
    Ok(ids)
}

fn decode_array(
    address: &str,
    a: &RpcAccount,
    pool: &str,
    start: i32,
    spacing: u16,
    rewards: bool,
) -> Result<OrcaTickArray> {
    let d = &a.data;
    if a.executable || a.owner != PROGRAM_ID || d.len() < 8 {
        return Err(err("ACCOUNT_OWNER_OR_LAYOUT"));
    }
    let dynamic = d[..8] == discriminator("DynamicTickArray");
    if dynamic {
        if !(148..=10004).contains(&d.len()) {
            return Err(err("ORCA_ARRAY_LAYOUT"));
        }
    } else if d[..8] != discriminator("TickArray") || d.len() != 9988 {
        return Err(err("ORCA_ARRAY_LAYOUT"));
    }
    if i32_at(d, 8) != start || key_at(d, if dynamic { 12 } else { 9956 }) != pool {
        return Err(err("ORCA_ARRAY_IDENTITY"));
    }
    let mut bitmap = 0u128;
    let declared = if dynamic { u128_at(d, 44) } else { 0 };
    if declared >> 88 != 0 {
        return Err(err("ORCA_ARRAY_BITMAP"));
    }
    let mut ticks = Vec::new();
    let mut offset = if dynamic { 60 } else { 12 };
    for i in 0..88 {
        let flag = *d.get(offset).ok_or_else(|| err("ORCA_ARRAY_TRUNCATED"))?;
        if flag > 1 || dynamic && (flag == 1) != (declared & (1u128 << i) != 0) {
            return Err(err("ORCA_ARRAY_BITMAP_OR_FLAG"));
        }
        let len = if dynamic && flag == 0 { 1 } else { 113 };
        let tick = d
            .get(offset..offset + len)
            .ok_or_else(|| err("ORCA_ARRAY_TRUNCATED"))?;
        if flag == 0 {
            if !zero(&tick[1..]) {
                return Err(err("ORCA_UNINITIALIZED_TICK_DATA"));
            }
        } else {
            let index = i64::from(start) + i as i64 * i64::from(spacing);
            let net = i128::from_le_bytes(tick[1..17].try_into().unwrap());
            let gross = u128_at(tick, 17);
            if !(i64::from(MIN_TICK)..=i64::from(MAX_TICK)).contains(&index)
                || gross == 0
                || net.unsigned_abs() > gross
                || !rewards && !zero(&tick[65..113])
            {
                return Err(err("ORCA_TICK_LIQUIDITY_OR_REWARD"));
            }
            bitmap |= 1u128 << i;
            ticks.push(OrcaTick {
                reward_growths_outside: if rewards {
                    (0..3)
                        .map(|i| amount(u128_at(tick, 65 + 16 * i)))
                        .collect::<Result<Vec<_>>>()?
                } else {
                    Vec::new()
                },
                tick: index as i32,
                liquidity_gross: amount(gross)?,
                liquidity_net: net.to_string(),
                fee_growth_outside0: amount(u128_at(tick, 33))?,
                fee_growth_outside1: amount(u128_at(tick, 49))?,
            });
        }
        offset += len;
    }
    if dynamic && offset != d.len() {
        return Err(err("ORCA_ARRAY_TRAILING_DATA"));
    }
    Ok(OrcaTickArray {
        address: address.into(),
        start_tick_index: start,
        layout: if dynamic { "dynamic" } else { "fixed" }.into(),
        initialized_bitmap: amount(bitmap)?,
        ticks,
    })
}

pub fn decode_orca_whirlpool(
    ch: &Chain,
    p: &PoolConfig,
    accounts: &BTreeMap<String, RpcAccount>,
    bank_slot: u64,
) -> Result<Pool> {
    if !ch.key.starts_with("solana:") {
        return Err(err("PROTOCOL_CHAIN_FAMILY_MISMATCH"));
    }
    if ch.confirmation_tag != "finalized" {
        return Err(err("ORCA_REQUIRES_FINALIZED"));
    }
    let ids = required_accounts(p)?;
    let cfg = p
        .orca_whirlpool
        .as_ref()
        .ok_or_else(|| err("ORCA_CONFIG_REQUIRED"))?;
    let program = get(accounts, PROGRAM_ID)?;
    if !program.executable
        || program.owner != LOADER
        || program.data.len() != 36
        || tag_at(&program.data, 0) != 2
        || key_at(&program.data, 4) != cfg.expected_programdata
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    let pd = get(accounts, &cfg.expected_programdata)?;
    if pd.executable
        || pd.owner != LOADER
        || pd.data.len() <= 45
        || tag_at(&pd.data, 0) != 3
        || pd.data[12] > 1
        || u64_at(&pd.data, 4) != cfg.expected_deployed_slot
        || cfg.expected_deployed_slot > bank_slot
        || digest(&pd.data[45..]) != cfg.expected_program_code_sha256
    {
        return Err(err("PROGRAM_VERSION_UNSUPPORTED"));
    }
    crate::token_program::verify(
        accounts,
        &cfg.expected_token_program_code_sha256,
        cfg.token_program_upgrade.as_ref(),
        bank_slot,
    )?;
    let clock = owned_data(accounts, CLOCK, SYSVAR_OWNER, 40)?;
    if u64_at(clock, 0) != bank_slot {
        return Err(err("CLOCK_CONTEXT_MISMATCH"));
    }
    let bank_unix_timestamp: u64 = i64::from_le_bytes(clock[32..40].try_into().unwrap())
        .try_into()
        .map_err(|_| err("CLOCK_TIMESTAMP_INVALID"))?;
    let oracle = oracle_address(p)?;
    let oa = get(accounts, &oracle)?;
    let adaptive_rewards = p.protocol == "orca-whirlpools-adaptive-rewards";
    let adaptive_dynamic = p.protocol == "orca-whirlpools-adaptive-dynamic";
    let adaptive_profile =
        p.protocol == "orca-whirlpools-adaptive" || adaptive_rewards || adaptive_dynamic;
    let adaptive = if adaptive_profile {
        Some(decode_initialized_oracle(oa, &p.id)?)
    } else {
        if oa.owner != SYSTEM || oa.executable || !oa.data.is_empty() {
            return Err(err("ORCA_INITIALIZED_OR_ADAPTIVE_ORACLE_UNSUPPORTED"));
        }
        None
    };
    let state = owned_data(accounts, &p.id, PROGRAM_ID, 653)?;
    // Pinned state/config.rs: 8 discriminator + 96 authority bytes + two u16s.
    let config = owned_data(accounts, &p.accounts[3], PROGRAM_ID, 108)?;
    if state[..8] != discriminator("Whirlpool")
        || config[..8] != discriminator("WhirlpoolsConfig")
        || u16_at(config, 104) > 2500
        // TOKEN_BADGE bit 0 enables badge administration, not classic-token
        // swap math. Retain rejection of all unmodelled feature bits.
        || u16_at(config, 106) & !1 != 0
    {
        return Err(err("ORCA_CONFIG_LAYOUT_OR_FLAGS"));
    }
    let spacing = u16_at(state, 41);
    let tier = u16_at(state, 43);
    if spacing == 0
        || (!adaptive_profile && tier != spacing)
        || (adaptive_profile && (tier == spacing || cfg.tick_array_starts.len() != 3))
    {
        return Err(err("ORCA_ADAPTIVE_FEE_UNSUPPORTED"));
    }
    let (pool_address, bump) = pda(
        &[
            b"whirlpool",
            &pubkey(&p.accounts[3])?,
            &pubkey(&p.token0)?,
            &pubkey(&p.token1)?,
            &tier.to_le_bytes(),
        ],
        &pubkey(PROGRAM_ID)?,
    )?;
    if pool_address != p.id || state[40] != bump || key_at(state, 8) != p.accounts[3] {
        return Err(err("POOL_ACCOUNT_IDENTITY"));
    }
    let span = i32::from(spacing) * 88;
    if cfg
        .tick_array_starts
        .windows(2)
        .any(|w| w[1] - w[0] != span)
    {
        return Err(err("ORCA_ARRAY_SEQUENCE"));
    }
    let current_tick = i32_at(state, 81);
    if !(MIN_TICK - 1..=MAX_TICK).contains(&current_tick) {
        return Err(err("ORCA_TICK_RANGE"));
    }
    let current_start = current_tick.div_euclid(span) * span;
    if !cfg.tick_array_starts.contains(&current_start) {
        return Err(err("ORCA_CURRENT_TICK_UNPROVEN"));
    }
    let rewards = p.protocol == "orca-whirlpools-rewards" || adaptive_profile;
    let mut reward_infos = Vec::new();
    for i in 0..3 {
        let reward = &state[269 + i * 128..269 + (i + 1) * 128];
        // Slot zero retains the reward authority; slots one/two contain control
        // extensions that remain unsupported for this explicitly static profile.
        if i > 0 && !zero(&reward[64..96])
            || !rewards && (!zero(&reward[..64]) || !zero(&reward[96..]))
        {
            return Err(err("ORCA_REWARD_OR_CONTROL_UNSUPPORTED"));
        }
        if rewards {
            reward_infos.push(OrcaRewardInfo {
                mint: key_at(reward, 0),
                vault: key_at(reward, 32),
                extension: reward[64..96].to_vec(),
                emissions_per_second_x64: amount(u128_at(reward, 96))?,
                growth_global_x64: amount(u128_at(reward, 112))?,
            });
        }
    }
    if u64_at(state, 261) > bank_unix_timestamp {
        return Err(err("ORCA_REWARD_TIMESTAMP"));
    }
    // initialize_config and initialize_pool use ordinary config/vault accounts,
    // not config/vault PDAs. Bind their keys through the pool and SPL authority.
    let mut raw = [0; 2];
    let mut decimals = [0; 2];
    for i in 0..2 {
        let mint_key = &p.accounts[4 + i];
        let mint = owned_data(accounts, mint_key, TOKEN_PROGRAM, 82)?;
        let vault = owned_data(accounts, &p.accounts[1 + i], TOKEN_PROGRAM, 165)?;
        let mint_offset = if i == 0 { 101 } else { 181 };
        let vault_offset = if i == 0 { 133 } else { 213 };
        if key_at(state, mint_offset) != *mint_key
            || key_at(state, vault_offset) != p.accounts[1 + i]
            || key_at(vault, 0) != *mint_key
            || key_at(vault, 32) != p.id
        {
            return Err(err("POOL_ACCOUNT_IDENTITY"));
        }
        // SPL COption has a u32 tag. Payloads of None may retain stale bytes.
        if mint[45] != 1
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
        decimals[i] = u32::from(mint[44]);
        if mint_key == NATIVE_MINT {
            if mint[44] != 9
                || tag_at(vault, 109) != 1
                || get(accounts, &p.accounts[1 + i])?
                    .lamports
                    .checked_sub(u64_at(vault, 113))
                    .is_none_or(|backing| backing < raw[i])
            {
                return Err(err("NATIVE_VAULT_BACKING"));
            }
        } else if tag_at(vault, 109) != 0 {
            return Err(err("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED"));
        }
    }
    let tick_arrays = cfg
        .tick_array_starts
        .iter()
        .zip(&ids[11..])
        .map(|(start, address)| {
            decode_array(
                address,
                get(accounts, address)?,
                &p.id,
                *start,
                spacing,
                rewards,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let fee = u16_at(state, 45);
    let s = OrcaWhirlpoolState {
        adaptive,
        reward_infos,
        program_id: PROGRAM_ID.into(),
        programdata: cfg.expected_programdata.clone(),
        program_code_sha256: cfg.expected_program_code_sha256.clone(),
        deployed_slot: cfg.expected_deployed_slot,
        token_program_code_sha256: cfg.expected_token_program_code_sha256.clone(),
        whirlpools_config: p.accounts[3].clone(),
        whirlpool_bump: bump.into(),
        oracle,
        vault0: p.accounts[1].clone(),
        vault1: p.accounts[2].clone(),
        raw_vault0: raw[0],
        raw_vault1: raw[1],
        mint0_decimals: decimals[0],
        mint1_decimals: decimals[1],
        bank_slot,
        bank_unix_timestamp,
        bank_epoch: u64_at(clock, 16),
        fee_tier_index: u16_at(state, 43).into(),
        fee_rate: fee.into(),
        protocol_fee_rate: u16_at(state, 47).into(),
        sqrt_price_x64: amount(u128_at(state, 65))?,
        tick_current_index: current_tick,
        liquidity: amount(u128_at(state, 49))?,
        tick_spacing: spacing.into(),
        protocol_fee_owed0: u64_at(state, 85),
        protocol_fee_owed1: u64_at(state, 93),
        fee_growth_global0: amount(u128_at(state, 165))?,
        fee_growth_global1: amount(u128_at(state, 245))?,
        reward_last_updated_timestamp: u64_at(state, 261),
        tick_arrays,
        max_steps: cfg.max_steps,
    };
    let pool = Pool {
        pancake_bsc_lm: None,
        pancake_lm: None,
        slipstream_context: None,
        lfj: None,
        meteora: None,
        pump: None,
        infinity_bin: None,
        raydium_clmm: None,
        orca_whirlpool: Some(s),
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
        fee_numerator: fee.into(),
        fee_denominator: 1_000_000,
        adapter_version: if adaptive_dynamic {
            "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1"
        } else if adaptive_rewards {
            "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1"
        } else if adaptive_profile {
            "orca-whirlpools/cc683ec9-spl-adaptive-1"
        } else if rewards {
            "orca-whirlpools/cc683ec9-spl-rewards-1"
        } else {
            ADAPTER
        }
        .into(),
        token_behavior: "standard".into(),
    };
    pool.orca_whirlpool
        .as_ref()
        .ok_or_else(|| err("ORCA_STATE_REQUIRED"))?
        .validate(&pool)?;
    pool.validate()?;
    Ok(pool)
}

/// This entry is chosen only for an explicitly configured adaptive Oracle PDA.
/// Legacy profiles keep the strict null/system-empty decoder above.
pub fn decode_adaptive_rpc_account(v: &Value) -> Result<RpcAccount> {
    let account = RpcAccount::decode(v)?;
    if account.owner != PROGRAM_ID
        || account.executable
        || account.data.len() != 254
        || account.data[..8] != discriminator("Oracle")
    {
        return Err(err("ORCA_ADAPTIVE_ORACLE_LAYOUT"));
    }
    Ok(account)
}
fn decode_initialized_oracle(
    a: &RpcAccount,
    pool: &str,
) -> Result<xone_domain::orca_adaptive_state::OrcaAdaptiveState> {
    let d = &a.data;
    if a.owner != PROGRAM_ID || a.executable || d.len() != 254 || d[..8] != discriminator("Oracle")
    {
        return Err(err("ORCA_ADAPTIVE_ORACLE_LAYOUT"));
    }
    if key_at(d, 8) != pool {
        return Err(err("ORCA_ADAPTIVE_ORACLE_BINDING"));
    }
    Ok(xone_domain::orca_adaptive_state::OrcaAdaptiveState {
        whirlpool: key_at(d, 8),
        trade_enable_timestamp: u64_at(d, 40),
        filter_period: u16_at(d, 48).into(),
        decay_period: u16_at(d, 50).into(),
        reduction_factor: u16_at(d, 52).into(),
        adaptive_fee_control_factor: u32::from_le_bytes(d[54..58].try_into().unwrap()),
        max_volatility_accumulator: u32::from_le_bytes(d[58..62].try_into().unwrap()),
        tick_group_size: u16_at(d, 62).into(),
        major_swap_threshold_ticks: u16_at(d, 64).into(),
        constants_reserved: d[66..82].to_vec(),
        last_reference_update_timestamp: u64_at(d, 82),
        last_major_swap_timestamp: u64_at(d, 90),
        volatility_reference: u32::from_le_bytes(d[98..102].try_into().unwrap()),
        tick_group_index_reference: i32_at(d, 102),
        volatility_accumulator: u32::from_le_bytes(d[106..110].try_into().unwrap()),
        variables_reserved: d[110..126].to_vec(),
        reserved: d[126..254].to_vec(),
    })
}
